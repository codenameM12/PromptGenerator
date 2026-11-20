use egui::Ui;

use crate::backend::{Asset, AssetKind, Backend};
use crate::errors::error_text_of;
use crate::media_access::AllowedRoots;
use crate::theme::Theme;
use crate::thumbnails::{GRID_THUMB, ThumbnailCache};
use crate::view_model::ViewModel;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::compose::CallSink;

/// Media grid state driven by the harness and by the egui shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFlow {
    pub assets: Vec<Asset>,
    pub selected_asset_id: Option<String>,
    pub error: Option<String>,
    pub status: String,
    /// Set once the shell issued the live mount refresh.
    #[serde(default)]
    pub mounted: bool,
    /// Last `ViewModel::media_reload` this grid loaded.
    #[serde(default)]
    pub seen_reload: u64,
}

impl Default for MediaFlow {
    fn default() -> Self {
        Self {
            assets: Vec::new(),
            selected_asset_id: None,
            error: None,
            status: String::new(),
            mounted: false,
            seen_reload: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSnapshot {
    pub assets: Vec<Asset>,
    pub selected_asset_id: Option<String>,
    pub error: Option<String>,
    pub status: String,
}

impl MediaFlow {
    pub fn snapshot(&self) -> MediaSnapshot {
        MediaSnapshot {
            assets: self.assets.clone(),
            selected_asset_id: self.selected_asset_id.clone(),
            error: self.error.clone(),
            status: self.status.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MediaAction {
    Mount,
    SelectAsset {
        asset_id: String,
    },
    ImportPaths {
        paths: Vec<String>,
    },
    ImportClipboard {
        file_name: Option<String>,
        bytes_len: usize,
    },
    RemoveAsset {
        asset_id: String,
    },
}

pub fn dispatch(flow: &mut MediaFlow, action: MediaAction, call: CallSink) {
    match action {
        MediaAction::Mount => match call("assets_list", json!({})) {
            Ok(value) => {
                flow.assets = serde_json::from_value(value).unwrap_or_default();
                flow.error = None;
            }
            Err(error) => flow.error = Some(error),
        },
        MediaAction::SelectAsset { asset_id } => {
            if flow.assets.iter().any(|asset| asset.id == asset_id) {
                flow.selected_asset_id = Some(asset_id);
            }
        }
        MediaAction::ImportPaths { paths } => {
            if paths.is_empty() {
                return;
            }
            let args = json!({ "request": { "paths": paths } });
            match call("assets_import", args) {
                Ok(value) => {
                    let results: Vec<Value> = value.as_array().cloned().unwrap_or_default();
                    let imported = results
                        .iter()
                        .filter(|item| {
                            item.get("assetId").and_then(Value::as_str).is_some()
                                && item.get("error").and_then(Value::as_str).is_none()
                        })
                        .count();
                    flow.status = format!("Imported {imported} item(s)");
                    flow.error = None;
                    // A successful import refreshes the grid, as App.tsx does.
                    if imported > 0 {
                        if let Ok(value) = call("assets_list", json!({})) {
                            flow.assets = serde_json::from_value(value).unwrap_or_default();
                        }
                    }
                }
                Err(error) => flow.error = Some(error),
            }
        }
        MediaAction::ImportClipboard {
            file_name,
            bytes_len,
        } => {
            let name = file_name.unwrap_or_else(|| "clipboard.png".into());
            let bytes = vec![0u8; bytes_len];
            let args = json!({
                "request": { "fileName": name, "bytes": bytes }
            });
            match call("assets_import_clipboard", args) {
                Ok(_) => {
                    flow.status = "Imported clipboard image".into();
                    flow.error = None;
                }
                Err(error) => flow.error = Some(error),
            }
        }
        MediaAction::RemoveAsset { asset_id } => {
            let args = json!({
                "request": { "generationIds": [], "assetIds": [asset_id] }
            });
            match call("library_trash", args) {
                Ok(_) => {
                    if flow.selected_asset_id.as_deref() == Some(asset_id.as_str()) {
                        flow.selected_asset_id = None;
                    }
                    if let Ok(value) = call("assets_list", json!({})) {
                        flow.assets = serde_json::from_value(value).unwrap_or_default();
                    }
                }
                Err(error) => flow.error = Some(error),
            }
        }
    }
}

/// Maps the call sink onto the backend methods available today.
pub fn backend_sink(
    backend: &dyn Backend,
) -> impl FnMut(&str, Value) -> Result<Value, String> + '_ {
    move |method: &str, args: Value| -> Result<Value, String> {
        let request = args.get("request").cloned().unwrap_or(Value::Null);
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
                    Ok(results) => serde_json::to_value(results).map_err(|error| error.to_string()),
                    Err(error) => Err(error_text_of(&error)),
                }
            }
            "assets_import_clipboard" => {
                let name = request.get("fileName").and_then(Value::as_str);
                let bytes: Vec<u8> = request
                    .get("bytes")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_u64().map(|value| value as u8))
                            .collect()
                    })
                    .unwrap_or_default();
                match backend.import_clipboard(name, &bytes) {
                    Ok(results) => serde_json::to_value(results).map_err(|error| error.to_string()),
                    Err(error) => Err(error_text_of(&error)),
                }
            }
            // Generic commands go through the shared allowlisted seam; the
            // real backend resolves them in-process (or reports not-implemented
            // with a structured error rather than a fabricated result).
            other => backend
                .call(other, args)
                .map_err(|error| error_text_of(&error)),
        }
    }
}

thread_local! {
    static FLOW: std::cell::RefCell<MediaFlow> = std::cell::RefCell::new(MediaFlow::default());
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
        if !flow.mounted {
            flow.mounted = true;
            dispatch(&mut flow, MediaAction::Mount, &mut sink);
        }
        if flow.seen_reload != vm.media_reload {
            flow.seen_reload = vm.media_reload;
            dispatch(&mut flow, MediaAction::Mount, &mut sink);
        }
        if !vm.assets.is_empty() && flow.assets.len() != vm.assets.len() {
            dispatch(&mut flow, MediaAction::Mount, &mut sink);
        }

        ui.heading("Media");
        if vm.drop_hint {
            ui.label(
                egui::RichText::new("Drop files or folders here to import them")
                    .color(theme.accent),
            );
        } else {
            ui.label(
                egui::RichText::new("Tip: drag files or folders onto the window to import.")
                    .color(theme.faint),
            );
        }
        ui.add_space(theme.space_xs);
        ui.horizontal(|ui| {
            if ui.button("Add files…").clicked()
                && let Some(paths) = crate::platform::dialogs::pick_import_files()
            {
                let values: Vec<String> = paths
                    .iter()
                    .map(|path| path.to_string_lossy().to_string())
                    .collect();
                match crate::import::import_paths(backend, access, &values) {
                    Ok(results) => {
                        flow.status = format!("Imported {} item(s)", results.len());
                        dispatch(&mut flow, MediaAction::Mount, &mut sink);
                    }
                    Err(error) => flow.error = Some(error_text_of(&error)),
                }
            }
            if ui.button("Add folder…").clicked()
                && let Some(folder) = crate::platform::dialogs::pick_import_folder()
            {
                let value = folder.to_string_lossy().to_string();
                match crate::import::import_paths(backend, access, &[value]) {
                    Ok(results) => {
                        flow.status = format!("Imported {} item(s)", results.len());
                        dispatch(&mut flow, MediaAction::Mount, &mut sink);
                    }
                    Err(error) => flow.error = Some(error_text_of(&error)),
                }
            }
            if ui.button("Paste image").clicked() {
                match crate::platform::clipboard::read_image() {
                    Ok(image) => match crate::import::import_clipboard(backend, &image) {
                        Ok(results) => {
                            flow.status = format!("Imported {} item(s)", results.len());
                            dispatch(&mut flow, MediaAction::Mount, &mut sink);
                        }
                        Err(error) => flow.error = Some(error_text_of(&error)),
                    },
                    Err(error) => flow.error = Some(error_text_of(&error)),
                }
            }
            if ui.button("Refresh").clicked() {
                dispatch(&mut flow, MediaAction::Mount, &mut sink);
            }
        });
        // Status and errors go above the grid: below an auto-expanding scroll
        // area they were pushed half under the window's status bar.
        if !flow.status.is_empty() {
            ui.label(egui::RichText::new(&flow.status).color(theme.muted));
        }
        if let Some(error) = &flow.error {
            ui.add(egui::Label::new(egui::RichText::new(error).color(theme.danger)).wrap());
        }
        if let Some(error) = thumbnails.last_error.clone() {
            ui.label(egui::RichText::new(error).color(theme.warning));
        }

        let mut draw_grid = |ui: &mut Ui, flow: &mut MediaFlow| {
            egui::ScrollArea::vertical()
                .id_salt("media-scroll")
                .scroll_source(egui::scroll_area::ScrollSource {
                    drag: egui::scroll_area::DragScroll::Never,
                    ..Default::default()
                })
                .auto_shrink([false, false])
                .show(ui, |ui| {
                if flow.assets.is_empty() {
                    ui.label(
                        egui::RichText::new("No media yet — drop files here, or use Add files.")
                            .color(theme.muted),
                    );
                } else {
                    let mut remove: Option<String> = None;
                    let mut select: Option<String> = None;
                    // `horizontal_wrapped` does not wrap nested `vertical`
                    // groups (every card landed on one row), so lay the grid
                    // out in explicit rows sized to the available width.
                    let cell_width = GRID_THUMB.x + theme.space_xs;
                    let spacing = ui.spacing().item_spacing.x;
                    let columns = (((ui.available_width() + spacing) / (cell_width + spacing))
                        .floor() as usize)
                        .max(1);
                    let assets = flow.assets.clone();
                    for row in assets.chunks(columns) {
                        ui.horizontal_top(|ui| {
                            for asset in row {
                                let selected =
                                    flow.selected_asset_id.as_deref() == Some(asset.id.as_str());
                                ui.vertical(|ui| {
                                    ui.set_width(cell_width);
                                    if thumbnails.paint_button(ui, asset, GRID_THUMB).clicked() {
                                        select = Some(asset.id.clone());
                                    }
                                    if ui
                                        .add(
                                            egui::Button::selectable(
                                                selected,
                                                egui::RichText::new(&asset.display_name),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&asset.display_name)
                                        .clicked()
                                    {
                                        select = Some(asset.id.clone());
                                    }
                                    ui.horizontal(|ui| {
                                        if crate::widgets::trash_button(ui, theme, &asset.id, "Move to trash").clicked() {
                                            remove = Some(asset.id.clone());
                                        }
                                        let mut meta = media_meta(asset);
                                        if asset.missing {
                                            meta = format!("offline {meta}");
                                        }
                                        if !meta.is_empty() {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(meta)
                                                        .size(11.0)
                                                        .color(if asset.missing {
                                                            theme.warning
                                                        } else {
                                                            theme.faint
                                                        }),
                                                )
                                                .truncate(),
                                            );
                                        }
                                    });
                                });
                            }
                        });
                        ui.add_space(theme.space_xs);
                    }
                    if let Some(asset_id) = select {
                        dispatch(flow, MediaAction::SelectAsset { asset_id }, &mut sink);
                    }
                    if let Some(asset_id) = remove {
                        dispatch(flow, MediaAction::RemoveAsset { asset_id }, &mut sink);
                    }
                }

                });
        };

        // With an item selected, split the view: grid on the left, editor on
        // the right, each scrolling on its own (the editor used to sit below a
        // long grid and had to be scrolled to).
        let selected_asset = flow.selected_asset_id.clone();
        let height = (ui.available_height() - 6.0).max(160.0);
        match selected_asset {
            Some(selected) if !flow.assets.is_empty() => {
                ui.horizontal_top(|ui| {
                    let grid_width = (ui.available_width() * 0.36).clamp(220.0, 440.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(grid_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_height(height);
                            draw_grid(ui, &mut flow);
                        },
                    );
                    ui.separator();
                    let editor_width = (ui.available_width() - 4.0).max(320.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(editor_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_height(height);
                            egui::ScrollArea::vertical()
                                .id_salt("media-editor")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    super::editor::ui(ui, vm, theme, backend, &selected);
                                });
                        },
                    );
                });
            }
            _ => {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_height(height);
                        draw_grid(ui, &mut flow);
                    },
                );
            }
        }

        // Tell the other views (Compose pickers) when the library changed here
        // (import, remove, trash); otherwise they kept showing deleted media.
        let changed = vm.assets.len() != flow.assets.len()
            || vm
                .assets
                .iter()
                .zip(flow.assets.iter())
                .any(|(old, new)| old.id != new.id);
        if changed {
            vm.edits_version += 1;
        }
        vm.asset_count = flow.assets.len();
        vm.assets = flow.assets.clone();
        vm.media.selected_asset_id = flow.selected_asset_id.clone();
    });
}

fn media_meta(asset: &Asset) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let (Some(width), Some(height)) = (asset.width, asset.height) {
        parts.push(format!("{width}×{height}"));
    }
    // ffprobe reports a nominal frame rate/zero duration for stills; only show
    // timing for video.
    if asset.kind == crate::backend::AssetKind::Video {
        if let Some(fps) = asset.fps {
            parts.push(format!("{fps:.0} fps"));
        }
        if let Some(duration) = asset.duration_ms {
            parts.push(format!("{:.1}s", duration as f64 / 1000.0));
        }
    }
    if asset.kind == AssetKind::Video {
        parts.push(if asset.has_audio {
            "audio".to_string()
        } else {
            "silent".to_string()
        });
    }
    parts.join(" · ")
}
