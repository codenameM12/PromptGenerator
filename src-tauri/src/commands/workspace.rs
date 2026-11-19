use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::workspace::WorkspaceStore;

use super::edits::EditOps;
use super::{CommandError, Guidance};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDraft {
    pub asset_id: String,
    pub edit_ops: Option<EditOps>,
    pub selected_preset_id: Option<String>,
    pub guidance: Guidance,
    #[serde(default)]
    pub params: Option<Value>,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDraftRequest {
    pub draft: WorkspaceDraft,
}

#[tauri::command]
pub async fn workspace_get_draft(
    state: State<'_, Mutex<WorkspaceStore>>,
) -> Result<Option<WorkspaceDraft>, CommandError> {
    let store = state
        .lock()
        .map_err(|_| CommandError::internal("Workspace lock is poisoned"))?;
    Ok(store.get())
}

#[tauri::command]
pub async fn workspace_save_draft(
    request: SaveDraftRequest,
    state: State<'_, Mutex<WorkspaceStore>>,
) -> Result<WorkspaceDraft, CommandError> {
    let mut store = state
        .lock()
        .map_err(|_| CommandError::internal("Workspace lock is poisoned"))?;
    store.save(request.draft)
}

#[tauri::command]
pub async fn workspace_clear_draft(
    state: State<'_, Mutex<WorkspaceStore>>,
) -> Result<(), CommandError> {
    let mut store = state
        .lock()
        .map_err(|_| CommandError::internal("Workspace lock is poisoned"))?;
    store.clear()
}
