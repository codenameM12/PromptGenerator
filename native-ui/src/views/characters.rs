//! Characters tab: named sets of reference images (people, products...).
//! Images are copied into the app workspace; Compose loads them on demand.
//! See docs/spec-v2.md.

use std::collections::HashMap;

use egui::Ui;
use serde_json::{Value, json};

use crate::backend::{Asset, AssetKind, Backend};
use crate::theme::Theme;
use crate::thumbnails::{PICKER_THUMB, ThumbnailCache};
use crate::view_model::{ViewId, ViewModel};

type Sink<'a> = &'a mut dyn FnMut(&str, Value) -> Result<Value, String>;

fn text_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn refresh_list(sink: Sink, vm: &mut ViewModel) {
    match sink("characters_list", json!({})) {
        Ok(list) => {
            vm.characters.list = list.as_array().cloned().unwrap_or_default();
            vm.characters_version += 1;
            vm.characters.error = None;
        }
        Err(error) => vm.characters.error = Some(error),
    }
}

fn load_detail(sink: Sink, vm: &mut ViewModel, id: &str) {
    match sink("characters_get", json!({ "id": id })) {
        Ok(detail) => {
            vm.characters.name_draft = text_of(&detail, "name").to_string();
            vm.characters.description_draft = text_of(&detail, "description").to_string();
            vm.characters.drafts = HashMap::new();
            vm.characters.detail = Some(detail);
            vm.characters.error = None;
        }
        Err(error) => vm.characters.error = Some(error),
    }
}

fn run(sink: Sink, vm: &mut ViewModel, command: &str, args: Value, notice: &str) -> bool {
    match sink(command, args) {
        Ok(_) => {
            vm.characters.notice = Some(notice.to_string());
            vm.characters.error = None;
            true
        }
        Err(error) => {
            vm.characters.error = Some(error);
            false
        }
    }
}

/// Names the image after its description (short title, words not used by the
/// set's other images) and renames the app's own copy of the file.
fn autoname(sink: Sink, vm: &mut ViewModel, image_id: &str) {
    match sink("characters_image_autoname", json!({ "imageId": image_id })) {
        Ok(value) => {
            if value.get("renamed").and_then(Value::as_bool) == Some(true) {
                vm.characters.notice = Some(format!(
                    "Renamed to \"{}\"",
                    text_of(&value, "fileName")
                ));
                vm.characters.error = None;
            }
        }
        Err(error) => vm.characters.error = Some(error),
    }
}

/// Describes the given images with the describer model, saves the text as
/// each description, then names the image from it.
fn describe_images(sink: Sink, vm: &mut ViewModel, character_id: &str, image_ids: &[String]) {
    let mut done = 0;
    for image_id in image_ids {
        let loaded = sink(
            "characters_load",
            json!({ "id": character_id, "imageIds": [image_id] }),
        );
        let asset_id = match loaded {
            Ok(value) => value["images"][0]["assetId"].as_str().map(str::to_string),
            Err(error) => {
                vm.characters.error = Some(error);
                return;
            }
        };
        let Some(asset_id) = asset_id else { continue };
        let described = sink(
            "pipeline_describe",
            json!({ "request": {
                "assetId": asset_id,
                "editId": Value::Null,
                "framePaths": [],
                "mode": "image",
                "hint": Value::Null,
                "forceRefresh": false,
                "tag": "<image1>"
            }}),
        );
        let text = match described {
            Ok(value) => crate::views::compose::describe_text(&value),
            Err(error) => {
                vm.characters.error = Some(error);
                return;
            }
        };
        if text.trim().is_empty() {
            vm.characters.error = Some("The describer returned no text".to_string());
            return;
        }
        let saved = vm
            .characters
            .detail
            .as_ref()
            .and_then(|detail| detail.get("images").and_then(Value::as_array))
            .and_then(|images| images.iter().find(|image| text_of(image, "id") == image_id))
            .cloned()
            .unwrap_or(Value::Null);
        let updated = sink(
            "characters_image_update",
            json!({
                "imageId": image_id,
                "fileName": text_of(&saved, "fileName"),
                "role": text_of(&saved, "role"),
                "description": text.trim(),
                "isDefault": saved.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
            }),
        );
        if let Err(error) = updated {
            vm.characters.error = Some(error);
            return;
        }
        autoname(sink, vm, image_id);
        done += 1;
    }
    vm.characters.notice = Some(format!("Described {done} image(s)"));
    vm.characters.error = None;
}

fn draw_list(ui: &mut Ui, vm: &mut ViewModel, theme: &Theme, sink: Sink) {
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut vm.characters.new_name)
                .hint_text("New name")
                .desired_width(150.0)
                .frame(theme.text_edit_frame()),
        );
        if response.has_focus() {
            vm.text_input_focused = true;
        }
        let name = vm.characters.new_name.trim().to_string();
        if ui
            .add_enabled(!name.is_empty(), egui::Button::new("Create"))
            .clicked()
            && let Ok(created) = sink("characters_create", json!({ "name": name }))
        {
            vm.characters.new_name.clear();
            let id = text_of(&created, "id").to_string();
            refresh_list(sink, vm);
            vm.characters.selected = Some(id.clone());
            load_detail(sink, vm, &id);
        }
    });
    ui.add_space(theme.space_xs);
    if vm.characters.list.is_empty() {
        ui.label(egui::RichText::new("Nothing here yet. Create a name, then import images.").color(theme.faint));
        return;
    }
    let list = vm.characters.list.clone();
    let mut clicked: Option<String> = None;
    let mut zones: Vec<(egui::Rect, String)> = Vec::new();
    egui::ScrollArea::vertical()
        .id_salt("characters-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for item in &list {
                let id = text_of(item, "id").to_string();
                let selected = vm.characters.selected.as_deref() == Some(id.as_str());
                let count = item.get("imageCount").and_then(Value::as_i64).unwrap_or(0);
                let defaults = item.get("defaultCount").and_then(Value::as_i64).unwrap_or(0);
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    text_of(item, "name"),
                    0.0,
                    egui::TextFormat {
                        color: theme.ink,
                        font_id: egui::FontId::proportional(15.0),
                        ..Default::default()
                    },
                );
                job.append(
                    &format!("\n{count} image(s), {defaults} default"),
                    0.0,
                    egui::TextFormat {
                        color: theme.faint,
                        font_id: egui::FontId::proportional(12.0),
                        ..Default::default()
                    },
                );
                let response = ui
                    .with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                        ui.add(egui::Button::selectable(selected, job))
                    })
                    .inner;
                zones.push((response.rect, id.clone()));
                if response.clicked() {
                    clicked = Some(id);
                }
            }
        });
    vm.characters.drop_zones.extend(zones);
    if let Some(id) = clicked {
        vm.characters.selected = Some(id.clone());
        load_detail(sink, vm, &id);
    }
}

fn draw_detail(
    ui: &mut Ui,
    vm: &mut ViewModel,
    theme: &Theme,
    sink: Sink,
    thumbnails: &mut ThumbnailCache,
) {
    let Some(id) = vm.characters.selected.clone() else {
        ui.label(egui::RichText::new("Select or create one on the left.").color(theme.faint));
        return;
    };
    let Some(detail) = vm.characters.detail.clone() else {
        return;
    };

    ui.horizontal_wrapped(|ui| {
        if ui.button("Show in Library").clicked() {
            vm.library.character_filter = id.clone();
            vm.library.loaded = false;
            vm.navigate(ViewId::Library);
        }
        if ui.button("Import images…").clicked()
            && let Some(paths) = crate::platform::dialogs::pick_import_files()
        {
            let paths: Vec<String> = paths
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect();
            if run(
                sink,
                vm,
                "characters_add_images",
                json!({ "id": id, "paths": paths }),
                "Images added",
            ) {
                refresh_list(sink, vm);
                load_detail(sink, vm, &id);
            }
        }
        if ui.button("Import folder…").clicked()
            && let Some(folder) = crate::platform::dialogs::pick_import_folder()
            && run(
                sink,
                vm,
                "characters_add_images",
                json!({ "id": id, "paths": [folder.to_string_lossy()] }),
                "Folder imported",
            )
        {
            refresh_list(sink, vm);
            load_detail(sink, vm, &id);
        }
        let confirm_id = egui::Id::new(("characters-delete", id.as_str()));
        let armed = ui.data(|data| data.get_temp::<bool>(confirm_id)).unwrap_or(false);
        if armed {
            if ui
                .button(egui::RichText::new("Confirm: delete it and its images").color(theme.danger))
                .clicked()
            {
                ui.data_mut(|data| data.remove::<bool>(confirm_id));
                if run(sink, vm, "characters_delete", json!({ "id": id }), "Deleted") {
                    vm.characters.selected = None;
                    vm.characters.detail = None;
                    refresh_list(sink, vm);
                }
            }
            if ui.button("Cancel").clicked() {
                ui.data_mut(|data| data.remove::<bool>(confirm_id));
            }
        } else if crate::widgets::trash_button(ui, theme, "characters-delete", "Delete (with its images)")
            .clicked()
        {
            ui.data_mut(|data| data.insert_temp(confirm_id, true));
        }
    });
    ui.label(
        egui::RichText::new("Tip: drop image files or folders on a name, or on this panel, to add them.")
            .color(theme.faint),
    );
    ui.add_space(theme.space_xs);

    ui.label(egui::RichText::new("Name").color(theme.ink_2));
    let name_response = ui.add(
        egui::TextEdit::singleline(&mut vm.characters.name_draft)
            .desired_width(f32::INFINITY)
            .frame(theme.text_edit_frame()),
    );
    if name_response.has_focus() {
        vm.text_input_focused = true;
    }
    ui.label(egui::RichText::new("Description (optional)").color(theme.ink_2));
    let description_response = ui.add(
        egui::TextEdit::multiline(&mut vm.characters.description_draft)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .frame(theme.text_edit_frame()),
    );
    if description_response.has_focus() {
        vm.text_input_focused = true;
    }
    crate::widgets::clear_overlay(ui, &description_response, &mut vm.characters.description_draft, theme);
    let dirty = vm.characters.name_draft.trim() != text_of(&detail, "name")
        || vm.characters.description_draft.trim() != text_of(&detail, "description");
    if ui
        .add_enabled(dirty && !vm.characters.name_draft.trim().is_empty(), egui::Button::new("Save details"))
        .clicked()
        && run(
            sink,
            vm,
            "characters_update",
            json!({
                "id": id,
                "name": vm.characters.name_draft,
                "description": vm.characters.description_draft,
            }),
            "Saved",
        )
    {
        refresh_list(sink, vm);
        load_detail(sink, vm, &id);
    }

    ui.add_space(theme.space_sm);
    ui.label(egui::RichText::new("Images").color(theme.accent));
    ui.label(
        egui::RichText::new(
            "Tick Default for the images loaded when this is added in Compose. With none ticked, one random image is used.",
        )
        .color(theme.faint),
    );
    let images = detail
        .get("images")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut remove: Option<String> = None;
    let mut save: Option<String> = None;
    let mut rename_now: Option<String> = None;
    let mut describe: Vec<String> = Vec::new();
    let mut save_all = false;
    ui.horizontal_wrapped(|ui| {
        if ui.button("Select all defaults").clicked() {
            for image in &images {
                if let Some(draft) = vm.characters.drafts.get_mut(text_of(image, "id")) {
                    draft.3 = true;
                }
            }
        }
        if ui.button("Clear defaults").clicked() {
            for image in &images {
                if let Some(draft) = vm.characters.drafts.get_mut(text_of(image, "id")) {
                    draft.3 = false;
                }
            }
        }
        if ui.button("Save all changes").clicked() {
            save_all = true;
        }
        let missing: Vec<String> = images
            .iter()
            .filter(|image| text_of(image, "description").trim().is_empty())
            .map(|image| text_of(image, "id").to_string())
            .collect();
        if ui
            .add_enabled(
                !missing.is_empty(),
                egui::Button::new(format!("Describe missing ({})", missing.len())),
            )
            .on_hover_text("Runs the describer on images without a description, then names them")
            .clicked()
        {
            describe = missing;
        }
        if ui
            .button("Describe all")
            .on_hover_text("Re-describes every image and renames them")
            .clicked()
        {
            describe = images
                .iter()
                .map(|image| text_of(image, "id").to_string())
                .collect();
        }
    });
    egui::ScrollArea::vertical()
        .id_salt("characters-images")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Compact cards in rows sized to the width, so more fit together.
            const CARD_WIDTH: f32 = 236.0;
            const INNER: f32 = CARD_WIDTH - 12.0;
            let spacing = ui.spacing().item_spacing.x;
            let columns = (((ui.available_width() + spacing) / (CARD_WIDTH + spacing + 30.0))
                .floor() as usize)
                .max(1);
            for row in images.chunks(columns) {
                ui.horizontal_top(|ui| {
                    for image in row {
                        let image_id = text_of(image, "id").to_string();
                        let draft = vm
                            .characters
                            .drafts
                            .entry(image_id.clone())
                            .or_insert_with(|| {
                                (
                                    text_of(image, "fileName").to_string(),
                                    text_of(image, "role").to_string(),
                                    text_of(image, "description").to_string(),
                                    image.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
                                )
                            });
                        let saved = (
                            text_of(image, "fileName").to_string(),
                            text_of(image, "role").to_string(),
                            text_of(image, "description").to_string(),
                            image.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
                        );
                        let changed = *draft != saved;
                        let stored_path = text_of(image, "path").to_string();
                        theme.output_frame().show(ui, |ui| {
                            ui.set_width(CARD_WIDTH);
                            ui.vertical(|ui| {
                                let preview = Asset {
                                    id: image_id.clone(),
                                    kind: AssetKind::Image,
                                    path: stored_path.clone(),
                                    display_name: draft.0.clone(),
                                    hash: String::new(),
                                    width: None,
                                    height: None,
                                    fps: None,
                                    duration_ms: None,
                                    codec: None,
                                    has_audio: false,
                                    thumbnail_path: Some(stored_path.clone()),
                                    proxy_path: None,
                                    missing: false,
                                    favorite: false,
                                    created_at: String::new(),
                                };
                                thumbnails.paint(ui, &preview, egui::vec2(INNER, 132.0));
                                ui.add(
                                    egui::TextEdit::singleline(&mut draft.0)
                                        .hint_text("Name")
                                        .desired_width(INNER)
                                        .frame(theme.text_edit_frame()),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut draft.1)
                                        .hint_text("Role (front, profile...)")
                                        .desired_width(INNER)
                                        .frame(theme.text_edit_frame()),
                                );
                                egui::CollapsingHeader::new(if draft.2.trim().is_empty() {
                                    "Description"
                                } else {
                                    "Description (set)"
                                })
                                .id_salt(("characters-desc", image_id.as_str()))
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut draft.2)
                                            .desired_rows(3)
                                            .desired_width(INNER)
                                            .frame(theme.text_edit_frame()),
                                    );
                                });
                                ui.horizontal_wrapped(|ui| {
                                    ui.checkbox(&mut draft.3, "Default");
                                    if ui.add_enabled(changed, egui::Button::new("Save")).clicked() {
                                        save = Some(image_id.clone());
                                    }
                                    if ui
                                        .add_enabled(
                                            !changed && !draft.2.trim().is_empty(),
                                            egui::Button::new("Name"),
                                        )
                                        .on_hover_text("Name this image (and its file) from its description")
                                        .clicked()
                                    {
                                        rename_now = Some(image_id.clone());
                                    }
                                    if ui
                                        .button("Describe")
                                        .on_hover_text("Describe this image with the describer model")
                                        .clicked()
                                    {
                                        describe = vec![image_id.clone()];
                                    }
                                    if crate::widgets::trash_button(ui, theme, &image_id, "Remove image").clicked() {
                                        remove = Some(image_id.clone());
                                    }
                                });
                            });
                        });
                    }
                });
                ui.add_space(theme.space_xs);
            }
        });
    if let Some(image_id) = save
        && let Some(draft) = vm.characters.drafts.get(&image_id).cloned()
        && run(
            sink,
            vm,
            "characters_image_update",
            json!({
                "imageId": image_id,
                "fileName": draft.0,
                "role": draft.1,
                "description": draft.2,
                "isDefault": draft.3,
            }),
            "Image saved",
        )
    {
        // A newly written description names the file after itself, unless the
        // name was typed by hand in this same save.
        let before = images
            .iter()
            .find(|image| text_of(image, "id") == image_id)
            .cloned()
            .unwrap_or(Value::Null);
        let description_changed =
            draft.2.trim() != text_of(&before, "description").trim() && !draft.2.trim().is_empty();
        let name_typed = draft.0.trim() != text_of(&before, "fileName").trim();
        if description_changed && !name_typed {
            autoname(sink, vm, &image_id);
        }
        refresh_list(sink, vm);
        load_detail(sink, vm, &id);
    }
    if save_all {
        let changed: Vec<(String, (String, String, String, bool))> = images
            .iter()
            .filter_map(|image| {
                let image_id = text_of(image, "id").to_string();
                let draft = vm.characters.drafts.get(&image_id)?.clone();
                let saved = (
                    text_of(image, "fileName").to_string(),
                    text_of(image, "role").to_string(),
                    text_of(image, "description").to_string(),
                    image.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
                );
                (draft != saved).then_some((image_id, draft))
            })
            .collect();
        let count = changed.len();
        for (image_id, draft) in changed {
            let _ = run(
                sink,
                vm,
                "characters_image_update",
                json!({ "imageId": image_id, "fileName": draft.0, "role": draft.1,
                        "description": draft.2, "isDefault": draft.3 }),
                "Image saved",
            );
        }
        vm.characters.notice = Some(format!("Saved {count} image(s)"));
        refresh_list(sink, vm);
        load_detail(sink, vm, &id);
    }
    if !describe.is_empty() {
        describe_images(sink, vm, &id, &describe);
        refresh_list(sink, vm);
        load_detail(sink, vm, &id);
        vm.media_reload += 1;
        vm.edits_version += 1;
    }
    if let Some(image_id) = rename_now {
        autoname(sink, vm, &image_id);
        refresh_list(sink, vm);
        load_detail(sink, vm, &id);
    }
    if let Some(image_id) = remove
        && run(
            sink,
            vm,
            "characters_image_remove",
            json!({ "imageId": image_id }),
            "Image removed",
        )
    {
        refresh_list(sink, vm);
        load_detail(sink, vm, &id);
    }
}

pub fn ui(
    ui: &mut Ui,
    vm: &mut ViewModel,
    theme: &Theme,
    backend: &dyn Backend,
    thumbnails: &mut ThumbnailCache,
) {
    let mut call = |command: &str, args: Value| backend.call(command, args).map_err(|e| crate::errors::error_text_of(&e));
    let sink: Sink = &mut call;
    if !vm.characters.loaded {
        vm.characters.loaded = true;
        refresh_list(sink, vm);
        if let Some(id) = vm.characters.selected.clone()
            && vm.characters.detail.is_none()
        {
            load_detail(sink, vm, &id);
        }
    }
    vm.characters.drop_zones.clear();
    ui.heading("Characters");
    ui.label(
        egui::RichText::new(
            "Named sets of reference images (people, products). Stored inside the app; add them from Compose.",
        )
        .color(theme.muted),
    );
    if let Some(notice) = vm.characters.notice.clone() {
        ui.label(egui::RichText::new(notice).color(theme.success));
    }
    if let Some(error) = vm.characters.error.clone() {
        ui.add(egui::Label::new(egui::RichText::new(error).color(theme.danger)).wrap());
    }
    ui.add_space(theme.space_xs);
    let height = (ui.available_height() - 6.0).max(200.0);
    let detail_area = egui::Rect::from_min_size(
        egui::pos2(ui.cursor().min.x + 270.0, ui.cursor().min.y),
        egui::vec2((ui.available_width() - 270.0).max(100.0), height),
    );
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(260.0, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_height(height);
                draw_list(ui, vm, theme, sink);
            },
        );
        ui.separator();
        let rest = (ui.available_width() - 4.0).max(320.0);
        ui.allocate_ui_with_layout(
            egui::vec2(rest, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_height(height);
                draw_detail(ui, vm, theme, sink, thumbnails);
            },
        );
        if let Some(selected) = vm.characters.selected.clone() {
            vm.characters.drop_zones.push((detail_area, selected));
        }
    });
}
