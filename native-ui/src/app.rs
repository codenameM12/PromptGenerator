//! eframe application shell: installs the theme, hosts the rail and the active
//! view, dispatches keyboard shortcuts, and persists workspace state.

use std::sync::Arc;

use egui::{Context, ViewportCommand};

use crate::backend::Backend;
use crate::media_access::AllowedRoots;
use crate::persistence::{PersistedState, STORAGE_KEY, encode};
use crate::shortcuts::{KeyModifiers, Shortcut, os_prefers_reduced_motion, resolve_shortcut};
use crate::single_instance::SingleInstance;
use crate::theme::Theme;
use crate::thumbnails::ThumbnailCache;
use crate::view_model::{ViewId, ViewModel};
use crate::views::{
    characters, compose, datasets, jobs, library, media, presets, rail, runtime,
};

pub struct NativeApp {
    pub vm: ViewModel,
    pub theme: Theme,
    thumbnails: ThumbnailCache,
    backend: Box<dyn Backend>,
    access: Arc<AllowedRoots>,
    single_instance: Option<SingleInstance>,
    reduced_motion: bool,
    restored: bool,
    /// Set once the window has been fitted to the current monitor.
    fitted: bool,
    /// Fingerprint of the last persisted state, so persistence does not encode
    /// and store JSON on every frame.
    last_saved: Option<String>,
}

impl NativeApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        backend: Box<dyn Backend>,
        access: Arc<AllowedRoots>,
        single_instance: Option<SingleInstance>,
    ) -> Self {
        let reduced_motion = os_prefers_reduced_motion();
        Self {
            vm: ViewModel::default(),
            theme: Theme::dark(reduced_motion),
            thumbnails: ThumbnailCache::default(),
            backend,
            access,
            single_instance,
            reduced_motion,
            restored: false,
            fitted: false,
            last_saved: None,
        }
    }

    fn restore_once(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        if self.restored {
            return;
        }
        self.restored = true;
        self.theme.install(ctx);

        if let Some(raw) = frame
            .storage()
            .and_then(|storage| storage.get_string(STORAGE_KEY))
        {
            if let Ok(state) = crate::persistence::decode(&raw) {
                self.vm.restore_draft(&state.draft);
                self.vm.media.selected_asset_id = state.last_asset_id.clone();
                // Size is fixed to a screen-friendly default: a stored
                // Window geometry is not replayed: restoring a size captured at
                // another DPI scale overflows the monitor and pushes the
                // right-hand column out of reach. `main.rs` declares the size.
                let _ = state.geometry;
                self.vm.status = "Restored previous workspace".to_string();
            }
        }
    }

    /// Keeps the window fully on the current monitor. eframe restores the last
    /// viewport size and position on its own, which can place the window (and
    /// its right-hand column) off a smaller display.
    fn fit_to_monitor(&mut self, ctx: &Context) {
        if self.fitted {
            return;
        }
        let (rect, monitor) =
            ctx.input(|input| (input.viewport().outer_rect, input.viewport().monitor_size));
        let (Some(rect), Some(monitor)) = (rect, monitor) else {
            return;
        };
        self.fitted = true;
        let width = rect.width().min(monitor.x);
        let height = rect.height().min(monitor.y);
        let max_x = (monitor.x - width).max(0.0);
        let max_y = (monitor.y - height).max(0.0);
        let x = rect.min.x.clamp(0.0, max_x);
        let y = rect.min.y.clamp(0.0, max_y);
        if width < rect.width() || height < rect.height() {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::Vec2::new(width, height)));
        }
        if x != rect.min.x || y != rect.min.y {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::Pos2::new(x, y)));
        }
    }

    fn handle_shortcuts(&mut self, ctx: &Context) {
        let (enter, escape, modifiers) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Enter),
                input.key_pressed(egui::Key::Escape),
                KeyModifiers {
                    ctrl: input.modifiers.ctrl,
                    meta: input.modifiers.command,
                    shift: input.modifiers.shift,
                },
            )
        });
        if !enter && !escape {
            return;
        }

        let focus = self.vm.focus_context();
        let describe_mode = self.vm.compose.mode == crate::view_model::ComposeMode::Describe;
        let action = if escape {
            resolve_shortcut("Escape", modifiers, focus, self.vm.busy, describe_mode)
        } else {
            resolve_shortcut("Enter", modifiers, focus, self.vm.busy, describe_mode)
        };

        match action {
            Some(Shortcut::Compose) => {
                self.vm.busy = true;
                compose::run_compose(&mut self.vm, self.backend.as_ref());
                self.vm.busy = false;
            }
            Some(Shortcut::Describe) => {
                self.vm.busy = true;
                compose::run_describe(&mut self.vm, self.backend.as_ref());
                self.vm.busy = false;
            }
            Some(Shortcut::Cancel) => match self.backend.cancel() {
                Ok(()) => self.vm.status = "Cancelled".to_string(),
                Err(error) => self.vm.status = crate::errors::error_text_of(&error),
            },
            None => {}
        }
    }

    /// Imports files dropped on the window, mirroring the source App-level
    /// drag-and-drop handler.
    fn handle_drops(&mut self, ctx: &Context) {
        let dropped: Vec<std::path::PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
        self.vm.drop_hint = hovering;
        if dropped.is_empty() {
            return;
        }
        let paths: Vec<String> = dropped
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        if self.vm.active == ViewId::Characters {
            // Files dropped on a name (or on the open panel) go to that one.
            // Windows sends no pointer positions during a file drag, so the
            // position may be missing or stale; the open one is the fallback.
            let pointer = ctx.input(|input| input.pointer.hover_pos().or(input.pointer.latest_pos()));
            let under_pointer = pointer.and_then(|pos| {
                self.vm
                    .characters
                    .drop_zones
                    .iter()
                    .find(|(rect, _)| rect.contains(pos))
                    .map(|(_, id)| id.clone())
            });
            let target = under_pointer.or_else(|| self.vm.characters.selected.clone());
            match target {
                Some(id) => {
                    match self.backend.call(
                        "characters_add_images",
                        serde_json::json!({ "id": id, "paths": paths }),
                    ) {
                        Ok(_) => {
                            self.vm.status = format!("Added {} dropped item(s) to the set", paths.len());
                            self.vm.characters.loaded = false;
                            self.vm.characters.selected = Some(id);
                            self.vm.characters.detail = None;
                            self.vm.characters_version += 1;
                            ctx.request_repaint();
                        }
                        Err(error) => self.vm.status = crate::errors::error_text_of(&error),
                    }
                }
                None => {
                    self.vm.status =
                        "Select a name first, then drop files or folders to add them to it".to_string();
                }
            }
            return;
        }
        match crate::import::import_paths(self.backend.as_ref(), &self.access, &paths) {
            Ok(results) => {
                self.vm.status = format!("Imported {} item(s)", results.len());
                match self.backend.list_assets() {
                    Ok(assets) => {
                        self.vm.asset_count = assets.len();
                        self.vm.assets = assets;
                        // Compose pickers and the Media grid reload on these signals.
                        self.vm.edits_version += 1;
                        self.vm.media_reload += 1;
                    }
                    Err(error) => self.vm.status = crate::errors::error_text_of(&error),
                }
            }
            Err(error) => self.vm.status = crate::errors::error_text_of(&error),
        }
    }

    fn save_state(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        let geometry = ctx.input(|input| {
            input
                .viewport()
                .outer_rect
                .map(|rect| crate::persistence::Geometry {
                    x: rect.min.x,
                    y: rect.min.y,
                    width: rect.width(),
                    height: rect.height(),
                })
        });
        let last_asset_id = self
            .vm
            .media
            .selected_asset_id
            .clone()
            .or_else(|| self.vm.compose.selected_asset_ids.first().cloned());
        let state = PersistedState {
            geometry,
            last_asset_id,
            draft: self.vm.draft(),
        };
        if let Ok(raw) = encode(&state) {
            if self.last_saved.as_deref() != Some(raw.as_str()) {
                if let Some(storage) = frame.storage_mut() {
                    storage.set_string(STORAGE_KEY, raw.clone());
                }
                self.last_saved = Some(raw);
            }
        }
    }
}

impl eframe::App for NativeApp {
    /// Non-UI work: restore state, focus handling, shortcuts, persistence.
    fn logic(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        self.restore_once(ctx, frame);

        if let Some(instance) = self.single_instance.as_mut() {
            if instance.poll() {
                // A second launch already gave up and exited, so this process
                // must raise itself: the focus request alone is ignored unless
                // this process owns the foreground.
                ctx.send_viewport_cmd(ViewportCommand::Focus);
                let title = ctx
                    .input(|input| input.viewport().title.clone())
                    .unwrap_or_default();
                if !crate::platform::foreground::raise_window(&title) {
                    crate::platform::foreground::raise_window(
                        "Prompt Studio (native \u{b7} port in progress)",
                    );
                }
            }
        }

        // Focus is re-derived from widget responses during `ui` each frame.
        self.vm.text_input_focused = false;
        self.handle_shortcuts(ctx);
        self.save_state(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_drops(&ctx);

        egui::Panel::left("rail").show(ui, |rail_ui| {
            rail::ui(rail_ui, &mut self.vm, &self.theme);
        });

        egui::Panel::bottom("status").show(ui, |status_ui| {
            status_ui.horizontal(|ui| {
                if self.vm.status.is_empty() {
                    ui.label(egui::RichText::new("Ready").color(self.theme.faint));
                } else {
                    // Long errors wrapped into a second line that the fixed-height
                    // bar clipped; keep one line and show the full text on hover.
                    ui.add(egui::Label::new(&self.vm.status).truncate())
                        .on_hover_text(&self.vm.status);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.vm.drop_hint {
                        ui.label(
                            egui::RichText::new("Drop files to import").color(self.theme.accent),
                        );
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{} asset(s)",
                            self.vm.assets.len().max(self.vm.asset_count)
                        ))
                        .color(self.theme.faint),
                    );
                });
            });
        });

        egui::CentralPanel::default().show(ui, |central| {
            // Only the active section is drawn, but every section's state lives
            // in the view model and is never cleared by navigation.
            match self.vm.active {
                ViewId::Compose => compose::ui(
                    central,
                    &mut self.vm,
                    &self.theme,
                    self.backend.as_ref(),
                    &self.access,
                    &mut self.thumbnails,
                ),
                ViewId::Media => media::ui(
                    central,
                    &mut self.vm,
                    &self.theme,
                    self.backend.as_ref(),
                    &self.access,
                    &mut self.thumbnails,
                ),
                ViewId::Library => {
                    library::ui(central, &mut self.vm, &self.theme, self.backend.as_ref())
                }
                ViewId::Jobs => jobs::ui(central, &mut self.vm, &self.theme, self.backend.as_ref()),
                ViewId::Presets => {
                    presets::ui(central, &mut self.vm, &self.theme, self.backend.as_ref())
                }
                ViewId::Runtime => {
                    runtime::ui(central, &mut self.vm, &self.theme, self.backend.as_ref())
                }
                ViewId::Characters => characters::ui(
                    central,
                    &mut self.vm,
                    &self.theme,
                    self.backend.as_ref(),
                    &mut self.thumbnails,
                ),
                ViewId::Datasets => {
                    datasets::ui(central, &mut self.vm, &self.theme, self.backend.as_ref())
                }
            }
        });

        if self.vm.modal_open {
            let reduced = self.reduced_motion;
            egui::Window::new("About")
                .resizable(false)
                .show(&ctx, |about| {
                    about.label("Prompt Studio — native UI port in progress");
                    about.label(if reduced {
                        "Reduced motion: on"
                    } else {
                        "Reduced motion: off"
                    });
                    if about.button("Close").clicked() {
                        self.vm.modal_open = false;
                    }
                });
        }
    }
}
