//! Runtime panel with source-driven replay of models/settings flows. The egui
//! shell issues the same commands through `Backend::call`.

use egui::Ui;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::backend::Backend;
use crate::errors::error_text_of;
use crate::theme::Theme;
use crate::view_model::ViewModel;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUiState {
    pub online: bool,
    pub describer: Option<String>,
    pub error: Option<String>,
}

fn refresh(record: &dyn Fn(&str, Value)) {
    record("models_list", json!({}));
    record("models_status", json!({}));
    record("settings_get", json!({}));
}

/// `fixtures` holds the mocked responses the source panel was driven with.
pub fn replay(
    action: &Value,
    record: &dyn Fn(&str, Value),
    state: &mut RuntimeUiState,
    fixtures: &Value,
) {
    let kind = action.get("kind").and_then(Value::as_str).unwrap_or("");
    match kind {
        "mount" => {
            refresh(record);
            state.online = true;
            state.describer = fixtures
                .get("settings_get")
                .and_then(|value| value.get("describerModel"))
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        "setRoles" => {
            record(
                "models_set_roles",
                json!({
                    "request": {
                        "describer": "lfm3-vl-2b",
                        "composer": "gemma4-12b-qat-uncensored-hauhaucs-balanced",
                    }
                }),
            );
            refresh(record);
        }
        "load" => {
            let role = action
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("describer");
            let model = if role == "describer" {
                "lfm3-vl-2b"
            } else {
                "gemma4-12b-qat-uncensored-hauhaucs-balanced"
            };
            record(
                "models_load",
                json!({
                    "request": {
                        "role": role,
                        "model": model,
                        "contextLength": 8192,
                        "ttlSeconds": 600,
                        "gpuOffload": 1,
                    }
                }),
            );
            refresh(record);
        }
        "setPolicy" => {
            record(
                "models_set_policy",
                json!({
                    "request": {
                        "keepWarm": "none",
                        "describerConcurrency": 2,
                        "ttlSeconds": 600,
                    }
                }),
            );
            refresh(record);
        }
        "unloadAll" => {
            let force = action
                .get("force")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            record("models_unload_all", json!({ "force": force }));
            refresh(record);
        }
        "settingsUpdate" => {
            record(
                "settings_update",
                json!({
                    "patch": {
                        "describerSystemPrompt": "",
                        "composerSystemPrompt": "",
                    }
                }),
            );
            refresh(record);
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

fn live_refresh(sink: &mut impl FnMut(&str, Value) -> Result<Value, String>, vm: &mut ViewModel) {
    vm.runtime.error = None;
    match sink("models_list", json!({})) {
        Ok(value) => {
            vm.runtime.models = serde_json::from_value(value).unwrap_or_default();
        }
        Err(error) => vm.runtime.error = Some(error),
    }
    match sink("models_status", json!({})) {
        Ok(value) => {
            vm.runtime.status = Some(value);
        }
        Err(error) => vm.runtime.error = Some(error),
    }
    match sink("settings_get", json!({})) {
        Ok(value) => {
            if let Some(model) = value.get("describerModel").and_then(Value::as_str) {
                vm.runtime.describer = model.to_string();
            }
            if let Some(model) = value.get("composerModel").and_then(Value::as_str) {
                vm.runtime.composer = model.to_string();
            }
            vm.runtime.endpoint = value
                .get("endpoint")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            vm.runtime.keep_warm = value
                .get("keepWarm")
                .and_then(Value::as_str)
                .unwrap_or("none")
                .to_string();
            vm.runtime.describer_prompt = value
                .get("describerSystemPrompt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            vm.runtime.composer_prompt = value
                .get("composerSystemPrompt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            vm.runtime.settings = Some(value);
        }
        Err(error) => vm.runtime.error = Some(error),
    }
}

fn live_action(
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    vm: &mut ViewModel,
    command: &str,
    args: Value,
) {
    vm.runtime.error = None;
    vm.runtime.notice = None;
    match sink(command, args) {
        Ok(_) => {
            live_refresh(sink, vm);
            vm.runtime.notice = Some(
                match command {
                    "settings_update" => "Saved. New describe and compose runs use it now.",
                    "models_set_roles" => "Models saved.",
                    "models_set_policy" => "Keep-warm policy saved.",
                    _ => "Done.",
                }
                .to_string(),
            );
        }
        Err(error) => vm.runtime.error = Some(error),
    }
}

fn model_label(model: &Value) -> String {
    let id = model
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let mut parts = vec![id];
    if let Some(context) = model.get("contextLength").and_then(Value::as_u64) {
        parts.push(format!("{context} ctx"));
    }
    if let Some(quant) = model.get("quantization").and_then(Value::as_str) {
        parts.push(quant.to_string());
    }
    match model.get("vision").and_then(Value::as_bool) {
        Some(true) => parts.push("vision".to_string()),
        Some(false) => parts.push("text only".to_string()),
        None => {}
    }
    if model
        .get("loaded")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        parts.push("loaded".to_string());
    }
    parts.join(" · ")
}

fn model_choice(
    ui: &mut Ui,
    theme: &Theme,
    id: &str,
    label: &str,
    current: &mut String,
    models: &[Value],
) {
    ui.label(egui::RichText::new(label).color(theme.ink_2));
    egui::ComboBox::from_id_salt(id)
        .selected_text(if current.is_empty() {
            "Choose model".to_string()
        } else {
            current.clone()
        })
        .width(420.0)
        .show_ui(ui, |ui| {
            for model in models {
                let name = model
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if ui
                    .selectable_label(*current == name, model_label(model))
                    .clicked()
                {
                    *current = name;
                }
            }
        });
}

pub fn ui(ui: &mut Ui, vm: &mut ViewModel, theme: &Theme, backend: &dyn Backend) {
    let mut sink = backend_sink(backend);
    if !vm.runtime.loaded {
        vm.runtime.loaded = true;
        live_refresh(&mut sink, vm);
    }

    ui.heading("Runtime");
    ui.add_space(theme.space_xs);
    let online = vm
        .runtime
        .status
        .as_ref()
        .and_then(|status| status.get("reachable").or_else(|| status.get("online")))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(if online {
                "LM Studio online"
            } else {
                "LM Studio unreachable"
            })
            .color(if online { theme.success } else { theme.danger }),
        );
        ui.label(egui::RichText::new(&vm.runtime.endpoint).color(theme.muted));
        if ui.button("Refresh").clicked() {
            live_refresh(&mut sink, vm);
        }
    });

    let models = vm.runtime.models.clone();
    egui::ScrollArea::vertical()
        .id_salt("runtime")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            model_choice(
                ui,
                theme,
                "runtime-describer",
                "Describer",
                &mut vm.runtime.describer,
                &models,
            );
            model_choice(
                ui,
                theme,
                "runtime-composer",
                "Composer",
                &mut vm.runtime.composer,
                &models,
            );
            ui.horizontal(|ui| {
                if ui.button("Save roles").clicked() {
                    live_action(
                        &mut sink,
                        vm,
                        "models_set_roles",
                        json!({
                            "request": {
                                "describer": vm.runtime.describer,
                                "composer": vm.runtime.composer,
                            }
                        }),
                    );
                }
                if ui.button("Suggest roles").clicked() {
                    let vision = models
                        .iter()
                        .find(|model| model.get("vision").and_then(Value::as_bool) == Some(true));
                    if let Some(model) = vision
                        && let Some(id) = model.get("id").and_then(Value::as_str)
                    {
                        vm.runtime.describer = id.to_string();
                    }
                    let largest = models
                        .iter()
                        .max_by_key(|model| {
                            model
                                .get("contextLength")
                                .and_then(Value::as_u64)
                                .unwrap_or(0)
                        })
                        .and_then(|model| model.get("id").and_then(Value::as_str))
                        .map(str::to_string);
                    if let Some(id) = largest {
                        vm.runtime.composer = id;
                    }
                }
            });

            ui.add_space(theme.space_sm);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Load").color(theme.ink_2));
                if ui.button("Describer").clicked() {
                    live_action(
                        &mut sink,
                        vm,
                        "models_load",
                        json!({
                            "request": {
                                "role": "describer",
                                "model": vm.runtime.describer,
                                "contextLength": 8192,
                                "ttlSeconds": 600,
                                "gpuOffload": 1,
                            }
                        }),
                    );
                }
                if ui.button("Composer").clicked() {
                    live_action(
                        &mut sink,
                        vm,
                        "models_load",
                        json!({
                            "request": {
                                "role": "composer",
                                "model": vm.runtime.composer,
                                "contextLength": 8192,
                                "ttlSeconds": 600,
                                "gpuOffload": 1,
                            }
                        }),
                    );
                }
                if ui.button("Unload describer").clicked() {
                    live_action(
                        &mut sink,
                        vm,
                        "models_unload",
                        json!({ "model": vm.runtime.describer }),
                    );
                }
                if ui.button("Unload composer").clicked() {
                    live_action(
                        &mut sink,
                        vm,
                        "models_unload",
                        json!({ "model": vm.runtime.composer }),
                    );
                }
                if ui.button("Unload all").clicked() {
                    live_action(
                        &mut sink,
                        vm,
                        "models_unload_all",
                        json!({ "force": false }),
                    );
                }
            });

            ui.add_space(theme.space_sm);
            ui.label(egui::RichText::new("Keep warm").color(theme.ink_2));
            ui.horizontal(|ui| {
                for (value, label) in [
                    ("both", "Both"),
                    ("describer", "Describer"),
                    ("composer", "Composer"),
                    ("none", "None"),
                ] {
                    if ui
                        .selectable_label(vm.runtime.keep_warm == value, label)
                        .clicked()
                    {
                        vm.runtime.keep_warm = value.to_string();
                        live_action(
                            &mut sink,
                            vm,
                            "models_set_policy",
                            json!({
                                "request": {
                                    "keepWarm": vm.runtime.keep_warm,
                                    "describerConcurrency": 2,
                                    "ttlSeconds": 600,
                                }
                            }),
                        );
                    }
                }
            });

            ui.add_space(theme.space_sm);
            ui.label(egui::RichText::new("Model instructions").color(theme.ink_2));
            ui.label(
                egui::RichText::new(
                    "Describer instructions: sent as the system message of every image description,                      replacing the built-in describer text. The describer sees one image at a time and                      is told its tag (<image1>, <image2>, ...). Empty restores the built-in.",
                )
                .color(theme.faint),
            );
            let describer = ui.add(
                egui::TextEdit::multiline(&mut vm.runtime.describer_prompt)
                    .desired_rows(3)
                    .frame(theme.text_edit_frame()),
            );
            if describer.has_focus() {
                vm.text_input_focused = true;
            }
            crate::widgets::clear_overlay(ui, &describer, &mut vm.runtime.describer_prompt, theme);
            ui.label(
                egui::RichText::new(
                    "Composer instructions: sent as the composer's system message in place of the                      preset's own text (preset guidance, limits and your guidance are still added).                      Empty restores the preset.",
                )
                .color(theme.faint),
            );
            let composer = ui.add(
                egui::TextEdit::multiline(&mut vm.runtime.composer_prompt)
                    .desired_rows(3)
                    .frame(theme.text_edit_frame()),
            );
            if composer.has_focus() {
                vm.text_input_focused = true;
            }
            crate::widgets::clear_overlay(ui, &composer, &mut vm.runtime.composer_prompt, theme);
            let describer_active = vm
                .runtime
                .settings
                .as_ref()
                .and_then(|value| value.get("describerSystemPrompt"))
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty());
            let composer_active = vm
                .runtime
                .settings
                .as_ref()
                .and_then(|value| value.get("composerSystemPrompt"))
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty());
            ui.label(
                egui::RichText::new(format!(
                    "In use: describer {} · composer {}",
                    if describer_active { "custom instructions" } else { "built-in" },
                    if composer_active { "custom instructions" } else { "preset text" },
                ))
                .color(theme.muted),
            );
            if ui.button("Save instructions").clicked() {
                live_action(
                    &mut sink,
                    vm,
                    "settings_update",
                    json!({
                        "patch": {
                            "describerSystemPrompt": vm.runtime.describer_prompt,
                            "composerSystemPrompt": vm.runtime.composer_prompt,
                        }
                    }),
                );
            }

            if let Some(notice) = vm.runtime.notice.clone() {
                ui.label(egui::RichText::new(notice).color(theme.success));
            }
            ui.add_space(theme.space_sm);
            ui.label(
                egui::RichText::new(format!("{} model(s) reported", models.len()))
                    .color(theme.muted),
            );
            for model in &models {
                ui.label(egui::RichText::new(model_label(model)).color(theme.ink_2));
            }
            if let Some(error) = &vm.runtime.error {
                ui.label(egui::RichText::new(error).color(theme.danger));
            }
        });
}
