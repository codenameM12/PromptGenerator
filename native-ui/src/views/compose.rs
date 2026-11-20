use egui::Ui;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::backend::{Backend, ComposeReference, ComposeRequest, DescribeRequest};
use crate::errors::error_text_of;
use crate::import;
use crate::media_access::AllowedRoots;
use crate::shortcuts::{self, FocusContext, KeyModifiers, Shortcut};
use crate::theme::Theme;
use crate::thumbnails::{PICKER_THUMB, ThumbnailCache};
use crate::view_model::{ComposeMode, ViewModel};

use std::collections::BTreeMap;

/// A call sink receives `(command name, JSON arguments)` in order and returns
/// the scripted reply. The egui shell maps it onto `Backend`; the differential
/// harness supplies a `RecordingBackend` so the exact command stream can be
/// compared with the TypeScript source.
pub type CallSink<'a> = &'a mut dyn FnMut(&str, Value) -> Result<Value, String>;

// ---------------------------------------------------------------------------
// View-model state (5.1). Kept here because view_model.rs is not owned by this
// wave; the egui shell mirrors the subset it needs into `ViewModel`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceSelection {
    pub asset_id: String,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetView {
    pub id: String,
    pub display_name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_path: Option<String>,
    pub edit_id: Option<String>,
    /// Output of the latest edit; Compose shows and uses this instead of the
    /// original image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescriptionView {
    pub asset_id: String,
    pub text: String,
    pub draft: String,
    pub editing: bool,
}

/// A named set of images added from the Characters tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedSet {
    pub id: String,
    pub name: String,
    pub description: String,
    pub send_description: bool,
    /// Media asset ids of the images loaded from this set.
    pub asset_ids: Vec<String>,
    /// True when no defaults exist and one random image was picked.
    pub random: bool,
    /// The set's image id behind that random pick (avoided when shuffling).
    pub last_image: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeFlow {
    pub mode: String,
    pub assets: Vec<AssetView>,
    pub selection: Vec<ReferenceSelection>,
    pub preset_id: String,
    pub presets: Vec<String>,
    pub text_only: bool,
    pub brief: String,
    pub hint: String,
    pub language: String,
    pub variations: i64,
    pub seed: String,
    pub bypass_cache: bool,
    pub send_images: bool,
    pub locked: String,
    pub global_guidance: String,
    pub run_guidance: String,
    pub preset_guidance: String,
    pub prompt_limit: Option<i64>,
    /// Labelled inputs of the preset (key, label, placeholder) and their values.
    pub brief_fields: Vec<(String, String, String)>,
    /// Terms picked from the vocabulary list; sent with the brief.
    pub selected_terms: Vec<String>,
    pub field_values: std::collections::HashMap<String, String>,
    pub descriptions: Vec<DescriptionView>,
    pub refine_source_id: String,
    pub refine_existing: String,
    pub refine_instruction: String,
    pub refine_parent_id: Option<String>,
    pub sources: Vec<(String, String, String)>,
    /// Short generated titles keyed by generation id (Refine source list).
    pub source_titles: std::collections::HashMap<String, String>,
    pub results: Vec<String>,
    pub result_index: usize,
    pub prompt_draft: String,
    pub editing_prompt: bool,
    pub copied: Option<String>,
    pub error: Option<String>,
    pub busy: bool,
    pub busy_stage: Option<String>,
    pub batch_asset_ids: Vec<String>,
    pub batch_job_id: Option<String>,
    pub effective_guidance: String,
    pub conflicts: Vec<String>,
    /// Mirrors the presets_get effect's dependency: it only fires when the
    /// selected preset id changes, not on every list refresh.
    pub loaded_preset_id: Option<String>,
    /// Full generation records for the current run, used by the result card
    /// (negative, params, warnings, seed, model, cached). The recorded snapshot
    /// intentionally keeps observing prompts only.
    #[serde(default)]
    pub result_meta: Vec<Value>,
    /// Set once the shell has issued the mount refresh, so the preset list,
    /// library sources and preset guidance load against the live backend.
    #[serde(default)]
    pub mounted: bool,
    /// Last `ViewModel::edits_version` this flow loaded, to reload after edits.
    pub seen_edits: u64,
    /// Sets added from the Characters tab.
    #[serde(default)]
    pub sets: Vec<LoadedSet>,
    /// (id, name) of every set that can be added.
    #[serde(default)]
    pub available_sets: Vec<(String, String)>,
    /// Assets whose description/role is NOT sent (the default is to send it).
    #[serde(default)]
    pub no_send: Vec<String>,
    /// Role label of an asset loaded from a set.
    #[serde(default)]
    pub roles: std::collections::HashMap<String, String>,
    /// Authored descriptions of set images; restored after a media edit.
    #[serde(default)]
    pub authored: std::collections::HashMap<String, String>,
    /// Reference slots of the selected preset (0 or None: not limited).
    #[serde(default)]
    pub reference_slots: Option<i64>,
    /// Last `ViewModel::characters_version` the set list was fetched for.
    #[serde(default)]
    pub seen_characters: Option<u64>,
    /// Set when media was added, so the shell reloads the Media tab.
    #[serde(default)]
    pub media_dirty: bool,
}

impl Default for ComposeFlow {
    fn default() -> Self {
        Self {
            mode: "compose".into(),
            assets: Vec::new(),
            selection: Vec::new(),
            preset_id: String::new(),
            presets: Vec::new(),
            text_only: false,
            brief: String::new(),
            hint: String::new(),
            language: String::new(),
            variations: 1,
            seed: String::new(),
            bypass_cache: false,
            send_images: false,
            locked: String::new(),
            global_guidance: String::new(),
            run_guidance: String::new(),
            preset_guidance: String::new(),
            prompt_limit: None,
            brief_fields: Vec::new(),
            selected_terms: Vec::new(),
            sets: Vec::new(),
            available_sets: Vec::new(),
            no_send: Vec::new(),
            roles: std::collections::HashMap::new(),
            authored: std::collections::HashMap::new(),
            reference_slots: None,
            media_dirty: false,
            seen_characters: None,
            field_values: std::collections::HashMap::new(),
            descriptions: Vec::new(),
            refine_source_id: String::new(),
            refine_existing: String::new(),
            refine_instruction: String::new(),
            refine_parent_id: None,
            sources: Vec::new(),
            source_titles: std::collections::HashMap::new(),
            results: Vec::new(),
            result_index: 0,
            prompt_draft: String::new(),
            editing_prompt: false,
            copied: None,
            error: None,
            busy: false,
            busy_stage: None,
            batch_asset_ids: Vec::new(),
            batch_job_id: None,
            effective_guidance: String::new(),
            conflicts: Vec::new(),
            loaded_preset_id: None,
            result_meta: Vec::new(),
            mounted: false,
            seen_edits: 0,
        }
    }
}

/// The observable snapshot recorded by both implementations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeSnapshot {
    pub mode: String,
    pub selection: Vec<ReferenceSelection>,
    pub text_only: bool,
    pub brief: String,
    pub hint: String,
    pub global_guidance: String,
    pub run_guidance: String,
    pub effective_guidance: String,
    pub conflicts: Vec<String>,
    pub locked: String,
    pub language: String,
    pub variations: i64,
    pub seed: String,
    pub bypass_cache: bool,
    pub descriptions: Vec<(String, String)>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub busy: bool,
    pub batch_asset_ids: Vec<String>,
    pub batch_job_id: Option<String>,
    pub copied: Option<String>,
}

impl ComposeFlow {
    pub fn snapshot(&self) -> ComposeSnapshot {
        ComposeSnapshot {
            mode: self.mode.clone(),
            selection: self.selection.clone(),
            text_only: self.text_only,
            brief: self.brief.clone(),
            hint: self.hint.clone(),
            global_guidance: self.global_guidance.clone(),
            run_guidance: self.run_guidance.clone(),
            effective_guidance: self.effective_guidance.clone(),
            conflicts: self.conflicts.clone(),
            locked: self.locked.clone(),
            language: self.language.clone(),
            variations: self.variations,
            seed: self.seed.clone(),
            bypass_cache: self.bypass_cache,
            descriptions: self
                .selection
                .iter()
                .filter_map(|reference| {
                    self.descriptions
                        .iter()
                        .find(|entry| entry.asset_id == reference.asset_id)
                        .map(|entry| (entry.asset_id.clone(), entry.text.clone()))
                })
                .collect(),
            result: self.results.get(self.result_index).cloned(),
            error: self.error.clone(),
            busy: self.busy,
            batch_asset_ids: self.batch_asset_ids.clone(),
            batch_job_id: self.batch_job_id.clone(),
            copied: self.copied.clone(),
        }
    }

    fn uses_media(&self) -> bool {
        !self.text_only && !self.selection.is_empty()
    }

    fn all_described(&self) -> bool {
        !self.selection.is_empty()
            && self
                .selection
                .iter()
                .all(|reference| self.description(reference.asset_id.as_str()).is_some())
    }

    fn description(&self, asset_id: &str) -> Option<&DescriptionView> {
        self.descriptions
            .iter()
            .find(|entry| entry.asset_id == asset_id)
    }

    /// Overall descriptions of added sets, tied to the image tags they cover.
    /// Wording is neutral on purpose: only the user's own names and text.
    pub fn reference_notes(&self) -> String {
        let lines: Vec<String> = self
            .sets
            .iter()
            .filter(|set| set.send_description && !set.description.trim().is_empty())
            .filter_map(|set| {
                let tags: Vec<String> = set
                    .asset_ids
                    .iter()
                    .filter_map(|asset_id| {
                        self.selection
                            .iter()
                            .position(|reference| &reference.asset_id == asset_id)
                            .map(|index| format!("<image{}>", index + 1))
                    })
                    .collect();
                (!tags.is_empty()).then(|| {
                    format!("{} ({}): {}", tags.join(", "), set.name, set.description.trim())
                })
            })
            .collect();
        if lines.is_empty() {
            String::new()
        } else {
            format!("Reference notes:\n{}", lines.join("\n"))
        }
    }

    /// The text sent for one image: its role label and description, unless the
    /// user switched sending off for it.
    pub fn sent_description(&self, asset_id: &str) -> Option<String> {
        if self.no_send.iter().any(|id| id == asset_id) {
            return None;
        }
        let role = self.roles.get(asset_id).map(|text| text.trim()).unwrap_or("");
        let text = self
            .description(asset_id)
            .map(|entry| entry.draft.trim())
            .unwrap_or("");
        match (role.is_empty(), text.is_empty()) {
            (true, true) => None,
            (false, true) => Some(role.to_string()),
            (true, false) => Some(text.to_string()),
            (false, false) => Some(format!("{role}: {text}")),
        }
    }

    /// The brief as sent to the composer: the typed brief plus the chosen terms.
    pub fn effective_brief(&self) -> String {
        let mut brief = self.brief.trim().to_string();
        if !self.selected_terms.is_empty() {
            if !brief.is_empty() {
                brief.push_str("\n\n");
            }
            brief.push_str("Style terms: ");
            brief.push_str(&self.selected_terms.join(", "));
        }
        let notes = self.reference_notes();
        if !notes.is_empty() {
            if !brief.is_empty() {
                brief.push_str("\n\n");
            }
            brief.push_str(&notes);
        }
        brief
    }

    fn refresh_guidance(&mut self) {
        self.effective_guidance = merged_guidance(&[
            self.global_guidance.clone(),
            self.preset_guidance.clone(),
            self.run_guidance.clone(),
        ]);
        self.conflicts = guidance_conflicts(&[
            ("Global guidance".to_string(), self.global_guidance.clone()),
            ("Preset guidance".to_string(), self.preset_guidance.clone()),
            ("Run guidance".to_string(), self.run_guidance.clone()),
        ]);
    }

    fn request_guidance(&self) -> Value {
        json!({
            "global": non_empty(&self.global_guidance),
            "preset": Value::Null,
            "run": non_empty(&self.run_guidance),
        })
    }
}

fn non_empty(value: &str) -> Value {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Value::Null
    } else {
        Value::String(trimmed.to_string())
    }
}

/// `pipeline_describe` returns one `Description` object; the typed adapter
/// returns a list. Accept both so a live run and a replayed run agree.
fn describe_text(value: &Value) -> String {
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .find_map(|item| item.get("text").and_then(Value::as_str))
            .unwrap_or_default()
            .to_string();
    }
    value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn split_phrases(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn merged_guidance(parts: &[String]) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    for part in parts {
        let trimmed = part.trim();
        let key = trimmed.to_lowercase();
        if trimmed.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        kept.push(trimmed.to_string());
    }
    kept.join(". ")
}

const CONFLICT_PAIRS: [(&str, &str); 7] = [
    ("cinematic", "flat lighting"),
    ("bright", "dark"),
    ("wide shot", "close-up"),
    ("static camera", "camera movement"),
    ("sharp focus", "blurry"),
    ("color", "black and white"),
    ("minimal", "detailed"),
];

fn guidance_conflicts(levels: &[(String, String)]) -> Vec<String> {
    let active: Vec<(&str, String)> = levels
        .iter()
        .map(|(label, text)| (label.as_str(), text.to_lowercase()))
        .filter(|(_, text)| !text.trim().is_empty())
        .collect();
    let mut messages: Vec<String> = Vec::new();
    for (left, right) in CONFLICT_PAIRS {
        for (a_label, a_text) in &active {
            for (b_label, b_text) in &active {
                if a_label == b_label {
                    continue;
                }
                if a_text.contains(left) && b_text.contains(right) {
                    let message = format!(
                        "{a_label} says \u{201c}{left}\u{201d} while {b_label} says \u{201c}{right}\u{201d}."
                    );
                    if !messages.contains(&message) {
                        messages.push(message);
                    }
                }
            }
        }
    }
    messages
}

fn clamp_weight(value: f64) -> f64 {
    if !value.is_finite() {
        return 1.0;
    }
    value.clamp(0.0, 2.0)
}

/// JS serializes a whole number as `1`, not `1.0`; mirror that so typed JSON
/// comparisons do not report a false difference.
fn json_number(value: f64) -> Value {
    if value.is_finite() && value.fract() == 0.0 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

fn as_array(value: &Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}

fn as_str_vec(value: &Value) -> Vec<String> {
    as_array(value)
        .iter()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

fn parse_generations(value: &Value) -> Vec<String> {
    as_array(value)
        .iter()
        .filter_map(|item| {
            item.get("prompt")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Actions — one dispatch entry point for buttons, shortcuts and the harness.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ComposeAction {
    Mount,
    SetMode {
        mode: String,
    },
    ToggleTextOnly {
        value: bool,
    },
    SetBrief {
        value: String,
    },
    SetHint {
        value: String,
    },
    SetGlobalGuidance {
        value: String,
    },
    SetRunGuidance {
        value: String,
    },
    SetLocked {
        value: String,
    },
    SetLanguage {
        value: String,
    },
    SetVariations {
        value: i64,
    },
    SetSeed {
        value: String,
    },
    SetBypassCache {
        value: bool,
    },
    ToggleAsset {
        asset_id: String,
    },
    SetWeight {
        asset_id: String,
        value: f64,
    },
    ClearSelection,
    SelectAll,
    AddSet {
        id: String,
    },
    RemoveSet {
        id: String,
    },
    ShuffleSet {
        id: String,
    },
    SetSetDescription {
        id: String,
        value: bool,
    },
    SetAssetDescription {
        asset_id: String,
        value: bool,
    },
    DescribeAsset {
        asset_id: String,
    },
    Describe {
        force: bool,
    },
    Compose,
    /// The TS generator records the Refine button as its own action; the
    /// flow's `mode` already selects the refine branch of compose.
    Refine,
    Regenerate {
        seed: i64,
    },
    /// Opens the description editor; the snapshot only observes committed text.
    EditDescription {
        asset_id: String,
    },
    SetDescriptionDraft {
        asset_id: String,
        value: String,
    },
    /// Result card editing: the prompt is editable before copying.
    SetPromptDraft {
        value: String,
    },
    EditPrompt,
    SaveDescription {
        asset_id: String,
    },
    ChooseRefineSource {
        id: String,
    },
    SetRefineExisting {
        value: String,
    },
    SetRefineInstruction {
        value: String,
    },
    BatchSelectAll,
    BatchQueue,
    RemoveAsset {
        asset_id: String,
    },
    Cancel,
    Shortcut {
        key: String,
        ctrl: bool,
        meta: bool,
        shift: bool,
        modal: bool,
        text_focused: bool,
    },
    Reset,
}

pub fn dispatch(flow: &mut ComposeFlow, action: ComposeAction, call: CallSink) {
    match action {
        ComposeAction::Mount => load_lists(flow, call),
        ComposeAction::SetMode { mode } => {
            flow.mode = mode;
            flow.error = None;
        }
        ComposeAction::ToggleTextOnly { value } => flow.text_only = value,
        ComposeAction::SetBrief { value } => flow.brief = value,
        ComposeAction::SetHint { value } => flow.hint = value,
        ComposeAction::SetGlobalGuidance { value } => {
            flow.global_guidance = value;
            flow.refresh_guidance();
        }
        ComposeAction::SetRunGuidance { value } => {
            flow.run_guidance = value;
            flow.refresh_guidance();
        }
        ComposeAction::SetLocked { value } => flow.locked = value,
        ComposeAction::SetLanguage { value } => flow.language = value,
        ComposeAction::SetVariations { value } => flow.variations = value,
        ComposeAction::SetSeed { value } => flow.seed = value,
        ComposeAction::SetBypassCache { value } => flow.bypass_cache = value,
        ComposeAction::ToggleAsset { asset_id } => {
            if flow.selection.iter().any(|item| item.asset_id == asset_id) {
                flow.selection.retain(|item| item.asset_id != asset_id);
            } else {
                flow.selection.push(ReferenceSelection {
                    asset_id,
                    weight: 1.0,
                });
            }
        }
        ComposeAction::SetWeight { asset_id, value } => {
            let weight = clamp_weight(value);
            if let Some(reference) = flow
                .selection
                .iter_mut()
                .find(|item| item.asset_id == asset_id)
            {
                reference.weight = weight;
            }
        }
        ComposeAction::ClearSelection => flow.selection.clear(),
        ComposeAction::SelectAll => {
            let weights: BTreeMap<String, f64> = flow
                .selection
                .iter()
                .map(|item| (item.asset_id.clone(), item.weight))
                .collect();
            flow.selection = flow
                .assets
                .iter()
                .map(|asset| ReferenceSelection {
                    asset_id: asset.id.clone(),
                    weight: weights.get(&asset.id).copied().unwrap_or(1.0),
                })
                .collect();
        }
        ComposeAction::Describe { force } => run_describe_flow(flow, force, call),
        ComposeAction::DescribeAsset { asset_id } => {
            run_describe_filtered(flow, true, Some(asset_id.as_str()), call)
        }
        ComposeAction::AddSet { id } => add_set(flow, &id, None, call),
        ComposeAction::ShuffleSet { id } => {
            let avoid = flow
                .sets
                .iter()
                .find(|set| set.id == id)
                .and_then(|set| set.last_image.clone());
            add_set(flow, &id, avoid, call)
        }
        ComposeAction::RemoveSet { id } => {
            if let Some(position) = flow.sets.iter().position(|set| set.id == id) {
                let removed = flow.sets.remove(position);
                // Only the Compose reference list changes; media stays.
                flow.selection
                    .retain(|reference| !removed.asset_ids.contains(&reference.asset_id));
            }
        }
        ComposeAction::SetSetDescription { id, value } => {
            if let Some(set) = flow.sets.iter_mut().find(|set| set.id == id) {
                set.send_description = value;
            }
        }
        ComposeAction::SetAssetDescription { asset_id, value } => {
            flow.no_send.retain(|id| id != &asset_id);
            if !value {
                flow.no_send.push(asset_id);
            }
        }
        ComposeAction::Compose | ComposeAction::Refine => {
            run_compose_flow(flow, None, None, None, call)
        }
        ComposeAction::Regenerate { seed } => {
            run_compose_flow(flow, Some(true), Some(seed), None, call)
        }
        ComposeAction::EditDescription { asset_id } => {
            if let Some(entry) = flow
                .descriptions
                .iter_mut()
                .find(|item| item.asset_id == asset_id)
            {
                entry.editing = true;
            }
        }
        ComposeAction::SetDescriptionDraft { asset_id, value } => {
            if let Some(entry) = flow
                .descriptions
                .iter_mut()
                .find(|item| item.asset_id == asset_id)
            {
                entry.draft = value;
                entry.editing = true;
            }
        }
        ComposeAction::EditPrompt => {
            flow.prompt_draft = flow
                .results
                .get(flow.result_index)
                .cloned()
                .unwrap_or_default();
            flow.editing_prompt = true;
        }
        ComposeAction::SetPromptDraft { value } => {
            flow.prompt_draft = value;
            flow.editing_prompt = true;
        }
        ComposeAction::SaveDescription { asset_id } => {
            let text = flow
                .descriptions
                .iter()
                .find(|item| item.asset_id == asset_id)
                .map(|item| item.draft.trim().to_string())
                .unwrap_or_default();
            if text.is_empty() {
                flow.error =
                    Some("A description cannot be empty; cancel to keep the previous text".into());
                return;
            }
            if let Some(entry) = flow
                .descriptions
                .iter_mut()
                .find(|item| item.asset_id == asset_id)
            {
                entry.text = text.clone();
                entry.draft = text;
                entry.editing = false;
            }
            flow.error = None;
        }
        ComposeAction::ChooseRefineSource { id } => {
            flow.refine_source_id = id.clone();
            if let Some((_, prompt, preset_id)) = flow
                .sources
                .iter()
                .find(|(source_id, _, _)| source_id == &id)
                .cloned()
            {
                flow.refine_existing = prompt;
                flow.refine_parent_id = Some(id);
                flow.refine_instruction.clear();
                if flow.presets.contains(&preset_id) {
                    flow.preset_id = preset_id;
                }
            }
            flow.error = None;
        }
        ComposeAction::SetRefineExisting { value } => flow.refine_existing = value,
        ComposeAction::SetRefineInstruction { value } => flow.refine_instruction = value,
        ComposeAction::BatchSelectAll => {
            flow.batch_asset_ids = flow.assets.iter().map(|asset| asset.id.clone()).collect()
        }
        ComposeAction::BatchQueue => queue_batch(flow, call),
        ComposeAction::RemoveAsset { asset_id } => {
            let args = json!({ "request": { "generationIds": [], "assetIds": [asset_id] } });
            match call("library_trash", args) {
                Ok(_) => load_lists(flow, call),
                Err(error) => flow.error = Some(error),
            }
        }
        ComposeAction::Cancel => {
            flow.busy_stage = Some("Cancelling\u{2026}".into());
            match call("pipeline_cancel", json!({})) {
                Ok(_) => {}
                Err(_) => {} // source swallows the rejection
            }
        }
        ComposeAction::Shortcut {
            key,
            ctrl,
            meta,
            shift,
            modal,
            text_focused,
        } => {
            let mods = KeyModifiers { ctrl, meta, shift };
            let focus = FocusContext {
                modal_open: modal,
                text_input_focused: text_focused,
            };
            let resolved =
                shortcuts::resolve_shortcut(&key, mods, focus, flow.busy, flow.mode == "describe");
            match resolved {
                Some(Shortcut::Describe) => run_describe_flow(flow, false, call),
                Some(Shortcut::Cancel) => {
                    flow.busy_stage = Some("Cancelling\u{2026}".into());
                    let _ = call("pipeline_cancel", json!({}));
                }
                Some(Shortcut::Compose) => {
                    if flow.mode == "batch" {
                        return;
                    }
                    run_compose_flow(flow, None, None, None, call);
                }
                None => {}
            }
        }
        ComposeAction::Reset => {
            flow.results.clear();
            flow.result_index = 0;
            flow.prompt_draft.clear();
            flow.editing_prompt = false;
            flow.copied = None;
            flow.descriptions.clear();
            flow.brief.clear();
            flow.selected_terms.clear();
            flow.hint.clear();
            flow.locked.clear();
            flow.run_guidance.clear();
            flow.refine_source_id.clear();
            flow.refine_existing.clear();
            flow.refine_instruction.clear();
            flow.refine_parent_id = None;
            flow.selection.clear();
            flow.batch_asset_ids.clear();
            flow.send_images = false;
            flow.error = None;
            flow.refresh_guidance();
        }
    }
}

/// Mirrors `loadLists` in ComposePanel.tsx, including call order.
pub fn load_lists(flow: &mut ComposeFlow, call: CallSink) {
    let assets_value = match call("assets_list", json!({})) {
        Ok(value) => value,
        Err(error) => {
            flow.error = Some(error);
            return;
        }
    };
    let assets = as_array(&assets_value);
    let presets_value = call("presets_list", json!({})).unwrap_or_else(|_| json!([]));
    let _datasets = call("datasets_list", json!({})).unwrap_or_else(|_| json!([]));
    let library = call(
        "library_query",
        json!({
            "query": {
                "text": null, "presetId": null, "kind": null, "model": null,
                "favorite": null, "status": null, "tags": [], "collectionId": null,
                "from": null, "to": null, "ids": [], "offset": 0, "limit": 50,
                "sort": "newest"
            }
        }),
    )
    .unwrap_or_else(|_| json!({ "items": [] }));

    let previous_edits: Vec<(String, Option<String>)> = flow
        .assets
        .iter()
        .map(|asset| (asset.id.clone(), asset.edit_id.clone()))
        .collect();
    flow.assets = assets
        .iter()
        .filter_map(|asset| {
            Some(AssetView {
                id: asset.get("id")?.as_str()?.to_string(),
                display_name: asset.get("displayName")?.as_str()?.to_string(),
                kind: asset.get("kind")?.as_str()?.to_string(),
                thumbnail_path: asset
                    .get("thumbnailPath")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                edit_id: None,
                edited_path: None,
            })
        })
        .collect();
    flow.presets = as_array(&presets_value)
        .iter()
        .filter_map(|preset| preset.get("id")?.as_str().map(str::to_string))
        .collect();
    flow.sources = as_array(library.get("items").unwrap_or(&json!([])))
        .iter()
        .filter_map(|item| {
            Some((
                item.get("id")?.as_str()?.to_string(),
                item.get("prompt")?.as_str()?.to_string(),
                item.get("presetId")?.as_str()?.to_string(),
            ))
        })
        .collect();

    flow.source_titles = as_array(library.get("items").unwrap_or(&json!([])))
        .iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?.to_string();
            let title = item.get("title").and_then(Value::as_str).unwrap_or("");
            let title = if title.is_empty() {
                // Not titled yet: first few words of the prompt.
                let prompt = item.get("prompt").and_then(Value::as_str).unwrap_or("");
                prompt.split_whitespace().take(5).collect::<Vec<_>>().join(" ")
            } else {
                title.to_string()
            };
            Some((id, title))
        })
        .collect();

    let ids: Vec<Value> = flow.assets.iter().map(|asset| json!(asset.id)).collect();
    let latest = call("edits_latest", json!({ "assetIds": ids })).unwrap_or_else(|_| json!([]));
    for edit in as_array(&latest) {
        if let (Some(asset_id), Some(edit_id)) = (
            edit.get("assetId").and_then(Value::as_str),
            edit.get("editId").and_then(Value::as_str),
        ) && let Some(asset) = flow.assets.iter_mut().find(|item| item.id == asset_id)
        {
            asset.edit_id = Some(edit_id.to_string());
            asset.edited_path = edit
                .get("outputPath")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }
    // A new edit means the old description (of the original image) no longer
    // applies; drop it so Describe runs again on the edited image.
    let changed: Vec<String> = flow
        .assets
        .iter()
        .filter(|asset| {
            previous_edits
                .iter()
                .find(|(id, _)| id == &asset.id)
                .is_some_and(|(_, old)| old != &asset.edit_id)
        })
        .map(|asset| asset.id.clone())
        .collect();
    if !changed.is_empty() {
        flow.descriptions
            .retain(|entry| !changed.contains(&entry.asset_id));
    }
    call("models_status", json!({})).ok();

    let available: Vec<String> = flow.assets.iter().map(|asset| asset.id.clone()).collect();
    flow.selection
        .retain(|reference| available.contains(&reference.asset_id));
    if flow.selection.is_empty()
        && let Some(first) = flow.assets.first()
    {
        flow.selection.push(ReferenceSelection {
            asset_id: first.id.clone(),
            weight: 1.0,
        });
    }
    flow.batch_asset_ids.retain(|id| available.contains(id));
    // Descriptions and results of media that no longer exists are stale.
    flow.descriptions
        .retain(|entry| available.contains(&entry.asset_id));
    // Authored descriptions of set images survive a media edit.
    let restore: Vec<(String, String)> = flow
        .authored
        .iter()
        .filter(|(id, _)| available.contains(id) && flow.description(id).is_none())
        .map(|(id, text)| (id.clone(), text.clone()))
        .collect();
    for (asset_id, text) in restore {
        flow.descriptions.push(DescriptionView {
            asset_id,
            text: text.clone(),
            draft: text,
            editing: false,
        });
    }
    if flow.preset_id.is_empty()
        && let Some(first) = flow.presets.first()
    {
        flow.preset_id = first.clone();
    }
    if !flow.preset_id.is_empty()
        && flow.loaded_preset_id.as_deref() != Some(flow.preset_id.as_str())
    {
        preset_guidance(flow, call);
    }
    flow.error = None;
    flow.refresh_guidance();
}

fn run_describe_flow(flow: &mut ComposeFlow, force: bool, call: CallSink) {
    run_describe_filtered(flow, force, None, call)
}

fn run_describe_filtered(
    flow: &mut ComposeFlow,
    force: bool,
    only: Option<&str>,
    call: CallSink,
) {
    if flow.selection.is_empty() {
        flow.error = Some("Choose media to describe".into());
        return;
    }
    let targets: Vec<AssetView> = flow
        .selection
        .iter()
        .filter_map(|reference| {
            flow.assets
                .iter()
                .find(|asset| asset.id == reference.asset_id)
                .filter(|asset| force || flow.description(&asset.id).is_none())
                .filter(|asset| only.is_none_or(|id| id == asset.id))
                .cloned()
        })
        .collect();
    if targets.is_empty() {
        flow.error = Some("Every selected asset already has a description".into());
        return;
    }
    flow.busy = true;
    flow.busy_stage = Some(if targets.len() > 1 {
        if force {
            format!("Re-describing {} media items", targets.len())
        } else {
            format!("Describing {} media items", targets.len())
        }
    } else if force {
        "Re-describing media".into()
    } else {
        "Describing media".into()
    });
    flow.error = None;
    for asset in &targets {
        // Neutral tag by position in the selection, so descriptions (and the
        // prompts built from them) never need the file name.
        let position = flow
            .selection
            .iter()
            .position(|reference| reference.asset_id == asset.id)
            .map(|index| index + 1)
            .unwrap_or(1);
        let args = json!({
            "request": {
                "assetId": asset.id,
                "editId": asset.edit_id.clone().map(Value::String).unwrap_or(Value::Null),
                "framePaths": [],
                "mode": if asset.kind == "video" { "video" } else { "image" },
                "hint": non_empty(&flow.hint),
                "forceRefresh": force,
                "tag": format!("<image{position}>")
            }
        });
        match call("pipeline_describe", args) {
            Ok(value) => {
                let text = describe_text(&value);
                if text.trim().is_empty() {
                    flow.error = Some(format!(
                        "The describer returned no text for {}",
                        asset.display_name
                    ));
                    break;
                }
                flow.descriptions.retain(|entry| entry.asset_id != asset.id);
                flow.descriptions.push(DescriptionView {
                    asset_id: asset.id.clone(),
                    text: text.clone(),
                    draft: text,
                    editing: false,
                });
            }
            Err(error) => {
                flow.error = Some(error);
                break;
            }
        }
    }
    flow.busy = false;
    flow.busy_stage = None;
}

/// Loads a set's images into Media and the Compose reference list.
fn add_set(flow: &mut ComposeFlow, id: &str, avoid: Option<String>, call: CallSink) {
    if avoid.is_none() && flow.sets.iter().any(|set| set.id == id) {
        return;
    }
    let loaded = match call(
        "characters_load",
        json!({ "id": id, "avoid": avoid }),
    ) {
        Ok(value) => value,
        Err(error) => {
            flow.error = Some(error);
            return;
        }
    };
    let images = as_array(loaded.get("images").unwrap_or(&json!([])));
    if images.is_empty() {
        flow.error = Some("It has no images yet. Add some in the Characters tab.".into());
        return;
    }
    // Shuffle replaces the previous pick.
    if let Some(position) = flow.sets.iter().position(|set| set.id == id) {
        let old = flow.sets.remove(position);
        flow.selection
            .retain(|reference| !old.asset_ids.contains(&reference.asset_id));
    }
    let was_empty = flow.selection.is_empty();
    load_lists(flow, call);
    if was_empty {
        // load_lists auto-selects the first media item; this run is about the set.
        flow.selection.clear();
    }
    let mut asset_ids = Vec::new();
    let mut last_image = None;
    for image in &images {
        let Some(asset_id) = image.get("assetId").and_then(Value::as_str) else {
            continue;
        };
        asset_ids.push(asset_id.to_string());
        last_image = image.get("id").and_then(Value::as_str).map(str::to_string);
        if !flow.selection.iter().any(|item| item.asset_id == asset_id) {
            flow.selection.push(ReferenceSelection {
                asset_id: asset_id.to_string(),
                weight: 1.0,
            });
        }
        let role = image.get("role").and_then(Value::as_str).unwrap_or("").trim();
        if !role.is_empty() {
            flow.roles.insert(asset_id.to_string(), role.to_string());
        }
        let text = image
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if !text.is_empty() {
            flow.authored.insert(asset_id.to_string(), text.clone());
            if flow.description(asset_id).is_none() {
                flow.descriptions.push(DescriptionView {
                    asset_id: asset_id.to_string(),
                    text: text.clone(),
                    draft: text,
                    editing: false,
                });
            }
        }
    }
    let random = !images
        .iter()
        .any(|image| image.get("isDefault").and_then(Value::as_bool).unwrap_or(false));
    flow.sets.push(LoadedSet {
        id: id.to_string(),
        name: loaded.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        description: loaded
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        send_description: true,
        asset_ids,
        random,
        last_image: if random { last_image } else { None },
    });
    flow.media_dirty = true;
    flow.error = None;
}

fn run_compose_flow(
    flow: &mut ComposeFlow,
    bypass: Option<bool>,
    seed: Option<i64>,
    _instruction: Option<String>,
    call: CallSink,
) {
    if flow.preset_id.is_empty() {
        flow.error = Some("Choose a preset first".into());
        return;
    }
    let refine = flow.mode == "refine";
    if refine {
        if flow.refine_existing.trim().is_empty() {
            flow.error = Some("Enter the existing prompt to refine".into());
            return;
        }
        if flow.refine_instruction.trim().is_empty() {
            flow.error = Some("Enter a refinement instruction".into());
            return;
        }
    } else if !flow.uses_media() && flow.effective_brief().trim().is_empty() {
        flow.error = Some("Enter a prompt brief or choose media".into());
        return;
    }
    if !refine
        && flow.uses_media()
        && let Some(slots) = flow.reference_slots.filter(|slots| *slots > 0)
        && flow.selection.len() as i64 > slots
    {
        flow.error = Some(format!(
            "This preset takes at most {slots} reference image(s); {} are selected. Untick some.",
            flow.selection.len()
        ));
        return;
    }

    flow.busy = true;
    flow.busy_stage = Some("Composing prompt".into());
    flow.error = None;
    flow.results.clear();
    flow.copied = None;

    let references: Vec<Value> = if refine || !flow.uses_media() {
        Vec::new()
    } else {
        flow.selection
            .iter()
            .map(|reference| {
                let asset = flow
                    .assets
                    .iter()
                    .find(|item| item.id == reference.asset_id);
                let description = flow.sent_description(&reference.asset_id);
                json!({
                    "assetId": reference.asset_id,
                    "editId": asset.and_then(|item| item.edit_id.clone())
                        .map(Value::String).unwrap_or(Value::Null),
                    "weight": json_number(reference.weight),
                    "name": asset.map(|item| Value::String(item.display_name.clone()))
                        .unwrap_or(Value::Null),
                    "description": description.map(Value::String)
                        .unwrap_or(Value::Null)
                })
            })
            .collect()
    };
    let primary = references.first().cloned();
    let args = json!({
        "request": {
            "presetId": flow.preset_id,
            "assetId": primary.as_ref().and_then(|value| value.get("assetId")).cloned()
                .unwrap_or(Value::Null),
            "editId": primary.as_ref().and_then(|value| value.get("editId")).cloned()
                .unwrap_or(Value::Null),
            "references": references,
            "parentId": if refine { flow.refine_parent_id.clone().map(Value::String).unwrap_or(Value::Null) } else { Value::Null },
            "description": primary.as_ref().and_then(|value| value.get("description")).cloned()
                .unwrap_or(Value::Null),
            "brief": if refine { Value::Null } else { non_empty(&flow.effective_brief()) },
            "instruction": if refine { Value::String(flow.refine_instruction.clone()) } else { Value::Null },
            "guidance": flow.request_guidance(),
            "locked": split_phrases(&flow.locked),
            "existingPrompt": if refine { Value::String(flow.refine_existing.clone()) } else { Value::Null },
            "seed": seed.map(|value| json!(value))
                .unwrap_or_else(|| {
                    let trimmed = flow.seed.trim();
                    if trimmed.is_empty() { Value::Null }
                    else { trimmed.parse::<f64>().map(json_number).unwrap_or(Value::Null) }
                }),
            "variations": flow.variations.clamp(1, 4),
            "bypassCache": bypass.unwrap_or(flow.bypass_cache),
            "outputLanguage": non_empty(&flow.language),
            "includeImages": flow.uses_media() && flow.send_images,
            "params": {}
        }
    });
    match call("pipeline_compose", args) {
        Ok(value) => {
            let generations = parse_generations(&value);
            if generations.is_empty() {
                flow.error = Some("The composer returned no result".into());
            } else {
                flow.results = generations;
                flow.result_index = 0;
                flow.prompt_draft = flow.results[0].clone();
                flow.result_meta = as_array(&value);
                if !flow.sets.is_empty() {
                    let generation_ids: Vec<Value> = flow
                        .result_meta
                        .iter()
                        .filter_map(|record| record.get("id").cloned())
                        .collect();
                    let set_ids: Vec<Value> =
                        flow.sets.iter().map(|set| json!(set.id)).collect();
                    let _ = call(
                        "characters_link_generations",
                        json!({ "characterIds": set_ids, "generationIds": generation_ids }),
                    );
                }
                load_lists(flow, call);
            }
        }
        Err(error) => flow.error = Some(error),
    }
    flow.busy = false;
    flow.busy_stage = None;
}

fn queue_batch(flow: &mut ComposeFlow, call: CallSink) {
    if flow.preset_id.is_empty() {
        flow.error = Some("Choose a preset first".into());
        return;
    }
    let chosen: Vec<AssetView> = flow
        .assets
        .iter()
        .filter(|asset| flow.batch_asset_ids.contains(&asset.id))
        .cloned()
        .collect();
    if chosen.is_empty() {
        flow.error = Some("Select at least one asset".into());
        return;
    }
    flow.busy = true;
    flow.busy_stage = Some(format!("Describing {} asset(s)", chosen.len()));
    flow.error = None;
    let mut items = Vec::new();
    for asset in &chosen {
        let args = json!({
            "request": {
                "assetId": asset.id,
                "editId": asset.edit_id.clone().map(Value::String).unwrap_or(Value::Null),
                "framePaths": [],
                "mode": if asset.kind == "video" { "video" } else { "image" },
                "hint": non_empty(&flow.hint),
                "forceRefresh": false
            }
        });
        let described = call("pipeline_describe", args);
        if let Err(error) = &described {
            flow.error = Some(format!("Could not describe {}: {error}", asset.display_name));
        }
        if let Ok(value) = described {
            let text = value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            items.push(json!({
                "presetId": flow.preset_id,
                "assetId": asset.id,
                "editId": asset.edit_id.clone().map(Value::String).unwrap_or(Value::Null),
                "parentId": null,
                "description": text,
                "brief": non_empty(&flow.effective_brief()),
                "instruction": null,
                "guidance": flow.request_guidance(),
                "locked": split_phrases(&flow.locked),
                "params": {},
                "existingPrompt": null,
                "seed": null,
                "variations": 1,
                "bypassCache": flow.bypass_cache,
                "outputLanguage": non_empty(&flow.language)
            }));
        }
    }
    if !items.is_empty() {
        let note = format!("Batch: {} asset(s)", items.len());
        let args = json!({
            "request": { "kind": "compose", "items": items, "priority": 0, "note": note }
        });
        match call("jobs_enqueue", args) {
            Ok(value) => {
                if let Some(id) = value.pointer("/job/id").and_then(Value::as_str) {
                    flow.batch_job_id = Some(id.to_string());
                    // The batch-job effect polls `jobs_get` immediately after the id
                    // is set (then every 2s in the source).
                    call("jobs_get", json!({ "id": id })).ok();
                    flow.error = None;
                }
            }
            Err(error) => flow.error = Some(format!("Could not queue the batch: {error}")),
        }
    } else if flow.error.is_none() {
        flow.error = Some("Nothing was queued: no media could be described".into());
    }
    flow.busy = false;
    flow.busy_stage = None;
}

/// Maps the generic call sink onto the existing `Backend` trait for the calls
/// the trait supports. Other commands return a structured "not implemented"
/// error, matching the documented real-adapter gap.
pub fn backend_sink(
    backend: &dyn Backend,
) -> impl FnMut(&str, Value) -> Result<Value, String> + '_ {
    move |method: &str, args: Value| -> Result<Value, String> {
        let request = args.get("request").cloned().unwrap_or(Value::Null);
        // The typed `run_describe`/`run_compose` seams only carry a few fields
        // (no forceRefresh, descriptions, seed, variations, guidance, locks,
        // bypassCache or refine inputs), so a live backend must receive the
        // whole request. The typed adapters below remain as the fallback for
        // backends that do not implement the generic command surface.
        if matches!(method, "pipeline_describe" | "pipeline_compose") {
            match backend.call(method, args.clone()) {
                Ok(value) => return Ok(value),
                Err(error) if error.code != "not_implemented" => {
                    return Err(error_text_of(&error));
                }
                Err(_) => {}
            }
        }
        match method {
            "assets_list" => match backend.list_assets() {
                Ok(rows) => serde_json::to_value(rows).map_err(|e| e.to_string()),
                Err(error) => Err(error_text_of(&error)),
            },
            "assets_import" => {
                let paths: Vec<String> = request
                    .get("paths")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                match backend.import_paths(&paths) {
                    Ok(rows) => serde_json::to_value(rows).map_err(|e| e.to_string()),
                    Err(error) => Err(error_text_of(&error)),
                }
            }
            "pipeline_describe" => {
                let ids = as_str_vec(request.get("assetId").unwrap_or(&Value::Null));
                let ids = if ids.is_empty() {
                    request
                        .get("assetId")
                        .and_then(Value::as_str)
                        .map(|id| vec![id.to_string()])
                        .unwrap_or_default()
                } else {
                    ids
                };
                let described: Vec<Value> = backend
                    .run_describe(DescribeRequest {
                        asset_ids: ids,
                        fields: as_str_vec(request.get("fields").unwrap_or(&json!([]))),
                    })
                    .map_err(|error| error_text_of(&error))?
                    .into_iter()
                    .map(|text| json!({ "text": text }))
                    .collect();
                Ok(json!(described))
            }
            "pipeline_compose" => {
                let references: Vec<ComposeReference> =
                    as_array(request.get("references").unwrap_or(&json!([])))
                        .iter()
                        .filter_map(|item| {
                            Some(ComposeReference {
                                asset_id: item.get("assetId")?.as_str().map(str::to_string),
                                weight: item.get("weight")?.as_f64().unwrap_or(1.0),
                                name: item.get("name")?.as_str().map(str::to_string),
                            })
                        })
                        .collect();
                let composed = backend
                    .run_compose(ComposeRequest {
                        preset_id: request
                            .get("presetId")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        brief: request
                            .get("brief")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        output_language: request
                            .get("outputLanguage")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        references,
                    })
                    .map_err(|error| error_text_of(&error))?;
                serde_json::to_value(composed).map_err(|e| e.to_string())
            }
            "pipeline_cancel" => backend
                .cancel()
                .map(|_| Value::Null)
                .map_err(|error| error_text_of(&error)),
            // Everything not covered by a typed trait method goes through the
            // allowlisted generic seam. `NativeBackend` resolves it in-process;
            // a stub returns a structured "not implemented" error that the
            // shared error helper turns into readable text.
            other => backend
                .call(other, args)
                .map_err(|error| error_text_of(&error)),
        }
    }
}

// ---------------------------------------------------------------------------
// egui shell — drives the same actions as the harness.
// ---------------------------------------------------------------------------

thread_local! {
    static FLOW: std::cell::RefCell<ComposeFlow> = std::cell::RefCell::new(ComposeFlow::default());
}

pub fn ui(
    ui: &mut Ui,
    vm: &mut ViewModel,
    theme: &Theme,
    backend: &dyn Backend,
    access: &AllowedRoots,
    thumbnails: &mut ThumbnailCache,
) {
    let mut sink = backend_sink(backend);
    FLOW.with(|cell| {
        let mut flow = cell.borrow_mut();
        sync_from_vm(&mut flow, vm);
        if !flow.mounted {
            flow.mounted = true;
            // Mount effect: load assets, presets, library sources and the
            // selected preset's guidance from the live backend.
            dispatch(&mut flow, ComposeAction::Mount, &mut sink);
        }
        if flow.mounted && flow.seen_characters != Some(vm.characters_version) {
            // The Characters tab changed (or first visit): refresh the add list.
            flow.seen_characters = Some(vm.characters_version);
            flow.available_sets = sink("characters_list", json!({}))
                .map(|value| {
                    as_array(&value)
                        .iter()
                        .filter_map(|item| {
                            Some((
                                item.get("id")?.as_str()?.to_string(),
                                item.get("name")?.as_str()?.to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        if flow.mounted && flow.seen_edits != vm.edits_version {
            // An edit was applied in Media: reload so Compose shows and uses
            // the edited image instead of the original.
            flow.seen_edits = vm.edits_version;
            dispatch(&mut flow, ComposeAction::Mount, &mut sink);
        }
        if flow.preset_id.is_empty()
            && let Some(first) = flow.presets.first().cloned()
        {
            flow.preset_id = first;
            preset_guidance(&mut flow, &mut sink);
        }

        ui.heading("Compose");
        ui.add_space(theme.space_xs);
        ui.horizontal(|ui| {
            for (id, label) in [
                ("describe", "Describe"),
                ("compose", "Compose"),
                ("refine", "Refine"),
                ("batch", "Batch"),
            ] {
                if ui.selectable_label(flow.mode == id, label).clicked() && flow.mode != id {
                    dispatch(
                        &mut flow,
                        ComposeAction::SetMode { mode: id.into() },
                        &mut sink,
                    );
                }
            }
        });
        ui.add_space(theme.space_sm);

        let pane_height = (ui.available_height() - 6.0).max(200.0);
        ui.horizontal_top(|ui| {
            let panel_width = (ui.available_width() * 0.26).clamp(230.0, 330.0);
            ui.allocate_ui_with_layout(
                egui::vec2(panel_width, pane_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_height(pane_height);
                    draw_media_panel(ui, &mut flow, &mut sink, theme, thumbnails, vm);
                },
            );
            ui.separator();
            let rest_width = (ui.available_width() - 4.0).max(320.0);
            ui.allocate_ui_with_layout(
                egui::vec2(rest_width, pane_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_height(pane_height);
        ui.columns(2, |columns| {
            egui::ScrollArea::vertical()
                .id_salt("compose-form")
                .auto_shrink([false, false])
                .show(&mut columns[0], |ui| {
                    draw_form(ui, &mut flow, &mut sink, theme, thumbnails, vm);
                    ui.add_space(theme.space_sm);
                    draw_actions(ui, &mut flow, &mut sink, theme, backend, access);
                    if let Some(stage) = &flow.busy_stage {
                        ui.label(egui::RichText::new(stage).color(theme.warning));
                    }
                    if let Some(error) = &flow.error {
                        ui.label(egui::RichText::new(error).color(theme.danger));
                    }
                });

            egui::ScrollArea::vertical()
                .id_salt("compose-output")
                .auto_shrink([false, false])
                .show(&mut columns[1], |ui| {
                    draw_result(ui, &mut flow, &mut sink, theme);
                    ui.add_space(theme.space_sm);
                    draw_descriptions(ui, &mut flow, &mut sink, theme, vm);
                });
        });
                },
            );
        });

        if flow.media_dirty {
            flow.media_dirty = false;
            vm.media_reload += 1;
            vm.edits_version += 1;
            flow.seen_edits = vm.edits_version;
        }
        sync_to_vm(&flow, vm);
    });
}

fn preset_guidance(
    flow: &mut ComposeFlow,
    call: &mut dyn FnMut(&str, Value) -> Result<Value, String>,
) {
    flow.preset_guidance.clear();
    flow.prompt_limit = None;
    flow.brief_fields.clear();
    flow.loaded_preset_id = Some(flow.preset_id.clone());
    if flow.preset_id.is_empty() {
        flow.refresh_guidance();
        return;
    }
    if let Ok(document) = call("presets_get", json!({ "id": flow.preset_id })) {
        let spec = document.get("document").unwrap_or(&document);
        flow.preset_guidance = spec
            .get("guidance")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        flow.prompt_limit = spec
            .get("limits")
            .and_then(|limits| limits.get("promptChars"))
            .and_then(Value::as_i64);
        flow.reference_slots = spec.get("referenceSlots").and_then(Value::as_i64);
        flow.brief_fields = spec
            .get("briefFields")
            .and_then(Value::as_array)
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|field| {
                        Some((
                            field.get("key")?.as_str()?.to_string(),
                            field.get("label")?.as_str()?.to_string(),
                            field
                                .get("placeholder")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
    flow.refresh_guidance();
}

fn draw_form(
    ui: &mut Ui,
    flow: &mut ComposeFlow,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    theme: &Theme,
    thumbnails: &mut ThumbnailCache,
    vm: &mut ViewModel,
) {
    let typed_assets: Vec<crate::backend::Asset> = vm.assets.clone();
    // Preset selector: the list comes from `presets_list` at mount.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Preset").color(theme.ink_2));
        let selected = if flow.preset_id.is_empty() {
            "Choose preset".to_string()
        } else {
            flow.preset_id.clone()
        };
        egui::ComboBox::from_id_salt("compose-preset")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for preset in flow.presets.clone() {
                    if ui
                        .selectable_label(flow.preset_id == preset, &preset)
                        .clicked()
                        && flow.preset_id != preset
                    {
                        flow.preset_id = preset;
                        preset_guidance(flow, sink);
                    }
                }
            });
        if let Some(limit) = flow.prompt_limit {
            ui.label(egui::RichText::new(format!("limit {limit} chars")).color(theme.faint));
        }
    });

    match flow.mode.as_str() {
        "describe" => {
            ui.label(egui::RichText::new("Describe hint").color(theme.ink_2));
            let mut hint = flow.hint.clone();
            let response =
                ui.add(egui::TextEdit::singleline(&mut hint).frame(theme.text_edit_frame()));
            if response.has_focus() {
                vm.text_input_focused = true;
            }
            let cleared = crate::widgets::clear_overlay(ui, &response, &mut hint, theme);
            if response.changed() || cleared {
                let value = hint;
                dispatch(flow, ComposeAction::SetHint { value }, sink);
            }
        }
        "refine" => draw_refine(ui, flow, sink, theme, vm),
        "batch" => {
            if let Some(job) = flow.batch_job_id.clone() {
                ui.label(egui::RichText::new(format!("Queued job {job}")).color(theme.success));
            }
        }
        _ => {
            let mut text_only = flow.text_only;
            if ui
                .checkbox(&mut text_only, "Text only (skip the describer)")
                .changed()
            {
                dispatch(
                    flow,
                    ComposeAction::ToggleTextOnly { value: text_only },
                    sink,
                );
            }
            if !flow.brief_fields.is_empty() {
                draw_brief_fields(ui, flow, sink, theme, vm);
            } else {
            ui.label(egui::RichText::new("Brief").color(theme.ink_2));
            let mut brief = flow.brief.clone();
            let response = ui.add(
                egui::TextEdit::multiline(&mut brief)
                    .desired_rows(4)
                    .frame(theme.text_edit_frame()),
            );
            if response.has_focus() {
                vm.text_input_focused = true;
            }
            let cleared = crate::widgets::clear_overlay(ui, &response, &mut brief, theme);
            if response.changed() || cleared {
                let value = brief;
                dispatch(flow, ComposeAction::SetBrief { value }, sink);
            }
            }
            draw_selected_terms(ui, flow, theme);
            if flow.uses_media() {
                let mut send_images = flow.send_images;
                if ui
                    .checkbox(&mut send_images, "Send media to composer")
                    .changed()
                {
                    flow.send_images = send_images;
                }
            }
        }
    }
    ui.add_space(theme.space_sm);
    if flow.mode == "compose" || flow.mode == "describe" {
        draw_terms(ui, flow, theme);
    }
    draw_advanced(ui, flow, sink, theme, vm);
}

const TERMS_JSON: &str = include_str!("../../assets/terms.json");
const TERMS_NOTICE: &str = include_str!("../../assets/NOTICE-terms.txt");

/// Chips for the chosen terms, right under the brief. Clicking one removes it.
fn draw_selected_terms(ui: &mut Ui, flow: &mut ComposeFlow, theme: &Theme) {
    if flow.selected_terms.is_empty() {
        return;
    }
    let mut remove: Option<usize> = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.spacing_mut().button_padding = egui::vec2(7.0, 2.0);
        ui.spacing_mut().interact_size.y = 22.0;
        ui.horizontal_wrapped(|ui| {
            for (index, term) in flow.selected_terms.iter().enumerate() {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(format!("{term}  x")).color(theme.accent),
                        )
                        .fill(theme.accent.gamma_multiply(0.15)),
                    )
                    .on_hover_text("Click to remove")
                    .clicked()
                {
                    remove = Some(index);
                }
            }
        });
    });
    if let Some(index) = remove {
        flow.selected_terms.remove(index);
    }
}

/// Vocabulary pick-list (lighting, camera, mood...). Terms toggle in the
/// selected list shown under the brief; they are sent with the brief.
fn draw_terms(ui: &mut Ui, flow: &mut ComposeFlow, theme: &Theme) {
    let categories: Vec<Value> = serde_json::from_str(TERMS_JSON).unwrap_or_default();
    if categories.is_empty() {
        return;
    }
    let mut toggled: Option<String> = None;
    egui::CollapsingHeader::new("Terms")
        .id_salt("compose-terms")
        .default_open(false)
        .show(ui, |ui| {
            let tab_id = egui::Id::new("compose-terms-tab");
            let mut tab: usize = ui.data(|data| data.get_temp(tab_id)).unwrap_or(0);
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                ui.spacing_mut().button_padding = egui::vec2(7.0, 2.0);
                ui.spacing_mut().interact_size.y = 22.0;
                ui.horizontal_wrapped(|ui| {
                    for (index, category) in categories.iter().enumerate() {
                        let name = category
                            .get("category")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if ui.selectable_label(tab == index, name).clicked() {
                            tab = index;
                        }
                    }
                });
                ui.data_mut(|data| data.insert_temp(tab_id, tab));
                let terms = categories
                    .get(tab)
                    .and_then(|category| category.get("terms"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                egui::ScrollArea::vertical()
                    .id_salt("compose-terms-list")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for term in terms.iter().filter_map(Value::as_str) {
                                let chosen = flow.selected_terms.iter().any(|item| item == term);
                                if ui
                                    .add(
                                        egui::Button::new(egui::RichText::new(term).size(12.5))
                                            .selected(chosen),
                                    )
                                    .clicked()
                                {
                                    toggled = Some(term.to_string());
                                }
                            }
                        });
                    });
            });
            egui::CollapsingHeader::new("Credits")
                .id_salt("compose-terms-credits")
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(TERMS_NOTICE).color(theme.faint).size(11.0));
                });
        });
    if let Some(term) = toggled {
        if let Some(position) = flow.selected_terms.iter().position(|item| *item == term) {
            flow.selected_terms.remove(position);
        } else {
            flow.selected_terms.push(term);
        }
    }
}

/// Labelled inputs for presets that declare `briefFields`; the composed brief
/// is "Label: value" lines in field order, empty fields left out.
fn draw_brief_fields(
    ui: &mut Ui,
    flow: &mut ComposeFlow,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    theme: &Theme,
    vm: &mut ViewModel,
) {
    let fields = flow.brief_fields.clone();
    let mut changed = false;
    for (key, label, placeholder) in fields {
        ui.label(egui::RichText::new(label).color(theme.ink_2));
        let mut value = flow.field_values.get(&key).cloned().unwrap_or_default();
        let response = ui.add(
            egui::TextEdit::singleline(&mut value)
                .hint_text(placeholder)
                .desired_width(f32::INFINITY)
                .frame(theme.text_edit_frame()),
        );
        if response.has_focus() {
            vm.text_input_focused = true;
        }
        let cleared = crate::widgets::clear_overlay(ui, &response, &mut value, theme);
        if response.changed() || cleared {
            flow.field_values.insert(key, value);
            changed = true;
        }
    }
    if changed {
        let brief = compose_brief(&flow.brief_fields, &flow.field_values);
        dispatch(flow, ComposeAction::SetBrief { value: brief }, sink);
    }
}

fn compose_brief(
    fields: &[(String, String, String)],
    values: &std::collections::HashMap<String, String>,
) -> String {
    fields
        .iter()
        .filter_map(|(key, label, _)| {
            let value = values.get(key).map(|text| text.trim()).unwrap_or("");
            (!value.is_empty()).then(|| format!("{label}: {value}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Long file names widened the whole left column and pushed text over the right
/// panel; show a shortened name (the full name stays in the tooltip/labels).
fn short_name(name: &str) -> String {
    const LIMIT: usize = 34;
    if name.chars().count() <= LIMIT {
        return name.to_string();
    }
    let head: String = name.chars().take(LIMIT - 1).collect();
    format!("{head}…")
}

fn draw_picker(
    ui: &mut Ui,
    flow: &mut ComposeFlow,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    theme: &Theme,
    thumbnails: &mut ThumbnailCache,
    typed_assets: &[crate::backend::Asset],
) {
    if flow.assets.is_empty() {
        ui.label(
            egui::RichText::new("No media yet. Use Import media or paste an image.")
                .color(theme.muted),
        );
        return;
    }
    // In Batch mode the checkboxes choose which media to queue; otherwise they
    // build the ordered, weighted reference list.
    let batch = flow.mode == "batch";
    ui.horizontal_wrapped(|ui| {
        if ui.button("Select all").clicked() {
            if batch {
                dispatch(flow, ComposeAction::BatchSelectAll, sink);
            } else {
                dispatch(flow, ComposeAction::SelectAll, sink);
            }
        }
        if ui.button("Clear").clicked() {
            if batch {
                flow.batch_asset_ids.clear();
            } else {
                dispatch(flow, ComposeAction::ClearSelection, sink);
            }
        }
    });
    ui.label(
        egui::RichText::new(if batch {
            format!("{} queued", flow.batch_asset_ids.len())
        } else {
            format!("{} selected", flow.selection.len())
        })
        .color(theme.muted),
    );
    ui.add_space(theme.space_3xs);

    let assets: Vec<AssetView> = flow.assets.clone();
    for asset in assets {
        let order = flow
            .selection
            .iter()
            .position(|item| item.asset_id == asset.id)
            .map(|index| index + 1);
        let weight = flow
            .selection
            .iter()
            .find(|item| item.asset_id == asset.id)
            .map(|item| item.weight)
            .unwrap_or(1.0);
        let chosen = if batch {
            flow.batch_asset_ids.contains(&asset.id)
        } else {
            order.is_some()
        };
        let placeholder = crate::backend::Asset {
            id: asset.id.clone(),
            kind: crate::backend::AssetKind::Image,
            path: asset.thumbnail_path.clone().unwrap_or_default(),
            display_name: asset.display_name.clone(),
            hash: String::new(),
            width: None,
            height: None,
            fps: None,
            duration_ms: None,
            codec: None,
            has_audio: false,
            thumbnail_path: asset.thumbnail_path.clone(),
            proxy_path: None,
            missing: false,
            favorite: false,
            created_at: String::new(),
        };
        // Prefer the backend-typed asset: its thumbnail path is the one the
        // grid decodes successfully.
        let mut shown = match typed_assets.iter().find(|item| item.id == asset.id) {
            Some(typed) => typed.clone(),
            None => placeholder,
        };
        if let Some(edited) = &asset.edited_path {
            // Show the edited image: that is what Describe/Compose receive.
            shown.thumbnail_path = Some(edited.clone());
            shown.path = edited.clone();
        }

        egui::Frame::new()
            .fill(if chosen {
                theme.accent.gamma_multiply(0.14)
            } else {
                egui::Color32::TRANSPARENT
            })
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    thumbnails.paint(ui, &shown, PICKER_THUMB);
                    ui.vertical(|ui| {
                        let mut checked = chosen;
                        let mut label = short_name(&asset.display_name);
                        if let Some(order) = order
                            && !batch
                        {
                            label = format!("{order}. {label}");
                        }
                        if asset.edited_path.is_some() {
                            label.push_str(" (edited)");
                        }
                        let has_text = flow.sent_description(&asset.id).is_some()
                            || flow.description(&asset.id).is_some();
                        let sent = !flow.no_send.iter().any(|id| id == &asset.id);
                        let mut job = egui::text::LayoutJob::default();
                        job.append(
                            &label,
                            0.0,
                            egui::TextFormat {
                                color: theme.ink,
                                ..Default::default()
                            },
                        );
                        if has_text {
                            job.append(
                                if sent { "  [desc]" } else { "  [desc off]" },
                                0.0,
                                egui::TextFormat {
                                    color: if sent { theme.success } else { theme.faint },
                                    font_id: egui::FontId::proportional(11.0),
                                    ..Default::default()
                                },
                            );
                        }
                        if ui
                            .checkbox(&mut checked, job)
                            .on_hover_text(&asset.display_name)
                            .clicked()
                        {
                            if batch {
                                if checked {
                                    flow.batch_asset_ids.push(asset.id.clone());
                                } else {
                                    flow.batch_asset_ids.retain(|id| id != &asset.id);
                                }
                            } else {
                                dispatch(
                                    flow,
                                    ComposeAction::ToggleAsset {
                                        asset_id: asset.id.clone(),
                                    },
                                    sink,
                                );
                            }
                        }
                        if crate::widgets::trash_button(ui, theme, &asset.id, "Move this media to trash")
                            .clicked()
                        {
                            dispatch(
                                flow,
                                ComposeAction::RemoveAsset {
                                    asset_id: asset.id.clone(),
                                },
                                sink,
                            );
                        }
                        if order.is_some() && !batch {
                            ui.horizontal_wrapped(|ui| {
                                let has_text = flow.sent_description(&asset.id).is_some()
                                    || flow.description(&asset.id).is_some();
                                if has_text {
                                    let mut send = !flow.no_send.iter().any(|id| id == &asset.id);
                                    if ui.checkbox(&mut send, "Send description").changed() {
                                        dispatch(
                                            flow,
                                            ComposeAction::SetAssetDescription {
                                                asset_id: asset.id.clone(),
                                                value: send,
                                            },
                                            sink,
                                        );
                                    }
                                }
                                let described = flow.description(&asset.id).is_some();
                                if ui
                                    .add_enabled(
                                        !flow.busy,
                                        egui::Button::new(if described { "Re-describe" } else { "Describe" }),
                                    )
                                    .clicked()
                                {
                                    dispatch(
                                        flow,
                                        ComposeAction::DescribeAsset {
                                            asset_id: asset.id.clone(),
                                        },
                                        sink,
                                    );
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Weight").color(theme.muted));
                                let mut value = weight;
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut value)
                                            .range(0.0..=2.0)
                                            .speed(0.05)
                                            .max_decimals(2),
                                    )
                                    .changed()
                                {
                                    dispatch(
                                        flow,
                                        ComposeAction::SetWeight {
                                            asset_id: asset.id.clone(),
                                            value,
                                        },
                                        sink,
                                    );
                                }
                            });
                        }
                    });
                });
            });
    }
}

/// Left panel of Compose: the media list. In Refine/text-only modes the list is
/// not used, so say so instead of showing controls that do nothing.
fn draw_media_panel(
    ui: &mut Ui,
    flow: &mut ComposeFlow,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    theme: &Theme,
    thumbnails: &mut ThumbnailCache,
    vm: &ViewModel,
) {
    let typed_assets: Vec<crate::backend::Asset> = vm.assets.clone();
    ui.label(egui::RichText::new("Media").color(theme.accent));
    if flow.mode == "refine" {
        ui.label(
            egui::RichText::new("Refine edits an existing prompt as text; media is not used.")
                .color(theme.faint),
        );
        return;
    }
    if flow.mode == "compose" && flow.text_only {
        ui.label(egui::RichText::new("Text only is on: media is ignored.").color(theme.faint));
        return;
    }
    draw_sets(ui, flow, sink, theme);
    egui::ScrollArea::vertical()
        .id_salt("compose-media")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            draw_picker(ui, flow, sink, theme, thumbnails, &typed_assets);
        });
}

/// Add-from-Characters picker and the sets already added.
fn draw_sets(
    ui: &mut Ui,
    flow: &mut ComposeFlow,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    theme: &Theme,
) {
    if flow.available_sets.is_empty() && flow.sets.is_empty() {
        return;
    }
    let mut add: Option<String> = None;
    let mut remove: Option<String> = None;
    let mut shuffle: Option<String> = None;
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_id_salt("compose-add-set")
            .selected_text("Add from Characters")
            .show_ui(ui, |ui| {
                for (id, name) in &flow.available_sets {
                    let added = flow.sets.iter().any(|set| &set.id == id);
                    if ui.selectable_label(added, name).clicked() {
                        add = Some(id.clone());
                    }
                }
            });
    });
    let mut changes: Vec<(String, bool)> = Vec::new();
    for set in &flow.sets {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(&set.name).color(theme.accent));
            let mut send = set.send_description;
            if ui
                .add_enabled(
                    !set.description.trim().is_empty(),
                    egui::Checkbox::new(&mut send, "Send description"),
                )
                .changed()
            {
                changes.push((set.id.clone(), send));
            }
            if set.random && ui.button("Shuffle").on_hover_text("Pick another random image").clicked() {
                shuffle = Some(set.id.clone());
            }
            if ui.button("Remove").on_hover_text("Take it out of Compose (media stays)").clicked() {
                remove = Some(set.id.clone());
            }
        });
        if !set.description.trim().is_empty() {
            ui.add(
                egui::Label::new(egui::RichText::new(set.description.trim()).color(theme.muted).size(12.5))
                    .wrap(),
            );
        }
    }
    for (id, value) in changes {
        dispatch(flow, ComposeAction::SetSetDescription { id, value }, sink);
    }
    if let Some(id) = add {
        dispatch(flow, ComposeAction::AddSet { id }, sink);
    }
    if let Some(id) = shuffle {
        dispatch(flow, ComposeAction::ShuffleSet { id }, sink);
    }
    if let Some(id) = remove {
        dispatch(flow, ComposeAction::RemoveSet { id }, sink);
    }
    ui.separator();
}

fn draw_descriptions(
    ui: &mut Ui,
    flow: &mut ComposeFlow,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    theme: &Theme,
    vm: &mut ViewModel,
) {
    if flow.selection.is_empty() || flow.text_only {
        return;
    }
    ui.label(egui::RichText::new("Descriptions").color(theme.accent));
    let selected: Vec<String> = flow
        .selection
        .iter()
        .map(|item| item.asset_id.clone())
        .collect();
    for asset_id in selected {
        let name = flow
            .assets
            .iter()
            .find(|asset| asset.id == asset_id)
            .map(|asset| asset.display_name.clone())
            .unwrap_or_else(|| asset_id.clone());
        let Some(index) = flow
            .descriptions
            .iter()
            .position(|item| item.asset_id == asset_id)
        else {
            ui.label(egui::RichText::new(format!("{name}: not described yet")).color(theme.muted));
            continue;
        };
        let editing = flow.descriptions[index].editing;
        ui.label(egui::RichText::new(name).color(theme.ink_2));
        if editing {
            let mut draft = flow.descriptions[index].draft.clone();
            let response = ui.add(
                egui::TextEdit::multiline(&mut draft)
                    .desired_rows(6)
                    .frame(theme.text_edit_frame()),
            );
            if response.has_focus() {
                vm.text_input_focused = true;
            }
            let cleared = crate::widgets::clear_overlay(ui, &response, &mut draft, theme);
            if response.changed() || cleared {
                dispatch(
                    flow,
                    ComposeAction::SetDescriptionDraft {
                        asset_id: asset_id.clone(),
                        value: draft,
                    },
                    sink,
                );
            }
            ui.horizontal(|ui| {
                if ui.button("Save description").clicked() {
                    dispatch(
                        flow,
                        ComposeAction::SaveDescription {
                            asset_id: asset_id.clone(),
                        },
                        sink,
                    );
                }
                if ui.button("Cancel").clicked() {
                    let saved = flow.descriptions[index].text.clone();
                    flow.descriptions[index].draft = saved;
                    flow.descriptions[index].editing = false;
                }
            });
        } else {
            let text = flow.descriptions[index].text.clone();
            crate::widgets::scroll_box(ui, theme, ("desc-box", index), 160.0, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(text).color(theme.ink)).wrap());
            });
            if ui.button("Edit description").clicked() {
                dispatch(
                    flow,
                    ComposeAction::EditDescription {
                        asset_id: asset_id.clone(),
                    },
                    sink,
                );
            }
        }
        ui.add_space(theme.space_xs);
    }
}

fn draw_refine(
    ui: &mut Ui,
    flow: &mut ComposeFlow,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    theme: &Theme,
    vm: &mut ViewModel,
) {
    ui.label(egui::RichText::new("Start from a stored generation").color(theme.ink_2));
    let sources: Vec<(String, String, String)> = flow
        .sources
        .iter()
        .map(|(id, prompt, preset)| {
            let title = flow
                .source_titles
                .get(id)
                .cloned()
                .unwrap_or_else(|| prompt.split_whitespace().take(5).collect::<Vec<_>>().join(" "));
            (id.clone(), title, format!("{preset}: {}", prompt.chars().take(300).collect::<String>()))
        })
        .collect();
    let selected = sources
        .iter()
        .find(|(id, _, _)| *id == flow.refine_source_id)
        .map(|(_, title, _)| title.clone())
        .unwrap_or_else(|| "Free text".to_string());
    egui::ComboBox::from_id_salt("compose-refine-source")
        .selected_text(selected)
        .width(320.0)
        .show_ui(ui, |ui| {
            for (id, title, hover) in sources {
                if ui
                    .selectable_label(flow.refine_source_id == id, title)
                    .on_hover_text(hover)
                    .clicked()
                {
                    dispatch(flow, ComposeAction::ChooseRefineSource { id }, sink);
                }
            }
        });

    ui.label(egui::RichText::new("Existing prompt").color(theme.ink_2));
    let mut existing = flow.refine_existing.clone();
    let response = ui.add(
        egui::TextEdit::multiline(&mut existing)
            .desired_rows(5)
            .frame(theme.text_edit_frame()),
    );
    if response.has_focus() {
        vm.text_input_focused = true;
    }
    let cleared = crate::widgets::clear_overlay(ui, &response, &mut existing, theme);
    if response.changed() || cleared {
        let value = existing;
        dispatch(flow, ComposeAction::SetRefineExisting { value }, sink);
    }
    ui.label(egui::RichText::new("Refinement instruction").color(theme.ink_2));
    let mut instruction = flow.refine_instruction.clone();
    let response =
        ui.add(egui::TextEdit::multiline(&mut instruction).frame(theme.text_edit_frame()));
    if response.has_focus() {
        vm.text_input_focused = true;
    }
    let cleared = crate::widgets::clear_overlay(ui, &response, &mut instruction, theme);
    if response.changed() || cleared {
        let value = instruction;
        dispatch(flow, ComposeAction::SetRefineInstruction { value }, sink);
    }
}

fn draw_advanced(
    ui: &mut Ui,
    flow: &mut ComposeFlow,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    theme: &Theme,
    vm: &mut ViewModel,
) {
    egui::CollapsingHeader::new("Guidance, locks, and generation options")
        .id_salt("compose-advanced")
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Run guidance").color(theme.ink_2));
            let mut run = flow.run_guidance.clone();
            let response =
                ui.add(egui::TextEdit::singleline(&mut run).frame(theme.text_edit_frame()));
            if response.has_focus() {
                vm.text_input_focused = true;
            }
            let cleared = crate::widgets::clear_overlay(ui, &response, &mut run, theme);
            if response.changed() || cleared {
                let value = run;
                dispatch(flow, ComposeAction::SetRunGuidance { value }, sink);
            }
            ui.label(egui::RichText::new("Global guidance").color(theme.ink_2));
            let mut global = flow.global_guidance.clone();
            let response =
                ui.add(egui::TextEdit::singleline(&mut global).frame(theme.text_edit_frame()));
            if response.has_focus() {
                vm.text_input_focused = true;
            }
            let cleared = crate::widgets::clear_overlay(ui, &response, &mut global, theme);
            if response.changed() || cleared {
                let value = global;
                dispatch(flow, ComposeAction::SetGlobalGuidance { value }, sink);
            }
            if !flow.effective_guidance.is_empty() {
                ui.label(
                    egui::RichText::new(format!("Effective: {}", flow.effective_guidance))
                        .color(theme.muted),
                );
            }
            for conflict in flow.conflicts.clone() {
                ui.label(egui::RichText::new(conflict).color(theme.warning));
            }

            ui.label(egui::RichText::new("Locked phrases (comma separated)").color(theme.ink_2));
            let mut locked = flow.locked.clone();
            let response =
                ui.add(egui::TextEdit::singleline(&mut locked).frame(theme.text_edit_frame()));
            if response.has_focus() {
                vm.text_input_focused = true;
            }
            let cleared = crate::widgets::clear_overlay(ui, &response, &mut locked, theme);
            if response.changed() || cleared {
                let value = locked;
                dispatch(flow, ComposeAction::SetLocked { value }, sink);
            }

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Language").color(theme.ink_2));
                let mut language = flow.language.clone();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut language)
                            .desired_width(120.0)
                            .hint_text("auto")
                            .frame(theme.text_edit_frame()),
                );
                let cleared = crate::widgets::clear_overlay(ui, &response, &mut language, theme);
                if response.changed() || cleared {
                    let value = language;
                    dispatch(flow, ComposeAction::SetLanguage { value }, sink);
                }
                ui.label(egui::RichText::new("Variations").color(theme.ink_2));
                let mut variations = flow.variations;
                if ui
                    .add(egui::DragValue::new(&mut variations).range(1..=4))
                    .changed()
                {
                    dispatch(
                        flow,
                        ComposeAction::SetVariations { value: variations },
                        sink,
                    );
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Seed").color(theme.ink_2));
                let mut seed = flow.seed.clone();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut seed)
                            .desired_width(140.0)
                            .hint_text("random")
                            .frame(theme.text_edit_frame()),
                );
                let cleared = crate::widgets::clear_overlay(ui, &response, &mut seed, theme);
                if response.changed() || cleared {
                    let value = seed;
                    dispatch(flow, ComposeAction::SetSeed { value }, sink);
                }
                let mut bypass = flow.bypass_cache;
                if ui.checkbox(&mut bypass, "Bypass cache").changed() {
                    dispatch(flow, ComposeAction::SetBypassCache { value: bypass }, sink);
                }
            });
        });
}

fn draw_actions(
    ui: &mut Ui,
    flow: &mut ComposeFlow,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    theme: &Theme,
    backend: &dyn Backend,
    access: &AllowedRoots,
) {
    // Wrap: the left column is narrower than the full button row, which
    // otherwise clips "Reset" underneath the Output panel.
    ui.horizontal_wrapped(|ui| {
        let busy = flow.busy;
        let label = match flow.mode.as_str() {
            "describe" => "Describe",
            "batch" => "Queue batch",
            "refine" => "Refine",
            _ => "Compose",
        };
        if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
            match flow.mode.as_str() {
                "describe" => dispatch(flow, ComposeAction::Describe { force: false }, sink),
                "batch" => dispatch(flow, ComposeAction::BatchQueue, sink),
                _ => dispatch(flow, ComposeAction::Compose, sink),
            }
        }
        if flow.uses_media()
            && flow.mode != "describe"
            && ui
                .add_enabled(!busy, egui::Button::new("Describe media"))
                .clicked()
        {
            dispatch(flow, ComposeAction::Describe { force: false }, sink);
        }
        if ui
            .add_enabled(
                !flow.selection.is_empty() && !busy,
                egui::Button::new("Re-describe"),
            )
            .clicked()
        {
            dispatch(flow, ComposeAction::Describe { force: true }, sink);
        }
        if ui
            .add_enabled(busy, egui::Button::new("Cancel (Esc)"))
            .clicked()
        {
            dispatch(flow, ComposeAction::Cancel, sink);
        }
        let empty = flow.results.is_empty()
            && flow.brief.trim().is_empty()
            && flow.selection.is_empty()
            && flow.descriptions.is_empty()
            && !flow.busy;
        if ui.add_enabled(!empty, egui::Button::new("Reset")).clicked() {
            dispatch(flow, ComposeAction::Reset, sink);
        }
    });
    ui.horizontal(|ui| {
        if ui.button("Import media…").clicked() {
            if let Some(paths) = crate::platform::dialogs::pick_import_files() {
                let values: Vec<String> = paths
                    .iter()
                    .map(|path| path.to_string_lossy().to_string())
                    .collect();
                match import::import_paths(backend, access, &values) {
                    Ok(_) => dispatch(flow, ComposeAction::Mount, sink),
                    Err(error) => flow.error = Some(error_text_of(&error)),
                }
            }
        }
        if ui.button("Paste image").clicked() {
            match crate::platform::clipboard::read_image() {
                Ok(image) => match import::import_clipboard(backend, &image) {
                    Ok(_) => dispatch(flow, ComposeAction::Mount, sink),
                    Err(error) => flow.error = Some(error_text_of(&error)),
                },
                Err(error) => flow.error = Some(error_text_of(&error)),
            }
        }
        ui.label(
            egui::RichText::new(format!(
                "{} reference(s) · {} described",
                flow.selection.len(),
                flow.descriptions.len()
            ))
            .color(theme.faint),
        );
    });
}

fn draw_result(
    ui: &mut Ui,
    flow: &mut ComposeFlow,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    theme: &Theme,
) {
    ui.label(egui::RichText::new("Output").color(theme.accent));
    let Some(prompt) = flow.results.get(flow.result_index).cloned() else {
        theme.output_frame().show(ui, |ui| {
            ui.set_min_height(140.0);
            ui.label(egui::RichText::new("No output yet").color(theme.muted));
            ui.label(
                egui::RichText::new(
                    "Compose builds a prompt from the brief or the reviewed descriptions.",
                )
                .color(theme.faint),
            );
        });
        return;
    };
    let meta = flow.result_meta.get(flow.result_index).cloned();
    let editing = flow.editing_prompt;

    crate::widgets::scroll_box(ui, theme, "result-box", 340.0, |ui| {
        if editing {
            let mut draft = flow.prompt_draft.clone();
            let response = ui.add(
                egui::TextEdit::multiline(&mut draft)
                    .desired_rows(10)
                    .desired_width(f32::INFINITY)
                    .frame(theme.text_edit_frame()),
            );
            let cleared = crate::widgets::clear_overlay(ui, &response, &mut draft, theme);
            if response.changed() || cleared {
                dispatch(flow, ComposeAction::SetPromptDraft { value: draft }, sink);
            }
        } else {
            crate::widgets::formatted_prompt(ui, theme, &prompt);
        }
    });

    if flow.results.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            for index in 0..flow.results.len() {
                if ui
                    .selectable_label(
                        flow.result_index == index,
                        format!("Variation {}", index + 1),
                    )
                    .clicked()
                {
                    flow.result_index = index;
                    flow.prompt_draft = flow.results[index].clone();
                    flow.editing_prompt = false;
                }
            }
        });
    }

    ui.horizontal_wrapped(|ui| {
        if ui.button("Copy plain prompt").clicked() {
            let text = if editing {
                flow.prompt_draft.clone()
            } else {
                prompt.clone()
            };
            ui.ctx().copy_text(text);
            flow.copied = Some("plain".into());
        }
        if ui.button("Copy structured").clicked() {
            let text = if editing {
                flow.prompt_draft.clone()
            } else {
                prompt.clone()
            };
            let mut structured = meta.clone().unwrap_or_else(|| json!({}));
            if !structured.is_object() {
                structured = json!({});
            }
            if let Some(object) = structured.as_object_mut() {
                object.insert("prompt".into(), Value::String(text));
            }
            ui.ctx()
                .copy_text(serde_json::to_string_pretty(&structured).unwrap_or_default());
            flow.copied = Some("structured".into());
        }
        if editing {
            if ui.button("Save prompt").clicked() {
                flow.editing_prompt = false;
            }
        } else if ui.button("Edit prompt").clicked() {
            dispatch(flow, ComposeAction::EditPrompt, sink);
        }
        if ui.button("Regenerate").clicked() {
            // Reusing the previous seed would reproduce the same text; a fresh
            // seed (and bypassing the cache, set in dispatch) gives a new take.
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| (elapsed.as_nanos() & 0x7fff_ffff) as i64)
                .unwrap_or(1);
            dispatch(flow, ComposeAction::Regenerate { seed }, sink);
        }
    });

    if let Some(meta) = meta {
        let text = |key: &str| {
            meta.get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default()
        };
        let seed = meta
            .get("seed")
            .and_then(Value::as_i64)
            .map(|seed| seed.to_string())
            .unwrap_or_default();
        let cached = meta.get("cached").and_then(Value::as_bool).unwrap_or(false);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("model {}", text("model"))).color(theme.muted));
            ui.label(
                egui::RichText::new(format!("preset {}", text("presetId"))).color(theme.muted),
            );
            ui.label(egui::RichText::new(format!("seed {seed}")).color(theme.muted));
            ui.label(
                egui::RichText::new(if cached { "cached" } else { "fresh" }).color(if cached {
                    theme.info
                } else {
                    theme.success
                }),
            );
        });
        if let Some(negative) = meta.get("negative").and_then(Value::as_str) {
            ui.label(egui::RichText::new(format!("Negative: {negative}")).color(theme.ink_2));
        }
        if let Some(params) = meta.get("params").and_then(Value::as_object) {
            // Internal bookkeeping (token budget, reference echo) is not a
            // generation parameter; show only what the user can act on.
            let shown: Vec<String> = params
                .iter()
                .filter(|(key, _)| !matches!(key.as_str(), "budget" | "source" | "references"))
                .map(|(key, value)| match value {
                    Value::String(text) => format!("{key}: {text}"),
                    other => format!("{key}: {other}"),
                })
                .collect();
            if !shown.is_empty() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("Params: {}", shown.join(" · ")))
                            .color(theme.ink_2),
                    )
                    .wrap(),
                );
            }
        }
        if let Some(warnings) = meta.get("warnings").and_then(Value::as_array) {
            for warning in warnings {
                let text = warning
                    .get("message")
                    .and_then(Value::as_str)
                    .or_else(|| warning.as_str())
                    .unwrap_or("");
                if !text.is_empty() {
                    ui.label(egui::RichText::new(text).color(theme.warning));
                }
            }
        }
        if let Some(limit) = flow.prompt_limit {
            let chars = prompt.chars().count() as i64;
            if chars > limit {
                ui.label(
                    egui::RichText::new(format!("{chars} chars exceeds the {limit} char limit"))
                        .color(theme.danger),
                );
            }
        }
    }
    if let Some(copied) = &flow.copied {
        ui.label(egui::RichText::new(format!("Copied {copied} prompt")).color(theme.success));
    }
}

fn sync_from_vm(flow: &mut ComposeFlow, vm: &ViewModel) {
    flow.mode = vm.compose.mode.as_str().to_string();
    if flow.preset_id.is_empty() {
        flow.preset_id = vm.compose.preset_id.clone();
    }
    if flow.assets.is_empty() {
        flow.assets = vm
            .assets
            .iter()
            .map(|asset| AssetView {
                id: asset.id.clone(),
                display_name: asset.display_name.clone(),
                kind: match asset.kind {
                    crate::backend::AssetKind::Image => "image".into(),
                    crate::backend::AssetKind::Video => "video".into(),
                },
                thumbnail_path: asset.thumbnail_path.clone(),
                edit_id: None,
                edited_path: None,
            })
            .collect();
    }
    if flow.selection.is_empty() && !vm.compose.selected_asset_ids.is_empty() {
        flow.selection = vm
            .compose
            .selected_asset_ids
            .iter()
            .map(|id| ReferenceSelection {
                asset_id: id.clone(),
                weight: 1.0,
            })
            .collect();
    }
    flow.brief = vm.compose.brief.clone();
    flow.run_guidance = vm.compose.run_guidance.clone();
    let editing_description = flow.descriptions.iter().any(|entry| entry.editing);
    if !vm.compose.descriptions.is_empty() && !editing_description {
        flow.descriptions = vm
            .compose
            .descriptions
            .iter()
            .map(|(id, text)| DescriptionView {
                asset_id: id.clone(),
                text: text.clone(),
                draft: text.clone(),
                editing: false,
            })
            .collect();
    }
    // Restore the saved result only when the flow has none. Doing it every
    // frame collapsed multiple variations to one and wiped prompt edits.
    if flow.results.is_empty()
        && let Some(result) = vm.compose.result.clone()
    {
        flow.results = vec![result];
        flow.result_index = 0;
        flow.prompt_draft = flow.results[0].clone();
    }
}

fn sync_to_vm(flow: &ComposeFlow, vm: &mut ViewModel) {
    vm.compose.mode = match flow.mode.as_str() {
        "describe" => ComposeMode::Describe,
        "refine" => ComposeMode::Refine,
        "batch" => ComposeMode::Batch,
        _ => ComposeMode::Compose,
    };
    vm.compose.brief = flow.brief.clone();
    vm.compose.preset_id = flow.preset_id.clone();
    vm.compose.run_guidance = flow.run_guidance.clone();
    vm.compose.selected_asset_ids = flow
        .selection
        .iter()
        .map(|item| item.asset_id.clone())
        .collect();
    vm.compose.descriptions = flow
        .descriptions
        .iter()
        .map(|entry| (entry.asset_id.clone(), entry.text.clone()))
        .collect();
    vm.compose.result = flow.results.get(flow.result_index).cloned();
    vm.asset_count = flow.assets.len();
    vm.busy = flow.busy;
}

/// Runs a describe action from a button or a keyboard shortcut (kept for app.rs).
pub fn run_describe(vm: &mut ViewModel, backend: &dyn Backend) {
    FLOW.with(|cell| {
        let mut flow = cell.borrow_mut();
        sync_from_vm(&mut flow, vm);
        let mut sink = backend_sink(backend);
        dispatch(
            &mut flow,
            ComposeAction::Describe { force: false },
            &mut sink,
        );
        sync_to_vm(&flow, vm);
    });
}

/// Runs a compose action from a button or a keyboard shortcut (kept for app.rs).
pub fn run_compose(vm: &mut ViewModel, backend: &dyn Backend) {
    FLOW.with(|cell| {
        let mut flow = cell.borrow_mut();
        sync_from_vm(&mut flow, vm);
        let mut sink = backend_sink(backend);
        dispatch(&mut flow, ComposeAction::Compose, &mut sink);
        sync_to_vm(&flow, vm);
    });
}

#[cfg(test)]
mod brief_tests {
    use super::*;

    #[test]
    fn selected_terms_are_sent_with_the_brief() {
        let mut flow = ComposeFlow::default();
        flow.brief = "a red fox in snow".into();
        flow.selected_terms = vec!["macro".into(), "soft light".into()];
        assert_eq!(
            flow.effective_brief(),
            "a red fox in snow

Style terms: macro, soft light"
        );
        flow.brief.clear();
        assert_eq!(flow.effective_brief(), "Style terms: macro, soft light");
        flow.selected_terms.clear();
        assert_eq!(flow.effective_brief(), "");
    }
}

#[cfg(test)]
mod set_tests {
    use super::*;

    fn flow_with_set() -> ComposeFlow {
        let mut flow = ComposeFlow::default();
        flow.selection = vec![
            ReferenceSelection { asset_id: "a1".into(), weight: 1.0 },
            ReferenceSelection { asset_id: "a2".into(), weight: 1.0 },
        ];
        flow.sets = vec![LoadedSet {
            id: "s1".into(),
            name: "Mira".into(),
            description: "tall, red coat".into(),
            send_description: true,
            asset_ids: vec!["a1".into(), "a2".into()],
            random: false,
            last_image: None,
        }];
        flow
    }

    #[test]
    fn notes_use_tags_names_and_never_the_word_character() {
        let flow = flow_with_set();
        let brief = flow.effective_brief();
        assert_eq!(brief, "Reference notes:
<image1>, <image2> (Mira): tall, red coat");
        assert!(!brief.to_lowercase().contains("character"));
    }

    #[test]
    fn notes_are_off_when_the_toggle_is_off_or_images_are_unselected() {
        let mut flow = flow_with_set();
        flow.sets[0].send_description = false;
        assert_eq!(flow.effective_brief(), "");
        flow.sets[0].send_description = true;
        flow.selection.retain(|item| item.asset_id == "a2");
        assert_eq!(flow.effective_brief(), "Reference notes:
<image1> (Mira): tall, red coat");
        flow.selection.clear();
        assert_eq!(flow.effective_brief(), "");
    }

    #[test]
    fn image_description_combines_role_and_respects_the_toggle() {
        let mut flow = ComposeFlow::default();
        flow.roles.insert("a1".into(), "front".into());
        assert_eq!(flow.sent_description("a1").as_deref(), Some("front"));
        flow.descriptions.push(DescriptionView {
            asset_id: "a1".into(),
            text: "smiling".into(),
            draft: "smiling".into(),
            editing: false,
        });
        assert_eq!(flow.sent_description("a1").as_deref(), Some("front: smiling"));
        flow.no_send.push("a1".into());
        assert_eq!(flow.sent_description("a1"), None);
        assert_eq!(flow.sent_description("other"), None);
    }
}
