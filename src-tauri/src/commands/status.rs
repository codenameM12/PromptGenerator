use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::paths::{AppPaths, directory_size};
use crate::settings::SettingsStore;

use super::CommandError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSnapshot {
    pub endpoint: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub loaded_models: Vec<String>,
    pub active_jobs: i64,
    pub cache_bytes: u64,
    pub data_bytes: u64,
    pub portable: bool,
    pub storage_note: Option<String>,
    pub checked_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub loaded_models: Vec<String>,
}

/// Probes the configured LM Studio endpoint with a short timeout so the UI can
/// switch to an error state quickly.
pub async fn probe_endpoint(endpoint: &str, token: Option<&str>) -> ProbeResult {
    let started = Instant::now();
    let base = endpoint.trim_end_matches('/');

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            return ProbeResult {
                reachable: false,
                latency_ms: None,
                loaded_models: Vec::new(),
            };
        }
    };

    if let Some(models) = fetch_models(&client, &format!("{base}/api/v1/models"), token).await {
        return ProbeResult {
            reachable: true,
            latency_ms: Some(started.elapsed().as_millis() as u64),
            loaded_models: extract_loaded(&models),
        };
    }

    if fetch_models(&client, &format!("{base}/v1/models"), token)
        .await
        .is_some()
    {
        return ProbeResult {
            reachable: true,
            latency_ms: Some(started.elapsed().as_millis() as u64),
            loaded_models: Vec::new(),
        };
    }

    ProbeResult {
        reachable: false,
        latency_ms: None,
        loaded_models: Vec::new(),
    }
}

async fn fetch_models(client: &reqwest::Client, url: &str, token: Option<&str>) -> Option<Value> {
    let mut request = client.get(url);
    if let Some(token) = token
        && !token.is_empty()
    {
        request = request.bearer_auth(token);
    }

    let response = request.send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<Value>().await.ok()
}

fn extract_loaded(body: &Value) -> Vec<String> {
    body.get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("state").and_then(Value::as_str) == Some("loaded")
                        || item.get("loaded").and_then(Value::as_bool) == Some(true)
                })
                .filter_map(|item| {
                    item.get("id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tauri::command]
pub async fn status_snapshot(
    settings: State<'_, Mutex<SettingsStore>>,
    paths: State<'_, AppPaths>,
) -> Result<StatusSnapshot, CommandError> {
    let (endpoint, token) = {
        let store = settings
            .lock()
            .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
        (store.get().endpoint, store.token().map(ToOwned::to_owned))
    };

    let probe = probe_endpoint(&endpoint, token.as_deref()).await;

    let data_dir = paths.data_dir.clone();
    let cache_dir = paths.cache_dir.clone();
    let (data_bytes, cache_bytes) = tauri::async_runtime::spawn_blocking(move || {
        (directory_size(&data_dir), directory_size(&cache_dir))
    })
    .await
    .map_err(|error| CommandError::internal(format!("storage scan failed: {error}")))?;

    Ok(StatusSnapshot {
        endpoint,
        reachable: probe.reachable,
        latency_ms: probe.latency_ms,
        loaded_models: probe.loaded_models,
        active_jobs: 0,
        cache_bytes,
        data_bytes,
        portable: paths.portable,
        storage_note: paths.fallback_reason.clone(),
        checked_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::probe_endpoint;
    use crate::paths::directory_size;
    use std::fs;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn probe_reports_loaded_models() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": "lfm3-vl-2b", "state": "loaded" },
                    { "id": "gemma4-12b", "state": "not-loaded" }
                ]
            })))
            .mount(&server)
            .await;

        let probe = probe_endpoint(&server.uri(), None).await;
        assert!(probe.reachable);
        assert_eq!(probe.loaded_models, vec!["lfm3-vl-2b"]);
        assert!(probe.latency_ms.is_some());
    }

    #[tokio::test]
    async fn probe_falls_back_to_openai_models_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": "some-model" }]
            })))
            .mount(&server)
            .await;

        let probe = probe_endpoint(&server.uri(), None).await;
        assert!(probe.reachable);
    }

    #[tokio::test]
    async fn probe_reports_unreachable_after_server_stops() {
        let server = MockServer::start().await;
        let uri = server.uri();
        drop(server);

        let probe = probe_endpoint(&uri, None).await;
        assert!(!probe.reachable);
    }

    #[test]
    fn directory_size_sums_files() {
        let unique = format!(
            "prompt-studio-size-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(root.join("nested")).expect("create dirs");
        fs::write(root.join("a.bin"), [0_u8; 10]).expect("write a");
        fs::write(root.join("nested").join("b.bin"), [0_u8; 20]).expect("write b");

        assert_eq!(directory_size(&root), 30);
        let _ = fs::remove_dir_all(&root);
    }
}
