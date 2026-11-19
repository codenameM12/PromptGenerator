use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::drag_out::{self, Transfer};
use crate::paths::AppPaths;
use crate::settings::SettingsStore;

use super::CommandError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DragPayloadKind {
    PlainPrompt,
    StructuredJson,
    Original,
    Edited,
    Mask,
    Bundle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DragPrepareRequest {
    pub generation_id: Option<String>,
    pub asset_id: Option<String>,
    pub edit_id: Option<String>,
    pub payload: DragPayloadKind,
    pub sidecars: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DragPayload {
    pub files: Vec<String>,
    pub prepared_dir: String,
    pub suggested_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DragSaveRequest {
    pub source: String,
    pub output_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DragRevealRequest {
    pub path: String,
}

#[derive(Default)]
pub struct DragControl {
    pub current: Mutex<Option<Arc<Transfer>>>,
}

#[tauri::command]
pub async fn drag_prepare(
    request: DragPrepareRequest,
    app: AppHandle,
    database: State<'_, Mutex<crate::db::Database>>,
    paths: State<'_, AppPaths>,
    settings: State<'_, Mutex<SettingsStore>>,
    control: State<'_, DragControl>,
) -> Result<DragPayload, CommandError> {
    drag_out::cleanup(&paths.cache_dir, std::time::SystemTime::now());

    let transfer = Arc::new(Transfer::default());
    {
        let mut current = control
            .current
            .lock()
            .map_err(|_| CommandError::internal("Drag lock is poisoned"))?;
        if current.is_some() {
            return Err(CommandError::conflict(
                "Another drag file is still being prepared",
            ));
        }
        *current = Some(Arc::clone(&transfer));
    }

    let template = {
        let store = settings
            .lock()
            .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
        store.get().naming_template
    };

    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let mut on_progress = |progress: f64| {
        let _ = app.emit("drag://progress", progress);
    };
    let result = drag_out::prepare(
        guard.connection(),
        &request,
        &template,
        &paths.cache_dir,
        &transfer,
        &mut on_progress,
    );

    drop(guard);
    if let Ok(mut current) = control.current.lock() {
        *current = None;
    }
    match result {
        Ok(prepared) => Ok(DragPayload {
            files: prepared.files,
            prepared_dir: prepared.prepared_dir,
            suggested_name: prepared.suggested_name,
        }),
        Err(error) => {
            if transfer.cancel.load(Ordering::Relaxed) {
                Err(CommandError::cancelled())
            } else {
                Err(error)
            }
        }
    }
}

#[tauri::command]
pub async fn drag_cancel(control: State<'_, DragControl>) -> Result<(), CommandError> {
    if let Some(transfer) = control
        .current
        .lock()
        .map_err(|_| CommandError::internal("Drag lock is poisoned"))?
        .as_ref()
    {
        transfer.cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn drag_save(
    request: DragSaveRequest,
    paths: State<'_, AppPaths>,
) -> Result<u64, CommandError> {
    drag_out::save_as(
        Path::new(&request.source),
        Path::new(&request.output_path),
        &paths.cache_dir,
    )
}

#[tauri::command]
pub async fn drag_reveal(request: DragRevealRequest) -> Result<(), CommandError> {
    let path = PathBuf::from(&request.path);
    if !path.exists() {
        return Err(CommandError::not_found("That file no longer exists"));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(|error| CommandError::from_io(&error))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|error| CommandError::from_io(&error))?;
        Ok(())
    }
}
