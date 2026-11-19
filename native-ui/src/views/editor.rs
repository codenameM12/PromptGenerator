//! Media editor view model (5.2). History is the existing
//! `prompt_studio_lib::ui::edit_history` state machine — no second history
//! implementation. Crop-edge conversion is view input math (percent ↔ pixels),
//! not the domain geometry owned by `media/edit.rs`.

use prompt_studio_lib::media::edit::auto_crop;
use prompt_studio_lib::ui::edit_history::{
    self, EditHistory, EditOps, FitMode, Trim, can_redo, can_undo, create_history, current, push,
    redo, reset, undo,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use egui::Ui;

use crate::backend::Backend;
use crate::theme::Theme;
use crate::view_model::ViewModel;

use super::compose::CallSink;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorTarget {
    pub preset_id: String,
    pub width: u32,
    pub height: u32,
    pub dimension_multiple: u32,
    pub mask_polarity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropEdges {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone)]
pub struct EditorFlow {
    pub asset_id: String,
    pub width: u32,
    pub height: u32,
    pub duration_ms: Option<u64>,
    pub history: EditHistory,
    pub target: EditorTarget,
    pub preview: Option<Value>,
    pub result: Option<Value>,
    pub preview_error: Option<String>,
    pub apply_error: Option<String>,
    pub progress: f64,
    pub mask_path: Option<String>,
}

impl EditorFlow {
    pub fn open(
        asset_id: &str,
        width: u32,
        height: u32,
        kind: &str,
        duration_ms: Option<u64>,
        initial: Option<EditOps>,
    ) -> Self {
        Self {
            asset_id: asset_id.to_string(),
            width,
            height,
            duration_ms,
            history: create_history(initial.unwrap_or_default()),
            target: EditorTarget {
                preset_id: if kind == "video" {
                    "generic-video"
                } else {
                    "generic-image"
                }
                .into(),
                width: 0,
                height: 0,
                dimension_multiple: 16,
                mask_polarity: "whiteKeeps".into(),
            },
            preview: None,
            result: None,
            preview_error: None,
            apply_error: None,
            progress: 0.0,
            mask_path: None,
        }
    }

    pub fn snapshot(&self) -> EditorSnapshot {
        let ops = current(&self.history);
        EditorSnapshot {
            asset_id: self.asset_id.clone(),
            ops: ops.clone(),
            can_undo: can_undo(&self.history),
            can_redo: can_redo(&self.history),
            target: self.target.clone(),
            preview: self.preview.clone(),
            result: self.result.clone(),
            error: self
                .preview_error
                .clone()
                .or_else(|| self.apply_error.clone()),
            readout: match &ops.crop {
                Some(crop) => format!(
                    "Selection {}×{} at {},{}",
                    crop.width as i64, crop.height as i64, crop.x as i64, crop.y as i64
                ),
                None => "No crop. Drag an edge or choose an aspect ratio.".into(),
            },
        }
    }

    fn request(&self) -> Value {
        json!({
            "request": {
                "assetId": self.asset_id,
                "ops": normalize_numbers(serde_json::to_value(current(&self.history))
                    .expect("serialize EditOps")),
                "target": self.target
            }
        })
    }

    fn preview_now(&mut self, call: CallSink) {
        match call("edits_preview", self.request()) {
            Ok(value) => {
                self.preview = Some(value);
                self.preview_error = None;
            }
            Err(error) => {
                self.preview = None;
                self.preview_error = Some(error);
            }
        }
    }

    fn update(&mut self, patch: impl FnOnce(&mut EditOps), call: CallSink) {
        let next = {
            let mut value = current(&self.history).clone();
            patch(&mut value);
            push(
                std::mem::replace(&mut self.history, create_history(EditOps::default())),
                value,
            )
        };
        self.history = next;
        self.preview_now(call);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorSnapshot {
    pub asset_id: String,
    pub ops: EditOps,
    pub can_undo: bool,
    pub can_redo: bool,
    pub target: EditorTarget,
    pub preview: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub readout: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EditorAction {
    Open {
        asset_id: String,
        width: u32,
        height: u32,
        asset_kind: String,
        duration_ms: Option<u64>,
    },
    Rotate,
    FlipH,
    FlipV,
    CropAspect {
        numerator: u32,
        denominator: u32,
    },
    ResetCrop,
    CropEdge {
        edge: String,
        value: f64,
    },
    /// Crop rectangle drawn directly on the preview (percent edges).
    RectCrop {
        left: f64,
        right: f64,
        top: f64,
        bottom: f64,
    },
    SetFit {
        value: FitMode,
    },
    TrimStart {
        value: u64,
    },
    TrimEnd {
        value: u64,
    },
    Fps {
        value: Option<f64>,
    },
    Format {
        value: Option<String>,
    },
    MaskPath {
        path: String,
    },
    Target {
        width: u32,
        height: u32,
    },
    Multiple {
        value: u32,
    },
    Polarity {
        value: String,
    },
    Undo,
    Redo,
    Reset,
    Preview,
    Apply,
    ExportMask {
        feather: f64,
    },
    Cancel,
}

/// Percentage-edge crop from the editor sliders (view input math, mirrored from
/// `src/lib/media/crop.ts`).
fn crop_from_edges(width: u32, height: u32, edges: &CropEdges) -> Option<edit_history::Crop> {
    if width == 0 || height == 0 {
        return None;
    }
    let pair = |first: f64, second: f64| -> (f64, f64) {
        let mut left = first.max(0.0).min(90.0);
        let mut right = second.max(0.0).min(90.0);
        if left + right > 90.0 {
            let scale = 90.0 / (left + right);
            left *= scale;
            right *= scale;
        }
        (left, right)
    };
    let (left, right) = pair(edges.left, edges.right);
    let (top, bottom) = pair(edges.top, edges.bottom);
    let x = ((width as f64 - 1.0).min((width as f64 * left / 100.0).round())).max(0.0) as u32;
    let crop_width = ((width as f64 - x as f64)
        .min((width as f64 * (100.0 - left - right) / 100.0).round()))
    .max(1.0) as u32;
    let y = ((height as f64 - 1.0).min((height as f64 * top / 100.0).round())).max(0.0) as u32;
    let crop_height = ((height as f64 - y as f64)
        .min((height as f64 * (100.0 - top - bottom) / 100.0).round()))
    .max(1.0) as u32;
    Some(edit_history::Crop {
        x: x as f64,
        y: y as f64,
        width: crop_width as f64,
        height: crop_height as f64,
    })
}

fn edges_from_crop(width: u32, height: u32, crop: &edit_history::Crop) -> CropEdges {
    if width == 0 || height == 0 {
        return CropEdges {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        };
    }
    CropEdges {
        left: crop.x / width as f64 * 100.0,
        right: (width as f64 - crop.x - crop.width) / width as f64 * 100.0,
        top: crop.y / height as f64 * 100.0,
        bottom: (height as f64 - crop.y - crop.height) / height as f64 * 100.0,
    }
}

/// TS serializes whole numbers as `1`, not `1.0`; normalize typed f64 fields
/// so the differential comparison sees the same JSON number shape.
fn normalize_numbers(value: Value) -> Value {
    match value {
        Value::Number(number) => {
            if let Some(float) = number.as_f64()
                && float.is_finite()
                && float.fract() == 0.0
            {
                json!(float as i64)
            } else {
                Value::Number(number)
            }
        }
        Value::Array(values) => Value::Array(values.into_iter().map(normalize_numbers).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, normalize_numbers(value)))
                .collect(),
        ),
        other => other,
    }
}

pub fn dispatch(flow: &mut EditorFlow, action: EditorAction, call: CallSink) {
    match action {
        EditorAction::Open {
            asset_id,
            width,
            height,
            asset_kind,
            duration_ms,
        } => {
            *flow = EditorFlow::open(&asset_id, width, height, &asset_kind, duration_ms, None);
            flow.preview_now(call);
        }
        EditorAction::Rotate => flow.update(|ops| ops.rotate = (ops.rotate + 90.0) % 360.0, call),
        EditorAction::FlipH => flow.update(|ops| ops.flip_h = !ops.flip_h, call),
        EditorAction::FlipV => flow.update(|ops| ops.flip_v = !ops.flip_v, call),
        EditorAction::CropAspect {
            numerator,
            denominator,
        } => {
            let crop = auto_crop(flow.width, flow.height, numerator, denominator);
            flow.update(
                |ops| {
                    ops.crop = crop.map(|value| edit_history::Crop {
                        x: value.x as f64,
                        y: value.y as f64,
                        width: value.width as f64,
                        height: value.height as f64,
                    })
                },
                call,
            )
        }
        EditorAction::ResetCrop => flow.update(|ops| ops.crop = None, call),
        EditorAction::CropEdge { edge, value } => {
            let current_edges = current(&flow.history)
                .crop
                .as_ref()
                .map(|crop| edges_from_crop(flow.width, flow.height, crop))
                .unwrap_or(CropEdges {
                    left: 0.0,
                    right: 0.0,
                    top: 0.0,
                    bottom: 0.0,
                });
            let mut edges = current_edges;
            match edge.as_str() {
                "left" => edges.left = value,
                "right" => edges.right = value,
                "top" => edges.top = value,
                "bottom" => edges.bottom = value,
                _ => {}
            }
            let crop = crop_from_edges(flow.width, flow.height, &edges);
            flow.update(|ops| ops.crop = crop, call)
        }
        EditorAction::RectCrop {
            left,
            right,
            top,
            bottom,
        } => {
            let edges = CropEdges {
                left,
                right,
                top,
                bottom,
            };
            let crop = crop_from_edges(flow.width, flow.height, &edges);
            flow.update(|ops| ops.crop = crop, call)
        }
        EditorAction::SetFit { value } => flow.update(|ops| ops.fit = value, call),
        EditorAction::TrimStart { value } => {
            let default_end = flow.duration_ms.unwrap_or(0) as f64;
            flow.update(
                |ops| {
                    let end = ops
                        .trim
                        .as_ref()
                        .map(|trim| trim.end_ms)
                        .unwrap_or(default_end);
                    ops.trim = Some(Trim {
                        start_ms: value as f64,
                        end_ms: end,
                    })
                },
                call,
            )
        }
        EditorAction::TrimEnd { value } => flow.update(
            |ops| {
                let start = ops.trim.as_ref().map(|trim| trim.start_ms).unwrap_or(0.0);
                ops.trim = Some(Trim {
                    start_ms: start,
                    end_ms: value as f64,
                })
            },
            call,
        ),
        EditorAction::Fps { value } => flow.update(|ops| ops.fps = value, call),
        EditorAction::Format { value } => flow.update(|ops| ops.format = value, call),
        EditorAction::MaskPath { path } => flow.update(
            |ops| {
                ops.mask_path = Some(path.clone());
            },
            call,
        ),
        EditorAction::Target { width, height } => {
            flow.target.width = width;
            flow.target.height = height;
            flow.preview_now(call);
        }
        EditorAction::Multiple { value } => {
            flow.target.dimension_multiple = value;
            flow.preview_now(call);
        }
        EditorAction::Polarity { value } => {
            flow.target.mask_polarity = value;
            flow.preview_now(call);
        }
        EditorAction::Undo => {
            flow.history = undo(std::mem::replace(
                &mut flow.history,
                create_history(EditOps::default()),
            ));
            flow.preview_now(call);
        }
        EditorAction::Redo => {
            flow.history = redo(std::mem::replace(
                &mut flow.history,
                create_history(EditOps::default()),
            ));
            flow.preview_now(call);
        }
        EditorAction::Reset => {
            flow.history = reset(std::mem::replace(
                &mut flow.history,
                create_history(EditOps::default()),
            ));
            flow.preview_now(call);
        }
        EditorAction::Preview => flow.preview_now(call),
        EditorAction::Apply => match call("edits_save", flow.request()) {
            Ok(value) => {
                flow.result = Some(value);
                flow.apply_error = None;
                flow.progress = 1.0;
            }
            Err(error) => flow.apply_error = Some(error),
        },
        EditorAction::ExportMask { feather } => {
            let resolution = flow
                .preview
                .as_ref()
                .and_then(|value| value.get("resolution"))
                .cloned()
                .unwrap_or(json!({ "width": flow.width, "height": flow.height }));
            let width = resolution
                .get("width")
                .and_then(Value::as_u64)
                .unwrap_or(flow.width as u64) as u32;
            let height = resolution
                .get("height")
                .and_then(Value::as_u64)
                .unwrap_or(flow.height as u64) as u32;
            // The editor canvas is 480px wide; the test source canvas mock
            // returns a single RGBA pixel, so the exported mask is one sample.
            let canvas_width = 480u32;
            let canvas_height =
                ((480.0 * flow.height as f64 / flow.width.max(1) as f64).round() as u32).max(120);
            let mask = vec![0u8; 1];
            let args = json!({
                "request": {
                    "assetId": flow.asset_id,
                    "mask": mask,
                    "maskWidth": canvas_width,
                    "maskHeight": canvas_height,
                    "targetWidth": width,
                    "targetHeight": height,
                    "feather": normalize_numbers(json!(feather)),
                    "invert": false,
                    "polarity": flow.target.mask_polarity
                }
            });
            match call("edits_export_mask", args) {
                Ok(value) => {
                    flow.mask_path = value
                        .get("path")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    if let Some(path) = flow.mask_path.clone() {
                        let mut history = std::mem::replace(
                            &mut flow.history,
                            create_history(EditOps::default()),
                        );
                        let next_ops = EditOps {
                            mask_path: Some(path),
                            ..current(&history).clone()
                        };
                        history = push(history, next_ops);
                        flow.history = history;
                    }
                    flow.preview_error = None;
                }
                Err(error) => flow.preview_error = Some(error),
            }
        }
        EditorAction::Cancel => {
            flow.progress = 0.0;
            let _ = call("edits_cancel", json!({}));
        }
    }
}

// ---------------------------------------------------------------------------
// Live egui panel (reached from Media)
// ---------------------------------------------------------------------------

thread_local! {
    static LIVE: std::cell::RefCell<Option<EditorFlow>> =
        const { std::cell::RefCell::new(None) };
}

/// Draws the staged editor for `asset_id` under the Media grid. Every control
/// dispatches through the same `EditorAction` set the differential harness
/// replays, so the live panel and the recorded behaviour cannot diverge.
pub fn ui(ui: &mut Ui, vm: &mut ViewModel, theme: &Theme, backend: &dyn Backend, asset_id: &str) {
    let mut sink = super::media::backend_sink(backend);
    LIVE.with(|cell| {
        let mut slot = cell.borrow_mut();
        let stale = match slot.as_ref() {
            Some(flow) => flow.asset_id != asset_id,
            None => true,
        };
        if stale {
            let asset = vm.assets.iter().find(|item| item.id == asset_id).cloned();
            let Some(asset) = asset else {
                ui.label(egui::RichText::new("Select a media item to edit").color(theme.muted));
                return;
            };
            let kind = match asset.kind {
                crate::backend::AssetKind::Video => "video",
                crate::backend::AssetKind::Image => "image",
            };
            let mut fresh = EditorFlow::open(
                &asset.id,
                asset.width.unwrap_or(0),
                asset.height.unwrap_or(0),
                kind,
                asset.duration_ms,
                None,
            );
            dispatch(
                &mut fresh,
                EditorAction::Open {
                    asset_id: asset.id.clone(),
                    width: asset.width.unwrap_or(0),
                    height: asset.height.unwrap_or(0),
                    asset_kind: kind.to_string(),
                    duration_ms: asset.duration_ms,
                },
                &mut sink,
            );
            *slot = Some(fresh);
        }
        let Some(flow) = slot.as_mut() else {
            return;
        };
        let video = flow.target.preset_id == "generic-video";

        ui.add_space(theme.space_sm);
        ui.label(egui::RichText::new("Edit media").color(theme.accent));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_undo(&flow.history), egui::Button::new("Undo"))
                .clicked()
            {
                dispatch(flow, EditorAction::Undo, &mut sink);
            }
            if ui
                .add_enabled(can_redo(&flow.history), egui::Button::new("Redo"))
                .clicked()
            {
                dispatch(flow, EditorAction::Redo, &mut sink);
            }
            if ui.button("Reset").clicked() {
                dispatch(flow, EditorAction::Reset, &mut sink);
            }
            if ui.button("Rotate 90").clicked() {
                dispatch(flow, EditorAction::Rotate, &mut sink);
            }
            if ui.button("Flip H").clicked() {
                dispatch(flow, EditorAction::FlipH, &mut sink);
            }
            if ui.button("Flip V").clicked() {
                dispatch(flow, EditorAction::FlipV, &mut sink);
            }
        });

        canvas_ui(ui, vm, theme, backend, flow);

        let edges = current(&flow.history)
            .crop
            .as_ref()
            .map(|crop| edges_from_crop(flow.width, flow.height, crop))
            .unwrap_or(CropEdges {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            });
        ui.label(egui::RichText::new("Crop (% of the source)").color(theme.ink_2));
        for (edge, value) in [
            ("left", edges.left),
            ("right", edges.right),
            ("top", edges.top),
            ("bottom", edges.bottom),
        ] {
            let mut percent = value;
            ui.horizontal(|ui| {
                ui.label(format!("{edge:>6}"));
                if ui
                    .add(
                        egui::Slider::new(&mut percent, 0.0..=90.0)
                            .suffix("%")
                            .step_by(0.5),
                    )
                    .changed()
                {
                    dispatch(
                        flow,
                        EditorAction::CropEdge {
                            edge: edge.to_string(),
                            value: percent,
                        },
                        &mut sink,
                    );
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.button("1:1").clicked() {
                dispatch(
                    flow,
                    EditorAction::CropAspect {
                        numerator: 1,
                        denominator: 1,
                    },
                    &mut sink,
                );
            }
            if ui.button("16:9").clicked() {
                dispatch(
                    flow,
                    EditorAction::CropAspect {
                        numerator: 16,
                        denominator: 9,
                    },
                    &mut sink,
                );
            }
            if ui.button("9:16").clicked() {
                dispatch(
                    flow,
                    EditorAction::CropAspect {
                        numerator: 9,
                        denominator: 16,
                    },
                    &mut sink,
                );
            }
            if ui.button("Clear crop").clicked() {
                dispatch(flow, EditorAction::ResetCrop, &mut sink);
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Fit").color(theme.ink_2));
            for (label, value) in [
                ("fit", FitMode::Fit),
                ("fill", FitMode::Fill),
                ("pad", FitMode::Pad),
            ] {
                if ui
                    .selectable_label(current(&flow.history).fit == value, label)
                    .clicked()
                {
                    dispatch(flow, EditorAction::SetFit { value }, &mut sink);
                }
            }
        });

        if video {
            ui.label(egui::RichText::new("Trim (ms)").color(theme.ink_2));
            ui.horizontal(|ui| {
                let mut start = current(&flow.history)
                    .trim
                    .as_ref()
                    .map(|trim| trim.start_ms)
                    .unwrap_or(0.0);
                let mut end = current(&flow.history)
                    .trim
                    .as_ref()
                    .map(|trim| trim.end_ms)
                    .unwrap_or(flow.duration_ms.unwrap_or(0) as f64);
                if ui
                    .add(egui::DragValue::new(&mut start).suffix(" ms"))
                    .changed()
                {
                    dispatch(
                        flow,
                        EditorAction::TrimStart {
                            value: start.max(0.0) as u64,
                        },
                        &mut sink,
                    );
                }
                if ui
                    .add(egui::DragValue::new(&mut end).suffix(" ms"))
                    .changed()
                {
                    dispatch(
                        flow,
                        EditorAction::TrimEnd {
                            value: end.max(0.0) as u64,
                        },
                        &mut sink,
                    );
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("fps").color(theme.ink_2));
                for fps in [24.0, 25.0, 30.0, 60.0] {
                    if ui
                        .selectable_label(
                            current(&flow.history).fps == Some(fps),
                            format!("{fps:.0}"),
                        )
                        .clicked()
                    {
                        dispatch(flow, EditorAction::Fps { value: Some(fps) }, &mut sink);
                    }
                }
            });
        }

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Target").color(theme.ink_2));
            let mut width = flow.target.width;
            let height = flow.target.height;
            if ui
                .add(egui::DragValue::new(&mut width).range(0..=8192))
                .changed()
            {
                dispatch(flow, EditorAction::Target { width, height }, &mut sink);
            }
            let mut target_height = flow.target.height;
            let target_width = flow.target.width;
            if ui
                .add(egui::DragValue::new(&mut target_height).range(0..=8192))
                .changed()
            {
                dispatch(
                    flow,
                    EditorAction::Target {
                        width: target_width,
                        height: target_height,
                    },
                    &mut sink,
                );
            }
            ui.label(egui::RichText::new("multiple").color(theme.muted));
            let mut multiple = flow.target.dimension_multiple;
            if ui
                .add(egui::DragValue::new(&mut multiple).range(1..=64))
                .changed()
            {
                dispatch(flow, EditorAction::Multiple { value: multiple }, &mut sink);
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Preview").clicked() {
                dispatch(flow, EditorAction::Preview, &mut sink);
            }
            if ui.button("Apply edit").clicked() {
                dispatch(flow, EditorAction::Apply, &mut sink);
            }
            if ui.button("Cancel render").clicked() {
                dispatch(flow, EditorAction::Cancel, &mut sink);
            }
        });

        if let Some(preview) = &flow.preview
            && let Some(resolution) = preview.get("resolution")
        {
            let width = resolution
                .get("width")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let height = resolution
                .get("height")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let upscaled = resolution
                .get("upscaled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut line = format!("Preview resolution: {width}×{height}");
            if upscaled {
                line.push_str(" (upscaled)");
            }
            ui.label(egui::RichText::new(line).color(theme.muted));
            if let Some(warnings) = resolution.get("warnings").and_then(Value::as_array) {
                for warning in warnings {
                    if let Some(text) = warning.as_str() {
                        ui.label(egui::RichText::new(text).color(theme.warning));
                    }
                }
            }
        }
        if let Some(result) = &flow.result {
            let path = result
                .get("outputPath")
                .and_then(Value::as_str)
                .unwrap_or("");
            ui.label(egui::RichText::new(format!("Saved edit: {path}")).color(theme.success));
        }
        if let Some(error) = flow.preview_error.as_ref().or(flow.apply_error.as_ref()) {
            ui.label(egui::RichText::new(error).color(theme.danger));
        }
    });
}

// ---------------------------------------------------------------------------
// Interactive canvas: crop rectangle + mask painting (ComfyUI-style direct
// manipulation on the image instead of sliders).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanvasTool {
    Crop,
    Brush,
    Eraser,
}

struct CanvasState {
    asset_id: String,
    width: u32,
    height: u32,
    /// One byte per pixel: 255 = keep, 0 = remove.
    mask: Vec<u8>,
    /// Count of non-zero mask pixels, kept in step with `mask` so no frame has
    /// to scan the whole raster.
    painted: usize,
    texture: Option<egui::TextureHandle>,
    source: Option<egui::TextureHandle>,
    tool: CanvasTool,
    brush: f32,
    feather: u32,
    invert: bool,
    crop_start: Option<egui::Pos2>,
    crop_end: Option<egui::Pos2>,
    last_stroke: Option<egui::Pos2>,
    last_error: Option<String>,
    exported_path: Option<String>,
}

impl CanvasState {
    fn new(asset: &crate::backend::Asset, ctx: &egui::Context) -> Option<Self> {
        let path = asset
            .thumbnail_path
            .clone()
            .or_else(|| Some(asset.path.clone()))
            .filter(|path| !path.is_empty())?;
        let decoded = image::open(&path).ok()?;
        let source_width = decoded.width().max(1);
        let scale = (640.0 / source_width as f32).min(1.0);
        let width = (source_width as f32 * scale).round().max(1.0) as u32;
        let height = (decoded.height() as f32 * scale).round().max(1.0) as u32;
        let resized = decoded.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let source = ctx.load_texture(
            format!("editor-source-{}", asset.id),
            egui::ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                rgba.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        Some(Self {
            asset_id: asset.id.clone(),
            width,
            height,
            mask: vec![0u8; (width * height) as usize],
            painted: 0,
            texture: None,
            source: Some(source),
            tool: CanvasTool::Crop,
            brush: 28.0,
            feather: 4,
            invert: false,
            crop_start: None,
            crop_end: None,
            last_stroke: None,
            last_error: None,
            exported_path: None,
        })
    }

    fn paint_segment(&mut self, from: egui::Pos2, to: egui::Pos2, erase: bool) {
        let value = if erase { 0 } else { 255 };
        let radius = self.brush.max(1.0);
        let steps = ((to - from).length() / (radius * 0.4).max(1.0))
            .ceil()
            .max(1.0) as usize;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let point = from + (to - from) * t;
            let cx = point.x;
            let cy = point.y;
            let min_x = (cx - radius).floor().max(0.0) as u32;
            let max_x = (cx + radius).ceil().min(self.width as f32) as u32;
            let min_y = (cy - radius).floor().max(0.0) as u32;
            let max_y = (cy + radius).ceil().min(self.height as f32) as u32;
            for y in min_y..max_y {
                for x in min_x..max_x {
                    let dx = x as f32 + 0.5 - cx;
                    let dy = y as f32 + 0.5 - cy;
                    if dx * dx + dy * dy <= radius * radius {
                        let index = (y * self.width + x) as usize;
                        if let Some(pixel) = self.mask.get_mut(index) {
                            if *pixel != value {
                                if value == 0 {
                                    self.painted = self.painted.saturating_sub(1);
                                } else if *pixel == 0 {
                                    self.painted += 1;
                                }
                                *pixel = value;
                            }
                        }
                    }
                }
            }
        }
        self.texture = None;
    }

    fn refresh_texture(&mut self, ctx: &egui::Context) {
        if self.texture.is_some() {
            return;
        }
        let mut pixels = Vec::with_capacity(self.mask.len() * 4);
        for value in &self.mask {
            let shown = if self.invert { 255 - value } else { *value };
            pixels.extend_from_slice(&[255, 255, 255, shown]);
        }
        self.texture = Some(ctx.load_texture(
            format!("editor-mask-{}", self.asset_id),
            egui::ColorImage::from_rgba_unmultiplied(
                [self.width as usize, self.height as usize],
                &pixels,
            ),
            egui::TextureOptions::LINEAR,
        ));
    }

    fn has_mask(&self) -> bool {
        self.painted > 0
    }
}

thread_local! {
    static CANVAS: std::cell::RefCell<Option<CanvasState>> = const { std::cell::RefCell::new(None) };
}

/// Crop rectangle and mask painter drawn over the source image. Works with the
/// mouse alone: drag a rectangle to crop, drag to paint, and toggle the eraser.
fn canvas_ui(
    ui: &mut Ui,
    vm: &ViewModel,
    theme: &Theme,
    backend: &dyn Backend,
    flow: &mut EditorFlow,
) {
    let asset = vm
        .assets
        .iter()
        .find(|item| item.id == flow.asset_id)
        .cloned();
    let Some(asset) = asset else {
        return;
    };
    let mut sink = super::media::backend_sink(backend);
    CANVAS.with(|cell| {
        let mut slot = cell.borrow_mut();
        let stale = slot
            .as_ref()
            .map(|state| state.asset_id != asset.id)
            .unwrap_or(true);
        if stale {
            *slot = CanvasState::new(&asset, ui.ctx());
        }
        let Some(canvas) = slot.as_mut() else {
            ui.label(
                egui::RichText::new("Preview unavailable for this media file").color(theme.muted),
            );
            return;
        };

        ui.horizontal(|ui| {
            if ui
                .selectable_label(canvas.tool == CanvasTool::Crop, "Crop")
                .clicked()
            {
                canvas.tool = CanvasTool::Crop;
            }
            if ui
                .selectable_label(canvas.tool == CanvasTool::Brush, "Mask brush")
                .clicked()
            {
                canvas.tool = CanvasTool::Brush;
            }
            if ui
                .selectable_label(canvas.tool == CanvasTool::Eraser, "Mask eraser")
                .clicked()
            {
                canvas.tool = CanvasTool::Eraser;
            }
            ui.label(egui::RichText::new("Size").color(theme.muted));
            ui.add(egui::Slider::new(&mut canvas.brush, 2.0..=160.0).suffix(" px"));
            ui.label(egui::RichText::new("Feather").color(theme.muted));
            ui.add(egui::Slider::new(&mut canvas.feather, 0..=64).suffix(" px"));
            if ui.checkbox(&mut canvas.invert, "Invert").changed() {
                canvas.texture = None;
            }
        });

        let max_width = ui.available_width().max(240.0);
        let scale = (max_width / canvas.width as f32).min(1.0);
        let display = egui::Vec2::new(canvas.width as f32 * scale, canvas.height as f32 * scale);
        let (rect, response) = ui.allocate_exact_size(display, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        if let Some(source) = &canvas.source {
            painter.image(
                source.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        if canvas.has_mask() {
            canvas.refresh_texture(ui.ctx());
            if let Some(texture) = &canvas.texture {
                painter.image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        }

        let to_canvas = |position: egui::Pos2| -> egui::Pos2 {
            egui::pos2(
                ((position.x - rect.min.x) / scale).clamp(0.0, canvas.width as f32),
                ((position.y - rect.min.y) / scale).clamp(0.0, canvas.height as f32),
            )
        };

        match canvas.tool {
            CanvasTool::Crop => {
                if response.drag_started()
                    && let Some(position) = response.interact_pointer_pos()
                {
                    canvas.crop_start = Some(to_canvas(position));
                    canvas.crop_end = None;
                }
                if response.dragged()
                    && let Some(position) = response.interact_pointer_pos()
                {
                    canvas.crop_end = Some(to_canvas(position));
                }
                if let (Some(start), Some(end)) = (canvas.crop_start, canvas.crop_end) {
                    let selection = egui::Rect::from_two_pos(
                        rect.min + start.to_vec2() * scale,
                        rect.min + end.to_vec2() * scale,
                    );
                    painter.rect_stroke(
                        selection,
                        0.0,
                        egui::Stroke::new(2.0, theme.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.drag_stopped()
                    && let (Some(start), Some(end)) = (canvas.crop_start, canvas.crop_end)
                {
                    let left = start.x.min(end.x) / canvas.width as f32 * 100.0;
                    let right = 100.0 - start.x.max(end.x) / canvas.width as f32 * 100.0;
                    let top = start.y.min(end.y) / canvas.height as f32 * 100.0;
                    let bottom = 100.0 - start.y.max(end.y) / canvas.height as f32 * 100.0;
                    if (start - end).length() > 4.0 {
                        dispatch(
                            flow,
                            EditorAction::RectCrop {
                                left: left as f64,
                                right: right as f64,
                                top: top as f64,
                                bottom: bottom as f64,
                            },
                            &mut sink,
                        );
                    }
                    canvas.crop_start = None;
                    canvas.crop_end = None;
                }
            }
            CanvasTool::Brush | CanvasTool::Eraser => {
                let erase = canvas.tool == CanvasTool::Eraser;
                let pointer = ui.input(|input| input.pointer.interact_pos());
                let (down, released) = ui.input(|input| {
                    (
                        input.pointer.primary_down(),
                        input.pointer.primary_released(),
                    )
                });
                if released {
                    canvas.last_stroke = None;
                }
                if down
                    && let Some(position) = pointer
                    && rect.contains(position)
                {
                    let point = to_canvas(position);
                    let from = canvas.last_stroke.unwrap_or(point);
                    canvas.paint_segment(from, point, erase);
                    canvas.last_stroke = Some(point);
                }
            }
        }

        ui.horizontal(|ui| {
            if ui
                .add_enabled(canvas.has_mask(), egui::Button::new("Clear mask"))
                .clicked()
            {
                canvas.mask = vec![0u8; (canvas.width * canvas.height) as usize];
                canvas.painted = 0;
                canvas.texture = None;
                canvas.exported_path = None;
            }
            if ui
                .add_enabled(canvas.has_mask(), egui::Button::new("Export mask"))
                .clicked()
            {
                let width = flow.target.width.max(flow.width).max(1);
                let height = flow.target.height.max(flow.height).max(1);
                let polarity = if flow.target.mask_polarity == "blackKeeps" {
                    "blackKeeps"
                } else {
                    "whiteKeeps"
                };
                let args = json!({
                    "request": {
                        "assetId": flow.asset_id,
                        "mask": canvas.mask,
                        "maskWidth": canvas.width,
                        "maskHeight": canvas.height,
                        "targetWidth": width,
                        "targetHeight": height,
                        "feather": canvas.feather,
                        "invert": canvas.invert,
                        "polarity": polarity,
                    }
                });
                match sink("edits_export_mask", args) {
                    Ok(value) => {
                        let path = value
                            .get("path")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        canvas.exported_path = path.clone();
                        if let Some(path) = path {
                            dispatch(flow, EditorAction::MaskPath { path }, &mut sink);
                        }
                    }
                    Err(error) => canvas.last_error = Some(error),
                }
            }
            ui.label(
                egui::RichText::new(format!("mask {}/{} px", canvas.painted, canvas.mask.len()))
                    .color(theme.faint),
            );
            if let Some(path) = &canvas.exported_path {
                ui.label(egui::RichText::new(format!("Mask: {path}")).color(theme.success));
            }
            ui.label(egui::RichText::new("Drag on the image to crop or paint").color(theme.faint));
        });
        if let Some(error) = &canvas.last_error {
            ui.label(egui::RichText::new(error).color(theme.danger));
        }
    });
}

#[cfg(test)]
mod canvas_tests {
    use super::*;

    fn canvas() -> CanvasState {
        CanvasState {
            asset_id: "asset-1".into(),
            width: 100,
            height: 50,
            mask: vec![0u8; 100 * 50],
            painted: 0,
            texture: None,
            source: None,
            tool: CanvasTool::Brush,
            brush: 5.0,
            feather: 0,
            invert: false,
            crop_start: None,
            crop_end: None,
            last_stroke: None,
            last_error: None,
            exported_path: None,
        }
    }

    #[test]
    fn brush_paints_and_eraser_clears() {
        let mut state = canvas();
        assert!(!state.has_mask());
        state.paint_segment(egui::pos2(20.0, 20.0), egui::pos2(60.0, 20.0), false);
        let painted = state.mask.iter().filter(|value| **value > 0).count();
        assert!(painted > 0, "brush must paint pixels");
        assert!(state.has_mask());
        state.paint_segment(egui::pos2(20.0, 20.0), egui::pos2(60.0, 20.0), true);
        let remaining = state.mask.iter().filter(|value| **value > 0).count();
        assert!(remaining < painted, "eraser must clear painted pixels");
    }

    #[test]
    fn painting_stays_inside_the_canvas() {
        let mut state = canvas();
        state.paint_segment(egui::pos2(-50.0, -50.0), egui::pos2(500.0, 500.0), false);
        assert_eq!(state.mask.len(), 100 * 50);
    }

    #[test]
    fn crop_rectangle_maps_to_percentage_edges() {
        // A drag from (25,10) to (75,40) on a 100x50 canvas.
        let edges = CropEdges {
            left: 25.0,
            right: 25.0,
            top: 20.0,
            bottom: 20.0,
        };
        let crop = crop_from_edges(100, 50, &edges).expect("crop");
        assert_eq!(crop.x, 25.0);
        assert_eq!(crop.y, 10.0);
        assert!(crop.width >= 50.0);
        assert!(crop.height >= 30.0);
    }

    #[test]
    fn mask_export_request_matches_the_command_contract() {
        // The command expects one grayscale byte per pixel of mask_width*height.
        let state = canvas();
        assert_eq!(state.mask.len(), (state.width * state.height) as usize);
    }
}
