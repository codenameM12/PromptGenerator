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
    // A filter or search can remove the open item; never keep acting on a
    // generation that is no longer listed.
    if let Some(id) = vm.library.selected_generation_id.clone()
        && !vm
            .library
            .records
            .iter()
            .any(|record| record.get("id").and_then(Value::as_str) == Some(id.as_str()))
    {
        vm.library.selected_generation_id = None;
        vm.library.detail = None;
    }
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

/// Compact display of an ISO timestamp ("2026-11-20T09:38:57Z" -> "2026-11-20 09:38").
fn short_date(value: &str) -> String {
    value.chars().take(16).collect::<String>().replace('T', " ")
}

fn truncate_text(text: &str, limit: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= limit {
        flat
    } else {
        let mut cut: String = flat.chars().take(limit.saturating_sub(1)).collect();
        cut.push('…');
        cut
    }
}

fn record_str<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn load_detail(
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    vm: &mut ViewModel,
    id: &str,
) {
    match sink("library_get", json!({ "id": id })) {
        Ok(value) => vm.library.detail = Some(value),
        Err(error) => vm.library.error = Some(error),
    }
}

/// Runs a mutating command, then refreshes the list and the open detail.
fn detail_action(
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    vm: &mut ViewModel,
    command: &str,
    args: Value,
    notice: &str,
) {
    vm.library.error = None;
    match sink(command, args) {
        Ok(_) => {
            vm.library.notice = Some(notice.to_string());
            live_refresh(sink, vm);
            if let Some(id) = vm.library.selected_generation_id.clone() {
                // Items that left this view (trash/restore/purge) drop selection.
                let still_listed = vm
                    .library
                    .records
                    .iter()
                    .any(|record| record_str(record, "id") == id);
                if still_listed {
                    load_detail(sink, vm, &id);
                } else {
                    vm.library.selected_generation_id = None;
                    vm.library.detail = None;
                }
            }
        }
        Err(error) => vm.library.error = Some(error),
    }
}

fn draw_toolbar(
    ui: &mut Ui,
    vm: &mut ViewModel,
    theme: &Theme,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
) {
    ui.horizontal(|ui| {
        let search = ui.add(
            egui::TextEdit::singleline(&mut vm.library.query)
                .hint_text("Search prompts, guidance, descriptions")
                .desired_width(280.0)
                .frame(theme.text_edit_frame()),
        );
        if search.has_focus() {
            vm.text_input_focused = true;
        }
        let cleared = crate::widgets::clear_overlay(ui, &search, &mut vm.library.query, theme);
        if search.changed() || cleared {
            live_refresh(sink, vm);
        }
        if ui
            .checkbox(&mut vm.library.favorites_only, "Favorites")
            .changed()
        {
            live_refresh(sink, vm);
        }
        if ui.checkbox(&mut vm.library.trash_only, "Trash").changed() {
            vm.library.selected_generation_id = None;
            vm.library.detail = None;
            live_refresh(sink, vm);
        }
        ui.menu_button("Data ▾", |ui| {
            if ui.button("Export selected…").clicked()
                && let Some(path) = crate::platform::dialogs::pick_save_path("library.jsonl")
            {
                live_action(
                    sink,
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
                ui.close();
            }
            if ui.button("Import…").clicked()
                && let Some(path) = crate::platform::dialogs::pick_import_json()
            {
                live_action(
                    sink,
                    vm,
                    "library_import",
                    json!({ "request": { "path": path.to_string_lossy().to_string() } }),
                );
                ui.close();
            }
            ui.separator();
            if ui.button("Back up…").clicked()
                && let Some(path) =
                    crate::platform::dialogs::pick_save_path("prompt-studio-backup.zip")
            {
                live_action(
                    sink,
                    vm,
                    "library_backup",
                    json!({ "request": { "outputPath": path.to_string_lossy().to_string() } }),
                );
                ui.close();
            }
            if ui.button("Restore backup…").clicked()
                && let Some(path) = crate::platform::dialogs::pick_import_json()
            {
                live_action(
                    sink,
                    vm,
                    "library_restore",
                    json!({ "request": { "backupPath": path.to_string_lossy().to_string() } }),
                );
                ui.close();
            }
        });
        if ui.button("Refresh").clicked() {
            live_refresh(sink, vm);
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
                    live_refresh(sink, vm);
                }
                for preset in &presets {
                    if ui
                        .selectable_label(vm.library.preset_filter == *preset, preset)
                        .clicked()
                    {
                        vm.library.preset_filter = preset.clone();
                        live_refresh(sink, vm);
                    }
                }
            });
        ui.label(egui::RichText::new("Status").color(theme.ink_2));
        egui::ComboBox::from_id_salt("library-status")
            .selected_text(if vm.library.status_filter.is_empty() {
                "any".to_string()
            } else {
                vm.library.status_filter.clone()
            })
            .show_ui(ui, |ui| {
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
                        live_refresh(sink, vm);
                    }
                }
            });
        ui.label(egui::RichText::new("Sort").color(theme.ink_2));
        egui::ComboBox::from_id_salt("library-sort")
            .selected_text(vm.library.sort.clone())
            .show_ui(ui, |ui| {
                for sort in ["newest", "oldest", "rating", "prompt"] {
                    if ui.selectable_label(vm.library.sort == sort, sort).clicked() {
                        vm.library.sort = sort.to_string();
                        live_refresh(sink, vm);
                    }
                }
            });
        let tag = ui.add(
            egui::TextEdit::singleline(&mut vm.library.tag_filter)
                .hint_text("tag filter")
                .desired_width(100.0)
                .frame(theme.text_edit_frame()),
        );
        if tag.has_focus() {
            vm.text_input_focused = true;
        }
        let cleared = crate::widgets::clear_overlay(ui, &tag, &mut vm.library.tag_filter, theme);
        if tag.lost_focus() || cleared {
            live_refresh(sink, vm);
        }
    });
}

/// Ids of every generation matching the current filters (not just the loaded
/// page), fetched in pages of 500.
fn all_matching_ids(
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    vm: &ViewModel,
) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    loop {
        let mut args = live_query(vm, ids.len() as i64);
        args["query"]["limit"] = json!(500);
        let page = sink("library_query", args)?;
        let items = page
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if items.is_empty() {
            break;
        }
        ids.extend(
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .map(str::to_string),
        );
        if ids.len() as i64 >= page.get("total").and_then(Value::as_i64).unwrap_or(0) {
            break;
        }
    }
    Ok(ids)
}

/// Runs a bulk command on the ticked rows, then clears the ticks and refreshes.
fn bulk_action(
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    vm: &mut ViewModel,
    command: &str,
    notice: &str,
) {
    let ids: Vec<String> = vm.library.checked.iter().cloned().collect();
    if ids.is_empty() {
        return;
    }
    let mut request = json!({ "generationIds": ids, "assetIds": [] });
    if command == "library_purge" {
        request["confirm"] = json!(true);
    }
    vm.library.error = None;
    match sink(command, json!({ "request": request })) {
        Ok(_) => {
            vm.library.notice = Some(format!("{notice}: {} item(s)", ids.len()));
            vm.library.checked.clear();
            vm.library.check_anchor = None;
            vm.library.selected_generation_id = None;
            vm.library.detail = None;
            live_refresh(sink, vm);
        }
        Err(error) => vm.library.error = Some(error),
    }
}

fn draw_bulk_bar(
    ui: &mut Ui,
    vm: &mut ViewModel,
    theme: &Theme,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
) {
    let count = vm.library.checked.len();
    ui.horizontal_wrapped(|ui| {
        if ui.button("Select loaded").clicked() {
            vm.library.checked = vm
                .library
                .records
                .iter()
                .map(|record| record_str(record, "id").to_string())
                .collect();
        }
        if vm.library.total > vm.library.records.len() as i64
            && ui
                .button(format!("Select all {}", vm.library.total))
                .clicked()
        {
            match all_matching_ids(sink, vm) {
                Ok(ids) => vm.library.checked = ids.into_iter().collect(),
                Err(error) => vm.library.error = Some(error),
            }
        }
        if count > 0 {
            if ui.button("Clear selection").clicked() {
                vm.library.checked.clear();
                vm.library.check_anchor = None;
            }
            ui.label(egui::RichText::new(format!("{count} selected")).color(theme.accent));
        }
    });
    if count == 0 {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        if vm.library.trash_only {
            if ui.button(format!("Restore {count}")).clicked() {
                bulk_action(sink, vm, "library_restore_deleted", "Restored");
            }
            let confirm_id = egui::Id::new("library-bulk-purge-confirm");
            let armed = ui
                .data(|data| data.get_temp::<usize>(confirm_id))
                .is_some_and(|armed_count| armed_count == count);
            if armed {
                if ui
                    .button(
                        egui::RichText::new(format!("Confirm: delete {count} forever"))
                            .color(theme.danger),
                    )
                    .clicked()
                {
                    ui.data_mut(|data| data.remove::<usize>(confirm_id));
                    bulk_action(sink, vm, "library_purge", "Deleted permanently");
                }
                if ui.button("Cancel").clicked() {
                    ui.data_mut(|data| data.remove::<usize>(confirm_id));
                }
            } else if ui
                .button(
                    egui::RichText::new(format!("Delete {count} forever…")).color(theme.danger),
                )
                .clicked()
            {
                ui.data_mut(|data| data.insert_temp(confirm_id, count));
            }
        } else {
            if crate::widgets::trash_button(ui, theme, "library-bulk-trash", "Move selected to trash")
                .clicked()
            {
                bulk_action(sink, vm, "library_trash", "Moved to trash");
            }
            ui.label(egui::RichText::new(format!("Move {count} to trash")).color(theme.muted));
        }
    });
}

fn draw_list(
    ui: &mut Ui,
    vm: &mut ViewModel,
    theme: &Theme,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
) {
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
    if vm.library.records.is_empty() {
        ui.add_space(theme.space_sm);
        ui.label(
            egui::RichText::new(if vm.library.trash_only {
                "Trash is empty."
            } else {
                "Nothing here yet. Compose a prompt and it will appear in this list."
            })
            .color(theme.faint),
        );
        return;
    }
    draw_bulk_bar(ui, vm, theme, sink);
    let records = vm.library.records.clone();
    let mut clicked: Option<String> = None;
    let mut toggled: Option<(String, bool)> = None;
    let mut load_more = false;
    egui::ScrollArea::vertical()
        .id_salt("library-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for record in &records {
                let id = record_str(record, "id").to_string();
                let selected = vm.library.selected_generation_id.as_deref() == Some(id.as_str());
                let favorite = record
                    .get("favorite")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let mut job = egui::text::LayoutJob::default();
                // The short generated title leads; the prompt is a faint second line.
                let generated = record_str(record, "title");
                let (title, snippet) = if generated.is_empty() {
                    (truncate_text(record_str(record, "prompt"), 110), String::new())
                } else {
                    (
                        generated.to_string(),
                        truncate_text(record_str(record, "prompt"), 90),
                    )
                };
                job.append(
                    &if favorite {
                        format!("[fav] {title}")
                    } else {
                        title
                    },
                    0.0,
                    egui::TextFormat {
                        color: theme.ink,
                        font_id: egui::FontId::proportional(if snippet.is_empty() {
                            14.0
                        } else {
                            15.5
                        }),
                        ..Default::default()
                    },
                );
                if !snippet.is_empty() {
                    job.append(
                        &format!("
{snippet}"),
                        0.0,
                        egui::TextFormat {
                            color: theme.muted,
                            font_id: egui::FontId::proportional(12.5),
                            ..Default::default()
                        },
                    );
                }
                let mut meta = vec![record_str(record, "presetId").to_string()];
                let status = record_str(record, "status");
                if !status.is_empty() && status != "succeeded" {
                    meta.push(status.to_string());
                }
                meta.push(short_date(record_str(record, "createdAt")));
                job.append(
                    &format!("\n{}", meta.join(" · ")),
                    0.0,
                    egui::TextFormat {
                        color: theme.faint,
                        font_id: egui::FontId::proportional(12.0),
                        ..Default::default()
                    },
                );
                ui.horizontal_top(|ui| {
                    let mut ticked = vm.library.checked.contains(&id);
                    if ui.checkbox(&mut ticked, "").changed() {
                        toggled = Some((id.clone(), ticked));
                    }
                    let response = ui
                        .with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                            ui.add(egui::Button::selectable(selected, job).wrap())
                        })
                        .inner;
                    if response.clicked() {
                        clicked = Some(id.clone());
                    }
                });
            }
            if (records.len() as i64) < vm.library.total {
                ui.add_space(theme.space_xs);
                if ui
                    .button(format!(
                        "Load more ({} of {})",
                        records.len(),
                        vm.library.total
                    ))
                    .clicked()
                {
                    load_more = true;
                }
            }
        });
    if let Some((id, ticked)) = toggled {
        let shift = ui.input(|input| input.modifiers.shift);
        let range: Vec<String> = match (&vm.library.check_anchor, shift) {
            (Some(anchor), true) => {
                let from = records.iter().position(|r| record_str(r, "id") == anchor);
                let to = records.iter().position(|r| record_str(r, "id") == id);
                match (from, to) {
                    (Some(a), Some(b)) => records[a.min(b)..=a.max(b)]
                        .iter()
                        .map(|r| record_str(r, "id").to_string())
                        .collect(),
                    _ => vec![id.clone()],
                }
            }
            _ => vec![id.clone()],
        };
        for item in range {
            if ticked {
                vm.library.checked.insert(item);
            } else {
                vm.library.checked.remove(&item);
            }
        }
        vm.library.check_anchor = Some(id);
    }
    if load_more {
        match sink("library_query", live_query(vm, records.len() as i64)) {
            Ok(value) => {
                let more: Vec<Value> = value
                    .get("items")
                    .cloned()
                    .and_then(|items| serde_json::from_value(items).ok())
                    .unwrap_or_default();
                vm.library.records.extend(more);
                vm.library.total = value.get("total").and_then(Value::as_i64).unwrap_or(0);
            }
            Err(error) => vm.library.error = Some(error),
        }
    }
    if let Some(id) = clicked {
        if vm.library.selected_generation_id.as_deref() == Some(id.as_str()) {
            vm.library.selected_generation_id = None;
            vm.library.detail = None;
        } else {
            vm.library.selected_generation_id = Some(id.clone());
            load_detail(sink, vm, &id);
        }
    }
}

/// Hand-entered render facts (engine, sampler, steps, CFG, notes) for a result.
/// Seed and model are stored automatically and shown in the facts line.
fn draw_render_meta(
    ui: &mut Ui,
    vm: &mut ViewModel,
    theme: &Theme,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    selected: &str,
) {
    const FIELDS: [(&str, &str); 5] = [
        ("engine", "Engine (e.g. ComfyUI, Hailuo)"),
        ("sampler", "Sampler"),
        ("steps", "Steps"),
        ("cfg", "CFG"),
        ("notes", "Render notes"),
    ];
    let id = egui::Id::new(("render-meta", selected));
    let saved_id = id.with("saved");
    let mut meta: Value = match ui.data(|data| data.get_temp::<Value>(id)) {
        Some(meta) => meta,
        None => {
            let loaded = sink("library_render_meta_get", json!({ "id": selected }))
                .unwrap_or_else(|_| json!({}));
            ui.data_mut(|data| {
                data.insert_temp(id, loaded.clone());
                data.insert_temp(saved_id, loaded.clone());
            });
            loaded
        }
    };
    ui.add_space(theme.space_xs);
    ui.label(egui::RichText::new("Render details").color(theme.accent));
    let mut changed = false;
    for (key, label) in FIELDS {
        let mut value = meta
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        ui.label(egui::RichText::new(label).color(theme.ink_2));
        let response = ui.add(
            egui::TextEdit::singleline(&mut value)
                .desired_width(f32::INFINITY)
                .frame(theme.text_edit_frame()),
        );
        if response.has_focus() {
            vm.text_input_focused = true;
        }
        let cleared = crate::widgets::clear_overlay(ui, &response, &mut value, theme);
        if response.changed() || cleared {
            meta[key] = json!(value);
            changed = true;
        }
    }
    if changed {
        ui.data_mut(|data| data.insert_temp(id, meta.clone()));
    }
    let saved: Option<Value> = ui.data(|data| data.get_temp::<Value>(saved_id));
    let dirty = saved.as_ref() != Some(&meta);
    if ui
        .add_enabled(dirty, egui::Button::new("Save render details"))
        .clicked()
    {
        match sink(
            "library_render_meta_set",
            json!({ "id": selected, "meta": meta }),
        ) {
            Ok(stored) => {
                ui.data_mut(|data| {
                    data.insert_temp(id, stored.clone());
                    data.insert_temp(saved_id, stored);
                });
                vm.library.notice = Some("Render details saved".to_string());
            }
            Err(error) => vm.library.error = Some(error),
        }
    }
}

fn draw_detail(
    ui: &mut Ui,
    vm: &mut ViewModel,
    theme: &Theme,
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
) {
    let Some(selected) = vm.library.selected_generation_id.clone() else {
        ui.add_space(theme.space_md);
        ui.label(
            egui::RichText::new("Select a generation to see its prompt, notes and actions.")
                .color(theme.faint),
        );
        return;
    };
    let Some(detail) = vm.library.detail.clone() else {
        ui.label(egui::RichText::new("Loading…").color(theme.muted));
        return;
    };
    let record = detail.get("record").unwrap_or(&detail).clone();
    let favorite = record
        .get("favorite")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let rating = record.get("rating").and_then(Value::as_u64).unwrap_or(0);
    let in_trash = vm.library.trash_only;

    // Actions first so they are never pushed off-screen by long text.
    ui.horizontal_wrapped(|ui| {
        if !in_trash {
            if ui
                .button(if favorite { "Favorited" } else { "Favorite" })
                .clicked()
            {
                detail_action(
                    sink,
                    vm,
                    "library_favorite",
                    json!({ "request": { "id": selected, "favorite": !favorite, "asset": false } }),
                    if favorite {
                        "Removed from favorites"
                    } else {
                        "Added to favorites"
                    },
                );
            }
            if ui.button("Copy prompt").clicked() {
                ui.ctx().copy_text(record_str(&record, "prompt").to_string());
                vm.library.notice = Some("Prompt copied".into());
            }
            if ui
                .button("Re-run")
                .on_hover_text("Compose this again with the same inputs")
                .clicked()
            {
                let preset = {
                    let own = record_str(&record, "presetId");
                    if own.is_empty() {
                        vm.compose.preset_id.clone()
                    } else {
                        own.to_string()
                    }
                };
                detail_action(
                    sink,
                    vm,
                    "library_rerun",
                    json!({ "request": { "ids": [selected], "presetId": preset, "guidance": null } }),
                    "Re-run queued",
                );
            }
            if crate::widgets::trash_button(ui, theme, "library-trash", "Move to trash").clicked() {
                detail_action(
                    sink,
                    vm,
                    "library_trash",
                    json!({ "request": { "generationIds": [selected], "assetIds": [] } }),
                    "Moved to trash",
                );
            }
        } else {
            if ui.button("Restore").clicked() {
                detail_action(
                    sink,
                    vm,
                    "library_restore_deleted",
                    json!({ "request": { "generationIds": [selected] } }),
                    "Restored from trash",
                );
            }
            let confirm_id = egui::Id::new(("library-purge-confirm", selected.as_str()));
            let armed = ui.data(|data| data.get_temp::<bool>(confirm_id)).unwrap_or(false);
            if armed {
                if ui
                    .button(egui::RichText::new("Confirm delete forever").color(theme.danger))
                    .clicked()
                {
                    ui.data_mut(|data| data.remove::<bool>(confirm_id));
                    detail_action(
                        sink,
                        vm,
                        "library_purge",
                        json!({
                            "request": {
                                "generationIds": [selected],
                                "assetIds": [],
                                "confirm": true,
                            }
                        }),
                        "Deleted permanently",
                    );
                }
                if ui.button("Cancel").clicked() {
                    ui.data_mut(|data| data.remove::<bool>(confirm_id));
                }
            } else if ui
                .button(egui::RichText::new("Delete forever…").color(theme.danger))
                .clicked()
            {
                ui.data_mut(|data| data.insert_temp(confirm_id, true));
            }
        }
    });

    egui::ScrollArea::vertical()
        .id_salt("library-detail")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(theme.space_xs);
            let mut facts = vec![record_str(&record, "presetId").to_string()];
            if let Some(models) = record.get("models").and_then(Value::as_array) {
                let names: Vec<&str> = models.iter().filter_map(Value::as_str).collect();
                if !names.is_empty() {
                    facts.push(names.join(", "));
                }
            }
            facts.push(short_date(record_str(&record, "createdAt")));
            let status = record_str(&record, "status");
            if !status.is_empty() {
                facts.push(status.to_string());
            }
            ui.label(egui::RichText::new(facts.join(" · ")).color(theme.muted));

            ui.add_space(theme.space_xs);
            ui.label(egui::RichText::new("Prompt").color(theme.accent));
            crate::widgets::scroll_box(ui, theme, "library-prompt-box", 280.0, |ui| {
                crate::widgets::formatted_prompt(ui, theme, record_str(&record, "prompt"));
            });
            if let Some(negative) = record
                .get("negative")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
            {
                ui.add_space(theme.space_xs);
                ui.label(egui::RichText::new("Negative").color(theme.accent));
                crate::widgets::scroll_box(ui, theme, "library-negative-box", 120.0, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(negative).color(theme.ink_2)).wrap());
                });
            }

            if let Some(references) = detail.get("references").and_then(Value::as_array)
                && !references.is_empty()
            {
                ui.add_space(theme.space_xs);
                ui.label(egui::RichText::new("References").color(theme.accent));
                for (index, reference) in references.iter().enumerate() {
                    let name = reference
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("reference");
                    let weight = reference
                        .get("weight")
                        .and_then(Value::as_f64)
                        .unwrap_or(1.0);
                    ui.label(
                        egui::RichText::new(format!("{}. {name} (weight {weight:.2})", index + 1))
                            .color(theme.ink_2),
                    );
                }
            }

            ui.add_space(theme.space_sm);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(if rating > 0 { format!("Rating {rating}/5") } else { "Rating".to_string() }).color(theme.accent));
                for star in 1..=5u64 {
                    let on = star <= rating;
                    if ui
                        .selectable_label(on, star.to_string())
                        .on_hover_text(format!("{star} star(s)"))
                        .clicked()
                    {
                        let request = if rating == star {
                            json!({ "ids": [selected], "clearRating": true })
                        } else {
                            json!({ "ids": [selected], "rating": star })
                        };
                        detail_action(
                            sink,
                            vm,
                            "library_update",
                            json!({ "request": request }),
                            "Rating saved",
                        );
                    }
                }
            });

            ui.add_space(theme.space_xs);
            ui.label(egui::RichText::new("Notes").color(theme.accent));
            let note_id = egui::Id::new(("library-note", selected.as_str()));
            let saved = record
                .get("notes")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let mut note = ui
                .data(|data| data.get_temp::<String>(note_id))
                .unwrap_or_else(|| saved.clone());
            let response = ui.add(
                egui::TextEdit::multiline(&mut note)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("Private notes about this result")
                    .frame(theme.text_edit_frame()),
            );
            if response.has_focus() {
                vm.text_input_focused = true;
            }
            let cleared = crate::widgets::clear_overlay(ui, &response, &mut note, theme);
            if response.changed() || cleared {
                let value = note.clone();
                ui.data_mut(|data| data.insert_temp(note_id, value));
            }
            let dirty = note != saved;
            if ui.add_enabled(dirty, egui::Button::new("Save notes")).clicked() {
                ui.data_mut(|data| data.remove::<String>(note_id));
                detail_action(
                    sink,
                    vm,
                    "library_update",
                    json!({ "request": { "ids": [selected], "notes": note } }),
                    "Notes saved",
                );
            }

            draw_render_meta(ui, vm, theme, sink, &selected);

            if let Some(tags) = record.get("tags").and_then(Value::as_array)
                && !tags.is_empty()
            {
                ui.add_space(theme.space_xs);
                let names: Vec<&str> = tags.iter().filter_map(Value::as_str).collect();
                ui.label(egui::RichText::new(format!("Tags: {}", names.join(", "))).color(theme.muted));
            }
        });
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
    draw_toolbar(ui, vm, theme, &mut sink);
    // Messages sit above the panes; below them they ran under the status bar.
    if let Some(notice) = vm.library.notice.clone() {
        ui.label(egui::RichText::new(notice).color(theme.success));
    }
    if let Some(error) = vm.library.error.clone() {
        ui.add(egui::Label::new(egui::RichText::new(error).color(theme.danger)).wrap());
    }
    ui.add_space(theme.space_xs);

    // Two panes: a compact list on the left, the selected item with its
    // actions on the right.
    let height = (ui.available_height() - 8.0).max(120.0);
    let total_width = ui.available_width();
    let list_width = (total_width * 0.42).clamp(240.0, 520.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(list_width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_height(height);
                draw_list(ui, vm, theme, &mut sink);
            },
        );
        ui.separator();
        let detail_width = (ui.available_width() - 4.0).max(200.0);
        ui.allocate_ui_with_layout(
            egui::vec2(detail_width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_height(height);
                draw_detail(ui, vm, theme, &mut sink);
            },
        );
    });
}
