//! Library panel with source-driven replay of query/filter, detail, compare,
//! metadata, trash, export/import, backup/restore, re-run and collections.
//! The egui shell routes every live action through `Backend::call`.

use egui::Ui;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::backend::Backend;
use crate::errors::error_text_of;
use crate::theme::Theme;
use crate::view_model::ViewModel;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryUiState {
    pub card_count: usize,
    pub checked: usize,
    pub has_detail: bool,
    pub compare_columns: usize,
    pub notice: Option<String>,
    pub error: Option<String>,
    pub has_trash: bool,
}

fn query(offset: i64, text: Option<&str>, collection_id: Option<&str>) -> Value {
    json!({
        "query": {
            "text": text,
            "presetId": Value::Null,
            "kind": Value::Null,
            "model": Value::Null,
            "favorite": Value::Null,
            "status": Value::Null,
            "tags": [],
            "collectionId": collection_id,
            "from": Value::Null,
            "to": Value::Null,
            "ids": [],
            "includeDeleted": false,
            "onlyDeleted": false,
            "offset": offset,
            "limit": 25,
            "sort": "newest",
        }
    })
}

fn refresh_meta(record: &dyn Fn(&str, Value), collection_id: Option<&str>) {
    record("library_query", query(0, None, collection_id));
    record("presets_list", json!({}));
    record("library_tags", json!({}));
    record("library_collections", json!({}));
}

fn open_detail(record: &dyn Fn(&str, Value), id: &str, asset_id: &str) {
    record("library_get", json!({ "id": id }));
    record(
        "drag_prepare",
        json!({
            "request": {
                "generationId": id,
                "assetId": asset_id,
                "editId": Value::Null,
                "payload": "plainPrompt",
                "sidecars": false,
            }
        }),
    );
}

/// `fixtures` holds the mocked responses used to drive the source panel.
pub fn replay(
    action: &Value,
    record: &dyn Fn(&str, Value),
    state: &mut LibraryUiState,
    fixtures: &Value,
) {
    let kind = action.get("kind").and_then(Value::as_str).unwrap_or("");
    let asset_id = fixtures
        .get("library_query")
        .and_then(|value| value.get("items"))
        .and_then(|value| value.get(0))
        .and_then(|value| value.get("assetId"))
        .and_then(Value::as_str)
        .unwrap_or("asset-1");
    match kind {
        "mount" => {
            refresh_meta(record, None);
            state.card_count = 4;
        }
        "query" => {
            let text = action.get("text").and_then(Value::as_str);
            record("library_query", query(0, text, None));
            record("presets_list", json!({}));
            record("library_tags", json!({}));
            record("library_collections", json!({}));
        }
        "page" => {
            let offset = action.get("offset").and_then(Value::as_i64).unwrap_or(25);
            record("library_query", query(offset, None, None));
        }
        "get" => {
            let id = action.get("id").and_then(Value::as_str).unwrap_or("gen-0");
            open_detail(record, id, asset_id);
            state.has_detail = true;
        }
        "favorite" => {
            let id = action.get("id").and_then(Value::as_str).unwrap_or("gen-0");
            record(
                "library_favorite",
                json!({ "request": { "id": id, "favorite": true, "asset": false } }),
            );
            refresh_meta(record, None);
        }
        "compare" => {
            let ids: Vec<String> = action
                .get("ids")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_else(|| vec!["gen-0".into(), "gen-1".into()]);
            record("library_compare", json!({ "request": { "ids": ids } }));
            record(
                "library_diff",
                json!({ "request": { "leftId": "gen-0", "rightId": "gen-1" } }),
            );
            state.checked = 2;
            state.has_detail = true;
            state.compare_columns = 2;
        }
        "versions" => {
            let id = action.get("id").and_then(Value::as_str).unwrap_or("gen-0");
            open_detail(record, id, asset_id);
            record("library_versions", json!({ "id": id }));
            state.has_detail = true;
        }
        "update" => {
            let id = action.get("id").and_then(Value::as_str).unwrap_or("gen-0");
            let notes = action
                .get("notes")
                .and_then(Value::as_str)
                .unwrap_or("reviewed");
            open_detail(record, id, asset_id);
            record(
                "library_update",
                json!({
                    "request": {
                        "ids": [id],
                        "rating": Value::Null,
                        "clearRating": true,
                        "notes": notes,
                    }
                }),
            );
            refresh_meta(record, None);
            open_detail(record, id, asset_id);
            state.has_detail = true;
        }
        "renameCollection" => {
            let id = action.get("id").and_then(Value::as_str).unwrap_or("col-1");
            let name = action
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("City spots");
            record(
                "library_collection_rename",
                json!({ "request": { "id": id, "name": name } }),
            );
            refresh_meta(record, Some(id));
        }
        "deleteCollection" => {
            let id = action.get("id").and_then(Value::as_str).unwrap_or("col-1");
            record("library_collection_delete", json!({ "id": id }));
            refresh_meta(record, None);
        }
        "assignCollection" => {
            let id = action.get("id").and_then(Value::as_str).unwrap_or("col-1");
            record(
                "library_collection_assign",
                json!({
                    "request": { "collectionId": id, "generationIds": ["gen-0"], "add": true }
                }),
            );
            refresh_meta(record, Some(id));
        }
        "purge" => {
            let ids: Vec<&str> = action
                .get("ids")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_str).collect())
                .unwrap_or_else(|| vec!["gen-0"]);
            record(
                "library_purge",
                json!({ "request": { "generationIds": ids, "confirm": true } }),
            );
            refresh_meta(record, None);
        }
        "libraryAssetsTrash" => {
            record("library_assets_trash", json!({}));
            refresh_meta(record, None);
        }
        "saveDrag" => {
            let path = action
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("C:\\cache\\drag\\prompt.txt");
            record(
                "drag_save",
                json!({
                    "request": {
                        "source": "C:\\cache\\drag\\session\\prompt.txt",
                        "outputPath": path,
                    }
                }),
            );
        }
        "revealDrag" => {
            record(
                "drag_reveal",
                json!({ "request": { "path": "C:\\cache\\drag\\session\\prompt.txt" } }),
            );
        }
        "trash" | "restoreDeleted" => {
            let ids: Vec<&str> = action
                .get("ids")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_str).collect())
                .unwrap_or_else(|| vec!["gen-0", "gen-1"]);
            let command = if kind == "trash" {
                "library_trash"
            } else {
                "library_restore_deleted"
            };
            record(command, json!({ "request": { "generationIds": ids } }));
            refresh_meta(record, None);
            state.checked = ids.len();
        }
        "export" => {
            let format = action
                .get("format")
                .and_then(Value::as_str)
                .unwrap_or("json");
            let output = action
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("C:\\out\\library.json");
            record(
                "library_export",
                json!({ "request": { "format": format, "outputPath": output, "ids": ["gen-0"] } }),
            );
            refresh_meta(record, None);
            state.checked = 1;
        }
        "import" => {
            let path = action
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("C:\\out\\library.json");
            record("library_import", json!({ "request": { "path": path } }));
            refresh_meta(record, None);
        }
        "backup" => {
            let output = action
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("C:\\out\\backup.zip");
            record(
                "library_backup",
                json!({ "request": { "outputPath": output } }),
            );
            state.notice = Some(format!("Backup written: {output} (100 bytes)"));
        }
        "restore" => {
            let path = action
                .get("backupPath")
                .and_then(Value::as_str)
                .unwrap_or("C:\\out\\backup.zip");
            record(
                "library_restore",
                json!({ "request": { "backupPath": path } }),
            );
            record("library_query", query(0, None, None));
            state.notice = Some(
                "Restored backup bundle. Restart Prompt Studio to apply settings and token."
                    .to_string(),
            );
        }
        "rerun" => {
            let preset = action
                .get("presetId")
                .and_then(Value::as_str)
                .unwrap_or("mmh3");
            record(
                "library_rerun",
                json!({ "request": { "ids": ["gen-0"], "presetId": preset } }),
            );
            refresh_meta(record, None);
            state.checked = 1;
        }
        "createCollection" => {
            let name = action.get("name").and_then(Value::as_str).unwrap_or("City");
            record(
                "library_collection_create",
                json!({ "request": { "name": name } }),
            );
            refresh_meta(record, None);
            refresh_meta(record, Some("col-1"));
        }
        "restoreWorkspace" => {
            let id = action.get("id").and_then(Value::as_str).unwrap_or("gen-0");
            open_detail(record, id, asset_id);
            record("library_restore_workspace", json!({ "id": id }));
            refresh_meta(record, None);
            open_detail(record, id, asset_id);
            state.has_detail = true;
        }
        "noop" => {}
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Live egui shell — every action goes through the generic Backend seam.
// ---------------------------------------------------------------------------

/// Maps the generic command sink onto `Backend::call` (the allowlisted seam).
pub fn backend_sink(
    backend: &dyn Backend,
) -> impl FnMut(&str, Value) -> Result<Value, String> + '_ {
    move |method: &str, args: Value| backend.call(method, args).map_err(|e| error_text_of(&e))
}

fn live_query(vm: &ViewModel, offset: i64) -> Value {
    let text = {
        let trimmed = vm.library.query.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    };
    let favorite = vm.library.favorites_only.then_some(true);
    let preset = (!vm.library.preset_filter.is_empty()).then(|| vm.library.preset_filter.clone());
    let status = (!vm.library.status_filter.is_empty()).then(|| vm.library.status_filter.clone());
    let tags: Vec<String> = {
        let tag = vm.library.tag_filter.trim();
        if tag.is_empty() {
            Vec::new()
        } else {
            vec![tag.to_string()]
        }
    };
    let trash = vm.library.trash_only;
    json!({
        "query": {
            "text": text,
            "presetId": preset,
            "kind": Value::Null,
            "model": Value::Null,
            "favorite": favorite,
            "status": status,
            "tags": tags,
            "collectionId": Value::Null,
            "from": Value::Null,
            "to": Value::Null,
            "ids": [],
            "includeDeleted": trash,
            "onlyDeleted": trash,
            "offset": offset,
            "limit": 50,
            "sort": vm.library.sort,
        }
    })
}

fn live_refresh(sink: &mut impl FnMut(&str, Value) -> Result<Value, String>, vm: &mut ViewModel) {
    vm.library.error = None;
    match sink("library_query", live_query(vm, 0)) {
        Ok(value) => {
            vm.library.records = value
                .get("items")
                .cloned()
                .and_then(|items| serde_json::from_value(items).ok())
                .unwrap_or_default();
            vm.library.total = value.get("total").and_then(Value::as_i64).unwrap_or(0);
        }
        Err(error) => vm.library.error = Some(error),
    }
    let mut meta = serde_json::Map::new();
    for (command, key) in [
        ("presets_list", "presets"),
        ("library_tags", "tags"),
        ("library_collections", "collections"),
    ] {
        match sink(command, json!({})) {
            Ok(value) => {
                meta.insert(key.to_string(), value);
            }
            Err(error) => vm.library.error = Some(error),
        }
    }
    vm.library.meta = Value::Object(meta);
}

fn live_action(
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    vm: &mut ViewModel,
    command: &str,
    args: Value,
) {
    vm.library.error = None;
    match sink(command, args) {
        Ok(_) => {
            vm.library.notice = Some(format!("{command} completed"));
            live_refresh(sink, vm);
        }
        Err(error) => vm.library.error = Some(error),
    }
}

pub fn ui(ui: &mut Ui, vm: &mut ViewModel, theme: &Theme, backend: &dyn Backend) {
    let mut sink = backend_sink(backend);
    if vm.library.sort.is_empty() {
        vm.library.sort = "newest".to_string();
    }
    if !vm.library.loaded {
        vm.library.loaded = true;
        live_refresh(&mut sink, vm);
    }

    ui.heading("Library");
    ui.add_space(theme.space_xs);
    ui.horizontal(|ui| {
        let search = ui.add(
            egui::TextEdit::singleline(&mut vm.library.query)
                .hint_text("Search prompts, guidance, descriptions")
                .frame(theme.text_edit_frame()),
        );
        if search.has_focus() {
            vm.text_input_focused = true;
        }
        if search.changed() {
            live_refresh(&mut sink, vm);
        }
        if ui
            .checkbox(&mut vm.library.favorites_only, "Favorites only")
            .changed()
        {
            live_refresh(&mut sink, vm);
        }
        if ui.checkbox(&mut vm.library.trash_only, "Trash").changed() {
            live_refresh(&mut sink, vm);
        }
    });

    let presets: Vec<String> = vm
        .library
        .meta
        .get("presets")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Preset").color(theme.ink_2));
        egui::ComboBox::from_id_salt("library-preset")
            .selected_text(if vm.library.preset_filter.is_empty() {
                "any".to_string()
            } else {
                vm.library.preset_filter.clone()
            })
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(vm.library.preset_filter.is_empty(), "any")
                    .clicked()
                {
                    vm.library.preset_filter.clear();
                    live_refresh(&mut sink, vm);
                }
                for preset in &presets {
                    if ui
                        .selectable_label(vm.library.preset_filter == *preset, preset)
                        .clicked()
                    {
                        vm.library.preset_filter = preset.clone();
                        live_refresh(&mut sink, vm);
                    }
                }
            });
        ui.label(egui::RichText::new("Status").color(theme.ink_2));
        for status in ["any", "succeeded", "invalid", "cancelled", "failed"] {
            let active = if status == "any" {
                vm.library.status_filter.is_empty()
            } else {
                vm.library.status_filter == status
            };
            if ui.selectable_label(active, status).clicked() {
                vm.library.status_filter = if status == "any" {
                    String::new()
                } else {
                    status.to_string()
                };
                live_refresh(&mut sink, vm);
            }
        }
        ui.label(egui::RichText::new("Sort").color(theme.ink_2));
        for sort in ["newest", "oldest", "rating", "prompt"] {
            if ui.selectable_label(vm.library.sort == sort, sort).clicked() {
                vm.library.sort = sort.to_string();
                live_refresh(&mut sink, vm);
            }
        }
        let tag = ui.add(
            egui::TextEdit::singleline(&mut vm.library.tag_filter)
                .hint_text("tag")
                .desired_width(90.0)
                .frame(theme.text_edit_frame()),
        );
        if tag.has_focus() {
            vm.text_input_focused = true;
        }
        if tag.lost_focus() {
            live_refresh(&mut sink, vm);
        }
        if ui.button("Refresh").clicked() {
            live_refresh(&mut sink, vm);
        }
    });

    ui.horizontal_wrapped(|ui| {
        if ui.button("Export selected…").clicked()
            && let Some(path) = crate::platform::dialogs::pick_save_path("library.jsonl")
        {
            live_action(
                &mut sink,
                vm,
                "library_export",
                json!({
                    "request": {
                        "format": "jsonl",
                        "outputPath": path.to_string_lossy().to_string(),
                        "ids": vm.library.selected_generation_id.clone().into_iter().collect::<Vec<_>>(),
                    }
                }),
            );
        }
        if ui.button("Import…").clicked()
            && let Some(path) = crate::platform::dialogs::pick_import_json()
        {
            live_action(
                &mut sink,
                vm,
                "library_import",
                json!({ "request": { "path": path.to_string_lossy().to_string() } }),
            );
        }
        if ui.button("Back up…").clicked()
            && let Some(path) = crate::platform::dialogs::pick_save_path("prompt-studio-backup.zip")
        {
            live_action(
                &mut sink,
                vm,
                "library_backup",
                json!({ "request": { "outputPath": path.to_string_lossy().to_string() } }),
            );
        }
        if ui.button("Restore back up…").clicked()
            && let Some(path) = crate::platform::dialogs::pick_import_json()
        {
            live_action(
                &mut sink,
                vm,
                "library_restore",
                json!({ "request": { "backupPath": path.to_string_lossy().to_string() } }),
            );
        }
    });

    ui.label(
        egui::RichText::new(format!(
            "{} generation(s){}",
            vm.library.total,
            if vm.library.trash_only {
                " in trash"
            } else {
                ""
            }
        ))
        .color(theme.muted),
    );

    let ids: Vec<(String, String)> = vm
        .library
        .records
        .iter()
        .map(|record| {
            (
                record
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                record
                    .get("prompt")
                    .and_then(Value::as_str)
                    .unwrap_or("(no prompt)")
                    .to_string(),
            )
        })
        .collect();
    egui::ScrollArea::vertical()
        .id_salt("library-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (id, prompt) in ids {
                if ui
                    .selectable_label(
                        vm.library.selected_generation_id.as_deref() == Some(id.as_str()),
                        format!("{id}: {prompt}"),
                    )
                    .clicked()
                {
                    let was_selected =
                        vm.library.selected_generation_id.as_deref() == Some(id.as_str());
                    vm.library.selected_generation_id =
                        if was_selected { None } else { Some(id.clone()) };
                    if !was_selected {
                        match sink("library_get", json!({ "id": id })) {
                            Ok(value) => vm.library.detail = Some(value),
                            Err(error) => vm.library.error = Some(error),
                        }
                    }
                }
            }

            if let Some(selected) = vm.library.selected_generation_id.clone() {
                ui.add_space(theme.space_sm);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Favorite").clicked() {
                        live_action(
                            &mut sink,
                            vm,
                            "library_favorite",
                            json!({ "request": { "id": selected, "favorite": true, "asset": false } }),
                        );
                    }
                    if ui.button("Move to trash").clicked() {
                        live_action(
                            &mut sink,
                            vm,
                            "library_trash",
                            json!({ "request": { "generationIds": [selected] } }),
                        );
                    }
                    if ui.button("Restore").clicked() {
                        live_action(
                            &mut sink,
                            vm,
                            "library_restore_deleted",
                            json!({ "request": { "generationIds": [selected] } }),
                        );
                    }
                    if ui.button("Delete forever").clicked() {
                        live_action(
                            &mut sink,
                            vm,
                            "library_purge",
                            json!({
                                "request": {
                                    "generationIds": [selected],
                                    "assetIds": [],
                                    "confirm": true,
                                }
                            }),
                        );
                    }
                    if ui.button("Re-run").clicked() {
                        let preset = if vm.library.preset_filter.is_empty() {
                            vm.compose.preset_id.clone()
                        } else {
                            vm.library.preset_filter.clone()
                        };
                        live_action(
                            &mut sink,
                            vm,
                            "library_rerun",
                            json!({
                                "request": {
                                    "ids": [selected],
                                    "presetId": preset,
                                    "guidance": null,
                                }
                            }),
                        );
                    }
                });
                let mut note = vm
                    .library
                    .detail
                    .as_ref()
                    .and_then(|detail| detail.get("record"))
                    .and_then(|record| record.get("notes"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                ui.label(egui::RichText::new("Notes").color(theme.ink_2));
                let response = ui.add(
                    egui::TextEdit::multiline(&mut note)
                        .desired_rows(3)
                        .frame(theme.text_edit_frame()),
                );
                if response.has_focus() {
                    vm.text_input_focused = true;
                }
                if response.lost_focus() {
                    let id = selected.clone();
                    live_action(
                        &mut sink,
                        vm,
                        "library_update",
                        json!({ "request": { "id": id, "notes": note, "rating": Value::Null } }),
                    );
                }
                if let Some(detail) = &vm.library.detail {
                    let prompt = detail
                        .get("record")
                        .and_then(|record| record.get("prompt"))
                        .or_else(|| detail.get("prompt"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !prompt.is_empty() {
                        theme.output_frame().show(ui, |ui| {
                            ui.label(egui::RichText::new(prompt).color(theme.ink));
                        });
                    }
                }
            }

            if let Some(notice) = &vm.library.notice {
                ui.label(egui::RichText::new(notice).color(theme.success));
            }
            if let Some(error) = &vm.library.error {
                ui.label(egui::RichText::new(error).color(theme.danger));
            }
        });
}
