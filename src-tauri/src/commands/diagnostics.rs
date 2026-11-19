use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::diagnostics;
use crate::paths::AppPaths;
use crate::settings::SettingsStore;

use super::CommandError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub data_dir: String,
    pub cache_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsRequest {
    pub output_path: String,
    pub include_prompt_text: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSummary {
    pub output_path: String,
    pub bytes: u64,
}

fn build_app_info(app: &AppHandle, paths: &AppPaths) -> AppInfo {
    let package = app.package_info();
    AppInfo {
        name: package.name.clone(),
        version: package.version.to_string(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        data_dir: paths.data_dir.display().to_string(),
        cache_dir: paths.cache_dir.display().to_string(),
    }
}

#[tauri::command]
pub fn app_info(app: AppHandle, paths: State<'_, AppPaths>) -> AppInfo {
    build_app_info(&app, &paths)
}

#[tauri::command]
pub async fn diagnostics_export(
    request: DiagnosticsRequest,
    app: AppHandle,
    settings: State<'_, Mutex<SettingsStore>>,
    paths: State<'_, AppPaths>,
) -> Result<DiagnosticsSummary, CommandError> {
    let (settings_value, endpoint, token) = {
        let store = settings
            .lock()
            .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
        let snapshot = store.get();
        let value = serde_json::to_value(&snapshot).map_err(|error| {
            CommandError::internal(format!("serialize settings snapshot: {error}"))
        })?;
        (
            value,
            snapshot.endpoint,
            store.token().map(ToOwned::to_owned),
        )
    };

    let app_info = build_app_info(&app, &paths);
    let bundle = diagnostics::collect(
        &app_info,
        settings_value,
        &endpoint,
        token.as_deref(),
        &paths,
        request.include_prompt_text,
    )
    .await;

    let output = std::path::PathBuf::from(&request.output_path);
    let bytes = diagnostics::export(&bundle, &output)
        .map_err(|error| CommandError::io(format!("export diagnostics: {error}")))?;

    Ok(DiagnosticsSummary {
        output_path: request.output_path,
        bytes,
    })
}
