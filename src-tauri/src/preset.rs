use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::commands::presets::{CompiledPrompt, PresetKind};
use crate::commands::{CommandError, Guidance, PromptWarning, Severity};
use crate::runtime::budget::estimate_tokens;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PresetMode {
    #[default]
    Compose,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub prompt_chars: usize,
    pub negative_chars: usize,
    pub max_input_tokens: usize,
    pub max_output_tokens: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            prompt_chars: 2000,
            negative_chars: 500,
            max_input_tokens: 6000,
            max_output_tokens: 900,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamSpec {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub values: Vec<Value>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub label: Option<String>,
}

impl ParamSpec {
    pub fn validate(&self, value: &Value) -> Result<Value, String> {
        match self.kind.as_str() {
            "enum" => {
                if self.values.contains(value) {
                    Ok(value.clone())
                } else {
                    Err(format!("value {value} is not one of {:?}", self.values))
                }
            }
            "number" | "integer" => {
                let number = value
                    .as_f64()
                    .ok_or_else(|| format!("value {value} is not a number"))?;
                if self.kind == "integer" && number.fract() != 0.0 {
                    return Err(format!("value {value} is not an integer"));
                }
                if let Some(min) = self.min
                    && number < min
                {
                    return Err(format!("value {value} is below {min}"));
                }
                if let Some(max) = self.max
                    && number > max
                {
                    return Err(format!("value {value} is above {max}"));
                }
                Ok(value.clone())
            }
            "boolean" => {
                if value.is_boolean() {
                    Ok(value.clone())
                } else {
                    Err(format!("value {value} is not a boolean"))
                }
            }
            _ => Ok(value.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    pub input: String,
    pub output: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetSpec {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub kind: PresetKind,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub mode: PresetMode,
    pub system_prompt: String,
    #[serde(default)]
    pub guidance: Option<String>,
    #[serde(default)]
    pub sections: Vec<String>,
    #[serde(default)]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub params: BTreeMap<String, ParamSpec>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub resolutions: Vec<[u32; 2]>,
    #[serde(default)]
    pub dimension_multiple: u32,
    #[serde(default)]
    pub supports_negative: bool,
    #[serde(default)]
    pub checklist: Vec<String>,
    #[serde(default)]
    pub examples: Vec<Example>,
    #[serde(default)]
    pub reference_slots: u8,
    #[serde(default)]
    pub instruction_style: bool,
    /// Labelled inputs that replace the free brief (see docs/spec-v2.md).
    #[serde(default)]
    pub brief_fields: Vec<BriefField>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefField {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub placeholder: String,
}

pub fn parse(value: &Value) -> Result<PresetSpec, CommandError> {
    serde_json::from_value(value.clone())
        .map_err(|error| CommandError::validation(format!("Invalid preset document: {error}")))
}

pub fn validate(spec: &PresetSpec) -> Result<(), CommandError> {
    let mut issues = Vec::new();

    if spec.schema != SCHEMA_VERSION {
        issues.push(format!(
            "Unsupported preset schema {} (expected {SCHEMA_VERSION})",
            spec.schema
        ));
    }
    if spec.id.is_empty()
        || !spec.id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        issues.push("Preset id must be kebab-case (a-z, 0-9, -)".into());
    }
    if spec.name.trim().is_empty() {
        issues.push("Preset name is required".into());
    }
    if !valid_version(&spec.version) {
        issues.push("Preset version must look like 1.0.0".into());
    }
    if spec.system_prompt.trim().is_empty() {
        issues.push("Preset system prompt is required".into());
    }
    if spec.limits.prompt_chars == 0 || spec.limits.max_output_tokens == 0 {
        issues.push("Preset limits must be positive".into());
    }
    if !matches!(spec.dimension_multiple, 0 | 1 | 2 | 4 | 8 | 16 | 32 | 64) {
        issues.push("Dimension multiple must be 1, 2, 4, 8, 16, 32, or 64".into());
    }
    if spec.kind == PresetKind::Edit && spec.mode != PresetMode::Edit {
        issues.push("Edit presets must use edit mode".into());
    }
    if spec.mode == PresetMode::Edit && spec.reference_slots == 0 {
        issues.push("Edit presets need at least one reference slot".into());
    }
    if spec.reference_slots > 4 {
        issues.push("At most four reference slots are supported".into());
    }
    if spec.sections.is_empty() {
        issues.push("At least one output section is required".into());
    }
    for (name, param) in &spec.params {
        if let Some(default) = &param.default
            && let Err(error) = param.validate(default)
        {
            issues.push(format!(
                "Parameter '{name}' has an invalid default: {error}"
            ));
        }
    }
    for resolution in &spec.resolutions {
        if resolution[0] == 0 || resolution[1] == 0 {
            issues.push("Resolutions must be positive".into());
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(CommandError::validation(issues.join("; ")))
    }
}

fn valid_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() >= 2 && parts.iter().all(|part| part.parse::<u32>().is_ok())
}

pub fn content_hash(spec: &PresetSpec) -> Result<String, CommandError> {
    let document = serde_json::to_string(spec)
        .map_err(|error| CommandError::internal(format!("serialize preset: {error}")))?;
    Ok(blake3::hash(document.as_bytes()).to_hex().to_string())
}

pub struct CompileInput {
    pub guidance: Guidance,
    pub description: Option<Value>,
    pub brief: Option<String>,
    pub locked: Vec<String>,
    pub params: Value,
    pub existing_prompt: Option<String>,
}

pub fn compile(spec: &PresetSpec, input: &CompileInput) -> CompiledPrompt {
    let mut warnings = Vec::new();
    let description = input
        .description
        .as_ref()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut parts = Vec::new();

    if spec.mode == PresetMode::Edit {
        let change = description
            .get("change")
            .and_then(value_to_text)
            .or_else(|| input.brief.clone())
            .or_else(|| input.existing_prompt.clone())
            .unwrap_or_default();
        let preserve = description
            .get("preserve")
            .and_then(value_to_text)
            .unwrap_or_default();
        if !change.is_empty() {
            parts.push(format!("Change: {change}"));
        }
        if !preserve.is_empty() {
            parts.push(format!("Preserve: {preserve}"));
        }
    } else {
        for section in &spec.sections {
            if let Some(value) = description.get(section)
                && let Some(text) = value_to_text(value)
                && !text.trim().is_empty()
            {
                parts.push(text);
            }
        }
        if parts.is_empty()
            && let Some(text) = input
                .brief
                .clone()
                .or_else(|| input.existing_prompt.clone())
            && !text.trim().is_empty()
        {
            parts.push(text);
        }
    }

    if let Some(guidance) = effective_guidance(spec, &input.guidance)
        && !parts
            .iter()
            .any(|part| part.eq_ignore_ascii_case(&guidance))
    {
        parts.push(guidance);
    }

    for locked in &input.locked {
        let phrase = locked.trim();
        if !phrase.is_empty()
            && !parts.iter().any(|part| {
                part.to_ascii_lowercase()
                    .contains(&phrase.to_ascii_lowercase())
            })
        {
            parts.push(phrase.to_string());
        }
    }

    let mut prompt = parts.join(". ");
    let (truncated, prompt_was_truncated) = truncate(&mut prompt, spec.limits.prompt_chars);
    if prompt_was_truncated {
        warnings.push(warning(
            "prompt_truncated",
            format!(
                "Prompt was truncated to {} characters",
                spec.limits.prompt_chars
            ),
        ));
    }

    let negative = description.get("negative").and_then(value_to_text);
    let negative = match (spec.supports_negative, negative) {
        (true, Some(value)) => {
            let mut value = value;
            let (_, was_truncated) = truncate(&mut value, spec.limits.negative_chars);
            if was_truncated {
                warnings.push(warning(
                    "negative_truncated",
                    "Negative prompt was truncated to the preset limit".into(),
                ));
            }
            Some(value)
        }
        (false, Some(_)) => {
            warnings.push(warning(
                "negative_unsupported",
                "This target does not support negative prompts; it was omitted".into(),
            ));
            None
        }
        (_, None) => None,
    };

    let mut params = Map::new();
    for (name, param) in &spec.params {
        params.insert(name.clone(), param.default.clone().unwrap_or(Value::Null));
    }
    if let Value::Object(requested) = &input.params {
        for (name, value) in requested {
            if let Some(param) = spec.params.get(name) {
                match param.validate(value) {
                    Ok(valid) => {
                        params.insert(name.clone(), valid);
                    }
                    Err(error) => {
                        warnings.push(warning(
                            "invalid_parameter",
                            format!("Parameter '{name}' fell back to its default: {error}"),
                        ));
                    }
                }
            } else {
                params.insert(name.clone(), value.clone());
                warnings.push(warning(
                    "unknown_parameter",
                    format!("Parameter '{name}' is not declared by this preset"),
                ));
            }
        }
    }

    let _ = truncated;
    CompiledPrompt {
        char_count: prompt.chars().count(),
        token_estimate: estimate_tokens(&prompt),
        prompt,
        negative,
        params: Value::Object(params),
        warnings,
    }
}

pub fn preflight(spec: &PresetSpec, compiled: &CompiledPrompt) -> Vec<PromptWarning> {
    let mut warnings = Vec::new();

    if compiled.prompt.trim().is_empty() {
        warnings.push(warning(
            "empty_prompt",
            "The compiled prompt is empty".into(),
        ));
    }

    if compiled.char_count > spec.limits.prompt_chars {
        warnings.push(warning(
            "prompt_over_limit",
            format!(
                "Prompt is {} characters, limit is {}",
                compiled.char_count, spec.limits.prompt_chars
            ),
        ));
    }

    let lower = compiled.prompt.to_ascii_lowercase();
    for item in &spec.checklist {
        if !lower.contains(&item.to_ascii_lowercase()) {
            warnings.push(warning(
                "checklist_missing",
                format!("Prompt may be missing required element: {item}"),
            ));
        }
    }

    warnings
}

fn effective_guidance(spec: &PresetSpec, guidance: &Guidance) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for value in [
        guidance.global.as_ref(),
        spec.guidance.as_ref(),
        guidance.preset.as_ref(),
        guidance.run.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        let value = value.trim();
        if !value.is_empty() && !parts.iter().any(|part| part.eq_ignore_ascii_case(value)) {
            parts.push(value.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(". "))
    }
}

fn value_to_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let values: Vec<String> = items.iter().filter_map(value_to_text).collect();
            if values.is_empty() {
                None
            } else {
                Some(values.join(", "))
            }
        }
        Value::Null => None,
        Value::Bool(_) | Value::Number(_) | Value::Object(_) => Some(value.to_string()),
    }
}

fn truncate(text: &mut String, limit: usize) -> (bool, bool) {
    if text.chars().count() <= limit {
        return (false, false);
    }
    let truncated: String = text.chars().take(limit).collect();
    let cut = truncated
        .rfind(char::is_whitespace)
        .map(|index| truncated[..index].to_string())
        .unwrap_or(truncated);
    *text = cut;
    (true, true)
}

fn warning(code: &str, message: String) -> PromptWarning {
    PromptWarning {
        code: code.into(),
        message,
        severity: Severity::Warning,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{
        CompileInput, Limits, ParamSpec, PresetMode, PresetSpec, compile, content_hash, parse,
        preflight, validate,
    };
    use crate::commands::Guidance;
    use crate::commands::presets::PresetKind;

    fn spec() -> PresetSpec {
        let mut params = BTreeMap::new();
        params.insert(
            "duration_s".into(),
            ParamSpec {
                kind: "enum".into(),
                values: vec![json!(5), json!(10)],
                default: Some(json!(5)),
                min: None,
                max: None,
                label: None,
            },
        );
        params.insert(
            "aspect_ratio".into(),
            ParamSpec {
                kind: "enum".into(),
                values: vec![json!("16:9"), json!("9:16")],
                default: Some(json!("16:9")),
                min: None,
                max: None,
                label: None,
            },
        );

        PresetSpec {
            schema: 1,
            id: "test-video".into(),
            name: "Test video".into(),
            kind: PresetKind::Video,
            version: "1.0.0".into(),
            description: "test".into(),
            mode: PresetMode::Compose,
            system_prompt: "Write a video prompt".into(),
            guidance: Some("cinematic".into()),
            sections: vec!["subject".into(), "camera".into()],
            output_schema: Some(json!({ "type": "object" })),
            params,
            limits: Limits {
                prompt_chars: 200,
                negative_chars: 80,
                max_input_tokens: 3000,
                max_output_tokens: 400,
            },
            resolutions: vec![[1280, 720]],
            dimension_multiple: 16,
            supports_negative: true,
            checklist: vec!["subject".into(), "camera".into()],
            examples: vec![],
            reference_slots: 0,
            instruction_style: false,
            brief_fields: Vec::new(),
        }
    }

    #[test]
    fn validates_a_complete_preset() {
        validate(&spec()).expect("valid");
    }

    #[test]
    fn rejects_missing_system_prompt_and_bad_version() {
        let mut value = spec();
        value.system_prompt = " ".into();
        value.version = "one".into();
        let error = validate(&value).expect_err("invalid");
        assert!(error.message.contains("system prompt"));
        assert!(error.message.contains("version"));
    }

    #[test]
    fn compiles_sections_guidance_and_params() {
        let compiled = compile(
            &spec(),
            &CompileInput {
                guidance: Guidance {
                    global: Some("natural light".into()),
                    preset: None,
                    run: Some("snow".into()),
                },
                description: Some(json!({
                    "subject": "a red fox",
                    "camera": "low angle",
                    "negative": "blurry"
                })),
                brief: None,
                locked: vec!["red fox".into()],
                params: json!({ "duration_s": 10, "aspect_ratio": "9:16" }),
                existing_prompt: None,
            },
        );

        assert_eq!(
            compiled.prompt,
            "a red fox. low angle. natural light. cinematic. snow"
        );
        assert_eq!(compiled.negative.as_deref(), Some("blurry"));
        assert_eq!(compiled.params["duration_s"], json!(10));
        assert_eq!(compiled.params["aspect_ratio"], json!("9:16"));
        assert_eq!(compiled.char_count, compiled.prompt.chars().count());
    }

    #[test]
    fn compiles_edit_instructions() {
        let mut value = spec();
        value.kind = PresetKind::Edit;
        value.mode = PresetMode::Edit;
        value.reference_slots = 1;
        value.sections = vec!["change".into(), "preserve".into()];

        let compiled = compile(
            &value,
            &CompileInput {
                guidance: Guidance::default(),
                description: Some(json!({
                    "change": "make the jacket red",
                    "preserve": "face and background"
                })),
                brief: None,
                locked: vec![],
                params: json!({}),
                existing_prompt: None,
            },
        );

        assert!(compiled.prompt.starts_with("Change: make the jacket red"));
        assert!(compiled.prompt.contains("Preserve: face and background"));
    }

    #[test]
    fn drops_negative_when_unsupported() {
        let mut value = spec();
        value.supports_negative = false;
        let compiled = compile(
            &value,
            &CompileInput {
                guidance: Guidance::default(),
                description: Some(json!({ "subject": "fox", "negative": "blurry" })),
                brief: None,
                locked: vec![],
                params: json!({}),
                existing_prompt: None,
            },
        );
        assert!(compiled.negative.is_none());
        assert!(
            compiled
                .warnings
                .iter()
                .any(|warning| warning.code == "negative_unsupported")
        );
    }

    #[test]
    fn invalid_parameter_falls_back() {
        let compiled = compile(
            &spec(),
            &CompileInput {
                guidance: Guidance::default(),
                description: Some(json!({ "subject": "fox" })),
                brief: None,
                locked: vec![],
                params: json!({ "duration_s": 99 }),
                existing_prompt: None,
            },
        );
        assert_eq!(compiled.params["duration_s"], json!(5));
        assert!(
            compiled
                .warnings
                .iter()
                .any(|warning| warning.code == "invalid_parameter")
        );
    }

    #[test]
    fn truncates_to_the_character_limit() {
        let mut value = spec();
        value.limits.prompt_chars = 20;
        let compiled = compile(
            &value,
            &CompileInput {
                guidance: Guidance::default(),
                description: Some(
                    json!({ "subject": "a very long subject that exceeds the limit" }),
                ),
                brief: None,
                locked: vec![],
                params: json!({}),
                existing_prompt: None,
            },
        );
        assert!(compiled.char_count <= 20);
        assert!(
            compiled
                .warnings
                .iter()
                .any(|warning| warning.code == "prompt_truncated")
        );
    }

    #[test]
    fn preflight_warns_about_missing_checklist_entries() {
        let compiled = compile(
            &spec(),
            &CompileInput {
                guidance: Guidance::default(),
                description: Some(json!({ "subject": "fox" })),
                brief: None,
                locked: vec![],
                params: json!({}),
                existing_prompt: None,
            },
        );
        let warnings = preflight(&spec(), &compiled);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.code == "checklist_missing")
        );
    }

    #[test]
    fn hash_is_stable() {
        let first = content_hash(&spec()).expect("hash");
        let second = content_hash(&spec()).expect("hash");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn documented_example_is_valid_and_compiles() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../../docs/example-preset.json"))
                .expect("documented example json");
        let spec = parse(&value).expect("documented example parses");
        validate(&spec).expect("documented example validates");
        let compiled = compile(
            &spec,
            &CompileInput {
                guidance: Guidance::default(),
                description: Some(json!({
                    "subject": "a lighthouse",
                    "composition": "wide shot",
                    "lighting": "sunset",
                    "style": "painterly"
                })),
                brief: None,
                locked: Vec::new(),
                params: json!({}),
                existing_prompt: None,
            },
        );
        assert!(compiled.prompt.contains("lighthouse"));
    }

    #[test]
    fn rejects_a_future_schema_major() {
        let mut value = spec();
        value.schema = 99;
        let error = validate(&value).expect_err("future schema");
        assert!(error.message.contains("schema"));
    }

    #[test]
    fn parses_a_json_document() {
        let value = serde_json::to_value(spec()).expect("json");
        let parsed = parse(&value).expect("parse");
        assert_eq!(parsed.id, "test-video");
    }
}
