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
                if response.clicked() {
                    clicked = Some(id);
                }
            }
        });
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
    egui::ScrollArea::vertical()
        .id_salt("characters-images")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for image in &images {
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
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
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
                        thumbnails.paint(ui, &preview, PICKER_THUMB);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Name");
                                ui.add(
                                    egui::TextEdit::singleline(&mut draft.0)
                                        .desired_width(150.0)
                                        .frame(theme.text_edit_frame()),
                                );
                                ui.label("Role");
                                ui.add(
                                    egui::TextEdit::singleline(&mut draft.1)
                                        .hint_text("front, profile...")
                                        .desired_width(130.0)
                                        .frame(theme.text_edit_frame()),
                                );
                            });
                            ui.add(
                                egui::TextEdit::multiline(&mut draft.2)
                                    .hint_text("Description of this image (optional)")
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY)
                                    .frame(theme.text_edit_frame()),
                            );
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut draft.3, "Default");
                                if ui.add_enabled(changed, egui::Button::new("Save")).clicked() {
                                    save = Some(image_id.clone());
                                }
                                if crate::widgets::trash_button(ui, theme, &image_id, "Remove image").clicked() {
                                    remove = Some(image_id.clone());
                                }
                            });
                        });
                    });
                });
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
    }
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
    });
}
