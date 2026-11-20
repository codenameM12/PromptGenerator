//! Named sets of reference images ("characters" in the UI; they can equally be
//! products). Images are copied into the workspace, never linked to the
//! source file. See docs/spec-v2.md.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::commands::CommandError;

fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

fn clean(text: &str, limit: usize) -> String {
    text.trim().chars().take(limit).collect()
}

pub fn characters_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("characters")
}

/// Stored paths are relative to the data folder (so a moved portable folder
/// keeps working); older rows with an absolute path are used as they are.
pub fn resolve(data_dir: &Path, stored: &str) -> PathBuf {
    let path = Path::new(stored);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        data_dir.join(path)
    }
}

/// Rewrites every image `path` in a set (or one image) to an absolute path.
pub fn absolutize(value: &mut Value, data_dir: &Path) {
    let fix = |image: &mut Value| {
        if let Some(stored) = image.get("path").and_then(Value::as_str) {
            let full = resolve(data_dir, stored).to_string_lossy().to_string();
            image["path"] = json!(full);
        }
    };
    if let Some(images) = value.get_mut("images").and_then(Value::as_array_mut) {
        images.iter_mut().for_each(fix);
    } else {
        fix(value);
    }
}

fn used_by_media(connection: &Connection, path: &Path) -> bool {
    connection
        .query_row(
            "SELECT COUNT(*) FROM assets WHERE source_path = ?1 AND deleted_at IS NULL",
            params![path.to_string_lossy()],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false)
}

pub fn list(connection: &Connection) -> Result<Value, CommandError> {
    let mut statement = connection.prepare(
        "SELECT c.id, c.name, c.description,
                (SELECT COUNT(*) FROM character_images i WHERE i.character_id = c.id),
                (SELECT COUNT(*) FROM character_images i WHERE i.character_id = c.id AND i.is_default = 1)
         FROM characters c ORDER BY c.name COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "description": row.get::<_, String>(2)?,
            "imageCount": row.get::<_, i64>(3)?,
            "defaultCount": row.get::<_, i64>(4)?,
        }))
    })?;
    Ok(Value::Array(rows.collect::<Result<Vec<_>, _>>()?))
}

pub fn get(connection: &Connection, id: &str) -> Result<Value, CommandError> {
    let head = connection
        .query_row(
            "SELECT name, description FROM characters WHERE id = ?1",
            params![id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| CommandError::not_found("Not found"))?;
    Ok(json!({
        "id": id,
        "name": head.0,
        "description": head.1,
        "images": images(connection, id)?,
    }))
}

fn images(connection: &Connection, character_id: &str) -> Result<Vec<Value>, CommandError> {
    let mut statement = connection.prepare(
        "SELECT id, file_name, stored_path, role, description, is_default
         FROM character_images WHERE character_id = ?1 ORDER BY position, created_at",
    )?;
    let rows = statement.query_map(params![character_id], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "fileName": row.get::<_, String>(1)?,
            "path": row.get::<_, String>(2)?,
            "role": row.get::<_, String>(3)?,
            "description": row.get::<_, String>(4)?,
            "isDefault": row.get::<_, i64>(5)? != 0,
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn create(connection: &Connection, name: &str) -> Result<Value, CommandError> {
    let name = clean(name, 120);
    if name.is_empty() {
        return Err(CommandError::validation("Enter a name"));
    }
    let id = new_id();
    connection.execute(
        "INSERT INTO characters (id, name, description, created_at)
         VALUES (?1, ?2, '', strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
        params![id, name],
    )?;
    get(connection, &id)
}

pub fn update(
    connection: &Connection,
    id: &str,
    name: &str,
    description: &str,
) -> Result<Value, CommandError> {
    let name = clean(name, 120);
    if name.is_empty() {
        return Err(CommandError::validation("Enter a name"));
    }
    let changed = connection.execute(
        "UPDATE characters SET name = ?2, description = ?3 WHERE id = ?1",
        params![id, name, clean(description, 4000)],
    )?;
    if changed == 0 {
        return Err(CommandError::not_found("Not found"));
    }
    get(connection, id)
}

/// Deletes the set and its stored image copies. Generations keep their text;
/// only the link rows go.
pub fn delete(connection: &Connection, data_dir: &Path, id: &str) -> Result<(), CommandError> {
    let stored: Vec<String> = images(connection, id)?
        .iter()
        .filter_map(|image| image["path"].as_str().map(str::to_string))
        .collect();
    connection.execute("DELETE FROM characters WHERE id = ?1", params![id])?;
    // Images that were loaded into Media are still used there; keep those files.
    for path in stored {
        let full = resolve(data_dir, &path);
        if !used_by_media(connection, &full) {
            let _ = fs::remove_file(full);
        }
    }
    let folder = characters_dir(data_dir).join(id);
    let _ = fs::remove_dir(folder); // only succeeds when empty
    Ok(())
}

const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jfif", "jpe", "webp", "bmp", "gif", "tif", "tiff", "avif",
];
const MAX_IMPORT: usize = 500;

/// Files and folders in, image files out. Folders are scanned recursively.
pub fn expand_sources(inputs: &[String]) -> Vec<String> {
    fn visit(path: &Path, out: &mut Vec<String>) {
        if out.len() >= MAX_IMPORT {
            return;
        }
        if path.is_dir() {
            let mut entries: Vec<PathBuf> = fs::read_dir(path)
                .map(|iter| iter.filter_map(Result::ok).map(|entry| entry.path()).collect())
                .unwrap_or_default();
            entries.sort();
            for entry in entries {
                visit(&entry, out);
            }
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        {
            out.push(path.to_string_lossy().to_string());
        }
    }
    let mut out = Vec::new();
    for input in inputs {
        visit(Path::new(input), &mut out);
    }
    out
}

/// Copies each file into `<data>/characters/<id>/` and records it. Folders
/// are expanded to the image files inside them.
pub fn add_images(
    connection: &Connection,
    data_dir: &Path,
    character_id: &str,
    sources: &[String],
) -> Result<Value, CommandError> {
    get(connection, character_id)?;
    let sources = &expand_sources(sources);
    let folder = characters_dir(data_dir).join(character_id);
    fs::create_dir_all(&folder).map_err(|error| CommandError::from_io(&error))?;
    let mut position: i64 = connection.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM character_images WHERE character_id = ?1",
        params![character_id],
        |row| row.get(0),
    )?;
    for source in sources {
        let source_path = Path::new(source);
        let Some(file_name) = source_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let image_id = new_id();
        let extension = source_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("png");
        let relative = format!("characters/{character_id}/{image_id}.{extension}");
        let stored = data_dir.join(&relative);
        fs::copy(source_path, &stored).map_err(|error| CommandError::from_io(&error))?;
        let stem = source_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or(file_name);
        connection.execute(
            "INSERT INTO character_images
               (id, character_id, file_name, stored_path, role, description, is_default, position, created_at)
             VALUES (?1, ?2, ?3, ?4, '', '', 0, ?5, strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
            params![
                image_id,
                character_id,
                clean(stem, 120),
                relative,
                position
            ],
        )?;
        position += 1;
    }
    get(connection, character_id)
}

pub fn update_image(
    connection: &Connection,
    image_id: &str,
    file_name: &str,
    role: &str,
    description: &str,
    is_default: bool,
) -> Result<(), CommandError> {
    connection.execute(
        "UPDATE character_images SET file_name = ?2, role = ?3, description = ?4, is_default = ?5 WHERE id = ?1",
        params![
            image_id,
            clean(file_name, 120),
            clean(role, 60),
            clean(description, 4000),
            i64::from(is_default)
        ],
    )?;
    Ok(())
}

fn name_words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|ch: char| !ch.is_alphanumeric() && ch != '\'' && ch != '-')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

fn file_safe(title: &str) -> String {
    title
        .chars()
        .filter(|ch| ch.is_alphanumeric() || *ch == ' ' || *ch == '-')
        .collect::<String>()
        .trim()
        .to_string()
}

/// Names an image after its description: a short title that avoids every word
/// already used by the other images of the same set. Renames the app's own
/// copy of the file (never the original) and keeps Media items that point at
/// it working. Returns `{ renamed: false }` when there is no description.
pub fn autoname_image(
    connection: &Connection,
    data_dir: &Path,
    image_id: &str,
    titler: &dyn Fn(&str, &HashSet<String>) -> String,
) -> Result<Value, CommandError> {
    let row = connection
        .query_row(
            "SELECT character_id, description, stored_path FROM character_images WHERE id = ?1",
            params![image_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CommandError::not_found("Image not found"))?;
    let (character_id, description, stored) = row;
    if description.trim().is_empty() {
        return Ok(json!({ "renamed": false }));
    }
    let used: HashSet<String> = {
        let mut statement = connection.prepare(
            "SELECT file_name FROM character_images WHERE character_id = ?1 AND id != ?2",
        )?;
        let names = statement.query_map(params![character_id, image_id], |row| {
            row.get::<_, String>(0)
        })?;
        let mut words = HashSet::new();
        for name in names {
            words.extend(name_words(&name?));
        }
        words
    };
    let title = file_safe(&titler(description.trim(), &used));
    if title.is_empty() {
        return Ok(json!({ "renamed": false }));
    }

    let old_full = resolve(data_dir, &stored);
    let extension = old_full
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("png")
        .to_string();
    let folder = old_full
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| characters_dir(data_dir).join(&character_id));
    let mut candidate = folder.join(format!("{title}.{extension}"));
    let mut suffix = 2;
    while candidate.exists() && candidate != old_full {
        candidate = folder.join(format!("{title} {suffix}.{extension}"));
        suffix += 1;
    }
    if candidate != old_full {
        fs::rename(&old_full, &candidate).map_err(|error| CommandError::from_io(&error))?;
    }
    let relative = candidate
        .strip_prefix(data_dir)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| candidate.to_string_lossy().to_string());
    connection.execute(
        "UPDATE character_images SET file_name = ?2, stored_path = ?3 WHERE id = ?1",
        params![image_id, title, relative],
    )?;
    // Media items imported from this copy must follow the file.
    let old_text = old_full.to_string_lossy().to_string();
    let new_text = candidate.to_string_lossy().to_string();
    for (from, to) in [
        (old_text.clone(), new_text.clone()),
        (old_text.replace('/', "\\"), new_text.replace('/', "\\")),
        (old_text.replace('\\', "/"), new_text.replace('\\', "/")),
    ] {
        connection.execute(
            "UPDATE assets SET source_path = ?2, display_name = ?3 WHERE source_path = ?1",
            params![from, to, format!("{title}.{extension}")],
        )?;
    }
    Ok(json!({
        "renamed": true,
        "fileName": title,
        "path": new_text,
    }))
}

pub fn remove_image(
    connection: &Connection,
    data_dir: &Path,
    image_id: &str,
) -> Result<(), CommandError> {
    let stored: Option<String> = connection
        .query_row(
            "SELECT stored_path FROM character_images WHERE id = ?1",
            params![image_id],
            |row| row.get(0),
        )
        .optional()?;
    connection.execute("DELETE FROM character_images WHERE id = ?1", params![image_id])?;
    if let Some(path) = stored {
        let full = resolve(data_dir, &path);
        if !used_by_media(connection, &full) {
            let _ = fs::remove_file(full);
        }
    }
    Ok(())
}

/// The images to load when the set is added to Compose: the requested ones, else
/// the defaults, else one random image (`avoid` is excluded when shuffling).
pub fn pick_images(
    connection: &Connection,
    character_id: &str,
    requested: &[String],
    avoid: Option<&str>,
) -> Result<Vec<Value>, CommandError> {
    let all = images(connection, character_id)?;
    let id_of = |image: &Value| image["id"].as_str().unwrap_or_default().to_string();
    if !requested.is_empty() {
        return Ok(all
            .into_iter()
            .filter(|image| requested.contains(&id_of(image)))
            .collect());
    }
    let defaults: Vec<Value> = all
        .iter()
        .filter(|image| image["isDefault"].as_bool().unwrap_or(false))
        .cloned()
        .collect();
    if !defaults.is_empty() {
        return Ok(defaults);
    }
    let mut pool: Vec<Value> = all
        .iter()
        .filter(|image| Some(id_of(image).as_str()) != avoid)
        .cloned()
        .collect();
    if pool.is_empty() {
        pool = all;
    }
    if pool.is_empty() {
        return Ok(Vec::new());
    }
    let entropy = uuid::Uuid::now_v7().as_u128() as usize;
    Ok(vec![pool[entropy % pool.len()].clone()])
}

pub fn link_generations(
    connection: &Connection,
    character_ids: &[String],
    generation_ids: &[String],
) -> Result<(), CommandError> {
    for character in character_ids {
        for generation in generation_ids {
            connection.execute(
                "INSERT OR IGNORE INTO generation_characters (generation_id, character_id) VALUES (?1, ?2)",
                params![generation, character],
            )?;
        }
    }
    Ok(())
}

pub fn generation_ids(
    connection: &Connection,
    character_id: &str,
) -> Result<Vec<String>, CommandError> {
    let mut statement = connection
        .prepare("SELECT generation_id FROM generation_characters WHERE character_id = ?1")?;
    let rows = statement.query_map(params![character_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn database() -> Database {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        database
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ps-char-{label}-{}", new_id()));
        fs::create_dir_all(&dir).expect("dir");
        dir
    }

    fn sample_files(dir: &Path, count: usize) -> Vec<String> {
        (0..count)
            .map(|index| {
                let path = dir.join(format!("shot{index}.png"));
                fs::write(&path, [index as u8; 8]).expect("write");
                path.to_string_lossy().to_string()
            })
            .collect()
    }

    #[test]
    fn create_update_and_list() {
        let database = database();
        let connection = database.connection();
        let made = create(connection, "  Mira ").expect("create");
        assert_eq!(made["name"], "Mira");
        let id = made["id"].as_str().unwrap().to_string();
        update(connection, &id, "Mira K", "tall, red coat").expect("update");
        let listed = list(connection).expect("list");
        assert_eq!(listed[0]["name"], "Mira K");
        assert_eq!(listed[0]["imageCount"], 0);
        assert!(create(connection, "  ").is_err());
    }

    #[test]
    fn images_are_copied_into_the_workspace() {
        let database = database();
        let connection = database.connection();
        let data = temp_dir("data");
        let source = temp_dir("src");
        let id = create(connection, "Mira").unwrap()["id"].as_str().unwrap().to_string();
        let got = add_images(connection, &data, &id, &sample_files(&source, 2)).expect("add");
        let relative = got["images"][0]["path"].as_str().unwrap().to_string();
        // Stored relative to the data folder, so a moved folder keeps working.
        assert!(relative.starts_with("characters/"), "{relative}");
        let stored = resolve(&data, &relative);
        assert!(stored.starts_with(&data));
        assert!(stored.exists());
        // Deleting the source does not break the stored copy.
        fs::remove_dir_all(&source).unwrap();
        assert!(stored.exists());
        delete(connection, &data, &id).expect("delete");
        assert!(!stored.exists());
        assert!(get(connection, &id).is_err());
    }

    #[test]
    fn defaults_then_random_pick() {
        let database = database();
        let connection = database.connection();
        let data = temp_dir("data2");
        let source = temp_dir("src2");
        let id = create(connection, "Box").unwrap()["id"].as_str().unwrap().to_string();
        let got = add_images(connection, &data, &id, &sample_files(&source, 3)).unwrap();
        let first = got["images"][0]["id"].as_str().unwrap().to_string();
        let second = got["images"][1]["id"].as_str().unwrap().to_string();

        // No defaults: exactly one image, never the avoided one.
        for _ in 0..10 {
            let picked = pick_images(connection, &id, &[], Some(&first)).unwrap();
            assert_eq!(picked.len(), 1);
            assert_ne!(picked[0]["id"], first.as_str());
        }
        update_image(connection, &first, "shot0", "front", "front view", true).unwrap();
        update_image(connection, &second, "shot1", "profile", "", true).unwrap();
        let picked = pick_images(connection, &id, &[], None).unwrap();
        assert_eq!(picked.len(), 2);
        assert_eq!(picked[0]["role"], "front");
        // Requested ids win over defaults.
        let one = pick_images(connection, &id, &[second.clone()], None).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0]["id"], second.as_str());
    }

    #[test]
    fn folders_are_expanded_to_their_image_files() {
        let database = database();
        let connection = database.connection();
        let data = temp_dir("data3");
        let root = temp_dir("src3");
        let nested = root.join("sub");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("a.jfif"), [1u8; 4]).unwrap();
        fs::write(nested.join("b.PNG"), [2u8; 4]).unwrap();
        fs::write(nested.join("notes.txt"), b"x").unwrap();
        let single = root.join("a.jfif").to_string_lossy().to_string();
        let found = expand_sources(&[root.to_string_lossy().to_string(), single.clone()]);
        assert_eq!(found.len(), 3, "{found:?}");
        let id = create(connection, "Pack").unwrap()["id"].as_str().unwrap().to_string();
        let got = add_images(connection, &data, &id, &[root.to_string_lossy().to_string()]).unwrap();
        assert_eq!(got["images"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn deleting_keeps_files_that_media_still_uses() {
        let database = database();
        let connection = database.connection();
        let data = temp_dir("data4");
        let source = temp_dir("src4");
        let id = create(connection, "Mira").unwrap()["id"].as_str().unwrap().to_string();
        let got = add_images(connection, &data, &id, &sample_files(&source, 2)).unwrap();
        let used = resolve(&data, got["images"][0]["path"].as_str().unwrap());
        let unused = resolve(&data, got["images"][1]["path"].as_str().unwrap());
        // The first image was loaded into Media (an asset points at the copy).
        connection
            .execute(
                "INSERT INTO assets (id, hash, kind, source_path, display_name, created_at)
                 VALUES ('a1', 'h1', 'image', ?1, 'x', '2026-01-01T00:00:00Z')",
                params![used.to_string_lossy()],
            )
            .unwrap();
        delete(connection, &data, &id).expect("delete");
        assert!(used.exists(), "a file Media still uses must stay");
        assert!(!unused.exists());
    }

    #[test]
    fn autoname_renames_the_copy_and_avoids_words_used_by_siblings() {
        let database = database();
        let connection = database.connection();
        let data = temp_dir("data5");
        let source = temp_dir("src5");
        let id = create(connection, "Mira").unwrap()["id"].as_str().unwrap().to_string();
        let got = add_images(connection, &data, &id, &sample_files(&source, 2)).unwrap();
        let first = got["images"][0]["id"].as_str().unwrap().to_string();
        let second = got["images"][1]["id"].as_str().unwrap().to_string();
        let original = source.join("shot0.png");
        update_image(connection, &first, "shot0", "", "red coat smiling woman", false).unwrap();
        update_image(connection, &second, "shot1", "", "red coat walking street", false).unwrap();
        // A Media item imported from the first copy.
        let first_path = resolve(&data, got["images"][0]["path"].as_str().unwrap());
        connection
            .execute(
                "INSERT INTO assets (id, hash, kind, source_path, display_name, created_at)
                 VALUES ('a1', 'h1', 'image', ?1, 'x', '2026-01-01T00:00:00Z')",
                params![first_path.to_string_lossy()],
            )
            .unwrap();

        // Stand-in titler: the first content words not excluded.
        let titler = |text: &str, exclude: &HashSet<String>| {
            text.split_whitespace()
                .filter(|word| !exclude.contains(&word.to_lowercase()))
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
        };
        let one = autoname_image(connection, &data, &first, &titler).unwrap();
        assert_eq!(one["fileName"], "red coat smiling");
        let renamed = PathBuf::from(one["path"].as_str().unwrap());
        assert!(renamed.exists());
        assert!(!first_path.exists());
        assert!(original.exists(), "the original is never touched");
        let asset_path: String = connection
            .query_row("SELECT source_path FROM assets WHERE id = 'a1'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(asset_path, renamed.to_string_lossy());

        // The sibling cannot reuse "red" or "coat" or "smiling".
        let two = autoname_image(connection, &data, &second, &titler).unwrap();
        let name = two["fileName"].as_str().unwrap().to_lowercase();
        assert!(!name.contains("red") && !name.contains("coat"), "{name}");
        assert_eq!(name, "walking street");
        // No description: nothing happens.
        let third = create(connection, "Empty").unwrap()["id"].as_str().unwrap().to_string();
        let got = add_images(connection, &data, &third, &sample_files(&source, 1)).unwrap();
        let none = autoname_image(connection, &data, got["images"][0]["id"].as_str().unwrap(), &titler).unwrap();
        assert_eq!(none["renamed"], false);
    }

    #[test]
    fn generation_links_round_trip() {
        let database = database();
        let connection = database.connection();
        // Foreign keys are not enforced for link rows in this check.
        let id = create(connection, "Mira").unwrap()["id"].as_str().unwrap().to_string();
        connection
            .execute("PRAGMA foreign_keys = OFF", [])
            .expect("pragma");
        link_generations(connection, &[id.clone()], &["g1".into(), "g2".into()]).unwrap();
        link_generations(connection, &[id.clone()], &["g1".into()]).unwrap();
        let mut ids = generation_ids(connection, &id).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["g1".to_string(), "g2".to_string()]);
    }
}
