//! Typed backend boundary for the native UI.
//!
//! The real backend (`prompt_studio_lib`) is intentionally *not* linked yet —
//! pulling the Tauri build script and webview stack into this binary is tracked
//! as an integration gap. Views depend only on this trait, so they can be driven
//! by `StubBackend` in tests and by the real implementation once it compiles as
//! a plain library. The DTOs mirror the Rust command DTOs in `src-tauri`, with
//! camelCase serde names matching the recorded source JSON shape.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Structured failure mirroring `src-tauri::commands::CommandError`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendError {
    pub code: String,
    pub category: String,
    pub message: String,
    pub user_message: String,
    pub retryable: bool,
    pub details: Option<String>,
}

impl BackendError {
    pub fn validation(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code: "validation".into(),
            category: "validation".into(),
            user_message: message.clone(),
            message,
            retryable: false,
            details: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code: "forbidden".into(),
            category: "forbidden".into(),
            user_message: message.clone(),
            message,
            retryable: false,
            details: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code: "not_found".into(),
            category: "notFound".into(),
            user_message: message.clone(),
            message,
            retryable: false,
            details: None,
        }
    }

    /// A capability that the native adapter has not wired yet. Distinct from a
    /// domain error so the UI can show "not available" instead of a fake result.
    pub fn not_implemented(feature: &str) -> Self {
        let message = format!("{feature} is not wired to the native backend yet");
        Self {
            code: "not_implemented".into(),
            category: "notImplemented".into(),
            user_message: message.clone(),
            message,
            retryable: false,
            details: None,
        }
    }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for BackendError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetKind {
    Image,
    Video,
}

/// Mirrors `src-tauri::commands::assets::Asset`.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct ComposeReference {
    pub asset_id: Option<String>,
    pub weight: f64,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeRequest {
    pub preset_id: String,
    pub brief: String,
    pub output_language: Option<String>,
    pub references: Vec<ComposeReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Generation {
    pub id: String,
    pub prompt: String,
    pub preset_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribeRequest {
    pub asset_ids: Vec<String>,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub payload: DragPayloadKind,
    pub sidecars: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedDrag {
    pub files: Vec<String>,
    pub prepared_dir: String,
    pub suggested_name: String,
}

/// The seam every native view talks to. Kept small on purpose: panel ports add
/// methods here rather than reaching into backend internals.
pub trait Backend: Send {
    fn list_assets(&self) -> Result<Vec<Asset>, BackendError>;
    fn import_paths(&self, paths: &[String]) -> Result<Vec<ImportResult>, BackendError>;
    fn import_clipboard(
        &self,
        file_name: Option<&str>,
        bytes: &[u8],
    ) -> Result<Vec<ImportResult>, BackendError>;
    fn run_describe(&self, request: DescribeRequest) -> Result<Vec<String>, BackendError>;
    fn run_compose(&self, request: ComposeRequest) -> Result<Vec<Generation>, BackendError>;
    fn cancel(&self) -> Result<(), BackendError>;
    fn prepare_drag(&self, request: DragPrepareRequest) -> Result<PreparedDrag, BackendError>;

    /// Generic command seam used by the live views. The default keeps existing
    /// implementors compiling; `NativeBackend` dispatches the full command
    /// surface through it in-process.
    fn call(
        &self,
        command: &str,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value, BackendError> {
        Err(BackendError::not_implemented(command))
    }
}

/// In-memory backend used by tests and by the runnable shell until the real
/// backend compiles as a plain library. It produces the same DTO shapes.
#[derive(Default)]
pub struct StubBackend {
    assets: Mutex<Vec<Asset>>,
    next_id: Mutex<u64>,
}

impl StubBackend {
    fn alloc_id(&self) -> String {
        let mut next = self.next_id.lock().expect("id lock");
        *next += 1;
        format!("asset-{}", *next)
    }

    fn classify(path: &str) -> AssetKind {
        let lower = path.to_ascii_lowercase();
        if [".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".avif"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        {
            AssetKind::Image
        } else {
            AssetKind::Video
        }
    }
}

impl Backend for StubBackend {
    fn list_assets(&self) -> Result<Vec<Asset>, BackendError> {
        Ok(self.assets.lock().expect("assets lock").clone())
    }

    fn import_paths(&self, paths: &[String]) -> Result<Vec<ImportResult>, BackendError> {
        let mut results = Vec::new();
        for path in paths {
            let name = std::path::Path::new(path)
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("media");
            let asset = Asset {
                id: self.alloc_id(),
                kind: Self::classify(path),
                path: path.clone(),
                display_name: name.to_string(),
                hash: format!("stub-hash-{}", results.len()),
                width: Some(128),
                height: Some(72),
                fps: None,
                duration_ms: None,
                codec: None,
                has_audio: false,
                thumbnail_path: None,
                proxy_path: None,
                missing: false,
                favorite: false,
                created_at: "2026-11-19T00:00:00Z".into(),
            };
            let id = asset.id.clone();
            self.assets.lock().expect("assets lock").push(asset);
            results.push(ImportResult {
                path: path.clone(),
                asset_id: Some(id),
                duplicate: false,
                restored: false,
                error: None,
            });
        }
        Ok(results)
    }

    fn import_clipboard(
        &self,
        file_name: Option<&str>,
        bytes: &[u8],
    ) -> Result<Vec<ImportResult>, BackendError> {
        if bytes.is_empty() {
            return Err(BackendError::validation("Clipboard image is empty"));
        }
        let name = file_name.unwrap_or("clipboard.png");
        self.import_paths(&[format!("clipboard://{name}")])
    }

    fn run_describe(&self, request: DescribeRequest) -> Result<Vec<String>, BackendError> {
        Ok(request
            .asset_ids
            .iter()
            .map(|id| format!("A description for {id}."))
            .collect())
    }

    fn run_compose(&self, request: ComposeRequest) -> Result<Vec<Generation>, BackendError> {
        Ok(vec![Generation {
            id: "gen-1".into(),
            prompt: format!("Prompt for: {}", request.brief),
            preset_id: request.preset_id,
        }])
    }

    fn cancel(&self) -> Result<(), BackendError> {
        Ok(())
    }

    fn prepare_drag(&self, request: DragPrepareRequest) -> Result<PreparedDrag, BackendError> {
        Ok(PreparedDrag {
            files: Vec::new(),
            prepared_dir: String::new(),
            suggested_name: request.asset_id.unwrap_or_else(|| "prompt".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_import_produces_source_metadata_shape() {
        let backend = StubBackend::default();
        let results = backend
            .import_paths(&[r"C:\Pictures\photo.png".to_string()])
            .expect("import");
        assert_eq!(results.len(), 1);
        let assets = backend.list_assets().expect("list");
        let asset = &assets[0];
        let json = serde_json::to_value(asset).expect("serialize");
        for key in [
            "id",
            "kind",
            "path",
            "displayName",
            "hash",
            "width",
            "height",
            "fps",
            "durationMs",
            "codec",
            "hasAudio",
            "thumbnailPath",
            "proxyPath",
            "missing",
            "favorite",
            "createdAt",
        ] {
            assert!(json.get(key).is_some(), "missing metadata key {key}");
        }
        assert_eq!(json["kind"], "image");
    }

    #[test]
    fn stub_clipboard_empty_is_validation_error() {
        let backend = StubBackend::default();
        let error = backend
            .import_clipboard(Some("x.png"), &[])
            .expect_err("empty clipboard must fail");
        assert_eq!(error.code, "validation");
    }

    #[test]
    fn compose_request_serializes_references_in_order() {
        let request = ComposeRequest {
            preset_id: "krea2".into(),
            brief: "a cat".into(),
            output_language: None,
            references: vec![
                ComposeReference {
                    asset_id: Some("a".into()),
                    weight: 1.0,
                    name: None,
                },
                ComposeReference {
                    asset_id: Some("b".into()),
                    weight: 0.5,
                    name: Some("second".into()),
                },
            ],
        };
        let json = serde_json::to_value(&request).expect("serialize");
        assert_eq!(json["references"][0]["assetId"], "a");
        assert_eq!(json["references"][1]["weight"], 0.5);
    }
}
