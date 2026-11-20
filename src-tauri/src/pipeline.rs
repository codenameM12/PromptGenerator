use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use base64::Engine;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::commands::models::ModelRole;
use crate::commands::pipeline::{ComposeReference, DescribeMode, Description, Generation};
use crate::commands::{CommandError, Guidance, PromptWarning, Severity};
use crate::db::Database;
use crate::db::repository::descriptions as desc_repo;
use crate::db::repository::generations as gen_repo;
use crate::preset::{self, PresetMode, PresetSpec};
use crate::runtime::budget::Section;
use crate::runtime::client::{ChatMessage, ChatRequest, ContentPart};
use crate::runtime::manager::ModelManager;

pub struct Pipeline<'a> {
    pub manager: &'a Arc<ModelManager>,
    pub database: &'a Mutex<Database>,
}

pub struct DescribeInput {
    pub asset_id: Option<String>,
    pub edit_id: Option<String>,
    pub asset_hash: String,
    pub mode: DescribeMode,
    pub frames: Vec<PathBuf>,
    pub hint: Option<String>,
    pub force_refresh: bool,
    /// Optional per-install override for the describer system prompt.
    pub system_prompt: Option<String>,
    /// Neutral name for this image (for example `<image1>`) so descriptions
    /// never need to mention file names.
    pub tag: Option<String>,
}

pub struct ComposeInput {
    pub preset_id: String,
    pub spec: PresetSpec,
    pub asset_id: Option<String>,
    pub edit_id: Option<String>,
    pub references: Vec<ComposeReference>,
    pub parent_id: Option<String>,
    pub description: Option<String>,
    pub brief: Option<String>,
    pub existing_prompt: Option<String>,
    pub instruction: Option<String>,
    pub guidance: Guidance,
    pub locked: Vec<String>,
    pub params: Value,
    pub variations: u8,
    pub seed: Option<u64>,
    pub output_language: Option<String>,
    pub bypass_cache: bool,
    pub cancel: Option<Arc<AtomicBool>>,
    /// Optional per-install override that replaces the preset's system prompt.
    pub system_prompt: Option<String>,
    /// Reference images sent alongside the prompt as data URLs. Resolved by the
    /// command layer; batch and job queue paths leave this empty (text-only).
    pub images: Vec<String>,
    /// Hash over `images`, included in the cache key so text-only and image
    /// runs never share a cache entry.
    pub images_key: Option<String>,
}

pub struct BatchReferenceInput {
    pub asset_id: Option<String>,
    pub edit_id: Option<String>,
    pub asset_hash: String,
    pub frames: Vec<PathBuf>,
    pub weight: Option<f64>,
    pub name: Option<String>,
    pub description: Option<String>,
}

pub struct BatchItemInput {
    pub asset_id: Option<String>,
    pub edit_id: Option<String>,
    pub asset_hash: String,
    pub frames: Vec<PathBuf>,
    pub brief: Option<String>,
    pub references: Vec<BatchReferenceInput>,
    pub system_prompt: Option<String>,
}

const DEFAULT_REFERENCE_WEIGHT: f64 = 1.0;
const MIN_REFERENCE_WEIGHT: f64 = 0.0;
const MAX_REFERENCE_WEIGHT: f64 = 2.0;
const MAX_REFERENCES: usize = 4;
const REFERENCE_SUMMARY_CHARS: usize = 120;

/// A reference after weights have been defaulted and clamped. Ordered by the
/// client-supplied position so prompt generation and cache keys stay stable.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedReference {
    pub index: usize,
    pub asset_id: Option<String>,
    pub edit_id: Option<String>,
    pub name: String,
    pub weight: f64,
    pub summary: String,
    pub description: Option<String>,
}

pub fn normalize_references(
    references: &[ComposeReference],
) -> (Vec<NormalizedReference>, Vec<PromptWarning>) {
    let mut warnings = Vec::new();
    if references.len() > MAX_REFERENCES {
        warnings.push(warning(
            "too_many_references",
            format!(
                "{} references supplied; most generation models use at most {}.",
                references.len(),
                MAX_REFERENCES
            ),
        ));
    }

    let normalized = references
        .iter()
        .enumerate()
        .map(|(index, reference)| {
            let requested = reference.weight.unwrap_or(DEFAULT_REFERENCE_WEIGHT);
            let weight = if requested.is_finite() {
                requested.clamp(MIN_REFERENCE_WEIGHT, MAX_REFERENCE_WEIGHT)
            } else {
                warnings.push(warning(
                    "reference_weight_clamped",
                    format!(
                        "Reference {} weight was invalid and was reset to {:.2}.",
                        index + 1,
                        DEFAULT_REFERENCE_WEIGHT
                    ),
                ));
                DEFAULT_REFERENCE_WEIGHT
            };
            if requested.is_finite() && (weight - requested).abs() > f64::EPSILON {
                warnings.push(warning(
                    "reference_weight_clamped",
                    format!(
                        "Reference {} weight {:.2} was clamped to {:.2}.",
                        index + 1,
                        requested,
                        weight
                    ),
                ));
            }
            let name = reference
                .name
                .as_ref()
                .map(|name| name.trim())
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| reference.asset_id.clone())
                .or_else(|| reference.edit_id.clone())
                .unwrap_or_else(|| format!("reference {}", index + 1));
            NormalizedReference {
                index: index + 1,
                asset_id: reference.asset_id.clone(),
                edit_id: reference.edit_id.clone(),
                name,
                weight,
                summary: reference_summary(reference.description.as_ref()),
                description: reference.description.clone(),
            }
        })
        .collect();

    (normalized, warnings)
}

fn reference_summary(description: Option<&String>) -> String {
    let Some(text) = description else {
        return String::new();
    };
    truncate_chars(text.trim(), REFERENCE_SUMMARY_CHARS)
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(limit.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

fn reference_block(references: &[NormalizedReference]) -> String {
    if references.is_empty() {
        return String::new();
    }
    let mut block = String::from("References:");
    for reference in references {
        block.push_str(&format!(
            "\n{}. <image{}> (weight {:.2})",
            reference.index, reference.index, reference.weight
        ));
    }
    block
}

/// Composer-facing reference list: ordered, weighted, with the full reviewed
/// description of each reference.
fn reference_input_block(references: &[NormalizedReference]) -> String {
    let mut block = String::from(
        "References (weight 1.00 = normal influence, below 1 = subtle, above 1 = dominant):",
    );
    for reference in references {
        block.push_str(&format!(
            "\n{}. <image{}> (weight {:.2})",
            reference.index, reference.index, reference.weight
        ));
        if let Some(description) = reference
            .description
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            block.push_str("\n   Description: ");
            block.push_str(description);
        }
    }
    block
}

fn reference_params(references: &[NormalizedReference]) -> Value {
    Value::Array(
        references
            .iter()
            .map(|reference| {
                json!({
                    "index": reference.index,
                    "assetId": reference.asset_id,
                    "editId": reference.edit_id,
                    "name": reference.name,
                    "weight": reference.weight,
                    "summary": reference.summary,
                })
            })
            .collect(),
    )
}

fn reference_key(references: &[NormalizedReference]) -> Value {
    Value::Array(
        references
            .iter()
            .map(|reference| {
                json!({
                    "assetId": reference.asset_id,
                    "editId": reference.edit_id,
                    "name": reference.name,
                    "weight": reference.weight,
                    "description": reference.description,
                })
            })
            .collect(),
    )
}

impl Pipeline<'_> {
    fn with_connection<T>(
        &self,
        action: impl FnOnce(&Connection) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        action(guard.connection())
    }

    pub async fn describe(&self, input: &DescribeInput) -> Result<Description, CommandError> {
        let model = self.manager.roles().describer;
        let key = content_key(&json!({
            "kind": "description",
            "version": "2",
            "format": "text",
            "asset": input.asset_hash,
            "mode": format!("{:?}", input.mode),
            "frames": input.frames.iter().map(|frame| frame.display().to_string()).collect::<Vec<_>>(),
            "hint": input.hint,
            "model": model,
            "system": input.system_prompt,
            "tag": input.tag,
        }))?;

        let cached = if input.force_refresh {
            None
        } else {
            self.with_connection(|connection| desc_repo::find_by_cache_key(connection, &key))?
        };

        if let Some(existing) = cached {
            let text = description_text_from_row(&existing)?;
            return Ok(Description {
                id: existing.id,
                mode: input.mode,
                text,
                cached: true,
                model: existing.model,
            });
        }

        let system = input
            .system_prompt
            .as_deref()
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
            .unwrap_or(DESCRIBE_SYSTEM);
        let mut messages = vec![
            ChatMessage::text("system", system),
            describe_user_message(input)?,
        ];

        let mut last_error = String::new();
        for attempt in 0..2 {
            let request = ChatRequest {
                model: String::new(),
                messages: messages.clone(),
                temperature: 0.3,
                max_tokens: 1200,
                json_schema: None,
                ttl_seconds: Some(600),
                seed: None,
                cancel: None,
            };

            let response = self
                .manager
                .chat(ModelRole::Describer, request, None)
                .await?;
            let text = normalize_description_text(&response.content);

            if usable_description(&text) {
                let id = Uuid::now_v7().to_string();
                let stored = serde_json::to_string(&text).map_err(|error| {
                    CommandError::internal(format!("serialize description: {error}"))
                })?;

                let description = desc_repo::NewDescription {
                    id: id.clone(),
                    asset_id: input.asset_id.clone(),
                    edit_id: input.edit_id.clone(),
                    model: model.clone(),
                    prompt_version: "2".into(),
                    sampling_json: "{}".into(),
                    cache_key: key,
                    json: stored,
                    confidence: None,
                    cached: false,
                };
                self.with_connection(|connection| desc_repo::insert(connection, &description))?;

                return Ok(Description {
                    id,
                    mode: input.mode,
                    text,
                    cached: false,
                    model,
                });
            }

            last_error = if text.is_empty() {
                "The response was empty".into()
            } else {
                "The response was too short to be useful".into()
            };

            if attempt == 0 {
                messages.push(ChatMessage::text("assistant", response.content.clone()));
                messages.push(ChatMessage::text(
                    "user",
                    format!(
                        "Your previous response was not usable: {last_error}. Write at least four detailed sentences covering the subject, setting, lighting, palette, composition, camera, and style. Return prose only, never JSON."
                    ),
                ));
            }
        }

        Err(CommandError::invalid_response(format!(
            "Describer did not return a usable description: {last_error}"
        )))
    }

    /// Deterministic compose cache key; exposed as a method so tests can assert
    /// that image runs and text-only runs never collide.
    fn compose_cache_key(
        &self,
        input: &ComposeInput,
        composer: &str,
        preset_hash: &str,
        references: &[NormalizedReference],
    ) -> Result<String, CommandError> {
        content_key(&json!({
            "kind": "generation",
            "asset": input.asset_id,
            "edit": input.edit_id,
            "references": reference_key(references),
            "description": input.description,
            "brief": input.brief,
            "existing": input.existing_prompt,
            "instruction": input.instruction,
            "guidance": input.guidance,
            "locked": input.locked,
            "params": input.params,
            "language": input.output_language,
            "preset": input.preset_id,
            "presetHash": preset_hash,
            "model": composer,
            "system": input.system_prompt,
            "images": input.images_key,
        }))
    }

    pub async fn compose(
        &self,
        input: &ComposeInput,
        mut on_token: Option<&mut (dyn for<'b> FnMut(&'b str) + Send)>,
    ) -> Result<Vec<Generation>, CommandError> {
        let composer = self.manager.roles().composer;
        let describer = self.manager.roles().describer;
        let count = input.variations.clamp(1, 4) as u64;
        let (references, reference_warnings) = normalize_references(&input.references);
        let preset_hash = preset::content_hash(&input.spec)?;
        let base_key = self.compose_cache_key(input, &composer, &preset_hash, &references)?;

        let cached = if count == 1 && !input.bypass_cache {
            self.with_connection(|connection| gen_repo::find_by_cache_key(connection, &base_key))?
        } else {
            None
        };
        if let Some(existing) = cached
            && existing.status == "succeeded"
        {
            return Ok(vec![generation_from_row(existing)]);
        }

        let base_seed = input.seed.unwrap_or_else(random_seed) & SEED_MASK;
        let status = self.manager.status().await;
        let reported = status
            .models
            .iter()
            .find(|model| model.id == composer)
            .and_then(|model| model.context_length)
            .map(|value| value as usize);
        let configured = input.spec.limits.max_input_tokens;
        let (budget_limit, budget_source) = match reported {
            Some(reported) if reported < configured => (reported, "reported"),
            _ => (configured, "configured"),
        };
        let mut results = Vec::new();

        for index in 0..count {
            let seed = base_seed.wrapping_add(index) & SEED_MASK;
            let started = Instant::now();
            let mut messages = compose_messages_with_budget(&input.spec, input, budget_limit);

            let mut parsed = None;
            let mut raw = String::new();
            let mut last_error = String::new();

            for attempt in 0..2 {
                let request = ChatRequest {
                    model: String::new(),
                    messages: messages.clone(),
                    temperature: 0.7,
                    max_tokens: input.spec.limits.max_output_tokens as u32,
                    json_schema: Some(
                        input
                            .spec
                            .output_schema
                            .clone()
                            .unwrap_or_else(default_output_schema),
                    ),
                    ttl_seconds: Some(600),
                    seed: Some(seed as i64),
                    cancel: input.cancel.clone(),
                };

                let response = match on_token.as_deref_mut() {
                    Some(callback) => {
                        self.manager
                            .chat(ModelRole::Composer, request, Some(callback))
                            .await
                    }
                    None => self.manager.chat(ModelRole::Composer, request, None).await,
                };

                let response = match response {
                    Ok(response) => response,
                    Err(error) if error.code == "cancelled" => {
                        self.record_cancelled(
                            input, &describer, &composer, seed, &base_key, &error,
                        )?;
                        return Err(error);
                    }
                    Err(error) => return Err(error),
                };

                raw = response.content.clone();
                match parse_json_object(&raw) {
                    Some(value) if value.get("prompt").and_then(Value::as_str).is_some() => {
                        parsed = Some(value);
                        break;
                    }
                    Some(_) => last_error = "The response had no prompt field".into(),
                    None => last_error = "The response was not valid JSON".into(),
                }

                if attempt == 0 {
                    messages.push(ChatMessage::text("assistant", raw.clone()));
                    messages.push(ChatMessage::text(
                        "user",
                        format!(
                            "Your previous response was invalid: {last_error}. Return only a JSON object with a prompt string."
                        ),
                    ));
                }
            }

            let (prompt, negative, params, warnings, invalid) = finalize_output(
                &input.spec,
                input,
                parsed.as_ref(),
                &raw,
                &references,
                &reference_warnings,
                budget_limit,
                budget_source,
            );
            let status = if invalid { "invalid" } else { "succeeded" };
            let id = Uuid::now_v7().to_string();
            let latency_ms = started.elapsed().as_millis() as u64;
            let cache_key = if count == 1 {
                base_key.clone()
            } else {
                format!("{base_key}-{seed}")
            };

            let generation = gen_repo::NewGeneration {
                id: id.clone(),
                parent_id: input.parent_id.clone(),
                asset_id: input.asset_id.clone(),
                edit_id: input.edit_id.clone(),
                description_id: None,
                project_id: None,
                shot_index: None,
                preset_id: input.preset_id.clone(),
                preset_version: input.spec.version.clone(),
                preset_hash: preset_hash.clone(),
                prompt: prompt.clone(),
                negative: negative.clone(),
                params_json: serde_json::to_string(&params).unwrap_or_else(|_| "{}".into()),
                guidance_json: serde_json::to_string(&input.guidance)
                    .unwrap_or_else(|_| "{}".into()),
                description_json: input
                    .description
                    .as_ref()
                    .and_then(|text| serde_json::to_string(text).ok()),
                seed: Some(seed as i64),
                models_json: serde_json::to_string(&[describer.clone(), composer.clone()])
                    .unwrap_or_else(|_| "[]".into()),
                status: status.into(),
                error: if invalid {
                    Some(last_error.clone())
                } else {
                    None
                },
                raw_response: if invalid { Some(raw.clone()) } else { None },
                tokens_in: None,
                tokens_out: None,
                latency_ms: Some(latency_ms as i64),
                cache_key: Some(cache_key),
            };
            self.with_connection(|connection| gen_repo::insert(connection, &generation))?;

            results.push(Generation {
                id,
                prompt,
                negative,
                params,
                warnings,
                seed,
                model: composer.clone(),
                preset_id: input.preset_id.clone(),
                preset_version: input.spec.version.clone(),
                cached: false,
                latency_ms,
            });
        }

        Ok(results)
    }

    pub async fn batch(
        &self,
        items: &[BatchItemInput],
        spec: &PresetSpec,
        guidance: &Guidance,
        language: Option<&str>,
    ) -> Result<Vec<Vec<Generation>>, CommandError> {
        // Stage 1: every description completes before the first composition.
        let mut prepared = Vec::with_capacity(items.len());
        for item in items {
            let primary_description = match (&item.asset_id, &item.edit_id) {
                (Some(_), _) | (_, Some(_)) => {
                    let description = self
                        .describe(&DescribeInput {
                            tag: None,
                            asset_id: item.asset_id.clone(),
                            edit_id: item.edit_id.clone(),
                            asset_hash: item.asset_hash.clone(),
                            mode: DescribeMode::Image,
                            frames: item.frames.clone(),
                            hint: item.brief.clone(),
                            force_refresh: false,
                            system_prompt: item.system_prompt.clone(),
                        })
                        .await?;
                    Some(description.text)
                }
                _ => None,
            };

            let mut references = Vec::with_capacity(item.references.len());
            for reference in &item.references {
                let description = match &reference.description {
                    Some(description) => Some(description.clone()),
                    None => match self
                        .describe(&DescribeInput {
                            tag: None,
                            asset_id: reference.asset_id.clone(),
                            edit_id: reference.edit_id.clone(),
                            asset_hash: reference.asset_hash.clone(),
                            mode: DescribeMode::Image,
                            frames: reference.frames.clone(),
                            hint: None,
                            force_refresh: false,
                            system_prompt: item.system_prompt.clone(),
                        })
                        .await
                    {
                        Ok(description) => Some(description.text),
                        Err(error) if error.code == "notFound" => None,
                        Err(error) => return Err(error),
                    },
                };
                references.push(ComposeReference {
                    asset_id: reference.asset_id.clone(),
                    edit_id: reference.edit_id.clone(),
                    weight: reference.weight,
                    name: reference.name.clone(),
                    description,
                });
            }

            let primary_description = primary_description.or_else(|| {
                references
                    .first()
                    .and_then(|reference| reference.description.clone())
            });
            prepared.push((primary_description, references));
        }

        // Stage 2: compositions.
        let mut results = Vec::with_capacity(items.len());
        for (item, (description, references)) in items.iter().zip(prepared) {
            let asset_id = item.asset_id.clone().or_else(|| {
                references
                    .first()
                    .and_then(|reference| reference.asset_id.clone())
            });
            let edit_id = item.edit_id.clone().or_else(|| {
                references
                    .first()
                    .and_then(|reference| reference.edit_id.clone())
            });
            results.push(
                self.compose(
                    &ComposeInput {
                        preset_id: spec.id.clone(),
                        spec: spec.clone(),
                        asset_id,
                        edit_id,
                        references,
                        parent_id: None,
                        description,
                        brief: item.brief.clone(),
                        existing_prompt: None,
                        instruction: None,
                        guidance: guidance.clone(),
                        locked: Vec::new(),
                        params: json!({}),
                        variations: 1,
                        seed: None,
                        output_language: language.map(ToOwned::to_owned),
                        bypass_cache: false,
                        cancel: None,
                        system_prompt: item.system_prompt.clone(),
                        images: Vec::new(),
                        images_key: None,
                    },
                    None,
                )
                .await?,
            );
        }

        Ok(results)
    }

    fn record_cancelled(
        &self,
        input: &ComposeInput,
        describer: &str,
        composer: &str,
        seed: u64,
        cache_key: &str,
        error: &CommandError,
    ) -> Result<(), CommandError> {
        let generation = gen_repo::NewGeneration {
            id: Uuid::now_v7().to_string(),
            parent_id: input.parent_id.clone(),
            asset_id: input.asset_id.clone(),
            edit_id: input.edit_id.clone(),
            description_id: None,
            project_id: None,
            shot_index: None,
            preset_id: input.preset_id.clone(),
            preset_version: input.spec.version.clone(),
            preset_hash: preset::content_hash(&input.spec)?,
            prompt: String::new(),
            negative: None,
            params_json: "{}".into(),
            guidance_json: serde_json::to_string(&input.guidance).unwrap_or_else(|_| "{}".into()),
            description_json: None,
            seed: Some(seed as i64),
            models_json: serde_json::to_string(&[describer, composer])
                .unwrap_or_else(|_| "[]".into()),
            status: "cancelled".into(),
            error: Some(error.message.clone()),
            raw_response: None,
            tokens_in: None,
            tokens_out: None,
            latency_ms: None,
            cache_key: Some(format!("{cache_key}-{seed}")),
        };
        self.with_connection(|connection| gen_repo::insert(connection, &generation))?;
        Ok(())
    }
}

const DESCRIBE_SYSTEM: &str = "You are a precise, verbose visual describer. Write detailed prose that a prompt writer can turn into an image or video prompt. Cover the subject and its expression, composition and framing, setting, lighting, color palette, camera and lens character, style, and any visible text. For video also describe camera motion, subject motion, and how the scene changes over time. For edit inputs state the intended change and what must be preserved. Never return JSON, Markdown, or bullet lists: write three to six connected sentences. Say when something is not visible instead of inventing it.";

fn describe_user_message(input: &DescribeInput) -> Result<ChatMessage, CommandError> {
    let mut parts = Vec::new();
    if let Some(hint) = &input.hint {
        parts.push(ContentPart::Text(format!("Focus: {hint}")));
    }
    if let Some(tag) = input.tag.as_deref().filter(|tag| !tag.trim().is_empty()) {
        parts.push(ContentPart::Text(format!(
            "This image is {tag}. If it must be named, call it {tag}; never use a file name."
        )));
    }
    parts.push(ContentPart::Text(match input.mode {
        DescribeMode::Image => "Describe this image in detailed prose.".into(),
        DescribeMode::Video => {
            "Describe this clip in detailed prose using the frames. Include motion and temporal change."
                .into()
        }
        DescribeMode::Edit => {
            "Describe the intended edit in prose: what changes and what must be preserved.".into()
        }
    }));

    for frame in input.frames.iter().take(9) {
        let bytes = std::fs::read(frame).map_err(|error| CommandError::from_io(&error))?;
        let mime = mime_for(frame);
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        parts.push(ContentPart::ImageUrl(format!(
            "data:{mime};base64,{encoded}"
        )));
    }

    Ok(ChatMessage::with_parts("user", parts))
}

fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

pub(crate) fn flatten_legacy_description(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.trim().to_string();
    }
    let Some(object) = value.as_object() else {
        return serde_json::to_string(value).unwrap_or_default();
    };
    let mut parts = Vec::new();
    for (key, label) in [
        ("subject", "Subject"),
        ("composition", "Composition"),
        ("setting", "Setting"),
        ("lighting", "Lighting"),
        ("palette", "Color palette"),
        ("camera", "Camera and lens"),
        ("style", "Style"),
        ("motion", "Camera and subject motion"),
        ("temporal", "Temporal change"),
        ("visible_text", "Visible text"),
        ("change", "Intended change"),
        ("preserve", "Preserve"),
    ] {
        if let Some(text) = object.get(key).and_then(Value::as_str) {
            let text = text.trim();
            if !text.is_empty() && !text.eq_ignore_ascii_case("none") {
                parts.push(format!("{label}: {text}"));
            }
        }
    }
    if let Some(segments) = object.get("segments").and_then(Value::as_array) {
        let joined = segments
            .iter()
            .filter_map(Value::as_str)
            .filter(|segment| !segment.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !joined.is_empty() {
            parts.push(format!("Segments: {joined}"));
        }
    }
    if parts.is_empty() {
        return serde_json::to_string(value).unwrap_or_default();
    }
    format!("{}.", parts.join(". "))
}

fn description_text_from_row(row: &desc_repo::DescriptionRow) -> Result<String, CommandError> {
    let value: Value = serde_json::from_str(&row.json)
        .map_err(|error| CommandError::internal(format!("read cached description: {error}")))?;
    Ok(flatten_legacy_description(&value))
}

/// Accepts prose as-is, but tolerates a model that still returns JSON.
fn normalize_description_text(raw: &str) -> String {
    let trimmed = raw.trim();
    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    if unfenced.starts_with('{')
        && let Ok(value) = serde_json::from_str::<Value>(unfenced)
    {
        return flatten_legacy_description(&value);
    }
    unfenced.to_string()
}

fn usable_description(text: &str) -> bool {
    text.chars().count() >= 40
}

pub fn compose_messages(spec: &PresetSpec, input: &ComposeInput) -> Vec<ChatMessage> {
    compose_messages_with_budget(spec, input, spec.limits.max_input_tokens)
}

fn compose_messages_with_budget(
    spec: &PresetSpec,
    input: &ComposeInput,
    max_input_tokens: usize,
) -> Vec<ChatMessage> {
    let guidance = merged_guidance(spec, &input.guidance);
    let language = input.output_language.as_deref().unwrap_or("English");

    let mut system = String::new();
    let base_prompt = input
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .unwrap_or(&spec.system_prompt);
    system.push_str(base_prompt);
    system.push_str("\n\nReturn only a JSON object with a prompt string, an optional negative string, and a params object.");
    system.push_str(&format!(
        "\nMaximum prompt length: {} characters. Output language: {language}.",
        spec.limits.prompt_chars
    ));
    if spec.mode == PresetMode::Edit {
        system.push_str("\nThis is an edit instruction: state the change and what to preserve. Do not re-describe the source.");
    }
    if !guidance.is_empty() {
        system.push_str(&format!("\nGuidance:\n{guidance}"));
    }
    if !input.locked.is_empty() {
        system.push_str(&format!(
            "\nKeep these phrases exactly: {}",
            input.locked.join(", ")
        ));
    }
    if !input.references.is_empty() {
        system.push_str(
            "\nOrdered references are supplied with weights. Keep each reference's role and relative weight in mind. Refer to references only by their tags (<image1>, <image2>, ...); never use file names.",
        );
    }

    let (references, _) = normalize_references(&input.references);
    let mut sections = Vec::new();
    if let Some(existing) = &input.existing_prompt {
        let text = match &input.instruction {
            Some(instruction) => {
                format!("Revise this existing prompt:\n{existing}\n\nInstruction:\n{instruction}")
            }
            None => format!("Revise this existing prompt:\n{existing}"),
        };
        sections.push(Section {
            name: "existing".into(),
            text,
            priority: 0,
        });
    } else if let Some(brief) = &input.brief {
        sections.push(Section {
            name: "brief".into(),
            text: format!("Write a prompt from this brief:\n{brief}"),
            priority: 0,
        });
    }

    let described_in_references = references
        .iter()
        .any(|reference| reference.description.is_some());
    if !references.is_empty() {
        // The composer gets every reference's full description with its weight
        // (the output-side block lists names and weights only).
        sections.push(Section {
            name: "references".into(),
            text: reference_input_block(&references),
            priority: if described_in_references { 1 } else { 2 },
        });
    }

    // A single description is already inside its reference entry; only add the
    // standalone section when no reference carries it (text-only / legacy use).
    if !described_in_references && let Some(description) = &input.description {
        sections.push(Section {
            name: "description".into(),
            text: format!("Visual description:\n{description}"),
            priority: 1,
        });
    }

    if sections.is_empty() {
        sections.push(Section {
            name: "brief".into(),
            text: format!(
                "Write a prompt from this brief:\n{}",
                input.brief.as_deref().unwrap_or("")
            ),
            priority: 0,
        });
    }

    let system_tokens = crate::runtime::budget::estimate_tokens(&system);
    let budget = max_input_tokens.saturating_sub(system_tokens).max(1);
    crate::runtime::budget::fit_sections(&mut sections, budget);
    let user = sections
        .iter()
        .map(|section| section.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut messages = Vec::new();
    let user_parts = {
        let mut parts = Vec::new();
        if !user.is_empty() {
            parts.push(ContentPart::Text(user));
        }
        for image in &input.images {
            parts.push(ContentPart::ImageUrl(image.clone()));
        }
        parts
    };
    messages.push(ChatMessage::text("system", system));
    messages.push(ChatMessage::with_parts("user", user_parts));
    messages
}

fn merged_guidance(spec: &PresetSpec, guidance: &Guidance) -> String {
    let mut parts = Vec::new();
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
        if !value.is_empty()
            && !parts
                .iter()
                .any(|part: &String| part.eq_ignore_ascii_case(value))
        {
            parts.push(value.to_string());
        }
    }
    parts.join(". ")
}

#[allow(clippy::too_many_arguments)]
fn finalize_output(
    spec: &PresetSpec,
    input: &ComposeInput,
    parsed: Option<&Value>,
    raw: &str,
    references: &[NormalizedReference],
    reference_warnings: &[PromptWarning],
    budget_limit: usize,
    budget_source: &str,
) -> (String, Option<String>, Value, Vec<PromptWarning>, bool) {
    let mut warnings = reference_warnings.to_vec();
    let invalid = parsed.is_none();
    let mut prompt = parsed
        .and_then(|value| value.get("prompt"))
        .and_then(Value::as_str)
        .unwrap_or(raw)
        .trim()
        .to_string();

    for locked in &input.locked {
        let phrase = locked.trim();
        if !phrase.is_empty()
            && !prompt
                .to_ascii_lowercase()
                .contains(&phrase.to_ascii_lowercase())
        {
            if !prompt.is_empty() {
                prompt.push(' ');
            }
            prompt.push_str(phrase);
        }
    }

    let references_block = reference_block(references);
    if !references_block.is_empty() {
        let limit = spec.limits.prompt_chars;
        let block_chars = references_block.chars().count();
        if block_chars >= limit {
            prompt = references_block.chars().take(limit).collect();
            warnings.push(warning(
                "prompt_truncated",
                "Prompt exceeded the preset limit and the references section was truncated".into(),
            ));
        } else {
            let budget = limit.saturating_sub(block_chars + 1);
            if prompt.chars().count() > budget {
                prompt = prompt.chars().take(budget).collect();
                warnings.push(warning(
                    "prompt_truncated",
                    "Prompt exceeded the preset limit and was truncated to fit the references"
                        .into(),
                ));
            }
            if !prompt.is_empty() {
                prompt.push('\n');
            }
            prompt.push_str(&references_block);
        }
    }

    if prompt.chars().count() > spec.limits.prompt_chars {
        prompt = prompt.chars().take(spec.limits.prompt_chars).collect();
        if !warnings.iter().any(|item| item.code == "prompt_truncated") {
            warnings.push(warning(
                "prompt_truncated",
                "Prompt exceeded the preset limit and was truncated".into(),
            ));
        }
    }

    let negative = parsed
        .and_then(|value| value.get("negative"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let negative = match (spec.supports_negative, negative) {
        (true, Some(mut value)) => {
            if value.chars().count() > spec.limits.negative_chars {
                value = value.chars().take(spec.limits.negative_chars).collect();
                warnings.push(warning(
                    "negative_truncated",
                    "Negative prompt was truncated".into(),
                ));
            }
            Some(value)
        }
        (false, Some(_)) => {
            warnings.push(warning(
                "negative_unsupported",
                "This target does not support negative prompts".into(),
            ));
            None
        }
        (_, None) => None,
    };

    let mut params = Map::new();
    for (name, spec) in &spec.params {
        params.insert(name.clone(), spec.default.clone().unwrap_or(Value::Null));
    }
    for source in [
        Some(&input.params),
        parsed.and_then(|value| value.get("params")),
    ] {
        if let Some(Value::Object(requested)) = source {
            for (name, value) in requested {
                if let Some(param) = spec.params.get(name) {
                    match param.validate(value) {
                        Ok(value) => {
                            params.insert(name.clone(), value);
                        }
                        Err(_) => warnings.push(warning(
                            "invalid_parameter",
                            format!("Parameter '{name}' fell back to its default"),
                        )),
                    }
                }
            }
        }
    }

    if !references.is_empty() {
        params.insert("references".into(), reference_params(references));
    }
    params.insert(
        "budget".into(),
        json!({ "maxInputTokens": budget_limit, "source": budget_source }),
    );

    if invalid {
        warnings.push(warning(
            "invalid_output",
            "The model output was not valid JSON; the raw response was kept".into(),
        ));
    }

    let lower = prompt.to_ascii_lowercase();
    for item in &spec.checklist {
        if !lower.contains(&item.to_ascii_lowercase()) {
            warnings.push(warning(
                "checklist_missing",
                format!("Prompt may be missing required element: {item}"),
            ));
        }
    }

    if prompt.chars().count() == 0 {
        warnings.push(warning("empty_prompt", "The prompt is empty".into()));
    }

    let params = Value::Object(params);
    (prompt, negative, params, warnings, invalid)
}

fn generation_from_row(row: gen_repo::GenerationRow) -> Generation {
    Generation {
        id: row.id,
        prompt: row.prompt,
        negative: row.negative,
        params: serde_json::from_str(&row.params_json).unwrap_or_else(|_| json!({})),
        warnings: Vec::new(),
        seed: row.seed.unwrap_or_default().max(0) as u64,
        model: serde_json::from_str::<Vec<String>>(&row.models_json)
            .ok()
            .and_then(|models| models.last().cloned())
            .unwrap_or_default(),
        preset_id: row.preset_id,
        preset_version: row.preset_version,
        cached: true,
        latency_ms: row.latency_ms.unwrap_or_default().max(0) as u64,
    }
}

fn default_output_schema() -> Value {
    json!({
        "type": "object",
        "required": ["prompt", "params"],
        "properties": {
            "prompt": { "type": "string" },
            "negative": { "type": "string" },
            "params": { "type": "object" },
            "notes": { "type": "string" }
        }
    })
}

fn content_key(value: &Value) -> Result<String, CommandError> {
    let text = serde_json::to_string(value)
        .map_err(|error| CommandError::internal(format!("serialize cache key: {error}")))?;
    Ok(blake3::hash(text.as_bytes()).to_hex().to_string())
}

pub fn parse_json_object(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed)
        && value.is_object()
    {
        return Some(value);
    }

    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end <= start {
        return None;
    }
    let candidate = &trimmed[start..=end];
    if let Ok(value) = serde_json::from_str::<Value>(candidate)
        && value.is_object()
    {
        return Some(value);
    }

    let repaired = candidate.replace(",}", "}").replace(",]", "]");
    serde_json::from_str::<Value>(&repaired)
        .ok()
        .filter(Value::is_object)
}

fn random_seed() -> u64 {
    Uuid::now_v7().as_u128() as u64
}

/// LM Studio accepts seeds from -1 upward. Keep generated seeds in the
/// non-negative `i64` range so a random `u64` cannot become a negative seed.
const SEED_MASK: u64 = i64::MAX as u64;

fn warning(code: &str, message: String) -> PromptWarning {
    PromptWarning {
        code: code.into(),
        message,
        severity: Severity::Warning,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashSet, VecDeque};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use serde_json::json;

    use crate::commands::models::{KeepWarm, LoadModelRequest, LoadModelResult, ModelInfo};
    use crate::commands::presets::PresetKind;
    use crate::db::Database;
    use crate::db::repository::assets;
    use crate::media::test_support::{make_image, temp_dir, tools};
    use crate::preset::{Limits, ParamSpec, PresetMode, PresetSpec};
    use crate::runtime::client::{ChatRequest, ChatResponse, ContentPart};
    use crate::runtime::manager::{ManagerConfig, RoleConfig, RuntimeBackend};

    use super::{ComposeInput, DescribeInput, Pipeline};

    struct FakeBackend {
        responses: Mutex<VecDeque<String>>,
        calls: Mutex<Vec<ChatRequest>>,
        loaded: Mutex<HashSet<String>>,
    }

    impl FakeBackend {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                calls: Mutex::new(Vec::new()),
                loaded: Mutex::new(HashSet::new()),
            }
        }

        fn calls(&self) -> Vec<ChatRequest> {
            self.calls.lock().expect("calls").clone()
        }
    }

    #[async_trait]
    impl RuntimeBackend for FakeBackend {
        async fn list_models(&self) -> Result<Vec<ModelInfo>, crate::commands::CommandError> {
            Ok(self
                .loaded
                .lock()
                .expect("loaded")
                .iter()
                .map(|id| ModelInfo {
                    id: id.clone(),
                    kind: "llm".into(),
                    loaded: true,
                    context_length: Some(8192),
                    path: None,
                    vision: None,
                    reasoning: None,
                    max_output: None,
                    quantization: None,
                })
                .collect())
        }

        async fn load_model(
            &self,
            request: &LoadModelRequest,
        ) -> Result<LoadModelResult, crate::commands::CommandError> {
            self.loaded
                .lock()
                .expect("loaded")
                .insert(request.model.clone());
            Ok(LoadModelResult {
                model: request.model.clone(),
                context_length: request.context_length,
                load_time_ms: 1,
                effective_gpu_offload: request.gpu_offload,
            })
        }

        async fn unload_model(&self, model: &str) -> Result<(), crate::commands::CommandError> {
            self.loaded.lock().expect("loaded").remove(model);
            Ok(())
        }

        async fn chat(
            &self,
            request: &ChatRequest,
        ) -> Result<ChatResponse, crate::commands::CommandError> {
            self.calls.lock().expect("calls").push(request.clone());
            let content = self
                .responses
                .lock()
                .expect("responses")
                .pop_front()
                .unwrap_or_else(|| "{}".into());
            Ok(ChatResponse {
                content,
                usage: None,
            })
        }

        async fn stream_chat<'a>(
            &'a self,
            request: &'a ChatRequest,
            on_token: &'a mut (dyn for<'b> FnMut(&'b str) + Send),
        ) -> Result<ChatResponse, crate::commands::CommandError> {
            let response = self.chat(request).await?;
            on_token(&response.content);
            Ok(response)
        }
    }

    fn manager(backend: Arc<FakeBackend>) -> Arc<crate::runtime::manager::ModelManager> {
        crate::runtime::manager::ModelManager::new(
            backend,
            None,
            ManagerConfig {
                roles: RoleConfig::default(),
                keep_warm: KeepWarm::None,
                ttl: Duration::from_secs(60),
                describer_concurrency: 2,
            },
        )
    }

    fn spec() -> PresetSpec {
        let mut params = std::collections::BTreeMap::new();
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
            output_schema: None,
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
            checklist: vec!["camera".into()],
            examples: vec![],
            reference_slots: 0,
            instruction_style: false,
        }
    }

    fn description_json() -> String {
        json!({
            "subject": "a red fox",
            "composition": "wide shot",
            "setting": "snow",
            "lighting": "dusk",
            "palette": "blue",
            "camera": "low angle",
            "style": "cinematic",
            "visible_text": "none",
            "confidence": 0.9
        })
        .to_string()
    }

    fn generation_json(prompt: &str) -> String {
        json!({
            "prompt": prompt,
            "negative": "blurry",
            "params": { "duration_s": 10 }
        })
        .to_string()
    }

    fn compose_input() -> ComposeInput {
        ComposeInput {
            preset_id: "test-video".into(),
            spec: spec(),
            asset_id: None,
            edit_id: None,
            references: vec![],
            parent_id: None,
            description: None,
            brief: Some("a red fox in snow".into()),
            existing_prompt: None,
            instruction: None,
            guidance: crate::commands::Guidance::default(),
            locked: vec![],
            params: json!({}),
            variations: 1,
            seed: Some(42),
            output_language: None,
            bypass_cache: false,
            cancel: None,
            system_prompt: None,
            images: Vec::new(),
            images_key: None,
        }
    }

    fn database() -> Mutex<Database> {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        Mutex::new(database)
    }

    #[test]
    fn images_change_cache_keys_and_append_url_parts() {
        let manager = manager(Arc::new(FakeBackend::new(vec![generation_json(
            "a red fox",
        )])));
        let pipeline = Pipeline {
            manager: &manager,
            database: &database(),
        };
        let composer = pipeline.manager.roles().composer;
        let preset_hash = crate::preset::content_hash(&compose_input().spec).expect("hash");
        let (references, _) = super::normalize_references(&compose_input().references);

        let mut input = compose_input();
        let text_key = pipeline
            .compose_cache_key(&input, &composer, &preset_hash, &references)
            .expect("key");
        input.images = vec!["data:image/png;base64,AAAA".into()];
        input.images_key = Some("img-hash".into());
        let image_key = pipeline
            .compose_cache_key(&input, &composer, &preset_hash, &references)
            .expect("key");
        assert_ne!(text_key, image_key);

        input.images_key = Some("img-hash-2".into());
        let other_key = pipeline
            .compose_cache_key(&input, &composer, &preset_hash, &references)
            .expect("key");
        assert_ne!(image_key, other_key);

        let messages = super::compose_messages(&input.spec, &input);
        let user = &messages[1];
        assert_eq!(
            user.content
                .iter()
                .filter(|part| matches!(part, super::ContentPart::ImageUrl(_)))
                .count(),
            1
        );
        assert!(matches!(
            messages[0].content[0],
            super::ContentPart::Text(_)
        ));
    }

    fn frame() -> std::path::PathBuf {
        let directory = temp_dir("pipeline-frame");
        let path = directory.join("frame.png");
        make_image(&tools(), &path, "32x32");
        path
    }

    #[test]
    fn composer_system_prompt_override_replaces_the_preset_text() {
        let mut input = compose_input();
        input.system_prompt = Some("CUSTOM COMPOSER INSTRUCTIONS".into());
        let messages = super::compose_messages(&input.spec, &input);
        let ContentPart::Text(system) = &messages[0].content[0] else {
            panic!("expected text part");
        };
        assert!(system.starts_with("CUSTOM COMPOSER INSTRUCTIONS"));
        assert!(system.contains("Return only a JSON object"));
        assert!(!system.contains(&input.spec.system_prompt));
    }

    #[tokio::test]
    async fn describe_uses_the_system_prompt_override() {
        let backend = Arc::new(FakeBackend::new(vec![description_json()]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };
        let input = DescribeInput {
            tag: None,
            asset_id: None,
            edit_id: None,
            asset_hash: "asset-hash".into(),
            mode: crate::commands::pipeline::DescribeMode::Image,
            frames: vec![frame()],
            hint: None,
            force_refresh: true,
            system_prompt: Some("CUSTOM DESCRIBER INSTRUCTIONS".into()),
        };

        pipeline.describe(&input).await.expect("describe");
        let calls = backend.calls();
        let ContentPart::Text(system) = &calls[0].messages[0].content[0] else {
            panic!("expected text part");
        };
        assert!(system.contains("CUSTOM DESCRIBER INSTRUCTIONS"));
    }

    #[tokio::test]
    async fn describes_and_caches() {
        let backend = Arc::new(FakeBackend::new(vec![description_json()]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };

        let input = DescribeInput {
            tag: None,
            asset_id: None,
            edit_id: None,
            asset_hash: "asset-hash".into(),
            mode: crate::commands::pipeline::DescribeMode::Image,
            frames: vec![frame()],
            hint: None,
            force_refresh: false,
            system_prompt: None,
        };

        let first = pipeline.describe(&input).await.expect("describe");
        assert!(!first.cached);
        assert!(first.text.contains("a red fox"));

        let second = pipeline.describe(&input).await.expect("cached");
        assert!(second.cached);
        assert_eq!(backend.calls().len(), 1);
    }

    #[tokio::test]
    async fn retries_a_short_description_then_stores_prose() {
        let backend = Arc::new(FakeBackend::new(vec![
            "too short".into(),
            "A red fox sits in deep snow at dusk, lit by a low amber sun. Its fur catches the light while the background falls into soft blue shadow. The frame is tight, cinematic, and shallow of field.".into(),
        ]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };
        let input = DescribeInput {
            tag: None,
            asset_id: None,
            edit_id: None,
            asset_hash: "text-retry".into(),
            mode: crate::commands::pipeline::DescribeMode::Image,
            frames: vec![frame()],
            hint: None,
            force_refresh: false,
            system_prompt: None,
        };

        let described = pipeline.describe(&input).await.expect("describe");
        assert!(!described.cached);
        assert!(described.text.contains("red fox"));
        assert_eq!(backend.calls().len(), 2);
        assert!(backend.calls()[0].json_schema.is_none());
    }

    #[test]
    fn legacy_structured_descriptions_flatten_to_prose() {
        let value = json!({
            "subject": "a wolf",
            "camera": "low angle",
            "visible_text": "none",
            "confidence": 0.9
        });
        let text = super::flatten_legacy_description(&value);
        assert!(text.contains("Subject: a wolf"));
        assert!(text.contains("Camera and lens: low angle"));
        assert!(!text.contains("confidence"));
        assert!(!text.contains("Visible text"));
    }

    #[tokio::test]
    async fn composes_and_persists_a_generation() {
        let backend = Arc::new(FakeBackend::new(vec![generation_json("a red fox")]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };

        let results = pipeline
            .compose(&compose_input(), None)
            .await
            .expect("compose");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].prompt, "a red fox");
        assert_eq!(results[0].negative.as_deref(), Some("blurry"));
        assert_eq!(results[0].params["duration_s"], json!(10));
        assert_eq!(backend.calls().len(), 1);

        let count: i64 = database
            .lock()
            .expect("db")
            .connection()
            .query_row("SELECT COUNT(*) FROM generations", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn seeds_are_non_negative_for_lm_studio() {
        let backend = Arc::new(FakeBackend::new(vec![generation_json("a red fox")]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };
        let mut input = compose_input();
        input.seed = Some(u64::MAX);

        let results = pipeline.compose(&input, None).await.expect("compose");

        assert_eq!(backend.calls()[0].seed, Some(i64::MAX));
        assert_eq!(results[0].seed, i64::MAX as u64);
    }

    #[tokio::test]
    async fn retries_invalid_json_once() {
        let backend = Arc::new(FakeBackend::new(vec![
            "not json".into(),
            generation_json("a red fox"),
        ]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };

        let results = pipeline
            .compose(&compose_input(), None)
            .await
            .expect("compose");
        assert_eq!(results[0].prompt, "a red fox");
        assert_eq!(backend.calls().len(), 2);
    }

    #[tokio::test]
    async fn keeps_raw_output_when_json_keeps_failing() {
        let backend = Arc::new(FakeBackend::new(vec!["nope".into(), "still nope".into()]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };

        let results = pipeline
            .compose(&compose_input(), None)
            .await
            .expect("compose");
        assert!(results[0].prompt.contains("still nope"));
        assert!(
            results[0]
                .warnings
                .iter()
                .any(|warning| warning.code == "invalid_output")
        );

        let status: String = database
            .lock()
            .expect("db")
            .connection()
            .query_row("SELECT status FROM generations LIMIT 1", [], |row| {
                row.get(0)
            })
            .expect("status");
        assert_eq!(status, "invalid");
    }

    #[tokio::test]
    async fn variations_have_distinct_seeds() {
        let backend = Arc::new(FakeBackend::new(vec![
            generation_json("one"),
            generation_json("two"),
            generation_json("three"),
        ]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };

        let mut input = compose_input();
        input.variations = 3;
        let results = pipeline.compose(&input, None).await.expect("variations");
        assert_eq!(results.len(), 3);
        let mut seeds: Vec<u64> = results.iter().map(|result| result.seed).collect();
        seeds.sort_unstable();
        seeds.dedup();
        assert_eq!(seeds.len(), 3);
    }

    #[tokio::test]
    async fn cache_hit_skips_the_model() {
        let backend = Arc::new(FakeBackend::new(vec![generation_json("a red fox")]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };
        let input = compose_input();

        let first = pipeline.compose(&input, None).await.expect("first");
        let second = pipeline.compose(&input, None).await.expect("second");
        assert!(!first[0].cached);
        assert!(second[0].cached);
        assert_eq!(backend.calls().len(), 1);
    }

    #[tokio::test]
    async fn batch_describes_everything_before_composing() {
        let backend = Arc::new(FakeBackend::new(vec![
            description_json(),
            description_json(),
            generation_json("one"),
            generation_json("two"),
        ]));
        let manager = manager(backend.clone());
        let database = database();
        {
            let guard = database.lock().expect("db");
            for (id, hash) in [("a", "hash-a"), ("b", "hash-b")] {
                assets::insert(
                    guard.connection(),
                    &assets::NewAsset {
                        id: id.into(),
                        hash: hash.into(),
                        kind: "image".into(),
                        source_path: format!("C:\\media\\{id}.png"),
                        display_name: format!("{id}.png"),
                        width: Some(32),
                        height: Some(32),
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
            }
        }
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };
        let frame = frame();
        let items = vec![
            super::BatchItemInput {
                asset_id: Some("a".into()),
                edit_id: None,
                asset_hash: "hash-a".into(),
                frames: vec![frame.clone()],
                brief: None,
                references: vec![],
                system_prompt: None,
            },
            super::BatchItemInput {
                asset_id: Some("b".into()),
                edit_id: None,
                asset_hash: "hash-b".into(),
                frames: vec![frame],
                brief: None,
                references: vec![],
                system_prompt: None,
            },
        ];

        let results = pipeline
            .batch(&items, &spec(), &crate::commands::Guidance::default(), None)
            .await
            .expect("batch");
        assert_eq!(results.len(), 2);

        let calls = backend.calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0].model, "lfm3-vl-2b");
        assert_eq!(calls[1].model, "lfm3-vl-2b");
        assert_eq!(
            calls[2].model,
            "gemma4-12b-qat-uncensored-hauhaucs-balanced"
        );
        assert_eq!(
            calls[3].model,
            "gemma4-12b-qat-uncensored-hauhaucs-balanced"
        );
    }

    #[tokio::test]
    async fn messages_merge_guidance_and_language() {
        let mut input = compose_input();
        input.guidance = crate::commands::Guidance {
            global: Some("natural light".into()),
            preset: None,
            run: Some("snow".into()),
        };
        input.locked = vec!["red fox".into()];
        input.output_language = Some("French".into());
        let messages = super::compose_messages(&spec(), &input);
        let system = match &messages[0].content[0] {
            super::ContentPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert!(system.contains("natural light. cinematic. snow"));
        assert!(system.contains("French"));
        assert!(system.contains("red fox"));
    }

    #[tokio::test]
    async fn self_check_warns_on_missing_checklist_terms() {
        let backend = Arc::new(FakeBackend::new(vec![generation_json("a red fox")]));
        let manager = manager(backend);
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };

        let results = pipeline
            .compose(&compose_input(), None)
            .await
            .expect("compose");
        assert!(
            results[0]
                .warnings
                .iter()
                .any(|warning| warning.code == "checklist_missing")
        );
    }

    #[test]
    fn repairs_trailing_commas() {
        let value = super::parse_json_object("{\"prompt\": \"a fox\",}").expect("repair");
        assert_eq!(value["prompt"], "a fox");
    }

    #[tokio::test]
    async fn refines_with_an_instruction_and_records_lineage() {
        let backend = Arc::new(FakeBackend::new(vec![
            generation_json("original prompt"),
            generation_json("refined prompt"),
        ]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };

        let parent = pipeline
            .compose(&compose_input(), None)
            .await
            .expect("parent");
        let mut input = compose_input();
        input.brief = None;
        input.existing_prompt = Some("original prompt".into());
        input.instruction = Some("make the lighting colder".into());
        input.parent_id = Some(parent[0].id.clone());

        let refined = pipeline.compose(&input, None).await.expect("refine");
        assert_eq!(refined[0].prompt, "refined prompt");

        let calls = backend.calls();
        let user = match &calls[1].messages[1].content[0] {
            super::ContentPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert!(user.contains("original prompt"));
        assert!(user.contains("make the lighting colder"));

        let stored_parent: Option<String> = database
            .lock()
            .expect("db")
            .connection()
            .query_row(
                "SELECT parent_id FROM generations WHERE id = ?1",
                [&refined[0].id],
                |row| row.get(0),
            )
            .expect("parent id");
        assert_eq!(stored_parent.as_deref(), Some(parent[0].id.as_str()));
    }

    #[tokio::test]
    async fn uses_an_edited_description() {
        let backend = Arc::new(FakeBackend::new(vec![generation_json("edited prompt")]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };
        let mut input = compose_input();
        input.brief = None;
        input.description =
            Some("A wolf seen from a low angle, its fur rimmed with snow-light.".into());

        pipeline.compose(&input, None).await.expect("compose");
        let calls = backend.calls();
        let user = match &calls[0].messages[1].content[0] {
            super::ContentPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert!(user.contains("wolf"));
    }

    fn reference(
        asset: &str,
        weight: Option<f64>,
        subject: &str,
    ) -> crate::commands::pipeline::ComposeReference {
        crate::commands::pipeline::ComposeReference {
            asset_id: Some(asset.into()),
            edit_id: None,
            weight,
            name: Some(format!("{asset}.png")),
            description: Some(format!("{subject} in a snowy setting.")),
        }
    }

    #[tokio::test]
    async fn clamps_reference_weights_and_surfaces_them_in_params() {
        let backend = Arc::new(FakeBackend::new(vec![generation_json("a red fox")]));
        let manager = manager(backend);
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };
        let mut input = compose_input();
        input.references = vec![
            reference("a", Some(-1.0), "a red fox"),
            reference("b", Some(7.5), "a grey wolf"),
            reference("c", None, "snow"),
        ];

        let results = pipeline.compose(&input, None).await.expect("compose");
        let params = results[0].params.as_object().expect("params");
        let references = params["references"].as_array().expect("references");
        assert_eq!(references.len(), 3);
        assert_eq!(references[0]["weight"], json!(0.0));
        assert_eq!(references[1]["weight"], json!(2.0));
        assert_eq!(references[2]["weight"], json!(1.0));
        assert!(
            results[0]
                .warnings
                .iter()
                .any(|warning| warning.code == "reference_weight_clamped")
        );
        assert!(results[0].prompt.contains("References:"));
        assert!(results[0].prompt.contains("weight 2.00"));
    }

    #[tokio::test]
    async fn cache_key_includes_the_ordered_reference_set() {
        let backend = Arc::new(FakeBackend::new(vec![
            generation_json("first"),
            generation_json("second"),
        ]));
        let manager = manager(backend.clone());
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };
        let mut input = compose_input();
        input.references = vec![reference("a", Some(0.5), "a red fox")];
        pipeline.compose(&input, None).await.expect("first");

        input.references = vec![reference("a", Some(0.75), "a red fox")];
        pipeline.compose(&input, None).await.expect("second");

        assert_eq!(backend.calls().len(), 2);
    }

    #[test]
    fn old_compose_payloads_still_deserialize() {
        let request: crate::commands::pipeline::ComposeRequest = serde_json::from_value(json!({
            "presetId": "mmh3",
            "assetId": "asset-1",
            "editId": null,
            "description": null,
            "brief": "a red fox",
            "guidance": { "global": null, "preset": null, "run": null },
            "locked": [],
            "existingPrompt": null,
            "seed": null,
            "variations": 1,
            "bypassCache": false,
            "outputLanguage": null
        }))
        .expect("deserialize");
        assert!(request.references.is_empty());
        assert_eq!(request.asset_id.as_deref(), Some("asset-1"));
    }

    #[tokio::test]
    async fn uses_the_smaller_reported_context_for_budgeting() {
        let backend = Arc::new(FakeBackend::new(vec![generation_json("a red fox")]));
        let manager = manager(backend);
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };
        let mut input = compose_input();
        input.spec.limits.max_input_tokens = 20_000;
        drop(
            manager
                .clone()
                .acquire(crate::commands::models::ModelRole::Composer)
                .await
                .expect("load composer"),
        );

        let results = pipeline.compose(&input, None).await.expect("compose");
        assert_eq!(results[0].params["budget"]["maxInputTokens"], json!(8192));
        assert_eq!(results[0].params["budget"]["source"], json!("reported"));
    }

    #[tokio::test]
    async fn warns_when_more_than_four_references_are_used() {
        let backend = Arc::new(FakeBackend::new(vec![generation_json("a red fox")]));
        let manager = manager(backend);
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };
        let mut input = compose_input();
        input.references = (0..5)
            .map(|index| reference(&format!("asset-{index}"), None, "subject"))
            .collect();

        let results = pipeline.compose(&input, None).await.expect("compose");
        assert!(
            results[0]
                .warnings
                .iter()
                .any(|warning| warning.code == "too_many_references")
        );
    }

    #[test]
    fn multiple_descriptions_are_trimmed_against_the_input_budget() {
        let mut input = compose_input();
        input.brief = Some("short brief".into());
        input.references = (0..4)
            .map(|index| reference(&format!("asset-{index}"), None, &"a".repeat(200)))
            .collect();
        let mut small = spec();
        small.limits.max_input_tokens = 40;

        let messages = super::compose_messages(&small, &input);
        let user = match &messages[1].content[0] {
            super::ContentPart::Text(text) => text,
            _ => panic!("expected text"),
        };
        assert!(user.contains("short brief"));
        assert!(!user.contains("References:"));
    }

    #[tokio::test]
    #[ignore = "requires LM Studio with lfm3-vl-2b and gemma4-12b"]
    async fn real_models_describe_and_compose() {
        let endpoint =
            std::env::var("LM_STUDIO_URL").unwrap_or_else(|_| "http://127.0.0.1:1234".into());
        let client = crate::runtime::client::LmStudioClient::new(endpoint, None).expect("client");
        if client.list_models().await.is_err() {
            panic!("LM Studio is not reachable; start the local server first");
        }
        let manager = crate::runtime::manager::ModelManager::new(
            Arc::new(client),
            None,
            ManagerConfig {
                roles: RoleConfig::default(),
                keep_warm: KeepWarm::Both,
                ttl: Duration::from_secs(900),
                describer_concurrency: 1,
            },
        );
        let database = database();
        let pipeline = Pipeline {
            manager: &manager,
            database: &database,
        };

        let described = pipeline
            .describe(&DescribeInput {
                tag: None,
                asset_id: None,
                edit_id: None,
                asset_hash: "real-smoke".into(),
                mode: crate::commands::pipeline::DescribeMode::Image,
                frames: vec![frame()],
                hint: Some("A simple test image".into()),
                force_refresh: true,
                system_prompt: None,
            })
            .await
            .expect("real describe");
        assert!(!described.text.trim().is_empty());

        let preset: serde_json::Value =
            serde_json::from_str(include_str!("../presets/mmh3.json")).expect("preset file");
        let spec = crate::preset::parse(&preset["document"]).expect("preset parse");
        let mut input = compose_input();
        input.spec = spec;
        input.description = Some(described.text);
        input.brief = None;
        let generations = pipeline.compose(&input, None).await.expect("real compose");
        assert!(!generations[0].prompt.trim().is_empty());
        assert!(!generations[0].model.is_empty());
    }
}
