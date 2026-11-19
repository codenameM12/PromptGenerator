use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

use crate::commands::CommandError;
use crate::commands::presets::PresetKind;
use crate::db::repository::presets::{self, NewPreset};
use crate::preset::{self, PresetSpec};

#[derive(Debug, Deserialize)]
struct BuiltinPresetFile {
    id: String,
    document: Value,
}

struct BuiltinSource {
    file: &'static str,
    json: &'static str,
}

const BUILTINS: &[BuiltinSource] = &[
    BuiltinSource {
        file: "mmh3.json",
        json: include_str!("../presets/mmh3.json"),
    },
    BuiltinSource {
        file: "krea2.json",
        json: include_str!("../presets/krea2.json"),
    },
    BuiltinSource {
        file: "krea2-edit.json",
        json: include_str!("../presets/krea2-edit.json"),
    },
    BuiltinSource {
        file: "qwen2-edit.json",
        json: include_str!("../presets/qwen2-edit.json"),
    },
    BuiltinSource {
        file: "generic-image.json",
        json: include_str!("../presets/generic-image.json"),
    },
    BuiltinSource {
        file: "generic-video.json",
        json: include_str!("../presets/generic-video.json"),
    },
];

fn kind_str(kind: PresetKind) -> &'static str {
    match kind {
        PresetKind::Image => "image",
        PresetKind::Video => "video",
        PresetKind::Edit => "edit",
    }
}

/// Inserts or refreshes built-in presets. Returns how many records changed.
/// Unchanged built-ins are skipped, so repeated startups are idempotent.
pub fn seed(connection: &Connection) -> Result<usize, CommandError> {
    let mut seeded = 0;

    for source in BUILTINS {
        let preset: BuiltinPresetFile = serde_json::from_str(source.json).map_err(|error| {
            CommandError::internal(format!("invalid built-in preset {}: {error}", source.file))
        })?;
        let spec: PresetSpec = preset::parse(&preset.document).map_err(|error| {
            CommandError::internal(format!(
                "invalid built-in document {}: {error}",
                source.file
            ))
        })?;
        preset::validate(&spec).map_err(|error| {
            CommandError::internal(format!("invalid built-in preset {}: {error}", source.file))
        })?;
        if spec.id != preset.id {
            return Err(CommandError::internal(format!(
                "built-in preset id mismatch: wrapper '{}', document '{}'",
                preset.id, spec.id
            )));
        }

        let document_json = serde_json::to_string(&spec).map_err(|error| {
            CommandError::internal(format!(
                "serialize built-in preset {}: {error}",
                source.file
            ))
        })?;
        let content_hash = preset::content_hash(&spec)?;

        if let Some(existing) = presets::get(connection, &spec.id)?
            && existing.owner == "builtin"
            && existing.version == spec.version
            && existing.content_hash == content_hash
            && existing.deleted_at.is_none()
        {
            continue;
        }

        let id = spec.id.clone();
        presets::upsert(
            connection,
            &NewPreset {
                id: id.clone(),
                owner: "builtin".into(),
                name: spec.name,
                kind: kind_str(spec.kind).into(),
                version: spec.version,
                description: spec.description,
                document_json,
                content_hash,
            },
        )?;
        connection.execute("UPDATE presets SET deleted_at = NULL WHERE id = ?1", [&id])?;
        seeded += 1;
    }

    Ok(seeded)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::BUILTINS;
    use crate::commands::Guidance;
    use crate::db::Database;
    use crate::db::repository::presets;
    use crate::preset::{self, CompileInput};

    fn database() -> Database {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        database
    }

    #[test]
    fn seeds_all_builtins_once() {
        let database = database();
        let connection = database.connection();

        assert_eq!(super::seed(connection).expect("seed"), 6);
        assert_eq!(super::seed(connection).expect("seed again"), 0);
        assert_eq!(presets::list(connection, false).expect("list").len(), 6);
    }

    fn builtin_spec(id: &str) -> crate::preset::PresetSpec {
        let source = BUILTINS
            .iter()
            .find(|source| source.file == format!("{id}.json"))
            .unwrap_or_else(|| panic!("{id} source"));
        let parsed: super::BuiltinPresetFile =
            serde_json::from_str(source.json).expect("parse builtin file");
        preset::parse(&parsed.document).expect("parse document")
    }

    #[test]
    fn builtin_limits_match_the_documented_targets() {
        let mmh3 = builtin_spec("mmh3");
        assert_eq!(mmh3.limits.prompt_chars, 7000);
        assert!(!mmh3.supports_negative);
        let duration = mmh3.params.get("duration_s").expect("duration param");
        assert_eq!(duration.kind, "integer");
        assert_eq!(duration.min, Some(4.0));
        assert_eq!(duration.max, Some(15.0));
        assert!(!mmh3.params.contains_key("fps"));

        let qwen = builtin_spec("qwen2-edit");
        assert_eq!(qwen.limits.prompt_chars, 4000);
        assert_eq!(qwen.reference_slots, 3);

        let krea = builtin_spec("krea2");
        assert_eq!(krea.reference_slots, 4);
        assert!(krea.params.contains_key("creativity"));
    }

    #[test]
    fn stores_a_blake3_content_hash() {
        let database = database();
        let connection = database.connection();
        super::seed(connection).expect("seed");

        let source = BUILTINS
            .iter()
            .find(|source| source.file == "mmh3.json")
            .expect("mmh3 source");
        let parsed: super::BuiltinPresetFile =
            serde_json::from_str(source.json).expect("parse builtin file");
        let spec = preset::parse(&parsed.document).expect("parse document");
        let expected = preset::content_hash(&spec).expect("hash");

        let row = presets::get(connection, "mmh3")
            .expect("get")
            .expect("present");
        assert_eq!(row.content_hash, expected);
        assert_eq!(row.owner, "builtin");
        assert_eq!(row.version, spec.version);
    }

    #[test]
    fn restores_a_soft_deleted_builtin() {
        let database = database();
        let connection = database.connection();
        super::seed(connection).expect("seed");
        assert!(presets::delete(connection, "mmh3").expect("delete"));

        assert_eq!(super::seed(connection).expect("reseed"), 1);
        let row = presets::get(connection, "mmh3")
            .expect("get")
            .expect("present");
        assert!(row.deleted_at.is_none());
    }

    #[test]
    fn every_source_parses_and_has_a_unique_id() {
        let mut ids = std::collections::HashSet::new();
        for source in BUILTINS {
            let parsed: super::BuiltinPresetFile =
                serde_json::from_str(source.json).expect("parse builtin");
            let spec = preset::parse(&parsed.document).expect("parse document");
            preset::validate(&spec).expect("valid preset");
            assert!(ids.insert(spec.id.clone()), "duplicate id {}", spec.id);
        }
        assert_eq!(ids.len(), 6);
    }

    #[test]
    fn builtins_compile_to_expected_shapes() {
        let database = database();
        let connection = database.connection();
        super::seed(connection).expect("seed");

        let cases = [
            (
                "mmh3",
                json!({
                    "subject": "a red fox",
                    "action": "runs through snow",
                    "camera": "low tracking shot",
                    "lighting": "blue hour",
                    "style": "cinematic"
                }),
            ),
            (
                "krea2",
                json!({
                    "subject": "a red fox",
                    "composition": "centered portrait",
                    "lighting": "soft studio light",
                    "style": "editorial photograph"
                }),
            ),
            (
                "krea2-edit",
                json!({ "change": "make the jacket red", "preserve": "face and background" }),
            ),
            (
                "qwen2-edit",
                json!({ "change": "add falling snow", "preserve": "the subject's pose" }),
            ),
            (
                "generic-image",
                json!({
                    "subject": "a lighthouse",
                    "composition": "wide shot",
                    "lighting": "sunset",
                    "style": "painterly"
                }),
            ),
            (
                "generic-video",
                json!({
                    "subject": "a lighthouse",
                    "action": "waves crash",
                    "camera": "slow push in",
                    "lighting": "sunset",
                    "style": "cinematic"
                }),
            ),
        ];

        for (id, description) in cases {
            let row = presets::get(connection, id).expect("get").expect("present");
            let document: serde_json::Value =
                serde_json::from_str(&row.document_json).expect("document json");
            let spec = preset::parse(&document).expect("spec");
            let compiled = preset::compile(
                &spec,
                &CompileInput {
                    guidance: Guidance::default(),
                    description: Some(description),
                    brief: None,
                    locked: Vec::new(),
                    params: json!({}),
                    existing_prompt: None,
                },
            );

            assert!(!compiled.prompt.is_empty(), "{id} produced no prompt");
            if id.ends_with("edit") {
                assert!(
                    compiled.prompt.starts_with("Change:"),
                    "{id} should be an edit instruction: {}",
                    compiled.prompt
                );
            }
            assert!(!compiled.params.as_object().unwrap().is_empty() || id.ends_with("edit"));
        }
    }
}
