//! Real in-process backend for the native UI.
//!
//! Calls the plain-library facade in `prompt_studio_lib::native`, which reuses
//! the same repository and media services as the Tauri commands. Capabilities
//! that still need Tauri-owned state are reported explicitly rather than
//! returning fabricated results.

use std::sync::Arc;

use serde_json::{Value, json};

use prompt_studio_lib::commands::assets::AssetKind as RustAssetKind;
use prompt_studio_lib::native::NativeState;

use crate::backend::{
    Asset, AssetKind, Backend, BackendError, ComposeRequest, DescribeRequest, DragPrepareRequest,
    Generation, ImportResult, PreparedDrag,
};

fn map_error(error: prompt_studio_lib::commands::CommandError) -> BackendError {
    let category = serde_json::to_value(&error.category)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "internal".to_string());
    BackendError {
        code: error.code,
        category,
        message: error.message,
        user_message: error.user_message,
        retryable: error.retryable,
        details: error.details,
    }
}

fn map_asset(asset: prompt_studio_lib::commands::assets::Asset) -> Asset {
    Asset {
        id: asset.id,
        kind: match asset.kind {
            RustAssetKind::Video => AssetKind::Video,
            RustAssetKind::Image => AssetKind::Image,
        },
        path: asset.path,
        display_name: asset.display_name,
        hash: asset.hash,
        width: asset.width,
        height: asset.height,
        fps: asset.fps,
        duration_ms: asset.duration_ms,
        codec: asset.codec,
        has_audio: asset.has_audio,
        thumbnail_path: asset.thumbnail_path,
        proxy_path: asset.proxy_path,
        missing: asset.missing,
        favorite: asset.favorite,
        created_at: asset.created_at,
    }
}

fn map_import(result: prompt_studio_lib::commands::assets::ImportResult) -> ImportResult {
    ImportResult {
        path: result.path,
        asset_id: result.asset_id,
        duplicate: result.duplicate,
        restored: result.restored,
        error: result.error,
    }
}

/// Backend implementation backed by real on-disk state.
pub struct NativeBackend {
    state: Arc<NativeState>,
}

impl NativeBackend {
    pub fn open() -> Result<Self, BackendError> {
        let state = NativeState::open().map_err(map_error)?;
        Ok(Self {
            state: Arc::new(state),
        })
    }
}

impl Backend for NativeBackend {
    fn list_assets(&self) -> Result<Vec<Asset>, BackendError> {
        self.state
            .list_assets()
            .map_err(map_error)
            .map(|rows| rows.into_iter().map(map_asset).collect())
    }

    fn import_paths(&self, paths: &[String]) -> Result<Vec<ImportResult>, BackendError> {
        self.state
            .import_paths(paths)
            .map_err(map_error)
            .map(|rows| rows.into_iter().map(map_import).collect())
    }

    fn import_clipboard(
        &self,
        file_name: Option<&str>,
        bytes: &[u8],
    ) -> Result<Vec<ImportResult>, BackendError> {
        self.state
            .import_clipboard(file_name, bytes)
            .map_err(map_error)
            .map(|rows| rows.into_iter().map(map_import).collect())
    }

    fn cancel(&self) -> Result<(), BackendError> {
        self.state.cancel_import();
        Ok(())
    }

    fn run_describe(&self, request: DescribeRequest) -> Result<Vec<String>, BackendError> {
        let mut described = Vec::new();
        for asset_id in &request.asset_ids {
            let args = json!({
                "request": {
                    "assetId": asset_id,
                    "framePaths": [],
                    "mode": "image",
                    "forceRefresh": false,
                }
            });
            let value = self.call("pipeline_describe", args)?;
            described.push(description_text(&value));
        }
        Ok(described)
    }

    fn run_compose(&self, request: ComposeRequest) -> Result<Vec<Generation>, BackendError> {
        let references: Vec<Value> = request
            .references
            .iter()
            .map(|reference| {
                json!({
                    "assetId": reference.asset_id,
                    "weight": reference.weight,
                    "name": reference.name,
                })
            })
            .collect();
        let args = json!({
            "request": {
                "presetId": request.preset_id,
                "brief": request.brief,
                "outputLanguage": request.output_language,
                "references": references,
                "guidance": {},
                "locked": [],
                "params": {},
                "variations": 1,
                "bypassCache": false,
            }
        });
        let value = self.call("pipeline_compose", args)?;
        serde_json::from_value(value).map_err(|error| BackendError::validation(error.to_string()))
    }

    fn prepare_drag(&self, request: DragPrepareRequest) -> Result<PreparedDrag, BackendError> {
        let args = json!({
            "request": {
                "generationId": request.generation_id,
                "assetId": request.asset_id,
                "payload": request.payload,
                "sidecars": request.sidecars,
            }
        });
        let value = self.call("drag_prepare", args)?;
        serde_json::from_value(value).map_err(|error| BackendError::validation(error.to_string()))
    }

    fn call(
        &self,
        command: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, BackendError> {
        self.state.call(command, args).map_err(map_error)
    }
}

/// `pipeline_describe` returns one `Description` object, while the typed
/// `Backend` seam exposes a list. Accept both shapes so the adapter never
/// reports a serde mismatch as a runtime error.
fn description_text(value: &Value) -> String {
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .next()
            .unwrap_or_default()
            .to_string();
    }
    value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::description_text;
    use serde_json::json;

    #[test]
    fn description_text_accepts_object_and_list_results() {
        assert_eq!(description_text(&json!({ "text": "fox" })), "fox");
        assert_eq!(
            description_text(&json!([{ "text": "fox" }, { "text": "river" }])),
            "fox"
        );
        assert_eq!(description_text(&json!({})), "");
    }
}
