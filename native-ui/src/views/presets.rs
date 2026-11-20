//! Preset panel with source-driven replay of list/get/preview/compile/save/
//! duplicate/delete/import/export flows. The egui shell issues the same calls
//! through `Backend::call`.

use egui::Ui;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::backend::Backend;
use crate::errors::error_text_of;
use crate::theme::Theme;
use crate::view_model::ViewModel;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetsUiState {
    pub builtin_note: bool,
    pub has_compiled: bool,
    pub error: Option<String>,
}

fn sample_description() -> Value {
    json!({
        "subject": "a red fox in fresh snow",
        "action": "turns toward the camera",
        "camera": "low angle, 35mm lens",
        "lighting": "blue hour, soft rim light",
        "composition": "centered portrait",
        "style": "cinematic photograph",
        "change": "replace the coat with a red leather jacket",
        "preserve": "the face, pose, and background",
    })
}

fn compile_request(document: Value, description: Value) -> Value {
    json!({
        "request": {
            "document": document,
            "guidance": { "global": Value::Null, "preset": Value::Null, "run": Value::Null },
            "description": description,
            "locked": [],
            "params": {},
        }
    })
}

fn refresh(record: &dyn Fn(&str, Value), id: &str, compile: Value) {
    record("presets_list", json!({}));
    record("presets_get", json!({ "id": id }));
    record("presets_compile", compile);
}

/// `fixtures` holds the mocked responses the source panel was driven with.
pub fn replay(
    action: &Value,
    record: &dyn Fn(&str, Value),
    state: &mut PresetsUiState,
    fixtures: &Value,
) {
    let kind = action.get("kind").and_then(Value::as_str).unwrap_or("");
    let selected = action.get("id").and_then(Value::as_str).unwrap_or("mmh3");
    let document = fixtures
        .get("documents")
        .and_then(|value| value.get(selected))
        .cloned()
        .unwrap_or_else(|| json!({ "id": selected }));
    let compile = compile_request(document.clone(), sample_description());

    match kind {
        "mount" => {
            record("presets_list", json!({}));
            record("presets_get", json!({ "id": selected }));
            record("presets_compile", compile);
            state.builtin_note = selected == "mmh3";
            state.has_compiled = true;
        }
        "duplicate" => {
            record("presets_duplicate", json!({ "id": "mmh3" }));
            record("presets_get", json!({ "id": "mmh3" }));
            refresh(record, "mmh3", compile);
        }
        "save" => {
            record(
                "presets_save",
                json!({ "request": { "document": document } }),
            );
            refresh(record, selected, compile);
            state.builtin_note = false;
        }
        "delete" => {
            record(
                "presets_delete",
                json!({ "request": { "id": selected, "confirmInUse": true } }),
            );
            refresh(record, selected, compile);
            state.builtin_note = false;
        }
        "import" => {
            let path = action
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("C:\\presets\\mmh3.json");
            record(
                "presets_import",
                json!({ "request": { "id": "", "path": path } }),
            );
            refresh(record, "mmh3", compile);
        }
        "export" => {
            let path = action
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("C:\\presets\\mmh3.json");
            record(
                "presets_export",
                json!({ "request": { "id": "mmh3", "path": path } }),
            );
            refresh(record, "mmh3", compile);
        }
        "noop" => {}
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Live egui shell
// ---------------------------------------------------------------------------

pub fn backend_sink(
    backend: &dyn Backend,
) -> impl FnMut(&str, Value) -> Result<Value, String> + '_ {
    move |method: &str, args: Value| backend.call(method, args).map_err(|e| error_text_of(&e))
}

fn sync_document(vm: &mut ViewModel, document: &Value) {
    vm.presets.document = Some(document.clone());
    vm.presets.json_text = serde_json::to_string_pretty(document).unwrap_or_default();
}

fn exchange_path(id: &str) -> String {
    std::env::temp_dir()
        .join(format!("prompt-studio-preset-{id}.json"))
        .display()
        .to_string()
}

/// Prefer the current selection, but never issue `presets_get` for a stale or
/// hardcoded id. An empty list must render as an empty state, not as an error.
fn selected_preset_id(vm: &ViewModel) -> Option<String> {
    let selected = vm.presets.selected_preset_id.as_deref();
    vm.presets
        .presets
        .iter()
        .find(|preset| {
            selected.is_some_and(|id| preset.get("id").and_then(Value::as_str) == Some(id))
        })
        .or_else(|| vm.presets.presets.first())
        .and_then(|preset| preset.get("id").and_then(Value::as_str))
        .map(str::to_string)
}

fn live_refresh(sink: &mut impl FnMut(&str, Value) -> Result<Value, String>, vm: &mut ViewModel) {
    vm.presets.error = None;
    match sink("presets_list", json!({})) {
        Ok(value) => {
            vm.presets.presets = serde_json::from_value(value).unwrap_or_default();
        }
        Err(error) => vm.presets.error = Some(error),
    }
    let id = selected_preset_id(vm);
    vm.presets.selected_preset_id = id.clone();
    let Some(id) = id else {
        vm.presets.builtin = false;
        vm.presets.document = None;
        vm.presets.compiled = None;
        return;
    };
    match sink("presets_get", json!({ "id": id })) {
        Ok(value) => {
            vm.presets.builtin = value
                .get("builtin")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let document = value.get("document").cloned().unwrap_or(value);
            sync_document(vm, &document);
            match sink(
                "presets_compile",
                compile_request(document, sample_description()),
            ) {
                Ok(compiled) => vm.presets.compiled = Some(compiled),
                Err(error) => vm.presets.error = Some(error),
            }
        }
        Err(error) => vm.presets.error = Some(error),
    }
}

fn live_action(
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    vm: &mut ViewModel,
    command: &str,
    args: Value,
) {
    vm.presets.error = None;
    match sink(command, args) {
        Ok(value) => {
            if command == "presets_duplicate" {
                if let Some(id) = value.get("id").and_then(Value::as_str) {
                    vm.presets.selected_preset_id = Some(id.to_string());
                }
            }
            live_refresh(sink, vm);
        }
        Err(error) => vm.presets.error = Some(error),
    }
}

const TAB_GUIDE: u8 = 0;
const TAB_JSON: u8 = 1;
const TAB_PREVIEW: u8 = 2;

fn kv_row(ui: &mut Ui, theme: &Theme, key: &str, value: String) {
    ui.label(egui::RichText::new(key).color(theme.muted));
    ui.label(egui::RichText::new(value).color(theme.ink));
    ui.end_row();
}

fn text_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn draw_guide(ui: &mut Ui, vm: &ViewModel, theme: &Theme) {
    let Some(document) = vm.presets.document.as_ref() else {
        ui.label(egui::RichText::new("No preset selected.").color(theme.faint));
        return;
    };
    let spec = document.get("document").unwrap_or(document);
    if let Some(description) = spec.get("description").and_then(Value::as_str) {
        ui.label(egui::RichText::new(description).color(theme.ink));
        ui.add_space(theme.space_xs);
    }

    egui::Grid::new("preset-facts")
        .num_columns(2)
        .spacing([theme.space_md, theme.space_3xs])
        .show(ui, |ui| {
            for (label, key) in [
                ("Kind", "kind"),
                ("Mode", "mode"),
                ("Instruction style", "instructionStyle"),
            ] {
                if let Some(value) = spec.get(key).and_then(Value::as_str) {
                    kv_row(ui, theme, label, value.to_string());
                }
            }
            if let Some(slots) = spec.get("referenceSlots").and_then(Value::as_u64) {
                kv_row(ui, theme, "Reference images", slots.to_string());
            }
            if let Some(multiple) = spec.get("dimensionMultiple").and_then(Value::as_u64) {
                kv_row(ui, theme, "Dimensions multiple of", multiple.to_string());
            }
            if let Some(negative) = spec.get("supportsNegative").and_then(Value::as_bool) {
                kv_row(
                    ui,
                    theme,
                    "Negative prompt",
                    if negative { "supported" } else { "not supported" }.to_string(),
                );
            }
            if let Some(limits) = spec.get("limits").and_then(Value::as_object) {
                for (key, label) in [
                    ("promptChars", "Prompt length (chars)"),
                    ("negativeChars", "Negative length (chars)"),
                    ("maxInputTokens", "Max input tokens"),
                    ("maxOutputTokens", "Max output tokens"),
                ] {
                    if let Some(value) = limits.get(key).and_then(Value::as_u64) {
                        kv_row(ui, theme, label, value.to_string());
                    }
                }
            }
        });

    if let Some(guidance) = spec
        .get("guidance")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        ui.add_space(theme.space_sm);
        ui.label(egui::RichText::new("Prompting guide").color(theme.accent));
        theme.output_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(guidance).color(theme.ink));
        });
    }

    let checklist = text_list(spec.get("checklist"));
    if !checklist.is_empty() {
        ui.add_space(theme.space_sm);
        ui.label(egui::RichText::new("Self-check").color(theme.accent));
        ui.label(egui::RichText::new(checklist.join(" · ")).color(theme.ink_2));
    }

    if let Some(params) = spec.get("params").and_then(Value::as_object)
        && !params.is_empty()
    {
        ui.add_space(theme.space_sm);
        ui.label(egui::RichText::new("Parameters").color(theme.accent));
        egui::Grid::new("preset-params")
            .num_columns(2)
            .spacing([theme.space_md, theme.space_3xs])
            .show(ui, |ui| {
                for (name, definition) in params {
                    let label = definition
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or(name);
                    let mut detail = Vec::new();
                    if let Some(kind) = definition.get("type").and_then(Value::as_str) {
                        detail.push(kind.to_string());
                    }
                    if let Some(default) = definition.get("default") {
                        detail.push(format!("default {default}"));
                    }
                    let values = text_list(definition.get("values"));
                    if !values.is_empty() {
                        detail.push(values.join(" / "));
                    }
                    kv_row(ui, theme, label, detail.join(" · "));
                }
            });
    }

    if let Some(resolutions) = spec.get("resolutions").and_then(Value::as_array)
        && !resolutions.is_empty()
    {
        let sizes: Vec<String> = resolutions
            .iter()
            .filter_map(|pair| {
                let pair = pair.as_array()?;
                Some(format!("{}×{}", pair.first()?.as_u64()?, pair.get(1)?.as_u64()?))
            })
            .collect();
        if !sizes.is_empty() {
            ui.add_space(theme.space_sm);
            ui.label(egui::RichText::new("Resolutions").color(theme.accent));
            ui.label(egui::RichText::new(sizes.join("   ")).color(theme.ink_2));
        }
    }

    if let Some(examples) = spec.get("examples").and_then(Value::as_array) {
        for (index, example) in examples.iter().enumerate() {
            let Some(prompt) = example
                .get("output")
                .and_then(|output| output.get("prompt"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            ui.add_space(theme.space_sm);
            egui::CollapsingHeader::new(format!("Example {}", index + 1))
                .id_salt(("preset-example", index))
                .default_open(index == 0)
                .show(ui, |ui| {
                    theme.output_frame().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(egui::RichText::new(prompt).color(theme.ink));
                    });
                });
        }
    }
}

pub fn ui(ui: &mut Ui, vm: &mut ViewModel, theme: &Theme, backend: &dyn Backend) {
    let mut sink = backend_sink(backend);
    if !vm.presets.loaded {
        vm.presets.loaded = true;
        live_refresh(&mut sink, vm);
    }

    ui.heading("Presets");
    ui.add_space(theme.space_xs);

    let entries: Vec<(String, String, bool, String)> = vm
        .presets
        .presets
        .iter()
        .map(|preset| {
            (
                preset
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                preset
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("preset")
                    .to_string(),
                preset
                    .get("builtin")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                preset
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .collect();
    let selected = selected_preset_id(vm);

    let height = (ui.available_height() - 8.0).max(160.0);
    let list_width = (ui.available_width() * 0.28).clamp(190.0, 300.0);
    ui.horizontal_top(|ui| {
        // ---- left: preset list ----
        ui.allocate_ui_with_layout(
            egui::vec2(list_width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_height(height);
                ui.label(egui::RichText::new(format!("{} preset(s)", entries.len())).color(theme.muted));
                egui::ScrollArea::vertical()
                    .id_salt("preset-list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (id, name, builtin, kind) in &entries {
                            let mut job = egui::text::LayoutJob::default();
                            job.append(
                                name,
                                0.0,
                                egui::TextFormat {
                                    color: theme.ink,
                                    ..Default::default()
                                },
                            );
                            let mut sub = Vec::new();
                            if !kind.is_empty() {
                                sub.push(kind.clone());
                            }
                            sub.push(if *builtin { "built-in" } else { "custom" }.to_string());
                            job.append(
                                &format!("\n{}", sub.join(" · ")),
                                0.0,
                                egui::TextFormat {
                                    color: theme.faint,
                                    font_id: egui::FontId::proportional(12.0),
                                    ..Default::default()
                                },
                            );
                            let response = ui
                                .with_layout(
                                    egui::Layout::top_down_justified(egui::Align::LEFT),
                                    |ui| {
                                        ui.add(egui::Button::selectable(
                                            selected.as_deref() == Some(id.as_str()),
                                            job,
                                        ))
                                    },
                                )
                                .inner;
                            if response.clicked() && selected.as_deref() != Some(id.as_str()) {
                                vm.presets.selected_preset_id = Some(id.clone());
                                live_refresh(&mut sink, vm);
                            }
                        }
                    });
            },
        );
        ui.separator();

        // ---- right: toolbar, then tabs ----
        let detail_width = (ui.available_width() - 4.0).max(240.0);
        ui.allocate_ui_with_layout(
            egui::vec2(detail_width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_height(height);
                let tab_id = egui::Id::new("presets-tab");
                let mut tab = ui.data(|data| data.get_temp::<u8>(tab_id)).unwrap_or(TAB_GUIDE);

                ui.horizontal_wrapped(|ui| {
                    let editable = selected.is_some() && !vm.presets.builtin;
                    if ui
                        .add_enabled(selected.is_some(), egui::Button::new("Duplicate"))
                        .on_hover_text("Make an editable copy")
                        .clicked()
                        && let Some(id) = selected.clone()
                    {
                        live_action(&mut sink, vm, "presets_duplicate", json!({ "id": id }));
                    }
                    if ui
                        .add_enabled(editable, egui::Button::new("Save JSON"))
                        .on_hover_text("Validate and save the JSON tab")
                        .clicked()
                    {
                        match serde_json::from_str::<Value>(&vm.presets.json_text) {
                            Ok(document) => live_action(
                                &mut sink,
                                vm,
                                "presets_save",
                                json!({ "request": { "document": document } }),
                            ),
                            Err(error) => {
                                vm.presets.error =
                                    Some(format!("Preset JSON is not valid JSON: {error}"))
                            }
                        }
                    }
                    let confirm_id = egui::Id::new("preset-delete-confirm");
                    let armed = ui.data(|data| data.get_temp::<bool>(confirm_id)).unwrap_or(false);
                    if armed && editable {
                        if ui
                            .button(egui::RichText::new("Confirm delete").color(theme.danger))
                            .clicked()
                            && let Some(id) = selected.clone()
                        {
                            ui.data_mut(|data| data.remove::<bool>(confirm_id));
                            live_action(
                                &mut sink,
                                vm,
                                "presets_delete",
                                json!({ "request": { "id": id, "confirmInUse": true } }),
                            );
                        }
                        if ui.button("Cancel").clicked() {
                            ui.data_mut(|data| data.remove::<bool>(confirm_id));
                        }
                    } else if ui
                        .add_enabled(editable, egui::Button::new("Delete…"))
                        .clicked()
                    {
                        ui.data_mut(|data| data.insert_temp(confirm_id, true));
                    }
                    ui.separator();
                    if ui.button("Import…").clicked()
                        && let Some(path) = crate::platform::dialogs::pick_import_json()
                    {
                        live_action(
                            &mut sink,
                            vm,
                            "presets_import",
                            json!({
                                "request": {
                                    "id": "",
                                    "path": path.to_string_lossy().to_string(),
                                }
                            }),
                        );
                    }
                    if ui
                        .add_enabled(selected.is_some(), egui::Button::new("Export…"))
                        .clicked()
                        && let Some(id) = selected.clone()
                        && let Some(path) =
                            crate::platform::dialogs::pick_save_path(&format!("{id}.json"))
                    {
                        live_action(
                            &mut sink,
                            vm,
                            "presets_export",
                            json!({
                                "request": {
                                    "id": id,
                                    "path": path.to_string_lossy().to_string(),
                                }
                            }),
                        );
                    }
                });

                if vm.presets.builtin {
                    ui.label(
                        egui::RichText::new("Built-in presets are read-only. Duplicate one to edit it.")
                            .color(theme.warning),
                    );
                }
                if let Some(error) = vm.presets.error.clone() {
                    ui.add(egui::Label::new(egui::RichText::new(error).color(theme.danger)).wrap());
                }

                ui.horizontal(|ui| {
                    for (value, label) in [
                        (TAB_GUIDE, "Guide"),
                        (TAB_JSON, "JSON"),
                        (TAB_PREVIEW, "Compiled preview"),
                    ] {
                        if ui.selectable_label(tab == value, label).clicked() {
                            tab = value;
                        }
                    }
                });
                ui.data_mut(|data| data.insert_temp(tab_id, tab));
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt(("preset-detail", tab))
                    .auto_shrink([false, false])
                    .show(ui, |ui| match tab {
                        TAB_JSON => {
                            let editor = ui.add(
                                egui::TextEdit::multiline(&mut vm.presets.json_text)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(24)
                                    .frame(theme.text_edit_frame()),
                            );
                            if editor.has_focus() {
                                vm.text_input_focused = true;
                            }
                            crate::widgets::clear_overlay(ui, &editor, &mut vm.presets.json_text, theme);
                            ui.label(
                                egui::RichText::new("JSON is authoritative for parameters.")
                                    .color(theme.faint),
                            );
                        }
                        TAB_PREVIEW => {
                            let prompt = vm
                                .presets
                                .compiled
                                .as_ref()
                                .and_then(|compiled| compiled.get("prompt"))
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            if prompt.is_empty() {
                                ui.label(
                                    egui::RichText::new("No compiled preview available.")
                                        .color(theme.faint),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("How this preset's instructions compile for a sample description")
                                        .color(theme.muted),
                                );
                                theme.output_frame().show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(egui::RichText::new(prompt).color(theme.ink));
                                });
                            }
                        }
                        _ => draw_guide(ui, vm, theme),
                    });
            },
        );
    });
}
