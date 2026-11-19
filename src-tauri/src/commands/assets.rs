use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::db::Database;
use crate::db::repository::{assets as asset_repo, frames as frame_repo};
use crate::media::ffmpeg::MediaTools;
use crate::media::frames::{self as frame_sampler, SampleRequest, SamplingStrategy};
use crate::media::import::{ImportControl, ImportService};
use crate::media::protocol::MediaAccess;
use crate::paths::AppPaths;

use super::CommandError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetKind {
    Image,
    Video,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub kind: AssetKind,
    pub path: String,
    pub display_name: String,
    pub hash: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub duration_ms: Option<u64>,
    pub codec: Option<String>,
    pub has_audio: bool,
    pub thumbnail_path: Option<String>,
    pub proxy_path: Option<String>,
    pub missing: bool,
    pub favorite: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub path: String,
    pub asset_id: Option<String>,
    pub duplicate: bool,
    pub restored: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardImportRequest {
    pub file_name: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleFramesRequest {
    pub asset_id: String,
    pub strategy: SamplingStrategy,
    pub count: u8,
    pub max_images: u8,
    pub contact_sheet: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameDto {
    pub index: u32,
    pub timestamp_ms: u64,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleFramesResult {
    pub frames: Vec<FrameDto>,
    pub contact_sheet: Option<String>,
    pub scene_count: Option<u32>,
    pub capped: bool,
}

#[tauri::command]
pub async fn assets_import(
    request: ImportRequest,
    app: AppHandle,
    database: State<'_, Mutex<Database>>,
    paths: State<'_, AppPaths>,
    tools: State<'_, MediaTools>,
    control: State<'_, ImportControl>,
    access: State<'_, MediaAccess>,
) -> Result<Vec<ImportResult>, CommandError> {
    run_import(
        request.paths,
        &app,
        &database,
        &paths,
        &tools,
        &control,
        &access,
    )
}

#[tauri::command]
pub async fn assets_import_clipboard(
    request: ClipboardImportRequest,
    app: AppHandle,
    database: State<'_, Mutex<Database>>,
    paths: State<'_, AppPaths>,
    tools: State<'_, MediaTools>,
    control: State<'_, ImportControl>,
    access: State<'_, MediaAccess>,
) -> Result<Vec<ImportResult>, CommandError> {
    if request.bytes.is_empty() {
        return Err(CommandError::validation("Clipboard image is empty"));
    }
    if request.bytes.len() > 64 * 1024 * 1024 {
        return Err(CommandError::validation(
            "Clipboard image is larger than 64 MB",
        ));
    }

    let directory = paths.cache_dir.join("imports");
    std::fs::create_dir_all(&directory).map_err(|error| CommandError::from_io(&error))?;

    let requested = request.file_name.unwrap_or_else(|| "clipboard.png".into());
    let stem = Path::new(&requested)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("clipboard");
    let safe_stem: String = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();

    let file = directory.join(format!("{}-{safe_stem}.png", Uuid::now_v7()));
    std::fs::write(&file, &request.bytes).map_err(|error| CommandError::from_io(&error))?;

    run_import(
        vec![file.display().to_string()],
        &app,
        &database,
        &paths,
        &tools,
        &control,
        &access,
    )
}

#[tauri::command]
pub async fn assets_import_cancel(control: State<'_, ImportControl>) -> Result<(), CommandError> {
    if let Some(flag) = control
        .current
        .lock()
        .map_err(|_| CommandError::internal("Import lock is poisoned"))?
        .as_ref()
    {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

fn run_import(
    inputs: Vec<String>,
    app: &AppHandle,
    database: &State<'_, Mutex<Database>>,
    paths: &State<'_, AppPaths>,
    tools: &State<'_, MediaTools>,
    control: &State<'_, ImportControl>,
    access: &State<'_, MediaAccess>,
) -> Result<Vec<ImportResult>, CommandError> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut current = control
            .current
            .lock()
            .map_err(|_| CommandError::internal("Import lock is poisoned"))?;
        if current.is_some() {
            return Err(CommandError::conflict("An import is already running"));
        }
        *current = Some(cancel.clone());
    }

    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let service = ImportService {
        tools: tools.inner(),
        connection: guard.connection(),
        paths: paths.inner(),
    };
    let app_handle = app.clone();
    let results = service.import(&inputs, &cancel, |progress| {
        let _ = app_handle.emit("assets://import-progress", &progress);
    });

    if let Ok(mut current) = control.current.lock() {
        *current = None;
    }

    for result in &results {
        if result.asset_id.is_some() {
            access.allow_path(Path::new(&result.path));
        }
    }

    Ok(results)
}

#[tauri::command]
pub async fn assets_list(database: State<'_, Mutex<Database>>) -> Result<Vec<Asset>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    asset_repo::refresh_missing(guard.connection())?;
    let rows = asset_repo::list(guard.connection(), 1000, 0)?;
    Ok(rows.into_iter().map(to_asset).collect())
}

#[tauri::command]
pub async fn assets_sample_frames(
    request: SampleFramesRequest,
    database: State<'_, Mutex<Database>>,
    paths: State<'_, AppPaths>,
    tools: State<'_, MediaTools>,
) -> Result<SampleFramesResult, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let asset = asset_repo::get(guard.connection(), &request.asset_id)?
        .ok_or_else(|| CommandError::not_found("Asset not found"))?;

    let is_video = asset.kind == "video";
    let duration_ms = asset.duration_ms.unwrap_or_default().max(0) as u64;

    let sampled = frame_sampler::sample(
        tools.inner(),
        &SampleRequest {
            source: Path::new(&asset.source_path),
            duration_ms,
            is_video,
            strategy: request.strategy,
            count: request.count,
            max_images: request.max_images,
            want_contact_sheet: request.contact_sheet,
            cache_dir: &paths.cache_dir,
            asset_hash: &asset.hash,
        },
    )?;

    let rows: Vec<frame_repo::FrameRow> = sampled
        .frames
        .iter()
        .map(|frame| frame_repo::FrameRow {
            frame_index: frame.index as i64,
            timestamp_ms: frame.timestamp_ms as i64,
            path: frame.path.display().to_string(),
        })
        .collect();
    frame_repo::replace_for_asset(guard.connection(), &request.asset_id, &rows)?;

    Ok(SampleFramesResult {
        frames: sampled
            .frames
            .into_iter()
            .map(|frame| FrameDto {
                index: frame.index,
                timestamp_ms: frame.timestamp_ms,
                path: frame.path.display().to_string(),
            })
            .collect(),
        contact_sheet: sampled.contact_sheet.map(|path| path.display().to_string()),
        scene_count: sampled.scene_count,
        capped: sampled.capped,
    })
}

fn to_asset(row: asset_repo::AssetRow) -> Asset {
    Asset {
        id: row.id,
        kind: if row.kind == "video" {
            AssetKind::Video
        } else {
            AssetKind::Image
        },
        path: row.source_path,
        display_name: row.display_name,
        hash: row.hash,
        width: row.width,
        height: row.height,
        fps: row.fps,
        duration_ms: row.duration_ms.map(|value| value as u64),
        codec: row.codec,
        has_audio: row.has_audio,
        thumbnail_path: row.thumbnail_path,
        proxy_path: row.proxy_path,
        missing: row.missing,
        favorite: row.favorite,
        created_at: row.created_at,
    }
}
