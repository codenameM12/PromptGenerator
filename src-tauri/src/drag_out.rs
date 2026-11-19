use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::commands::drag::{DragPayloadKind, DragPrepareRequest};
use crate::commands::edits::EditOps;
use crate::commands::{CommandError, Guidance};
use crate::db::repository::assets as asset_repo;
use crate::db::repository::edits as edit_repo;
use crate::db::repository::generations as gen_repo;

const DRAG_DIR: &str = "drag";
const RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedDrag {
    pub files: Vec<String>,
    pub prepared_dir: String,
    pub suggested_name: String,
}

struct Context<'a> {
    request: &'a DragPrepareRequest,
    transfer: &'a Transfer,
}

#[derive(Default)]
pub struct Transfer {
    pub cancel: AtomicBool,
    pub bytes_done: std::sync::atomic::AtomicU64,
    pub bytes_total: std::sync::atomic::AtomicU64,
}

impl Transfer {
    pub fn progress(&self) -> f64 {
        let total = self.bytes_total.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.bytes_done.load(Ordering::Relaxed) as f64 / total as f64
        }
    }
}

pub fn cleanup(cache_dir: &Path, now: SystemTime) -> usize {
    let directory = cache_dir.join(DRAG_DIR);
    let Ok(entries) = fs::read_dir(&directory) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map(|modified| now.duration_since(modified).unwrap_or_default() > RETENTION)
            .unwrap_or(true);
        if stale && fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

pub fn prepare(
    connection: &Connection,
    request: &DragPrepareRequest,
    template: &str,
    cache_dir: &Path,
    transfer: &Transfer,
    on_progress: &mut (dyn FnMut(f64) + Send),
) -> Result<PreparedDrag, CommandError> {
    let context = Context { request, transfer };
    let generation = request
        .generation_id
        .as_deref()
        .map(|id| {
            gen_repo::get(connection, id)?
                .ok_or_else(|| CommandError::not_found("Generation not found"))
        })
        .transpose()?;
    let asset = request
        .asset_id
        .as_deref()
        .or_else(|| generation.as_ref().and_then(|row| row.asset_id.as_deref()))
        .map(|id| {
            asset_repo::get(connection, id)?
                .ok_or_else(|| CommandError::not_found("Source asset not found"))
        })
        .transpose()?;
    let edit = request
        .edit_id
        .as_deref()
        .or_else(|| generation.as_ref().and_then(|row| row.edit_id.as_deref()))
        .map(|id| {
            edit_repo::get(connection, id)?
                .ok_or_else(|| CommandError::not_found("Edited media not found"))
        })
        .transpose()?;

    let preset = generation
        .as_ref()
        .map(|row| row.preset_id.clone())
        .or_else(|| asset.as_ref().map(|row| row.kind.clone()))
        .unwrap_or_else(|| "prompt".into());
    let asset_name = asset
        .as_ref()
        .map(|row| {
            Path::new(&row.display_name)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("asset")
                .to_string()
        })
        .unwrap_or_else(|| "prompt".into());
    let seed = generation
        .as_ref()
        .and_then(|row| row.seed)
        .map(|seed| seed.to_string())
        .unwrap_or_default();
    let (date, time) = now_parts(connection)?;
    let base = sanitize_component(&expand_template(
        template,
        &preset,
        &asset_name,
        &date,
        &time,
        "1",
        &seed,
    ));

    let session = cache_dir
        .join(DRAG_DIR)
        .join(uuid::Uuid::now_v7().to_string());
    fs::create_dir_all(&session).map_err(|error| CommandError::from_io(&error))?;

    let mut planned: Vec<PathBuf> = Vec::new();
    let mut notes = Vec::new();

    match request.payload {
        DragPayloadKind::PlainPrompt => {
            let text = prompt_text(&context, generation.as_ref())?;
            planned.push(write_text(&session, &base, "txt", &text)?);
        }
        DragPayloadKind::StructuredJson => {
            let value = structured(&context, generation.as_ref(), asset.as_ref())?;
            let text = serde_json::to_string_pretty(&value)
                .map_err(|error| CommandError::internal(format!("serialize drag JSON: {error}")))?;
            planned.push(write_text(&session, &base, "json", &text)?);
        }
        DragPayloadKind::Original => {
            let asset = asset
                .as_ref()
                .ok_or_else(|| CommandError::not_found("No source media selected"))?;
            let source = Path::new(&asset.source_path);
            planned.push(copy_media(&context, source, &session, &base, on_progress)?);
        }
        DragPayloadKind::Edited => {
            let edit = edit
                .as_ref()
                .ok_or_else(|| CommandError::not_found("No edited media selected"))?;
            let source = Path::new(&edit.output_path);
            planned.push(copy_media(&context, source, &session, &base, on_progress)?);
        }
        DragPayloadKind::Mask => {
            let edit = edit
                .as_ref()
                .ok_or_else(|| CommandError::not_found("No edit selected for the mask"))?;
            let ops: EditOps = serde_json::from_str(&edit.ops_json)
                .map_err(|error| CommandError::internal(format!("read edit ops: {error}")))?;
            let mask = ops
                .mask_path
                .ok_or_else(|| CommandError::not_found("This edit has no mask"))?;
            planned.push(copy_media(
                &context,
                Path::new(&mask),
                &session,
                &base,
                on_progress,
            )?);
        }
        DragPayloadKind::Bundle => {
            if let Some(edit) = &edit {
                let source = Path::new(&edit.output_path);
                planned.push(copy_media(&context, source, &session, &base, on_progress)?);
            } else if let Some(asset) = &asset {
                let source = Path::new(&asset.source_path);
                planned.push(copy_media(&context, source, &session, &base, on_progress)?);
            }
            let text = prompt_text(&context, generation.as_ref())?;
            planned.push(write_text(&session, &base, "txt", &text)?);
            let structured = serde_json::to_string_pretty(&structured(
                &context,
                generation.as_ref(),
                asset.as_ref(),
            )?)
            .map_err(|error| CommandError::internal(format!("serialize drag JSON: {error}")))?;
            planned.push(write_text(&session, &base, "json", &structured)?);
        }
    }

    if request.sidecars
        && !matches!(
            request.payload,
            DragPayloadKind::PlainPrompt
                | DragPayloadKind::StructuredJson
                | DragPayloadKind::Bundle
        )
    {
        let text = prompt_text(&context, generation.as_ref())?;
        planned.push(write_text(&session, &base, "txt", &text)?);
        let structured = serde_json::to_string_pretty(&structured(
            &context,
            generation.as_ref(),
            asset.as_ref(),
        )?)
        .map_err(|error| CommandError::internal(format!("serialize drag JSON: {error}")))?;
        planned.push(write_text(&session, &base, "json", &structured)?);
    }

    on_progress(1.0);
    notes.extend(planned.iter().map(|path| path.display().to_string()));
    Ok(PreparedDrag {
        files: notes,
        prepared_dir: session.display().to_string(),
        suggested_name: base,
    })
}

fn prompt_text(
    context: &Context<'_>,
    generation: Option<&gen_repo::GenerationRow>,
) -> Result<String, CommandError> {
    if let Some(generation) = generation {
        let mut text = generation.prompt.clone();
        if let Some(negative) = &generation.negative {
            text.push_str("\n\nNegative: ");
            text.push_str(negative);
        }
        return Ok(text);
    }
    let asset = context
        .request
        .asset_id
        .as_deref()
        .ok_or_else(|| CommandError::not_found("No prompt or asset selected"))?;
    Ok(asset.to_string())
}

fn structured(
    context: &Context<'_>,
    generation: Option<&gen_repo::GenerationRow>,
    asset: Option<&asset_repo::AssetRow>,
) -> Result<Value, CommandError> {
    let guidance: Guidance = generation
        .and_then(|row| serde_json::from_str(&row.guidance_json).ok())
        .unwrap_or_default();
    Ok(match generation {
        Some(row) => json!({
            "prompt": row.prompt,
            "negative": row.negative,
            "params": serde_json::from_str::<Value>(&row.params_json).unwrap_or_else(|_| json!({})),
            "guidance": guidance,
            "presetId": row.preset_id,
            "presetVersion": row.preset_version,
            "presetHash": row.preset_hash,
            "models": serde_json::from_str::<Value>(&row.models_json).unwrap_or_else(|_| json!([])),
            "seed": row.seed,
            "createdAt": row.created_at,
            "asset": asset.map(|asset| json!({
                "id": asset.id,
                "path": asset.source_path,
                "width": asset.width,
                "height": asset.height,
                "fps": asset.fps,
                "durationMs": asset.duration_ms,
            })),
        }),
        None => json!({
            "prompt": context.request.asset_id,
            "asset": asset.map(|asset| json!({
                "id": asset.id,
                "path": asset.source_path,
                "width": asset.width,
                "height": asset.height,
            })),
        }),
    })
}

pub fn expand_template(
    template: &str,
    preset: &str,
    asset: &str,
    date: &str,
    time: &str,
    index: &str,
    seed: &str,
) -> String {
    let template = if template.trim().is_empty() {
        "{preset}_{asset}_{date}"
    } else {
        template
    };
    template
        .replace("{preset}", preset)
        .replace("{target}", preset)
        .replace("{asset}", asset)
        .replace("{date}", date)
        .replace("{time}", time)
        .replace("{index}", index)
        .replace("{seed}", seed)
}

pub fn sanitize_component(value: &str) -> String {
    let mut cleaned = String::new();
    for character in value.chars() {
        if character.is_control() || "<>:\"/\\|?*".contains(character) {
            cleaned.push('_');
        } else {
            cleaned.push(character);
        }
    }
    let mut cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        cleaned = "prompt".into();
    }
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|name| cleaned.eq_ignore_ascii_case(name))
    {
        cleaned.push('_');
    }
    cleaned.chars().take(120).collect()
}

fn unique_path(directory: &Path, base: &str, extension: &str) -> PathBuf {
    let mut candidate = directory.join(format!("{base}.{extension}"));
    let mut index = 1;
    while candidate.exists() {
        candidate = directory.join(format!("{base} ({index}).{extension}"));
        index += 1;
    }
    candidate
}

fn write_text(
    directory: &Path,
    base: &str,
    extension: &str,
    text: &str,
) -> Result<PathBuf, CommandError> {
    let path = unique_path(directory, base, extension);
    let temporary = path.with_extension(format!("{extension}.partial"));
    fs::write(&temporary, text).map_err(|error| CommandError::from_io(&error))?;
    fs::rename(&temporary, &path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        CommandError::from_io(&error)
    })?;
    Ok(path)
}

fn copy_media(
    context: &Context<'_>,
    source: &Path,
    directory: &Path,
    base: &str,
    on_progress: &mut (dyn FnMut(f64) + Send),
) -> Result<PathBuf, CommandError> {
    if !source.is_file() {
        return Err(CommandError::not_found(format!(
            "Source file is missing: {}",
            source.display()
        )));
    }
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("bin");
    let path = unique_path(directory, base, extension);
    let temporary = path.with_extension(format!("{extension}.partial"));
    let result = copy_with_progress(source, &temporary, context.transfer, on_progress);
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, &path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        CommandError::from_io(&error)
    })?;
    Ok(path)
}

fn copy_with_progress(
    source: &Path,
    target: &Path,
    transfer: &Transfer,
    on_progress: &mut (dyn FnMut(f64) + Send),
) -> Result<(), CommandError> {
    let total = fs::metadata(source)
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    transfer.bytes_total.fetch_add(total, Ordering::Relaxed);
    let mut reader = fs::File::open(source).map_err(|error| CommandError::from_io(&error))?;
    let mut writer = fs::File::create(target).map_err(|error| CommandError::from_io(&error))?;
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        if transfer.cancel.load(Ordering::Relaxed) {
            return Err(CommandError::cancelled());
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| CommandError::from_io(&error))?;
        if read == 0 {
            break;
        }
        writer
            .write_all(&buffer[..read])
            .map_err(|error| CommandError::from_io(&error))?;
        transfer
            .bytes_done
            .fetch_add(read as u64, Ordering::Relaxed);
        on_progress(transfer.progress());
    }
    writer
        .flush()
        .map_err(|error| CommandError::from_io(&error))?;
    Ok(())
}

fn now_parts(connection: &Connection) -> Result<(String, String), CommandError> {
    let now = crate::db::now_iso(connection)?;
    let date = now.get(..10).unwrap_or("1970-01-01").to_string();
    let time = now.get(11..19).unwrap_or("00:00:00").replace(':', "");
    Ok((date, time))
}

/// Copies a prepared drag file to a user-chosen destination atomically.
pub fn save_as(source: &Path, output: &Path, cache_dir: &Path) -> Result<u64, CommandError> {
    let drag_root = cache_dir.join(DRAG_DIR);
    if !source.starts_with(&drag_root) || !source.is_file() {
        return Err(CommandError::validation(
            "Save-as source must be a prepared drag file",
        ));
    }
    if let Some(parent) = output.parent()
        && !parent.is_dir()
    {
        return Err(CommandError::validation(format!(
            "Destination folder does not exist: {}",
            parent.display()
        )));
    }
    let extension = output
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("tmp");
    let temporary = output.with_extension(format!("{extension}.partial"));
    let result = fs::copy(source, &temporary);
    match result {
        Ok(bytes) => {
            if output.exists() {
                fs::remove_file(output).map_err(|error| CommandError::from_io(&error))?;
            }
            fs::rename(&temporary, output).map_err(|error| {
                let _ = fs::remove_file(&temporary);
                CommandError::from_io(&error)
            })?;
            Ok(bytes)
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(CommandError::from_io(&error))
        }
    }
}

/// Reveals a prepared file in the OS file manager. Shared by the Tauri command
/// and the native facade so the path check and platform spawn stay identical.
pub fn reveal(path: &Path) -> Result<(), CommandError> {
    if !path.exists() {
        return Err(CommandError::not_found("That file no longer exists"));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(|error| CommandError::from_io(&error))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|error| CommandError::from_io(&error))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{cleanup, expand_template, sanitize_component, save_as};
    use crate::media::test_support::temp_dir;

    #[test]
    fn expands_the_naming_template() {
        assert_eq!(
            expand_template(
                "{preset}_{asset}_{date}",
                "mmh3",
                "beach",
                "2026-11-05",
                "",
                "1",
                ""
            ),
            "mmh3_beach_2026-11-05"
        );
        assert_eq!(
            expand_template(
                "{target}-{asset}-{time}-{seed}",
                "krea2",
                "beach",
                "",
                "120000",
                "",
                "7"
            ),
            "krea2-beach-120000-7"
        );
        assert_eq!(
            expand_template("", "mmh3", "beach", "2026-11-05", "", "", ""),
            "mmh3_beach_2026-11-05"
        );
    }

    #[test]
    fn sanitizes_windows_illegal_characters_and_reserved_names() {
        assert_eq!(
            sanitize_component("a:b?c*d\"e<f>g|h/i\\j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(sanitize_component("  trailing.  "), "trailing");
        assert_eq!(sanitize_component("CON"), "CON_");
        assert_eq!(sanitize_component("   "), "prompt");
    }

    #[test]
    fn keeps_unicode_and_bounds_long_names() {
        assert_eq!(sanitize_component("狐_étoile_🎬"), "狐_étoile_🎬");
        let long = "a".repeat(400);
        assert_eq!(sanitize_component(&long).chars().count(), 120);
        assert_eq!(
            expand_template(
                "{asset}_{date}",
                "mmh3",
                "狐_étoile",
                "2026-11-05",
                "",
                "1",
                ""
            ),
            "狐_étoile_2026-11-05"
        );
    }

    #[test]
    fn save_as_is_atomic_and_rejects_unprepared_sources() {
        let cache = temp_dir("drag-cache");
        let drag = cache.join("drag").join("session");
        fs::create_dir_all(&drag).expect("drag dir");
        let source = drag.join("prompt.txt");
        fs::write(&source, b"hello").expect("source");

        let destination = temp_dir("drag-dest").join("out.txt");
        let bytes = save_as(&source, &destination, &cache).expect("save");
        assert_eq!(bytes, 5);
        assert_eq!(fs::read_to_string(&destination).expect("dest"), "hello");
        assert_eq!(fs::read_to_string(&source).expect("source"), "hello");

        let outside = temp_dir("drag-outside").join("other.txt");
        fs::write(&outside, b"no").expect("outside");
        let error = save_as(&outside, &destination, &cache).expect_err("unprepared");
        assert_eq!(error.code, "validation");
        assert_eq!(fs::read_to_string(&destination).expect("dest"), "hello");
    }

    #[test]
    fn save_as_fails_without_leaving_a_partial_file() {
        let cache = temp_dir("drag-cache-2");
        let drag = cache.join("drag").join("session");
        fs::create_dir_all(&drag).expect("drag dir");
        let source = drag.join("clip.mp4");
        fs::write(&source, b"video").expect("source");

        let destination = temp_dir("drag-missing").join("missing").join("out.mp4");
        let error = save_as(&source, &destination, &cache).expect_err("missing folder");
        assert_eq!(error.code, "validation");
        assert!(!destination.exists());
    }

    #[test]
    fn cleanup_removes_stale_sessions() {
        let cache = temp_dir("drag-cleanup");
        let root = cache.join("drag");
        let stale = root.join("old");
        fs::create_dir_all(&stale).expect("old");
        let fresh = root.join("new");
        fs::create_dir_all(&fresh).expect("new");
        let now = std::time::SystemTime::now();
        let removed = cleanup(&cache, now + std::time::Duration::from_secs(48 * 60 * 60));
        assert_eq!(removed, 2);
        assert!(!stale.exists());
        assert!(!fresh.exists());
    }

    #[test]
    fn collisions_get_a_numeric_suffix_without_overwriting() {
        let directory = temp_dir("drag-collision");
        fs::write(directory.join("prompt.txt"), b"original").expect("first");
        let path = super::unique_path(&directory, "prompt", "txt");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("prompt (1).txt")
        );
        assert!(!path.exists());
        fs::write(&path, b"second").expect("second");
        let third = super::unique_path(&directory, "prompt", "txt");
        assert_eq!(
            third.file_name().and_then(|name| name.to_str()),
            Some("prompt (2).txt")
        );
        assert_eq!(
            fs::read_to_string(directory.join("prompt.txt")).expect("original"),
            "original"
        );
    }
}

#[cfg(test)]
mod payload_tests {
    use std::fs;

    use serde_json::json;

    use crate::commands::drag::{DragPayloadKind, DragPrepareRequest};
    use crate::db::Database;
    use crate::db::repository::assets;
    use crate::db::repository::edits;
    use crate::db::repository::generations;
    use crate::drag_out::{Transfer, prepare};
    use crate::media::test_support::temp_dir;

    fn setup() -> (Database, std::path::PathBuf, std::path::PathBuf) {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        let cache = temp_dir("drag-payload-cache");
        let media = temp_dir("drag-payload-media");
        let asset_path = media.join("beach.png");
        fs::write(&asset_path, b"asset-bytes").expect("asset");
        let edited_path = media.join("edited.png");
        fs::write(&edited_path, b"edited-bytes").expect("edited");
        let mask_path = media.join("mask.png");
        fs::write(&mask_path, b"mask-bytes").expect("mask");

        let connection = database.connection();
        assets::insert(
            connection,
            &assets::NewAsset {
                id: "asset-1".into(),
                hash: "hash-1".into(),
                kind: "image".into(),
                source_path: asset_path.display().to_string(),
                display_name: "beach.png".into(),
                width: Some(64),
                height: Some(64),
                fps: None,
                duration_ms: None,
                codec: Some("png".into()),
                has_audio: false,
                color: None,
                rotation: 0,
                thumbnail_path: None,
                proxy_path: None,
                embedded_metadata: None,
            },
        )
        .expect("asset");
        edits::insert(
            connection,
            &edits::NewEdit {
                id: "edit-1".into(),
                asset_id: "asset-1".into(),
                ops_json: json!({
                    "crop": null, "rotate": 0, "flipH": false, "flipV": false, "fit": "fit",
                    "trim": null, "fps": null, "removeAudio": false,
                    "maskPath": mask_path.display().to_string(), "format": null,
                    "loopVideo": false, "padColor": null
                })
                .to_string(),
                target_json: "{}".into(),
                output_path: edited_path.display().to_string(),
                cache_key: "edit-cache".into(),
                width: Some(64),
                height: Some(64),
                fps: None,
                duration_ms: None,
            },
        )
        .expect("edit");
        generations::insert(
            connection,
            &generations::NewGeneration {
                id: "gen-1".into(),
                parent_id: None,
                asset_id: Some("asset-1".into()),
                edit_id: Some("edit-1".into()),
                description_id: None,
                project_id: None,
                shot_index: None,
                preset_id: "mmh3".into(),
                preset_version: "1.0.0".into(),
                preset_hash: "hash".into(),
                prompt: "a red fox on the beach".into(),
                negative: Some("blurry".into()),
                params_json: json!({ "aspect_ratio": "16:9" }).to_string(),
                guidance_json: json!({ "global": null, "preset": null, "run": "neon" }).to_string(),
                description_json: None,
                seed: Some(7),
                models_json: json!(["gemma"]).to_string(),
                status: "succeeded".into(),
                error: None,
                raw_response: None,
                tokens_in: None,
                tokens_out: None,
                latency_ms: Some(100),
                cache_key: None,
            },
        )
        .expect("generation");

        (database, cache, media)
    }

    fn request(payload: DragPayloadKind, sidecars: bool) -> DragPrepareRequest {
        DragPrepareRequest {
            generation_id: Some("gen-1".into()),
            asset_id: Some("asset-1".into()),
            edit_id: Some("edit-1".into()),
            payload,
            sidecars,
        }
    }

    fn run(
        database: &Database,
        cache: &std::path::Path,
        payload: DragPayloadKind,
        sidecars: bool,
    ) -> Vec<std::path::PathBuf> {
        let transfer = Transfer::default();
        let mut progress = |_value: f64| {};
        prepare(
            database.connection(),
            &request(payload, sidecars),
            "{preset}_{asset}_{date}",
            cache,
            &transfer,
            &mut progress,
        )
        .expect("prepare")
        .files
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect()
    }

    #[test]
    fn plain_and_structured_payloads_contain_the_prompt() {
        let (database, cache, _media) = setup();
        let plain = run(&database, &cache, DragPayloadKind::PlainPrompt, false);
        assert_eq!(plain.len(), 1);
        let text = fs::read_to_string(&plain[0]).expect("plain");
        assert!(text.contains("a red fox on the beach"));
        assert!(text.contains("Negative: blurry"));

        let structured = run(&database, &cache, DragPayloadKind::StructuredJson, false);
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&structured[0]).expect("json"))
                .expect("parse");
        assert_eq!(value["prompt"], "a red fox on the beach");
        assert_eq!(value["negative"], "blurry");
        assert_eq!(value["params"]["aspect_ratio"], "16:9");
        assert_eq!(value["presetId"], "mmh3");
        assert_eq!(value["seed"], 7);
    }

    #[test]
    fn original_and_edited_media_are_copied_without_touching_sources() {
        let (database, cache, media) = setup();
        let original = run(&database, &cache, DragPayloadKind::Original, false);
        assert_eq!(original.len(), 1);
        assert_eq!(fs::read(&original[0]).expect("copy"), b"asset-bytes");
        assert_eq!(
            fs::read(media.join("beach.png")).expect("source"),
            b"asset-bytes"
        );

        let edited = run(&database, &cache, DragPayloadKind::Edited, false);
        assert_eq!(fs::read(&edited[0]).expect("copy"), b"edited-bytes");
        assert!(media.join("edited.png").is_file());
    }

    #[test]
    fn mask_payload_writes_the_masked_png() {
        let (database, cache, media) = setup();
        let mask = run(&database, &cache, DragPayloadKind::Mask, false);
        assert_eq!(mask.len(), 1);
        assert_eq!(fs::read(&mask[0]).expect("mask"), b"mask-bytes");
        assert!(media.join("mask.png").is_file());
    }

    #[test]
    fn bundle_and_sidecars_share_the_base_name() {
        let (database, cache, _media) = setup();
        let bundle = run(&database, &cache, DragPayloadKind::Bundle, true);
        let names: Vec<String> = bundle
            .iter()
            .map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        assert_eq!(names.len(), 3);
        assert!(names.iter().any(|name| name.ends_with(".txt")));
        assert!(names.iter().any(|name| name.ends_with(".json")));
        assert!(names.iter().any(|name| name.ends_with(".png")));
        let stem = bundle[0]
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("stem");
        assert!(names.iter().all(|name| name.starts_with(stem)));

        let media_only = run(&database, &cache, DragPayloadKind::Original, false);
        assert_eq!(media_only.len(), 1);
        let with_sidecars = run(&database, &cache, DragPayloadKind::Original, true);
        assert_eq!(with_sidecars.len(), 3);
    }

    #[test]
    fn missing_sources_fail_without_partial_files() {
        let (database, cache, media) = setup();
        fs::remove_file(media.join("beach.png")).expect("remove source");
        let transfer = Transfer::default();
        let mut progress = |_value: f64| {};
        let error = prepare(
            database.connection(),
            &request(DragPayloadKind::Original, false),
            "{preset}_{asset}",
            &cache,
            &transfer,
            &mut progress,
        )
        .expect_err("missing source");
        assert_eq!(error.category, crate::commands::ErrorCategory::NotFound);
        let sessions = fs::read_dir(cache.join("drag")).expect("sessions");
        for session in sessions.flatten() {
            let entries = fs::read_dir(session.path()).expect("entries");
            assert!(
                entries
                    .flatten()
                    .all(|entry| { !entry.path().to_string_lossy().ends_with(".partial") })
            );
        }
    }
}

#[cfg(test)]
mod cancel_tests {
    use std::fs;

    use crate::commands::drag::{DragPayloadKind, DragPrepareRequest};
    use crate::db::Database;
    use crate::db::repository::assets;
    use crate::drag_out::{Transfer, prepare};
    use crate::media::test_support::temp_dir;

    #[test]
    fn cancelled_transfers_leave_no_partial_output() {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        let cache = temp_dir("drag-cancel-cache");
        let media = temp_dir("drag-cancel-media");
        let source = media.join("clip.mp4");
        fs::write(&source, vec![7u8; 4 * 1024 * 1024]).expect("source");
        assets::insert(
            database.connection(),
            &assets::NewAsset {
                id: "asset-1".into(),
                hash: "hash-1".into(),
                kind: "video".into(),
                source_path: source.display().to_string(),
                display_name: "clip.mp4".into(),
                width: Some(64),
                height: Some(64),
                fps: Some(24.0),
                duration_ms: Some(1000),
                codec: Some("h264".into()),
                has_audio: false,
                color: None,
                rotation: 0,
                thumbnail_path: None,
                proxy_path: None,
                embedded_metadata: None,
            },
        )
        .expect("asset");

        let transfer = Transfer::default();
        transfer
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let mut progress = |_value: f64| {};
        let error = prepare(
            database.connection(),
            &DragPrepareRequest {
                generation_id: None,
                asset_id: Some("asset-1".into()),
                edit_id: None,
                payload: DragPayloadKind::Original,
                sidecars: false,
            },
            "{asset}",
            &cache,
            &transfer,
            &mut progress,
        )
        .expect_err("cancelled");
        assert_eq!(error.category, crate::commands::ErrorCategory::Cancelled);

        let sessions = fs::read_dir(cache.join("drag")).expect("sessions");
        for session in sessions.flatten() {
            for entry in fs::read_dir(session.path()).expect("entries").flatten() {
                assert!(!entry.path().to_string_lossy().ends_with(".partial"));
                assert!(!entry.path().to_string_lossy().ends_with(".mp4"));
            }
        }
    }
}
