use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::runtime::client::LmStudioClient;
use crate::runtime::manager::{ModelManager, RoleConfig};
use crate::runtime::roles::closest_models;
use crate::settings::SettingsStore;

use super::CommandError;
use super::settings::SettingsPatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelRole {
    Describer,
    Composer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeepWarm {
    Both,
    Describer,
    Composer,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub kind: String,
    pub loaded: bool,
    pub context_length: Option<u32>,
    pub path: Option<String>,
    #[serde(default)]
    pub vision: Option<bool>,
    #[serde(default)]
    pub reasoning: Option<bool>,
    #[serde(default)]
    pub max_output: Option<u32>,
    #[serde(default)]
    pub quantization: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadModelRequest {
    pub role: ModelRole,
    pub model: String,
    pub context_length: Option<u32>,
    pub ttl_seconds: Option<u64>,
    pub gpu_offload: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadModelResult {
    pub model: String,
    pub context_length: Option<u32>,
    pub load_time_ms: u64,
    pub effective_gpu_offload: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub reachable: bool,
    pub models: Vec<ModelInfo>,
    pub loaded: Vec<String>,
    pub describer_model: Option<String>,
    pub composer_model: Option<String>,
    pub keep_warm: KeepWarm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRolesRequest {
    pub describer: String,
    pub composer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPolicyRequest {
    pub keep_warm: KeepWarm,
    pub describer_concurrency: u8,
    pub ttl_seconds: u64,
}

fn endpoint_and_token(
    settings: &State<'_, Mutex<SettingsStore>>,
) -> Result<(String, Option<String>), CommandError> {
    let store = settings
        .lock()
        .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
    Ok((store.get().endpoint, store.token().map(ToOwned::to_owned)))
}

#[tauri::command]
pub async fn models_list(
    settings: State<'_, Mutex<SettingsStore>>,
) -> Result<Vec<ModelInfo>, CommandError> {
    let (endpoint, token) = endpoint_and_token(&settings)?;
    LmStudioClient::new(endpoint, token)?.list_models().await
}

#[tauri::command]
pub async fn models_load(
    request: LoadModelRequest,
    settings: State<'_, Mutex<SettingsStore>>,
    manager: State<'_, Arc<ModelManager>>,
) -> Result<LoadModelResult, CommandError> {
    let (endpoint, token) = endpoint_and_token(&settings)?;
    let client = LmStudioClient::new(endpoint, token)?;

    if let Ok(models) = client.list_models().await
        && !models.iter().any(|model| model.id == request.model)
    {
        let ids: Vec<String> = models.iter().map(|model| model.id.clone()).collect();
        let suggestions = closest_models(&request.model, &ids, 3);
        return Err(CommandError::model_missing(format!(
            "Model '{}' is not available in LM Studio",
            request.model
        ))
        .with_details(if suggestions.is_empty() {
            "No models were reported by the endpoint".to_string()
        } else {
            format!("Closest matches: {}", suggestions.join(", "))
        }));
    }

    manager.load(request.role, &request).await
}

#[tauri::command]
pub async fn models_unload(
    model: String,
    manager: State<'_, Arc<ModelManager>>,
) -> Result<(), CommandError> {
    manager.unload(&model).await
}

#[tauri::command]
pub async fn models_unload_all(
    force: bool,
    manager: State<'_, Arc<ModelManager>>,
) -> Result<(), CommandError> {
    manager.unload_all(force).await
}

#[tauri::command]
pub async fn models_status(
    manager: State<'_, Arc<ModelManager>>,
) -> Result<RuntimeStatus, CommandError> {
    Ok(manager.status().await)
}

#[tauri::command]
pub async fn models_set_roles(
    request: SetRolesRequest,
    settings: State<'_, Mutex<SettingsStore>>,
    manager: State<'_, Arc<ModelManager>>,
) -> Result<RuntimeStatus, CommandError> {
    {
        let mut store = settings
            .lock()
            .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
        store.update(SettingsPatch {
            describer_model: Some(request.describer.clone()),
            composer_model: Some(request.composer.clone()),
            ..Default::default()
        })?;
    }

    manager.set_roles(request.describer, request.composer);
    Ok(manager.status().await)
}

#[tauri::command]
pub async fn models_set_policy(
    request: SetPolicyRequest,
    settings: State<'_, Mutex<SettingsStore>>,
    manager: State<'_, Arc<ModelManager>>,
) -> Result<RuntimeStatus, CommandError> {
    {
        let mut store = settings
            .lock()
            .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
        store.update(SettingsPatch {
            keep_warm: Some(request.keep_warm),
            describer_concurrency: Some(request.describer_concurrency),
            ttl_seconds: Some(request.ttl_seconds),
            ..Default::default()
        })?;
    }

    manager.set_keep_warm(request.keep_warm);
    manager.set_ttl(std::time::Duration::from_secs(request.ttl_seconds));
    manager.set_describer_concurrency(request.describer_concurrency);
    Ok(manager.status().await)
}

pub fn role_config(settings: &super::settings::Settings) -> RoleConfig {
    RoleConfig {
        describer: settings.describer_model.clone(),
        composer: settings.composer_model.clone(),
    }
}

pub fn manager_config(
    settings: &super::settings::Settings,
) -> crate::runtime::manager::ManagerConfig {
    crate::runtime::manager::ManagerConfig {
        roles: role_config(settings),
        keep_warm: settings.keep_warm,
        ttl: std::time::Duration::from_secs(settings.ttl_seconds),
        describer_concurrency: settings.describer_concurrency,
    }
}
