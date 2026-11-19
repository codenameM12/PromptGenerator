//! Rust port of `src/features/presets/presetTemplate.ts`.
//!
//! Only the text-facing projection of a preset document is reproduced here —
//! the same fields the guide editor compiles. Advanced fields stay owned by
//! `crate::preset` (the Rust compilation source of truth); a test below proves
//! the compiled projection parses and validates through that path. Behaviour is
//! preserved, including the quirks in `slugify`, `nextVersion`, and the
//! "created / changed" version bump.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetKind {
    Image,
    Video,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetMode {
    Compose,
    Edit,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PresetGuide {
    name: String,
    kind: PresetKind,
    description: String,
    guide: String,
    prompt_chars: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BaseView {
    id: String,
    name: String,
    kind: PresetKind,
    version: String,
    description: String,
    system_prompt: String,
    params: Value,
    prompt_chars: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocView {
    id: String,
    name: String,
    kind: PresetKind,
    version: String,
    description: String,
    system_prompt: String,
    guidance: Option<String>,
    mode: PresetMode,
    sections: Vec<String>,
    supports_negative: bool,
    reference_slots: i64,
    resolutions: Vec<(i64, i64)>,
    checklist: Vec<String>,
    params: Value,
    prompt_chars: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GuideView {
    name: String,
    kind: PresetKind,
    description: String,
    guide: String,
    prompt_chars: i64,
}

/// Lowercase kebab id, capped so a long name still produces a usable id.
pub fn slugify(value: &str) -> String {
    let lower = value.to_lowercase();
    let mut slug = String::new();
    for character in lower.chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let slug: String = slug.chars().take(48).collect();
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "custom-target".into()
    } else {
        slug.into()
    }
}

fn js_number(part: &str) -> Option<f64> {
    let trimmed = part.trim();
    if trimmed.is_empty() {
        return Some(0.0); // Number("") === 0
    }
    trimmed.parse::<f64>().ok() // Err represents NaN
}

fn js_number_string(number: f64) -> String {
    if number == 0.0 || number.fract() == 0.0 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

/// Bumps the patch component, or leaves the version alone when no change was
/// made. Missing components default to 1/0/0; non-numeric components are NaN
/// and are mapped to 0 (the patch increments to 1 when it is not finite).
pub fn next_version(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    // A destructuring default applies only to a *missing* element; a present
    // but non-numeric element is NaN and is mapped to 0 at the end.
    let major = match parts.first() {
        Some(part) => js_number(part),
        None => Some(1.0),
    };
    let minor = match parts.get(1) {
        Some(part) => js_number(part),
        None => Some(0.0),
    };
    let patch = match parts.get(2) {
        Some(part) => js_number(part),
        None => Some(0.0),
    };
    let bumped = patch.map(|value| value + 1.0).or(Some(1.0));
    [major, minor, bumped]
        .iter()
        .map(|value| match value {
            Some(number) if number.is_finite() => js_number_string(*number),
            _ => "0".into(),
        })
        .collect::<Vec<_>>()
        .join(".")
}

struct KindDefaults {
    mode: PresetMode,
    sections: Vec<&'static str>,
    checklist: Vec<&'static str>,
    resolutions: Vec<(i64, i64)>,
    supports_negative: bool,
    reference_slots: i64,
}

fn kind_defaults(kind: PresetKind) -> KindDefaults {
    match kind {
        PresetKind::Image => KindDefaults {
            mode: PresetMode::Compose,
            sections: vec!["subject", "lighting", "style"],
            checklist: vec!["subject"],
            resolutions: vec![(1024, 1024)],
            supports_negative: true,
            reference_slots: 0,
        },
        PresetKind::Video => KindDefaults {
            mode: PresetMode::Compose,
            sections: vec!["subject", "action", "camera", "lighting", "style"],
            checklist: vec!["subject", "camera"],
            resolutions: vec![(1280, 720)],
            supports_negative: false,
            reference_slots: 0,
        },
        PresetKind::Edit => KindDefaults {
            mode: PresetMode::Edit,
            sections: vec!["change", "preserve", "style"],
            checklist: vec!["change", "preserve"],
            resolutions: vec![(1024, 1024)],
            supports_negative: true,
            reference_slots: 1,
        },
    }
}

pub fn validate_guide(guide: &PresetGuide) -> Option<String> {
    if guide.name.trim().is_empty() {
        return Some("Give the preset a name".into());
    }
    if guide.guide.trim().is_empty() {
        return Some("Write the prompt guide text".into());
    }
    if guide.prompt_chars.is_some_and(|limit| limit < 0) {
        return Some("The character limit cannot be negative".into());
    }
    None
}

pub fn document_from_guide(guide: &PresetGuide, base: &BaseView) -> DocView {
    let created = base.id.trim().is_empty();
    let kind_changed = guide.kind != base.kind;
    let defaults = kind_defaults(guide.kind);
    let name = guide.name.trim().to_string();
    let system_prompt = guide.guide.trim().to_string();
    let description = guide.description.trim().to_string();
    let prompt_chars = match guide.prompt_chars {
        Some(limit) if limit > 0 => limit,
        _ => base.prompt_chars,
    };
    let changed = created
        || name != base.name
        || guide.kind != base.kind
        || description != base.description
        || system_prompt != base.system_prompt
        || prompt_chars != base.prompt_chars;

    DocView {
        id: if created {
            slugify(&name)
        } else {
            base.id.clone()
        },
        name,
        kind: guide.kind,
        version: if created {
            "1.0.0".into()
        } else if changed {
            next_version(&base.version)
        } else {
            base.version.clone()
        },
        description,
        system_prompt,
        guidance: None,
        mode: defaults.mode,
        sections: defaults.sections.into_iter().map(str::to_string).collect(),
        supports_negative: defaults.supports_negative,
        reference_slots: defaults.reference_slots,
        resolutions: defaults.resolutions,
        checklist: defaults.checklist.into_iter().map(str::to_string).collect(),
        params: if kind_changed {
            Value::Object(Map::new())
        } else {
            base.params.clone()
        },
        prompt_chars,
    }
}

pub fn guide_from_document(document: &DocView) -> GuideView {
    let mut parts = Vec::new();
    for part in [
        document.system_prompt.trim(),
        document.guidance.as_deref().unwrap_or("").trim(),
    ] {
        if !part.is_empty() {
            parts.push(part);
        }
    }
    GuideView {
        name: document.name.clone(),
        kind: document.kind,
        description: document.description.clone(),
        guide: parts.join("\n\n"),
        prompt_chars: document.prompt_chars,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn slugify_keeps_the_source_quirks() {
        assert_eq!(slugify("Krea 2  Edit!"), "krea-2-edit");
        assert_eq!(slugify("   "), "custom-target");
        assert_eq!(slugify("Café Crème"), "caf-cr-me");
        assert!(slugify(&"x".repeat(80)).len() <= 48);
    }

    #[test]
    fn next_version_only_bumps_the_patch() {
        assert_eq!(next_version("1.0.0"), "1.0.1");
        assert_eq!(next_version("2.3.9"), "2.3.10");
        assert_eq!(next_version("1.x.3"), "1.0.4");
    }

    #[derive(Debug, Deserialize)]
    struct SlugCase {
        value: String,
        output: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct VersionCase {
        version: String,
        output: String,
    }

    #[derive(Debug, Deserialize)]
    struct ValidateCase {
        guide: PresetGuide,
        output: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct DocumentCase {
        label: String,
        guide: PresetGuide,
        base: BaseView,
        output: DocView,
    }

    #[derive(Debug, Deserialize)]
    struct GuideCase {
        document: DocView,
        output: GuideView,
    }

    #[derive(Debug, Deserialize)]
    struct Corpus {
        slugify: Vec<SlugCase>,
        #[serde(rename = "nextVersion")]
        next_version: Vec<VersionCase>,
        #[serde(rename = "validateGuide")]
        validate_guide: Vec<ValidateCase>,
        documents: Vec<DocumentCase>,
        guides: Vec<GuideCase>,
    }

    /// The compiled projection becomes a full `preset::PresetSpec` with the
    /// advanced defaults the text editor never exposes.
    fn to_preset_spec(document: &DocView) -> serde_json::Value {
        let mut params = serde_json::Map::new();
        if let Value::Object(map) = &document.params {
            params = map.clone();
        }
        json!({
            "schema": 1,
            "id": document.id,
            "name": document.name,
            "kind": document.kind,
            "version": document.version,
            "description": document.description,
            "mode": document.mode,
            "systemPrompt": document.system_prompt,
            "guidance": document.guidance,
            "sections": document.sections,
            "outputSchema": null,
            "params": Value::Object(params),
            "limits": {
                "promptChars": document.prompt_chars,
                "negativeChars": 400,
                "maxInputTokens": 6000,
                "maxOutputTokens": 700,
            },
            "resolutions": document.resolutions,
            "dimensionMultiple": 16,
            "supportsNegative": document.supports_negative,
            "checklist": document.checklist,
            "examples": [],
            "referenceSlots": document.reference_slots,
            "instructionStyle": false,
        })
    }

    #[test]
    fn matches_the_recorded_typescript_behaviour() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../port/corpus/preset_template.json"))
                .expect("parse preset_template corpus");

        let mut compared = 0usize;
        let mut differences: Vec<String> = Vec::new();

        for case in &corpus.slugify {
            compared += 1;
            let actual = slugify(&case.value);
            if actual != case.output {
                differences.push(format!(
                    "slugify {:?}: expected {:?}, got {:?}",
                    case.value, case.output, actual
                ));
            }
        }
        for case in &corpus.next_version {
            compared += 1;
            let actual = next_version(&case.version);
            if actual != case.output {
                differences.push(format!(
                    "nextVersion {:?}: expected {:?}, got {:?}",
                    case.version, case.output, actual
                ));
            }
        }
        for case in &corpus.validate_guide {
            compared += 1;
            let actual = validate_guide(&case.guide);
            if actual != case.output {
                differences.push(format!(
                    "validateGuide {:?}: expected {:?}, got {:?}",
                    case.guide, case.output, actual
                ));
            }
        }
        for case in &corpus.documents {
            compared += 1;
            let actual = document_from_guide(&case.guide, &case.base);
            if actual != case.output {
                differences.push(format!(
                    "document {}: expected {:?}, got {:?}",
                    case.label, case.output, actual
                ));
            }
        }
        for (index, case) in corpus.guides.iter().enumerate() {
            compared += 1;
            let actual = guide_from_document(&case.document);
            if actual != case.output {
                differences.push(format!(
                    "guide {index}: expected {:?}, got {:?}",
                    case.output, actual
                ));
            }
        }

        assert!(
            differences.is_empty(),
            "presetTemplate: {compared} inputs compared, {} differences\n{differences:#?}",
            differences.len()
        );
        println!(
            "presetTemplate: {compared} inputs compared, {} differences",
            differences.len()
        );
    }

    #[test]
    fn compiled_projection_validates_through_preset_rust_compilation() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../port/corpus/preset_template.json"))
                .expect("parse preset_template corpus");
        for case in &corpus.documents {
            let value = to_preset_spec(&case.output);
            let spec = crate::preset::parse(&value)
                .unwrap_or_else(|error| panic!("{}: parse failed: {error:?}", case.label));
            crate::preset::validate(&spec)
                .unwrap_or_else(|error| panic!("{}: validate failed: {error:?}", case.label));
        }
        println!(
            "presetTemplate → preset.rs: {} compiled documents validated",
            corpus.documents.len()
        );
    }
}
