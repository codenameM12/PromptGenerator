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

pub fn ui(ui: &mut Ui, vm: &mut ViewModel, theme: &Theme, backend: &dyn Backend) {
    let mut sink = backend_sink(backend);
    if !vm.presets.loaded {
        vm.presets.loaded = true;
        live_refresh(&mut sink, vm);
    }

    ui.heading("Presets");
    ui.add_space(theme.space_xs);

    let entries: Vec<(String, String, bool)> = vm
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
            )
        })
        .collect();
    let selected = selected_preset_id(vm);

    egui::ScrollArea::vertical()
        .id_salt("presets")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (id, name, builtin) in &entries {
                    let label = if *builtin {
                        format!("{name} (built-in)")
                    } else {
                        name.clone()
                    };
                    if ui
                        .selectable_label(selected.as_deref() == Some(id.as_str()), label)
                        .clicked()
                    {
                        vm.presets.selected_preset_id = Some(id.clone());
                        live_refresh(&mut sink, vm);
                    }
                }
            });
            ui.label(
                egui::RichText::new(format!(
                    "{} preset(s){}",
                    entries.len(),
                    selected
                        .as_deref()
                        .map(|id| format!(" · selected: {id}"))
                        .unwrap_or_default()
                ))
                .color(theme.muted),
            );
            if vm.presets.builtin {
                ui.label(
                    egui::RichText::new(
                        "Built-in presets are read-only; duplicate one to edit it.",
                    )
                    .color(theme.warning),
                );
            }

            let editor_height = (ui.available_height() * 0.55).clamp(200.0, 420.0);
            let editor = ui.add_sized(
                [ui.available_width(), editor_height],
                egui::TextEdit::multiline(&mut vm.presets.json_text)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(14)
                    .frame(theme.text_edit_frame()),
            );
            if editor.has_focus() {
                vm.text_input_focused = true;
            }
            ui.label(
                egui::RichText::new("JSON is authoritative for parameters.").color(theme.faint),
            );

            ui.horizontal_wrapped(|ui| {
                if ui.button("Refresh").clicked() {
                    live_refresh(&mut sink, vm);
                }
                if ui
                    .add_enabled(
                        selected.is_some() && !vm.presets.builtin,
                        egui::Button::new("Save"),
                    )
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
                if ui
                    .add_enabled(selected.is_some(), egui::Button::new("Duplicate"))
                    .clicked()
                    && let Some(id) = selected.clone()
                {
                    live_action(&mut sink, vm, "presets_duplicate", json!({ "id": id }));
                }
                if ui
                    .add_enabled(
                        selected.is_some() && !vm.presets.builtin,
                        egui::Button::new("Delete"),
                    )
                    .clicked()
                    && let Some(id) = selected.clone()
                {
                    live_action(
                        &mut sink,
                        vm,
                        "presets_delete",
                        json!({ "request": { "id": id, "confirmInUse": true } }),
                    );
                }
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

            if let Some(document) = vm.presets.document.as_ref() {
                ui.label(egui::RichText::new("Prompting guide").color(theme.accent));
                let spec = document.get("document").unwrap_or(document);
                if let Some(description) = spec.get("description").and_then(Value::as_str) {
                    ui.label(egui::RichText::new(description).color(theme.ink));
                }
                if let Some(guidance) = spec.get("guidance").and_then(Value::as_str) {
                    theme.output_frame().show(ui, |ui| {
                        ui.label(egui::RichText::new(guidance).color(theme.ink));
                    });
                }
                if let Some(sections) = spec.get("sections").and_then(Value::as_array) {
                    let names: Vec<String> = sections
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect();
                    if !names.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("Sections: {}", names.join(" → ")))
                                .color(theme.muted),
                        );
                    }
                }
                if let Some(checklist) = spec.get("checklist").and_then(Value::as_array) {
                    let names: Vec<String> = checklist
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect();
                    if !names.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("Self-check: {}", names.join(", ")))
                                .color(theme.muted),
                        );
                    }
                }
                if let Some(limits) = spec.get("limits") {
                    ui.label(egui::RichText::new(format!("Limits: {limits}")).color(theme.faint));
                }
                if spec.get("supportsNegative").and_then(Value::as_bool) == Some(false) {
                    ui.label(
                        egui::RichText::new("This target has no negative prompt")
                            .color(theme.warning),
                    );
                }
                if let Some(examples) = spec.get("examples").and_then(Value::as_array)
                    && let Some(example) = examples.first()
                    && let Some(output) = example.get("output")
                    && let Some(prompt) = output.get("prompt").and_then(Value::as_str)
                {
                    ui.label(egui::RichText::new("Example").color(theme.ink_2));
                    theme.output_frame().show(ui, |ui| {
                        ui.label(egui::RichText::new(prompt).color(theme.ink));
                    });
                }
            }

            if let Some(compiled) = vm.presets.compiled.as_ref() {
                let prompt = compiled
                    .get("prompt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !prompt.is_empty() {
                    ui.label(egui::RichText::new("Compiled preview").color(theme.accent));
                    theme.output_frame().show(ui, |ui| {
                        ui.label(egui::RichText::new(prompt).color(theme.ink));
                    });
                }
            }
            if let Some(error) = &vm.presets.error {
                ui.label(egui::RichText::new(error).color(theme.danger));
            }
        });
}
