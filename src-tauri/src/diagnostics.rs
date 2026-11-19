use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::commands::diagnostics::AppInfo;
use crate::commands::status::probe_endpoint;
use crate::paths::{AppPaths, directory_size};
use crate::runtime::client::LmStudioClient;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPolicy {
    pub include_prompt_text: bool,
    pub prompts_included: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapability {
    pub id: String,
    pub vision: Option<bool>,
    pub reasoning: Option<bool>,
    pub context_length: Option<u32>,
    pub max_output: Option<u32>,
    pub quantization: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSummary {
    pub endpoint: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub loaded_models: Vec<String>,
    pub models: Vec<ModelCapability>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSummary {
    pub data_bytes: u64,
    pub cache_bytes: u64,
    pub portable: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsBundle {
    pub schema: u32,
    pub generated_at_ms: u64,
    pub app: AppInfo,
    pub settings: Value,
    pub token_present: bool,
    pub runtime: RuntimeSummary,
    pub storage: StorageSummary,
    pub logs: Vec<String>,
    pub content: ContentPolicy,
}

pub async fn collect(
    app: &AppInfo,
    settings: Value,
    endpoint: &str,
    token: Option<&str>,
    paths: &AppPaths,
    include_prompt_text: bool,
) -> DiagnosticsBundle {
    let probe = probe_endpoint(endpoint, token).await;
    let models = match LmStudioClient::new(endpoint, token.map(str::to_string)) {
        Ok(client) => client
            .list_models()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|model| ModelCapability {
                id: model.id,
                vision: model.vision,
                reasoning: model.reasoning,
                context_length: model.context_length,
                max_output: model.max_output,
                quantization: model.quantization,
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    let data_bytes = directory_size(&paths.data_dir);
    let cache_bytes = directory_size(&paths.cache_dir);
    let logs = read_log_tail(
        &paths.data_dir.join("logs"),
        200,
        token,
        include_prompt_text,
    );

    DiagnosticsBundle {
        schema: 1,
        generated_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default(),
        app: app.clone(),
        settings,
        token_present: token.is_some(),
        runtime: RuntimeSummary {
            endpoint: endpoint.to_string(),
            reachable: probe.reachable,
            latency_ms: probe.latency_ms,
            loaded_models: probe.loaded_models,
            models,
        },
        storage: StorageSummary {
            data_bytes,
            cache_bytes,
            portable: paths.portable,
            note: paths.fallback_reason.clone(),
        },
        logs,
        content: ContentPolicy {
            include_prompt_text,
            prompts_included: 0,
        },
    }
}

fn read_log_tail(
    log_dir: &Path,
    limit: usize,
    token: Option<&str>,
    include_prompt_text: bool,
) -> Vec<String> {
    let path = log_dir.join("prompt-studio.log");
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };

    let secrets: Vec<&str> = token
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect();
    let mut lines: Vec<String> = text
        .lines()
        .rev()
        .take(limit)
        .map(|line| {
            let line = if include_prompt_text {
                line.to_string()
            } else {
                redact_prompt_fields(line)
            };
            redact_secrets(&line, &secrets)
        })
        .collect();
    lines.reverse();
    lines
}

/// Logs must never carry prompt or description bodies unless explicitly allowed.
fn redact_prompt_fields(line: &str) -> String {
    let mut redacted = line.to_string();
    for key in ["prompt=", "description=", "caption="] {
        if let Some(index) = redacted.to_lowercase().find(key) {
            let value_start = index + key.len();
            redacted.replace_range(value_start.., "[REDACTED]");
        }
    }
    redacted
}

pub fn redact_secrets(text: &str, secrets: &[&str]) -> String {
    let mut redacted = text.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            redacted = redacted.replace(secret, "[REDACTED]");
        }
    }
    redacted
}

/// Writes a zip archive containing `diagnostics.json` and a short README.
pub fn export(bundle: &DiagnosticsBundle, output: &Path) -> Result<u64, String> {
    if !output.is_absolute() {
        return Err("Diagnostics output path must be absolute".into());
    }
    let parent = output
        .parent()
        .ok_or_else(|| "Diagnostics output path has no parent".to_string())?;
    if !parent.is_dir() {
        return Err(format!(
            "Output folder does not exist: {}",
            parent.display()
        ));
    }

    let file = std::fs::File::create(output)
        .map_err(|error| format!("create diagnostics archive: {error}"))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let json = serde_json::to_string_pretty(bundle)
        .map_err(|error| format!("serialize diagnostics: {error}"))?;
    writer
        .start_file("diagnostics.json", options)
        .map_err(|error| format!("start diagnostics.json: {error}"))?;
    writer
        .write_all(json.as_bytes())
        .map_err(|error| format!("write diagnostics.json: {error}"))?;

    let readme = concat!(
        "Prompt Studio diagnostics\n\n",
        "See diagnostics.json for app, runtime, storage, and log data.\n",
        "API tokens are never included. Prompt and description text is omitted\n",
        "unless the user explicitly enabled it.\n"
    );
    writer
        .start_file("README.txt", options)
        .map_err(|error| format!("start README.txt: {error}"))?;
    writer
        .write_all(readme.as_bytes())
        .map_err(|error| format!("write README.txt: {error}"))?;

    writer
        .finish()
        .map_err(|error| format!("finish diagnostics archive: {error}"))?;

    std::fs::metadata(output)
        .map(|metadata| metadata.len())
        .map_err(|error| format!("stat diagnostics archive: {error}"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;
    use std::path::PathBuf;

    use super::{collect, export, redact_secrets};
    use crate::commands::diagnostics::AppInfo;
    use crate::paths::{AppPaths, resolve_app_paths};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "prompt-studio-diag-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn paths(temp: &TempDir) -> AppPaths {
        resolve_app_paths(
            &temp.path,
            &temp.path.join("data"),
            &temp.path.join("cache"),
            false,
        )
        .expect("resolve paths")
    }

    fn app_info(temp: &TempDir) -> AppInfo {
        AppInfo {
            name: "Prompt Studio".into(),
            version: "0.1.0".into(),
            os: "windows".into(),
            arch: "x86_64".into(),
            data_dir: temp.path.join("data").display().to_string(),
            cache_dir: temp.path.join("cache").display().to_string(),
        }
    }

    #[test]
    fn redacts_known_secrets() {
        let redacted = redact_secrets("token=abc123 and more", &["abc123"]);
        assert_eq!(redacted, "token=[REDACTED] and more");
        assert!(!redacted.contains("abc123"));
    }

    #[tokio::test]
    async fn bundle_includes_model_capabilities_without_prompts() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {
                        "id": "vision-model",
                        "state": "loaded",
                        "max_context_length": 32768,
                        "input_modalities": ["text", "image"],
                        "quantization": { "name": "Q4_K_M" }
                    },
                    {
                        "id": "text-model",
                        "state": "not-loaded",
                        "input_modalities": ["text"]
                    }
                ]
            })))
            .mount(&server)
            .await;

        let temp = TempDir::new("capabilities");
        let bundle = collect(
            &app_info(&temp),
            serde_json::json!({ "endpoint": server.uri() }),
            &server.uri(),
            None,
            &paths(&temp),
            false,
        )
        .await;

        assert_eq!(bundle.runtime.models.len(), 2);
        assert_eq!(bundle.runtime.models[0].vision, Some(true));
        assert_eq!(bundle.runtime.models[0].context_length, Some(32768));
        assert_eq!(
            bundle.runtime.models[0].quantization.as_deref(),
            Some("Q4_K_M")
        );
        assert_eq!(bundle.runtime.models[1].vision, Some(false));

        let json = serde_json::to_string(&bundle).expect("serialize");
        assert!(!json.contains("SECRET PROMPT"));
    }

    #[tokio::test]
    async fn export_omits_token_and_prompt_text() {
        let temp = TempDir::new("redaction");
        let paths = paths(&temp);
        let token = "SUPER_SECRET_TOKEN";
        let logs = paths.data_dir.join("logs");
        fs::create_dir_all(&logs).expect("create logs");
        fs::write(
            logs.join("prompt-studio.log"),
            "started\nrequest token=SUPER_SECRET_TOKEN prompt=SECRET PROMPT TEXT\n",
        )
        .expect("write log");

        let bundle = collect(
            &app_info(&temp),
            serde_json::json!({ "endpoint": "http://127.0.0.1:1", "hasToken": true }),
            "http://127.0.0.1:1",
            Some(token),
            &paths,
            false,
        )
        .await;

        let output = temp.path.join("diagnostics.zip");
        let bytes = export(&bundle, &output).expect("export archive");
        assert!(bytes > 0);

        let file = fs::File::open(&output).expect("open archive");
        let mut archive = zip::ZipArchive::new(file).expect("read zip");
        let mut json = String::new();
        archive
            .by_name("diagnostics.json")
            .expect("diagnostics entry")
            .read_to_string(&mut json)
            .expect("read json");

        assert!(!json.contains(token));
        assert!(!json.contains("SECRET PROMPT TEXT"));
        assert!(json.contains("\"includePromptText\": false"));
        assert!(json.contains("\"promptsIncluded\": 0"));
        assert!(json.contains("[REDACTED]"));
    }

    #[tokio::test]
    async fn export_contains_readme_and_json_entries() {
        let temp = TempDir::new("entries");
        let paths = paths(&temp);
        let bundle = collect(
            &app_info(&temp),
            serde_json::json!({}),
            "http://127.0.0.1:1",
            None,
            &paths,
            false,
        )
        .await;

        let output = temp.path.join("diagnostics.zip");
        export(&bundle, &output).expect("export");

        let mut archive =
            zip::ZipArchive::new(fs::File::open(&output).expect("open")).expect("zip");
        assert!(archive.by_name("diagnostics.json").is_ok());
        assert!(archive.by_name("README.txt").is_ok());
    }
}
