//! Datasets panel with source-driven replay of preview/import/run/export/delete.
//! The egui shell issues the same calls through `Backend::call`.

use egui::Ui;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::backend::Backend;
use crate::errors::error_text_of;
use crate::theme::Theme;
use crate::view_model::ViewModel;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetsUiState {
    pub dataset_count: usize,
    pub has_rows: bool,
    pub has_preview: bool,
    pub has_summary: bool,
    pub error: Option<String>,
}

fn refresh(record: &dyn Fn(&str, Value)) {
    record("datasets_list", json!({}));
    record("presets_list", json!({}));
    record("datasets_profiles", json!({}));
    record(
        "datasets_rows",
        json!({ "request": { "datasetId": "ds-1", "offset": 0, "limit": 500 } }),
    );
}

/// `fixtures` carries the mocked responses the source panel was driven with.
pub fn replay(
    action: &Value,
    record: &dyn Fn(&str, Value),
    state: &mut DatasetsUiState,
    fixtures: &Value,
) {
    let kind = action.get("kind").and_then(Value::as_str).unwrap_or("");
    match kind {
        "mount" => {
            refresh(record);
            state.dataset_count = 1;
            state.has_rows = true;
        }
        "preview" => {
            let path = action
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("C:\\data\\prompts.csv");
            record("datasets_preview", json!({ "request": { "path": path } }));
            state.has_preview = true;
        }
        "import" => {
            let path = action
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("C:\\data\\prompts.csv");
            record("datasets_preview", json!({ "request": { "path": path } }));
            let fields = fixtures
                .get("datasets_preview")
                .and_then(|value| value.get("mapping"))
                .and_then(|value| value.get("fields"))
                .cloned()
                .unwrap_or_else(|| json!({ "caption": "prompt" }));
            record(
                "datasets_import",
                json!({
                    "request": {
                        "path": path,
                        "mapping": { "fields": fields },
                        "dedupe": "skip",
                        "profileName": Value::Null,
                        "saveProfile": true,
                    }
                }),
            );
            refresh(record);
            state.has_preview = false;
            state.has_summary = true;
        }
        "run" => {
            let dataset_id = action
                .get("datasetId")
                .and_then(Value::as_str)
                .unwrap_or("ds-1");
            record(
                "datasets_run",
                json!({
                    "request": {
                        "datasetId": dataset_id,
                        "presetId": "mmh3",
                        "guidance": { "global": Value::Null, "preset": Value::Null, "run": Value::Null },
                    }
                }),
            );
            refresh(record);
        }
        "export" => {
            let dataset_id = action
                .get("datasetId")
                .and_then(Value::as_str)
                .unwrap_or("ds-1");
            let output = action
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("C:\\data\\out.csv");
            let columns = fixtures
                .get("datasets_rows")
                .and_then(|value| value.get("dataset"))
                .and_then(|value| value.get("exportColumns"))
                .cloned()
                .unwrap_or_else(|| json!(["caption"]));
            record(
                "datasets_export",
                json!({
                    "request": {
                        "datasetId": dataset_id,
                        "outputPath": output,
                        "format": "csv",
                        "columns": columns,
                    }
                }),
            );
            refresh(record);
        }
        "delete" => {
            let dataset_id = action
                .get("datasetId")
                .and_then(Value::as_str)
                .unwrap_or("ds-1");
            record("datasets_delete", json!({ "datasetId": dataset_id }));
            refresh(record);
            // Source effect re-reads rows once after the selection clears.
            record(
                "datasets_rows",
                json!({ "request": { "datasetId": dataset_id, "offset": 0, "limit": 500 } }),
            );
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

fn selected_id(vm: &ViewModel) -> Option<String> {
    vm.datasets
        .selected_dataset_id
        .clone()
        .or_else(|| {
            vm.datasets
                .datasets
                .first()
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            vm.datasets
                .datasets
                .first()
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn live_refresh(sink: &mut impl FnMut(&str, Value) -> Result<Value, String>, vm: &mut ViewModel) {
    vm.datasets.error = None;
    match sink("datasets_list", json!({})) {
        Ok(value) => {
            vm.datasets.datasets = serde_json::from_value(value).unwrap_or_default();
        }
        Err(error) => vm.datasets.error = Some(error),
    }
    sink("presets_list", json!({})).ok();
    sink("datasets_profiles", json!({})).ok();
    let Some(id) = selected_id(vm) else {
        // Empty state: never call datasets_rows with a fabricated dataset id.
        vm.datasets.rows = None;
        return;
    };
    match sink(
        "datasets_rows",
        json!({ "request": { "datasetId": id, "offset": 0, "limit": 500 } }),
    ) {
        Ok(value) => vm.datasets.rows = Some(value),
        Err(error) => vm.datasets.error = Some(error),
    }
}

fn live_action(
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    vm: &mut ViewModel,
    command: &str,
    args: Value,
) {
    vm.datasets.error = None;
    match sink(command, args) {
        Ok(value) => {
            if command == "datasets_preview" {
                vm.datasets.preview = Some(value);
                return;
            }
            if command == "datasets_import" {
                vm.datasets.summary = Some(value);
            }
            live_refresh(sink, vm);
        }
        Err(error) => vm.datasets.error = Some(error),
    }
}

pub fn ui(ui: &mut Ui, vm: &mut ViewModel, theme: &Theme, backend: &dyn Backend) {
    let mut sink = backend_sink(backend);
    if vm.datasets.dedupe.is_empty() {
        vm.datasets.dedupe = "skip".to_string();
    }
    if !vm.datasets.loaded {
        vm.datasets.loaded = true;
        live_refresh(&mut sink, vm);
    }

    ui.heading("Datasets");
    ui.add_space(theme.space_xs);
    ui.horizontal(|ui| {
        ui.label("Source");
        let field = ui.add(
            egui::TextEdit::singleline(&mut vm.datasets.source_path)
                .hint_text("CSV, JSONL, TXT, or an image folder")
                .frame(theme.text_edit_frame()),
        );
        if field.has_focus() {
            vm.text_input_focused = true;
        }
        if ui.button("Browse file…").clicked()
            && let Some(path) = crate::platform::dialogs::pick_import_dataset()
        {
            vm.datasets.source_path = path.to_string_lossy().to_string();
        }
        if ui.button("Browse folder…").clicked()
            && let Some(path) = crate::platform::dialogs::pick_import_folder()
        {
            vm.datasets.source_path = path.to_string_lossy().to_string();
        }
        if ui.button("Preview").clicked() {
            let path = vm.datasets.source_path.clone();
            live_action(
                &mut sink,
                vm,
                "datasets_preview",
                json!({ "request": { "path": path } }),
            );
        }
        if ui.button("Refresh").clicked() {
            live_refresh(&mut sink, vm);
        }
    });

    ui.horizontal_wrapped(|ui| {
        if ui.button("Import").clicked() {
            let path = vm.datasets.source_path.clone();
            let fields = vm
                .datasets
                .preview
                .as_ref()
                .and_then(|value| value.get("mapping"))
                .and_then(|value| value.get("fields"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            live_action(
                &mut sink,
                vm,
                "datasets_import",
                json!({
                    "request": {
                        "path": path,
                        "mapping": { "fields": fields },
                        "dedupe": vm.datasets.dedupe,
                        "profileName": if vm.datasets.mapping_profile.trim().is_empty() {
                            Value::Null
                        } else {
                            Value::String(vm.datasets.mapping_profile.clone())
                        },
                        "saveProfile": !vm.datasets.mapping_profile.trim().is_empty(),
                    }
                }),
            );
        }
        ui.label(egui::RichText::new("Duplicates").color(theme.ink_2));
        for dedupe in ["skip", "merge", "keep"] {
            if ui
                .selectable_label(vm.datasets.dedupe == dedupe, dedupe)
                .clicked()
            {
                vm.datasets.dedupe = dedupe.to_string();
            }
        }
        ui.label(egui::RichText::new("Profile").color(theme.ink_2));
        let field = ui.add(
            egui::TextEdit::singleline(&mut vm.datasets.mapping_profile)
                .desired_width(140.0)
                .hint_text("name")
                .frame(theme.text_edit_frame()),
        );
        if field.has_focus() {
            vm.text_input_focused = true;
        }
    });

    let datasets: Vec<(String, String, i64)> = vm
        .datasets
        .datasets
        .iter()
        .map(|value| {
            (
                value
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                value
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("dataset")
                    .to_string(),
                value.get("rowCount").and_then(Value::as_i64).unwrap_or(0),
            )
        })
        .collect();
    let id = selected_id(vm);
    egui::ScrollArea::vertical()
        .id_salt("datasets")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if datasets.is_empty() {
                ui.label(
                    egui::RichText::new("No dataset loaded — pick a file, then Import.")
                        .color(theme.muted),
                );
            } else {
                ui.horizontal_wrapped(|ui| {
                    for (dataset_id, name, rows) in &datasets {
                        if ui
                            .selectable_label(
                                vm.datasets.selected_dataset_id.as_deref()
                                    == Some(dataset_id.as_str()),
                                format!("{name} ({rows} rows)"),
                            )
                            .clicked()
                        {
                            vm.datasets.selected_dataset_id = Some(dataset_id.clone());
                            live_action(
                                &mut sink,
                                vm,
                                "datasets_rows",
                                json!({
                                    "request": {
                                        "datasetId": dataset_id,
                                        "offset": 0,
                                        "limit": 500,
                                    }
                                }),
                            );
                        }
                    }
                });
            }

            if let Some(preview) = vm.datasets.preview.clone() {
                let format = preview
                    .get("format")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let encoding = preview
                    .get("encoding")
                    .and_then(Value::as_str)
                    .unwrap_or("utf-8")
                    .to_string();
                ui.label(
                    egui::RichText::new(format!("Preview: {format} · {encoding}"))
                        .color(theme.muted),
                );
                let fields = preview
                    .get("mapping")
                    .and_then(|mapping| mapping.get("fields"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                ui.label(
                    egui::RichText::new(format!("Auto-mapped fields: {fields}")).color(theme.ink_2),
                );
                if let Some(rows) = preview.get("rows").and_then(Value::as_array) {
                    for row in rows.iter().take(5) {
                        ui.label(egui::RichText::new(row.to_string()).color(theme.faint));
                    }
                }
            }

            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(id.is_some(), egui::Button::new("Run batch"))
                    .clicked()
                {
                    if let Some(id) = id.clone() {
                        let preset = if vm.datasets.preset_id.is_empty() {
                            vm.compose.preset_id.clone()
                        } else {
                            vm.datasets.preset_id.clone()
                        };
                        live_action(
                            &mut sink,
                            vm,
                            "datasets_run",
                            json!({
                                "request": {
                                    "datasetId": id,
                                    "presetId": preset,
                                    "guidance": {
                                        "global": Value::Null,
                                        "preset": Value::Null,
                                        "run": Value::Null,
                                    },
                                }
                            }),
                        );
                    } else {
                        vm.datasets.error = Some("No dataset loaded".to_string());
                    }
                }
                if ui
                    .add_enabled(id.is_some(), egui::Button::new("Export…"))
                    .clicked()
                {
                    if let Some(id) = id.clone()
                        && let Some(path) = crate::platform::dialogs::pick_save_path("dataset.csv")
                    {
                        live_action(
                            &mut sink,
                            vm,
                            "datasets_export",
                            json!({
                                "request": {
                                    "datasetId": id,
                                    "outputPath": path.to_string_lossy().to_string(),
                                    "format": "csv",
                                    "columns": Value::Null,
                                }
                            }),
                        );
                    }
                }
                if ui
                    .add_enabled(id.is_some(), egui::Button::new("Delete"))
                    .clicked()
                {
                    if let Some(id) = id.clone() {
                        live_action(&mut sink, vm, "datasets_delete", json!({ "datasetId": id }));
                    } else {
                        vm.datasets.error = Some("No dataset loaded".to_string());
                    }
                }
            });

            if let Some(rows) = vm.datasets.rows.clone() {
                let counts = rows.get("summary").cloned().unwrap_or_else(|| json!({}));
                ui.label(egui::RichText::new(format!("Rows: {counts}")).color(theme.muted));
                if let Some(items) = rows.get("rows").and_then(Value::as_array) {
                    for row in items.iter().take(100) {
                        let index = row.get("index").and_then(Value::as_i64).unwrap_or(0);
                        let status = row
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("pending");
                        let prompt = row
                            .get("mapped")
                            .and_then(|mapped| mapped.get("prompt"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let summary: String = prompt.chars().take(60).collect();
                        ui.label(
                            egui::RichText::new(format!("{index}: {status} · {summary}"))
                                .color(theme.ink_2),
                        );
                    }
                }
            }

            if let Some(error) = &vm.datasets.error {
                ui.label(egui::RichText::new(error).color(theme.danger));
            }
        });
}
