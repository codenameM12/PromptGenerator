use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::commands::CommandError;
use crate::commands::edits::{Crop, EditOps, FitMode, TargetSpec, Trim};

#[derive(Debug, Clone, PartialEq)]
pub struct SourceInfo {
    pub width: u32,
    pub height: u32,
    pub duration_ms: Option<u64>,
    pub fps: Option<f64>,
    pub is_video: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionPlan {
    pub width: u32,
    pub height: u32,
    pub upscaled: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPlan {
    pub crop: Option<Crop>,
    pub resolution: ResolutionPlan,
    pub trim: Option<Trim>,
    pub fps: Option<f64>,
    pub remove_audio: bool,
    pub format: String,
    pub loop_video: bool,
    pub mask_path: Option<String>,
    pub warnings: Vec<String>,
}

/// Rounds to the nearest multiple, with ties rounding up.
pub fn snap(value: u32, multiple: u32) -> u32 {
    if multiple <= 1 {
        return value.max(1);
    }
    let multiple = multiple as u64;
    let value = value.max(1) as u64;
    (((value + multiple / 2) / multiple) * multiple).max(multiple) as u32
}

pub fn derive_resolution(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
    multiple: u32,
    fit: FitMode,
) -> ResolutionPlan {
    let source_width = source_width.max(1);
    let source_height = source_height.max(1);

    if target_width == 0 || target_height == 0 {
        let width = snap(source_width, multiple);
        let height = snap(source_height, multiple);
        return ResolutionPlan {
            width,
            height,
            upscaled: false,
            warnings: Vec::new(),
        };
    }

    let target_width = snap(target_width, multiple);
    let target_height = snap(target_height, multiple);

    let (width, height) = match fit {
        FitMode::Fit => {
            let scale = f64::min(
                target_width as f64 / source_width as f64,
                target_height as f64 / source_height as f64,
            );
            let scaled_width =
                snap((source_width as f64 * scale).round() as u32, multiple).min(target_width);
            let scaled_height =
                snap((source_height as f64 * scale).round() as u32, multiple).min(target_height);
            (scaled_width.max(1), scaled_height.max(1))
        }
        FitMode::Fill | FitMode::Pad => (target_width, target_height),
    };

    let upscaled = width > source_width || height > source_height;
    let mut warnings = Vec::new();
    if upscaled {
        warnings.push(format!(
            "Output {width}x{height} is larger than the source {source_width}x{source_height} and will be upscaled"
        ));
    }

    ResolutionPlan {
        width,
        height,
        upscaled,
        warnings,
    }
}

/// Center crop to the target aspect ratio. Returns `None` when the target has
/// no explicit box.
pub fn auto_crop(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> Option<Crop> {
    if target_width == 0 || target_height == 0 || source_width == 0 || source_height == 0 {
        return None;
    }

    let target_ratio = target_width as f64 / target_height as f64;
    let source_ratio = source_width as f64 / source_height as f64;

    if source_ratio > target_ratio {
        let width = (source_height as f64 * target_ratio).round() as u32;
        Some(Crop {
            x: (source_width - width) / 2,
            y: 0,
            width: width.max(1),
            height: source_height,
        })
    } else {
        let height = (source_width as f64 / target_ratio).round() as u32;
        Some(Crop {
            x: 0,
            y: (source_height - height) / 2,
            width: source_width,
            height: height.max(1),
        })
    }
}

pub fn plan(
    ops: &EditOps,
    target: &TargetSpec,
    source: &SourceInfo,
) -> Result<EditPlan, CommandError> {
    let mut warnings = Vec::new();

    let crop = match &ops.crop {
        Some(crop) => {
            if crop.width == 0 || crop.height == 0 {
                return Err(CommandError::validation(
                    "Crop width and height must be positive",
                ));
            }
            if crop.x + crop.width > source.width || crop.y + crop.height > source.height {
                return Err(CommandError::validation(format!(
                    "Crop {}+{} at {},{} exceeds the source {}x{}",
                    crop.width, crop.height, crop.x, crop.y, source.width, source.height
                )));
            }
            Some(crop.clone())
        }
        None if ops.fit == FitMode::Fill => {
            let crop = auto_crop(source.width, source.height, target.width, target.height);
            if crop.is_some() {
                warnings.push("A centered crop was applied for Fill placement".into());
            }
            crop
        }
        None => None,
    };

    let (effective_width, effective_height) = match &crop {
        Some(crop) => (crop.width, crop.height),
        None => (source.width, source.height),
    };
    let resolution = derive_resolution(
        effective_width,
        effective_height,
        target.width,
        target.height,
        target.dimension_multiple.max(1),
        ops.fit,
    );
    warnings.extend(resolution.warnings.clone());

    let trim = match &ops.trim {
        Some(trim) => {
            let duration = source.duration_ms.unwrap_or(0);
            if trim.end_ms <= trim.start_ms {
                return Err(CommandError::validation(
                    "Trim end must be after the trim start",
                ));
            }
            if duration > 0 && trim.end_ms > duration {
                warnings.push("Trim end was clamped to the source duration".into());
            }
            Some(Trim {
                start_ms: trim.start_ms.min(duration.saturating_sub(1)),
                end_ms: if duration > 0 {
                    trim.end_ms.min(duration)
                } else {
                    trim.end_ms
                },
            })
        }
        None => None,
    };

    if let Some(fps) = ops.fps
        && !matches!(fps as u32, 24 | 25 | 30 | 60)
    {
        return Err(CommandError::validation(
            "Frame rate must be 24, 25, 30, or 60",
        ));
    }

    let format = ops.format.clone().unwrap_or_else(|| {
        if source.is_video {
            "mp4".into()
        } else {
            "png".into()
        }
    });
    let allowed = if source.is_video {
        ["mp4", "webm"].as_slice()
    } else {
        ["png", "jpg", "jpeg", "webp"].as_slice()
    };
    if !allowed.contains(&format.as_str()) {
        return Err(CommandError::validation(format!(
            "Unsupported output format: {format}"
        )));
    }

    if ops.loop_video && !source.is_video {
        return Err(CommandError::validation(
            "Looping is only available for video",
        ));
    }

    if let Some(mask) = &ops.mask_path
        && !Path::new(mask).is_file()
    {
        return Err(CommandError::not_found(format!(
            "Mask file not found: {mask}"
        )));
    }

    Ok(EditPlan {
        crop,
        resolution,
        trim,
        fps: ops.fps,
        remove_audio: ops.remove_audio,
        format,
        loop_video: ops.loop_video,
        mask_path: ops.mask_path.clone(),
        warnings,
    })
}

/// Warnings produced when the written file differs from the plan.
pub fn verification_warnings(
    plan: &EditPlan,
    actual_width: u32,
    actual_height: u32,
    actual_fps: Option<f64>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if actual_width != plan.resolution.width || actual_height != plan.resolution.height {
        warnings.push(format!(
            "Output is {actual_width}x{actual_height}, expected {}x{}",
            plan.resolution.width, plan.resolution.height
        ));
    }
    if let Some(requested) = plan.fps
        && let Some(actual) = actual_fps
        && (actual - requested).abs() > 0.5
    {
        warnings.push(format!(
            "Frame rate is {actual:.2}, requested {requested:.2}"
        ));
    }
    warnings
}

/// Stable cache key for an edit result.
pub fn cache_key(
    source_hash: &str,
    ops: &EditOps,
    target: &TargetSpec,
) -> Result<String, CommandError> {
    let payload = serde_json::to_string(&(source_hash, ops, target))
        .map_err(|error| CommandError::internal(format!("serialize edit key: {error}")))?;
    Ok(blake3::hash(payload.as_bytes()).to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::{SourceInfo, auto_crop, cache_key, derive_resolution, plan, snap};
    use crate::commands::edits::{Crop, EditOps, FitMode, MaskPolarity, TargetSpec, Trim};

    fn ops() -> EditOps {
        EditOps {
            crop: None,
            rotate: 0,
            flip_h: false,
            flip_v: false,
            fit: FitMode::Fit,
            trim: None,
            fps: None,
            remove_audio: false,
            mask_path: None,
            format: None,
            loop_video: false,
            pad_color: None,
        }
    }

    fn target(width: u32, height: u32) -> TargetSpec {
        TargetSpec {
            preset_id: "test".into(),
            width,
            height,
            dimension_multiple: 16,
            mask_polarity: MaskPolarity::WhiteKeeps,
        }
    }

    fn source() -> SourceInfo {
        SourceInfo {
            width: 1920,
            height: 1080,
            duration_ms: Some(2000),
            fps: Some(30.0),
            is_video: true,
        }
    }

    #[test]
    fn snaps_to_the_nearest_multiple() {
        assert_eq!(snap(1000, 16), 1008);
        assert_eq!(snap(563, 16), 560);
        assert_eq!(snap(16, 16), 16);
        assert_eq!(snap(0, 16), 16);
    }

    #[test]
    fn derives_from_source_when_target_has_no_box() {
        let resolution = derive_resolution(1000, 563, 0, 0, 16, FitMode::Fit);
        assert_eq!(resolution.width, 1008);
        assert_eq!(resolution.height, 560);
        assert!(!resolution.upscaled);
    }

    #[test]
    fn fits_within_a_target_box() {
        let resolution = derive_resolution(1920, 1080, 1024, 1024, 16, FitMode::Fit);
        assert_eq!(resolution.width, 1024);
        assert_eq!(resolution.height, 576);
        assert!(!resolution.upscaled);
    }

    #[test]
    fn warns_on_upscale() {
        let resolution = derive_resolution(640, 360, 1024, 1024, 16, FitMode::Fit);
        assert!(resolution.upscaled);
        assert!(!resolution.warnings.is_empty());
    }

    #[test]
    fn auto_crops_to_target_aspect() {
        let crop = auto_crop(800, 600, 16, 9).expect("crop");
        assert_eq!(crop.width, 800);
        assert_eq!(crop.height, 450);
        assert_eq!(crop.y, 75);
    }

    #[test]
    fn plan_clamps_trim_and_rejects_bad_fps() {
        let mut value = ops();
        value.trim = Some(Trim {
            start_ms: 0,
            end_ms: 5000,
        });
        let planned = plan(&value, &target(0, 0), &source()).expect("plan");
        assert_eq!(planned.trim.expect("trim").end_ms, 2000);

        value.fps = Some(27.0);
        assert!(plan(&value, &target(0, 0), &source()).is_err());
    }

    #[test]
    fn plan_rejects_crop_outside_source() {
        let mut value = ops();
        value.crop = Some(Crop {
            x: 1800,
            y: 0,
            width: 200,
            height: 100,
        });
        assert!(plan(&value, &target(0, 0), &source()).is_err());
    }

    #[test]
    fn verification_reports_dimension_and_fps_mismatches() {
        let mut planned_ops = ops();
        planned_ops.fps = Some(30.0);
        let planned = plan(&planned_ops, &target(1024, 1024), &source()).expect("plan");
        let warnings = super::verification_warnings(&planned, 800, 600, Some(24.0));
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("expected 1024x576"))
        );
        assert!(warnings.iter().any(|warning| warning.contains("requested")));

        let warnings = super::verification_warnings(&planned, 1024, 576, Some(30.0));
        assert!(warnings.is_empty());
    }

    #[test]
    fn cache_key_changes_with_operations() {
        let first = cache_key("hash", &ops(), &target(1024, 1024)).expect("key");
        let mut changed = ops();
        changed.rotate = 90;
        let second = cache_key("hash", &changed, &target(1024, 1024)).expect("key");
        assert_ne!(first, second);
        assert_eq!(
            first,
            cache_key("hash", &ops(), &target(1024, 1024)).expect("key")
        );
    }
}
