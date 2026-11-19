use std::sync::Mutex;

use tauri::State;

use crate::settings::SettingsStore;

use super::CommandError;
use super::models::KeepWarm;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub endpoint: String,
    pub has_token: bool,
    pub describer_model: String,
    pub composer_model: String,
    pub describer_system_prompt: Option<String>,
    pub composer_system_prompt: Option<String>,
    pub describer_concurrency: u8,
    pub ttl_seconds: u64,
    pub keep_warm: KeepWarm,
    pub theme: String,
    pub naming_template: String,
    pub output_root: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub endpoint: Option<String>,
    pub token: Option<String>,
    pub describer_model: Option<String>,
    pub composer_model: Option<String>,
    pub describer_system_prompt: Option<String>,
    pub composer_system_prompt: Option<String>,
    pub describer_concurrency: Option<u8>,
    pub ttl_seconds: Option<u64>,
    pub keep_warm: Option<KeepWarm>,
    pub theme: Option<String>,
    pub naming_template: Option<String>,
    pub output_root: Option<String>,
}

#[tauri::command]
pub async fn settings_get(
    state: State<'_, Mutex<SettingsStore>>,
) -> Result<Settings, CommandError> {
    let store = state
        .lock()
        .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
    Ok(store.get())
}

#[tauri::command]
pub async fn settings_update(
    patch: SettingsPatch,
    state: State<'_, Mutex<SettingsStore>>,
) -> Result<Settings, CommandError> {
    let mut store = state
        .lock()
        .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
    store.update(patch)
}
