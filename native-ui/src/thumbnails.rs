//! Cached egui textures for imported media thumbnails.
//!
//! Decoding happens on a worker thread: a full-size photo takes hundreds of
//! milliseconds to decode and resize, and doing that inside `ui` froze the
//! window for every new asset. The UI now asks for a thumbnail, gets `None` on
//! the first frame, and paints it as soon as the worker delivers pixels.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;

use egui::{ColorImage, Context, TextureHandle, TextureOptions, Ui, Vec2};
use image::imageops::FilterType;

use crate::backend::Asset;

pub const GRID_THUMB: Vec2 = Vec2 { x: 168.0, y: 112.0 };
pub const PICKER_THUMB: Vec2 = Vec2 { x: 72.0, y: 54.0 };

struct Job {
    key: String,
    path: PathBuf,
    size: Vec2,
}

struct Finished {
    key: String,
    image: Result<ColorImage, String>,
}

pub struct ThumbnailCache {
    handles: HashMap<String, TextureHandle>,
    failed: HashSet<String>,
    pending: HashSet<String>,
    jobs: Option<Sender<Job>>,
    results: Option<Receiver<Finished>>,
    /// Last decode failure, surfaced by the views so a broken thumbnail path is
    /// visible instead of silently blank.
    pub last_error: Option<String>,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        let (job_tx, job_rx) = channel::<Job>();
        let (result_tx, result_rx) = channel::<Finished>();
        thread::Builder::new()
            .name("thumbnails".into())
            .spawn(move || {
                while let Ok(job) = job_rx.recv() {
                    let image = load_image(&job.path, job.size);
                    if result_tx
                        .send(Finished {
                            key: job.key,
                            image,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .ok();
        Self {
            handles: HashMap::new(),
            failed: HashSet::new(),
            pending: HashSet::new(),
            jobs: Some(job_tx),
            results: Some(result_rx),
            last_error: None,
        }
    }
}

impl ThumbnailCache {
    /// Upload any thumbnails the worker finished since the last frame.
    fn drain(&mut self, ctx: &Context) {
        let Some(results) = &self.results else {
            return;
        };
        loop {
            match results.try_recv() {
                Ok(finished) => {
                    self.pending.remove(&finished.key);
                    match finished.image {
                        Ok(image) => {
                            let handle = ctx.load_texture(
                                finished.key.clone(),
                                image,
                                TextureOptions::LINEAR,
                            );
                            self.handles.insert(finished.key, handle);
                            ctx.request_repaint();
                        }
                        Err(error) => {
                            self.last_error = Some(error);
                            self.failed.insert(finished.key);
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    /// Load a thumbnail once per asset/path pair. Decoding is queued, so this
    /// never blocks the frame; call it every frame and paint when it returns
    /// `Some`.
    pub fn handle(&mut self, ctx: &Context, asset: &Asset) -> Option<TextureHandle> {
        self.handle_sized(ctx, asset, GRID_THUMB)
    }

    fn handle_sized(&mut self, ctx: &Context, asset: &Asset, size: Vec2) -> Option<TextureHandle> {
        self.drain(ctx);
        let Some(path) = thumbnail_path(asset) else {
            self.last_error = Some(format!("no thumbnail path for {}", asset.id));
            return None;
        };
        let key = format!("{}|{}|{}x{}", asset.id, path.display(), size.x, size.y);
        if let Some(handle) = self.handles.get(&key) {
            return Some(handle.clone());
        }
        if self.failed.contains(&key) {
            return None;
        }
        if !self.pending.contains(&key) {
            if let Some(jobs) = &self.jobs {
                if jobs
                    .send(Job {
                        key: key.clone(),
                        path,
                        size,
                    })
                    .is_ok()
                {
                    self.pending.insert(key);
                }
            }
        }
        // Keep frames coming while a decode is in flight so the grid fills in
        // without needing another interaction.
        if !self.pending.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        None
    }

    /// Clickable thumbnail; returns the interaction response so a grid can
    /// select on a click anywhere in the image, not just its caption.
    pub fn paint_button(&mut self, ui: &mut Ui, asset: &Asset, size: Vec2) -> egui::Response {
        let response = match self.handle_sized(ui.ctx(), asset, size) {
            Some(handle) => {
                ui.add(egui::Image::new((handle.id(), size)).sense(egui::Sense::click()))
            }
            None => ui.allocate_response(size, egui::Sense::click()),
        };
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        response
    }

    /// Paint a thumbnail or reserve the same space when the asset has none.
    pub fn paint(&mut self, ui: &mut Ui, asset: &Asset, size: Vec2) {
        if let Some(handle) = self.handle_sized(ui.ctx(), asset, size) {
            ui.image((handle.id(), size));
        } else {
            ui.allocate_space(size);
        }
    }
}

fn thumbnail_path(asset: &Asset) -> Option<PathBuf> {
    if let Some(path) = asset.thumbnail_path.as_deref() {
        return Some(PathBuf::from(path));
    }
    let source = PathBuf::from(&asset.path);
    source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif"
            )
        })
        .then_some(source)
}

fn load_image(path: &Path, size: Vec2) -> Result<ColorImage, String> {
    let decoded =
        image::open(path).map_err(|error| format!("cannot decode {}: {error}", path.display()))?;
    let resized = decoded.resize_exact(
        size.x.max(1.0) as u32,
        size.y.max(1.0) as u32,
        FilterType::Triangle,
    );
    let rgba = resized.to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}
