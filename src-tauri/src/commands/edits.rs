use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::db::Database;
use crate::db::repository::assets as asset_repo;
use crate::db::repository::edits as edit_repo;
use crate::media::edit::{self, EditPlan, SourceInfo};
use crate::media::ffmpeg::MediaTools;
use crate::media::mask::{self, MaskExportRequest};
use crate::media::probe;
use crate::media::render::{self, RenderRequest};
use crate::settings::SettingsStore;

use super::CommandError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trim {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitMode {
    Fit,
    Fill,
    Pad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaskPolarity {
    WhiteKeeps,
    BlackKeeps,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetSpec {
    pub preset_id: String,
    pub width: u32,
    pub height: u32,
    pub dimension_multiple: u32,
    pub mask_polarity: MaskPolarity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOps {
    pub crop: Option<Crop>,
    pub rotate: i32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub fit: FitMode,
    pub trim: Option<Trim>,
    pub fps: Option<f64>,
    pub remove_audio: bool,
    pub mask_path: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub loop_video: bool,
    #[serde(default)]
    pub pad_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequest {
    pub asset_id: String,
    pub ops: EditOps,
    pub target: TargetSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    pub edit_id: String,
    pub output_path: String,
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub duration_ms: Option<u64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportMaskRequest {
    pub asset_id: String,
    pub mask: Vec<u8>,
    pub mask_width: u32,
    pub mask_height: u32,
    pub target_width: u32,
    pub target_height: u32,
    pub feather: u32,
    pub invert: bool,
    pub polarity: MaskPolarity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportMaskResult {
    pub path: String,
}

/// Cancellation flag for the single active render.
#[derive(Default)]
pub struct EditControl {
    pub current: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestEdit {
    pub asset_id: String,
    pub edit_id: String,
    pub output_path: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Newest applied edit per asset. Assets without an applied edit are omitted.
#[tauri::command]
pub async fn edits_latest(
    asset_ids: Vec<String>,
    database: State<'_, Mutex<Database>>,
) -> Result<Vec<LatestEdit>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    Ok(edit_repo::latest_by_assets(guard.connection(), &asset_ids)?
        .into_iter()
        .map(|edit| LatestEdit {
            asset_id: edit.asset_id,
            edit_id: edit.id,
            output_path: edit.output_path,
            width: edit.width,
            height: edit.height,
        })
        .collect())
}

#[tauri::command]
pub async fn edits_preview(
    request: EditRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<EditPlan, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let asset = asset_repo::get(guard.connection(), &request.asset_id)?
        .ok_or_else(|| CommandError::not_found("Asset not found"))?;

    edit::plan(&request.ops, &request.target, &source_info(&asset))
}

#[tauri::command]
pub async fn edits_save(
    request: EditRequest,
    app: AppHandle,
    database: State<'_, Mutex<Database>>,
    settings: State<'_, Mutex<SettingsStore>>,
    tools: State<'_, MediaTools>,
    control: State<'_, EditControl>,
) -> Result<EditResult, CommandError> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut current = control
            .current
            .lock()
            .map_err(|_| CommandError::internal("Edit lock is poisoned"))?;
        if current.is_some() {
            return Err(CommandError::conflict("An edit is already running"));
        }
        *current = Some(cancel.clone());
    }

    let outcome = (|| {
        let guard = database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let asset = asset_repo::get(guard.connection(), &request.asset_id)?
            .ok_or_else(|| CommandError::not_found("Asset not found"))?;
        let source = source_info(&asset);

        let ops_json = serde_json::to_string(&request.ops)
            .map_err(|error| CommandError::internal(format!("serialize edit ops: {error}")))?;
        let target_json = serde_json::to_string(&request.target)
            .map_err(|error| CommandError::internal(format!("serialize target: {error}")))?;
        let cache_key = edit::cache_key(&asset.hash, &request.ops, &request.target)?;

        if let Some(existing) = edit_repo::find_by_cache_key(guard.connection(), &cache_key)?
            && Path::new(&existing.output_path).is_file()
        {
            return Ok(EditResult {
                edit_id: existing.id,
                output_path: existing.output_path,
                width: existing.width.unwrap_or_default(),
                height: existing.height.unwrap_or_default(),
                fps: existing.fps,
                duration_ms: existing.duration_ms.map(|value| value.max(0) as u64),
                warnings: Vec::new(),
            });
        }

        let output_root = {
            let store = settings
                .lock()
                .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
            store.get().output_root.map(std::path::PathBuf::from)
        };

        let app_handle = app.clone();
        let rendered = render::render(
            tools.inner(),
            &RenderRequest {
                source: Path::new(&asset.source_path),
                source_hash: &asset.hash,
                source_info: source,
                ops: &request.ops,
                target: &request.target,
                output_root: output_root.as_deref(),
            },
            &cancel,
            |progress| {
                let _ = app_handle.emit("edits://progress", progress);
            },
        )?;

        let mut warnings = rendered.plan.warnings.clone();
        let actual = probe::probe(tools.inner(), &rendered.output_path).ok();

        if let Some(actual) = &actual {
            warnings.extend(edit::verification_warnings(
                &rendered.plan,
                actual.display_width,
                actual.display_height,
                actual.fps,
            ));
        }

        let width = actual
            .as_ref()
            .map(|value| value.display_width)
            .unwrap_or(rendered.plan.resolution.width);
        let height = actual
            .as_ref()
            .map(|value| value.display_height)
            .unwrap_or(rendered.plan.resolution.height);
        let fps = actual.as_ref().and_then(|value| value.fps);
        let duration_ms = actual.as_ref().and_then(|value| value.duration_ms);

        let edit_id = Uuid::now_v7().to_string();
        edit_repo::insert(
            guard.connection(),
            &edit_repo::NewEdit {
                id: edit_id.clone(),
                asset_id: request.asset_id.clone(),
                ops_json,
                target_json,
                output_path: rendered.output_path.display().to_string(),
                cache_key,
                width: Some(width),
                height: Some(height),
                fps,
                duration_ms: duration_ms.map(|value| value as i64),
            },
        )?;

        Ok(EditResult {
            edit_id,
            output_path: rendered.output_path.display().to_string(),
            width,
            height,
            fps,
            duration_ms,
            warnings,
        })
    })();

    if let Ok(mut current) = control.current.lock() {
        *current = None;
    }
    outcome
}

#[tauri::command]
pub async fn edits_cancel(control: State<'_, EditControl>) -> Result<(), CommandError> {
    if let Some(flag) = control
        .current
        .lock()
        .map_err(|_| CommandError::internal("Edit lock is poisoned"))?
        .as_ref()
    {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn edits_export_mask(
    request: ExportMaskRequest,
    database: State<'_, Mutex<Database>>,
    settings: State<'_, Mutex<SettingsStore>>,
) -> Result<ExportMaskResult, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let asset = asset_repo::get(guard.connection(), &request.asset_id)?
        .ok_or_else(|| CommandError::not_found("Asset not found"))?;

    let output_root = {
        let store = settings
            .lock()
            .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
        store.get().output_root.map(std::path::PathBuf::from)
    };

    let path = mask::export(&MaskExportRequest {
        source: Path::new(&asset.source_path),
        alternate_root: output_root.as_deref(),
        mask: &request.mask,
        mask_width: request.mask_width,
        mask_height: request.mask_height,
        target_width: request.target_width,
        target_height: request.target_height,
        feather: request.feather,
        invert: request.invert,
        polarity: request.polarity,
    })?;

    Ok(ExportMaskResult {
        path: path.display().to_string(),
    })
}

fn source_info(asset: &asset_repo::AssetRow) -> SourceInfo {
    SourceInfo {
        width: asset.width.unwrap_or(1).max(1),
        height: asset.height.unwrap_or(1).max(1),
        duration_ms: asset.duration_ms.map(|value| value.max(0) as u64),
        fps: asset.fps,
        is_video: asset.kind == "video",
    }
}
