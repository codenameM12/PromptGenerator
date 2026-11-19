use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::commands::CommandError;
use crate::commands::models::{LoadModelRequest, LoadModelResult, ModelInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Vec<ContentPart>,
}

impl ChatMessage {
    pub fn text(role: &str, text: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: vec![ContentPart::Text(text.into())],
        }
    }

    pub fn with_parts(role: &str, parts: Vec<ContentPart>) -> Self {
        Self {
            role: role.into(),
            content: parts,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentPart {
    Text(String),
    ImageUrl(String),
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub json_schema: Option<Value>,
    pub ttl_seconds: Option<u64>,
    pub seed: Option<i64>,
    pub cancel: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub usage: Option<Value>,
}

/// HTTP client for a local LM Studio server.
pub struct LmStudioClient {
    http: reqwest::Client,
    endpoint: String,
    token: Option<String>,
}

impl LmStudioClient {
    pub fn new(endpoint: impl Into<String>, token: Option<String>) -> Result<Self, CommandError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|error| CommandError::runtime(format!("build HTTP client: {error}")))?;
        Ok(Self {
            http,
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            token,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.endpoint, path)
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut request = self.http.request(method, self.url(path));
        if let Some(token) = &self.token
            && !token.is_empty()
        {
            request = request.bearer_auth(token);
        }
        request
    }

    pub async fn list_models(&self) -> Result<Vec<ModelInfo>, CommandError> {
        let response = self
            .request(reqwest::Method::GET, "/api/v1/models")
            .send()
            .await
            .map_err(map_reqwest)?;

        if response.status().is_success() {
            let body: Value = response.json().await.map_err(map_reqwest)?;
            return Ok(parse_models(&body));
        }

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            let fallback = self
                .request(reqwest::Method::GET, "/v1/models")
                .send()
                .await
                .map_err(map_reqwest)?;
            if fallback.status().is_success() {
                let body: Value = fallback.json().await.map_err(map_reqwest)?;
                return Ok(parse_models(&body));
            }
            return Err(map_status(
                fallback.status(),
                &fallback.text().await.unwrap_or_default(),
            ));
        }

        Err(map_status(
            response.status(),
            &response.text().await.unwrap_or_default(),
        ))
    }

    pub async fn load_model(
        &self,
        request: &LoadModelRequest,
    ) -> Result<LoadModelResult, CommandError> {
        let started = Instant::now();
        let body = json!({
            "model": request.model,
            "context_length": request.context_length,
            "flash_attention": true,
            "offload_kv_cache_to_gpu": true,
        });

        let response = self
            .request(reqwest::Method::POST, "/api/v1/models/load")
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest)?;

        if !response.status().is_success() {
            return Err(map_status(
                response.status(),
                &response.text().await.unwrap_or_default(),
            ));
        }

        let value: Value = response.json().await.map_err(map_reqwest)?;
        Ok(LoadModelResult {
            model: request.model.clone(),
            context_length: value
                .get("load_config")
                .and_then(|config| config.get("context_length"))
                .and_then(Value::as_u64)
                .map(|value| value as u32)
                .or(request.context_length),
            load_time_ms: started.elapsed().as_millis() as u64,
            effective_gpu_offload: request.gpu_offload,
        })
    }

    pub async fn unload_model(&self, model: &str) -> Result<(), CommandError> {
        let instance_ids = self
            .loaded_instance_ids(model)
            .await
            .map(|ids| {
                if ids.is_empty() {
                    vec![model.to_string()]
                } else {
                    ids
                }
            })
            .unwrap_or_else(|_| vec![model.to_string()]);

        for instance_id in instance_ids {
            let response = self
                .request(reqwest::Method::POST, "/api/v1/models/unload")
                .json(&json!({ "instance_id": instance_id }))
                .send()
                .await
                .map_err(map_reqwest)?;

            if !response.status().is_success() {
                return Err(map_status(
                    response.status(),
                    &response.text().await.unwrap_or_default(),
                ));
            }
        }
        Ok(())
    }

    /// LM Studio's unload endpoint takes an instance id, and a key may have
    /// several loaded instances. Return every instance so unload removes the
    /// duplicates the app or the user created.
    async fn loaded_instance_ids(&self, model: &str) -> Result<Vec<String>, CommandError> {
        let response = self
            .request(reqwest::Method::GET, "/api/v1/models")
            .send()
            .await
            .map_err(map_reqwest)?;
        if !response.status().is_success() {
            return Err(map_status(
                response.status(),
                &response.text().await.unwrap_or_default(),
            ));
        }
        let body: Value = response.json().await.map_err(map_reqwest)?;
        Ok(find_instance_ids(&body, model))
    }

    pub async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, CommandError> {
        let body = chat_body(request, false);
        let response = self
            .request(reqwest::Method::POST, "/v1/chat/completions")
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest)?;

        if !response.status().is_success() {
            return Err(map_status(
                response.status(),
                &response.text().await.unwrap_or_default(),
            ));
        }

        let value: Value = response.json().await.map_err(map_reqwest)?;
        let content = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| CommandError::invalid_response("Chat response has no message content"))?
            .to_string();

        Ok(ChatResponse {
            content,
            usage: value.get("usage").cloned(),
        })
    }

    pub async fn stream_chat<'a>(
        &'a self,
        request: &'a ChatRequest,
        on_token: &'a mut (dyn for<'b> FnMut(&'b str) + Send),
    ) -> Result<ChatResponse, CommandError> {
        let body = chat_body(request, true);
        let response = self
            .request(reqwest::Method::POST, "/v1/chat/completions")
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest)?;

        if !response.status().is_success() {
            return Err(map_status(
                response.status(),
                &response.text().await.unwrap_or_default(),
            ));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut content = String::new();

        while let Some(chunk) = stream.next().await {
            if request
                .cancel
                .as_ref()
                .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
            {
                return Err(CommandError::cancelled());
            }
            let chunk = chunk.map_err(map_reqwest)?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(index) = buffer.find('\n') {
                let line = buffer[..index].trim().to_string();
                buffer.drain(..=index);

                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data == "[DONE]" {
                        return Ok(ChatResponse {
                            content,
                            usage: None,
                        });
                    }
                    if let Ok(value) = serde_json::from_str::<Value>(data)
                        && let Some(token) = value
                            .get("choices")
                            .and_then(Value::as_array)
                            .and_then(|choices| choices.first())
                            .and_then(|choice| choice.get("delta"))
                            .and_then(|delta| delta.get("content"))
                            .and_then(Value::as_str)
                    {
                        content.push_str(token);
                        on_token(token);
                    }
                }
            }
        }

        Ok(ChatResponse {
            content,
            usage: None,
        })
    }
}

fn chat_body(request: &ChatRequest, stream: bool) -> Value {
    let messages: Vec<Value> = request.messages.iter().map(message_body).collect();
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "temperature": request.temperature,
        "max_tokens": request.max_tokens,
        "stream": stream,
        // Reasoning-capable models default to thinking in LM Studio, which can
        // consume the whole output budget before any JSON content arrives.
        "reasoning_effort": "none",
    });
    if let Some(seed) = request.seed {
        body["seed"] = json!(seed);
    }
    if let Some(schema) = &request.json_schema {
        body["response_format"] = json!({
            "type": "json_schema",
            "json_schema": {
                "name": "prompt_response",
                "schema": schema,
                "strict": true
            }
        });
    }
    body
}

fn message_body(message: &ChatMessage) -> Value {
    let parts: Vec<Value> = message
        .content
        .iter()
        .map(|part| match part {
            ContentPart::Text(text) => json!({ "type": "text", "text": text }),
            ContentPart::ImageUrl(url) => {
                json!({ "type": "image_url", "image_url": { "url": url } })
            }
        })
        .collect();

    if parts.len() == 1
        && let Some(text) = message.content.first()
        && let ContentPart::Text(value) = text
    {
        return json!({ "role": message.role, "content": value });
    }

    json!({ "role": message.role, "content": parts })
}

fn parse_models(body: &Value) -> Vec<ModelInfo> {
    if let Some(items) = body.get("models").and_then(Value::as_array) {
        return items
            .iter()
            .filter_map(|item| {
                let id = item.get("key").and_then(Value::as_str)?;
                let instances = item
                    .get("loaded_instances")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let loaded = !instances.is_empty();
                let context_length = item
                    .get("max_context_length")
                    .or_else(|| item.get("context_length"))
                    .and_then(Value::as_u64)
                    .map(|value| value as u32)
                    .or_else(|| {
                        instances
                            .first()
                            .and_then(|instance| instance.get("config"))
                            .and_then(|config| config.get("context_length"))
                            .and_then(Value::as_u64)
                            .map(|value| value as u32)
                    });
                let capabilities = parse_capabilities(item);
                Some(ModelInfo {
                    id: id.to_string(),
                    kind: item
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("llm")
                        .to_string(),
                    loaded,
                    context_length,
                    path: None,
                    vision: capabilities.vision,
                    reasoning: capabilities.reasoning,
                    max_output: capabilities.max_output,
                    quantization: capabilities.quantization,
                })
            })
            .collect();
    }

    body.get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(Value::as_str)?;
                    let kind = item
                        .get("type")
                        .or_else(|| item.get("kind"))
                        .and_then(Value::as_str)
                        .unwrap_or("llm")
                        .to_string();
                    let loaded = item.get("state").and_then(Value::as_str) == Some("loaded")
                        || item.get("loaded").and_then(Value::as_bool) == Some(true);
                    let context_length = item
                        .get("max_context_length")
                        .or_else(|| item.get("context_length"))
                        .and_then(Value::as_u64)
                        .map(|value| value as u32);
                    let path = item
                        .get("path")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    let capabilities = parse_capabilities(item);

                    Some(ModelInfo {
                        id: id.to_string(),
                        kind,
                        loaded,
                        context_length,
                        path,
                        vision: capabilities.vision,
                        reasoning: capabilities.reasoning,
                        max_output: capabilities.max_output,
                        quantization: capabilities.quantization,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Default)]
struct ModelCapabilities {
    vision: Option<bool>,
    reasoning: Option<bool>,
    max_output: Option<u32>,
    quantization: Option<String>,
}

/// Reads capability fields only when the endpoint reports them. Names are never
/// used to infer a capability.
fn parse_capabilities(item: &Value) -> ModelCapabilities {
    let vision = item
        .get("vision")
        .and_then(Value::as_bool)
        .or_else(|| item.get("supports_vision").and_then(Value::as_bool));

    let reasoning = item
        .get("reasoning")
        .and_then(|value| {
            value.as_bool().or_else(|| {
                value
                    .get("default")
                    .and_then(Value::as_str)
                    .map(|state| !matches!(state, "off" | "none"))
            })
        })
        .or_else(|| item.get("supports_reasoning").and_then(Value::as_bool));

    let mut capabilities = ModelCapabilities {
        vision,
        reasoning,
        max_output: None,
        quantization: None,
    };

    if let Some(entries) = item.get("capabilities") {
        if let Some(list) = entries.as_array() {
            for entry in list {
                match entry.as_str() {
                    Some("vision") | Some("image") | Some("image_input") => {
                        capabilities.vision = Some(true);
                    }
                    Some("reasoning") => {
                        capabilities.reasoning = Some(true);
                    }
                    _ => {}
                }
            }
        }
        if let Some(vision) = entries.get("vision").and_then(Value::as_bool) {
            capabilities.vision = Some(vision);
        }
        if let Some(reasoning) = entries.get("reasoning").and_then(Value::as_bool) {
            capabilities.reasoning = Some(reasoning);
        }
    }

    if let Some(modalities) = item.get("input_modalities").and_then(Value::as_array) {
        capabilities.vision = Some(
            modalities
                .iter()
                .any(|value| value.as_str() == Some("image")),
        );
    }

    capabilities.max_output = item
        .get("max_output_tokens")
        .or_else(|| item.get("max_tokens"))
        .and_then(Value::as_u64)
        .map(|value| value as u32);

    capabilities.quantization = item
        .get("quantization")
        .and_then(|value| {
            value.as_str().map(ToOwned::to_owned).or_else(|| {
                value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
        })
        .or_else(|| {
            item.get("quant")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        });

    capabilities
}

fn find_instance_ids(body: &Value, model: &str) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(items) = body.get("models").and_then(Value::as_array) {
        for item in items {
            if item.get("key").and_then(Value::as_str) != Some(model) {
                continue;
            }
            if let Some(instances) = item.get("loaded_instances").and_then(Value::as_array) {
                for instance in instances {
                    if let Some(id) = instance.get("id").and_then(Value::as_str) {
                        let id = id.to_string();
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
            }
        }
    }
    if ids.is_empty()
        && let Some(items) = body.get("data").and_then(Value::as_array)
    {
        for item in items {
            let Some(id) = item.get("id").and_then(Value::as_str) else {
                continue;
            };
            if id == model
                && (item.get("state").and_then(Value::as_str) == Some("loaded")
                    || item.get("loaded").and_then(Value::as_bool) == Some(true))
            {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

pub fn map_reqwest(error: reqwest::Error) -> CommandError {
    if error.is_timeout() {
        CommandError::timeout(format!("LM Studio request timed out: {error}"))
    } else if error.is_connect() || error.is_request() {
        CommandError::network(format!("Cannot reach LM Studio: {error}"))
    } else {
        CommandError::runtime(format!("LM Studio request failed: {error}"))
    }
}

pub fn map_status(status: reqwest::StatusCode, body: &str) -> CommandError {
    let snippet: String = body.trim().chars().take(400).collect();
    let snippet = snippet.as_str();

    match status.as_u16() {
        401 => CommandError::unauthorized("LM Studio rejected the API token"),
        403 => CommandError::forbidden("LM Studio denied the request"),
        404 => CommandError::model_missing(format!("Model or endpoint not found: {snippet}")),
        400 | 413 | 422 => {
            let lower = snippet.to_ascii_lowercase();
            if lower.contains("context") || lower.contains("token") {
                CommandError::context_overflow(format!(
                    "The request exceeds the model context: {snippet}"
                ))
            } else {
                CommandError::runtime(format!("LM Studio rejected the request: {snippet}"))
            }
        }
        _ if status.is_server_error() => {
            CommandError::runtime(format!("LM Studio backend error: {snippet}"))
        }
        _ => CommandError::runtime(format!("LM Studio error {}: {snippet}", status)),
    }
}

#[cfg(test)]
mod tests {
    use super::{ChatMessage, ChatRequest, ContentPart, LmStudioClient};
    use crate::commands::models::{LoadModelRequest, ModelRole};
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{body_json, body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn lists_models_and_loaded_state() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": "lfm3-vl-2b", "type": "llm", "state": "loaded", "max_context_length": 8192 },
                    { "id": "gemma", "type": "llm", "state": "not-loaded" }
                ]
            })))
            .mount(&server)
            .await;

        let client = LmStudioClient::new(server.uri(), None).expect("client");
        let models = client.list_models().await.expect("list");
        assert_eq!(models.len(), 2);
        assert!(models[0].loaded);
        assert_eq!(models[0].context_length, Some(8192));
    }

    #[tokio::test]
    async fn loads_and_unloads_models() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/models/load"))
            .and(body_json(serde_json::json!({
                "model": "lfm3-vl-2b",
                "context_length": 8192,
                "flash_attention": true,
                "offload_kv_cache_to_gpu": true
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "instance_id": "lfm3-vl-2b",
                "status": "loaded",
                "load_config": { "context_length": 8192 }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/models/unload"))
            .and(body_json(
                serde_json::json!({ "instance_id": "lfm3-vl-2b" }),
            ))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = LmStudioClient::new(server.uri(), None).expect("client");
        let loaded = client
            .load_model(&LoadModelRequest {
                role: ModelRole::Describer,
                model: "lfm3-vl-2b".into(),
                context_length: Some(8192),
                ttl_seconds: Some(300),
                gpu_offload: Some(1.0),
            })
            .await
            .expect("load");
        assert_eq!(loaded.context_length, Some(8192));
        client.unload_model("lfm3-vl-2b").await.expect("unload");
    }

    #[tokio::test]
    async fn chats_with_an_image() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{ "message": { "content": "a red fox" } }],
                "usage": { "total_tokens": 12 }
            })))
            .mount(&server)
            .await;

        let client = LmStudioClient::new(server.uri(), None).expect("client");
        let response = client
            .chat(&ChatRequest {
                model: "lfm".into(),
                messages: vec![ChatMessage::with_parts(
                    "user",
                    vec![
                        ContentPart::Text("Describe".into()),
                        ContentPart::ImageUrl("data:image/png;base64,AAAA".into()),
                    ],
                )],
                temperature: 0.2,
                max_tokens: 128,
                json_schema: None,
                ttl_seconds: None,
                seed: None,
                cancel: None,
            })
            .await
            .expect("chat");
        assert_eq!(response.content, "a red fox");
        assert!(response.usage.is_some());
    }

    #[tokio::test]
    async fn disables_reasoning_for_structured_chat() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_partial_json(serde_json::json!({
                "reasoning_effort": "none"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{ "message": { "content": "{\"prompt\":\"a red fox\"}" } }]
            })))
            .mount(&server)
            .await;

        let client = LmStudioClient::new(server.uri(), None).expect("client");
        let response = client
            .chat(&ChatRequest {
                model: "gemma".into(),
                messages: vec![ChatMessage::text("user", "compose")],
                temperature: 0.2,
                max_tokens: 32,
                json_schema: None,
                ttl_seconds: None,
                seed: None,
                cancel: None,
            })
            .await
            .expect("chat");
        assert_eq!(response.content, "{\"prompt\":\"a red fox\"}");
    }

    #[tokio::test]
    async fn streams_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"choices\":[{\"delta\":{\"content\":\"a \"}}]}\n\n\
                 data: {\"choices\":[{\"delta\":{\"content\":\"fox\"}}]}\n\n\
                 data: [DONE]\n\n",
            ))
            .mount(&server)
            .await;

        let client = LmStudioClient::new(server.uri(), None).expect("client");
        let tokens = Arc::new(Mutex::new(String::new()));
        let collected = tokens.clone();
        let response = client
            .stream_chat(
                &ChatRequest {
                    model: "lfm".into(),
                    messages: vec![ChatMessage::text("user", "describe")],
                    temperature: 0.2,
                    max_tokens: 32,
                    json_schema: None,
                    ttl_seconds: None,
                    seed: None,
                    cancel: None,
                },
                &mut |token| collected.lock().expect("lock").push_str(token),
            )
            .await
            .expect("stream");

        assert_eq!(response.content, "a fox");
        assert_eq!(&*tokens.lock().expect("lock"), "a fox");
    }

    #[test]
    fn parses_reported_capabilities_only() {
        let body = serde_json::json!({
            "models": [
                {
                    "key": "vision-model",
                    "type": "vlm",
                    "max_context_length": 32768,
                    "max_output_tokens": 4096,
                    "capabilities": { "vision": true, "trained_for_tool_use": true },
                    "quantization": { "name": "Q4_K_M", "bits_per_weight": 4 },
                    "loaded_instances": []
                },
                {
                    "key": "array-model",
                    "type": "vlm",
                    "capabilities": ["vision", "reasoning"],
                    "loaded_instances": []
                },
                { "key": "text-model", "type": "llm", "loaded_instances": [] }
            ]
        });
        let models = super::parse_models(&body);
        assert_eq!(models[0].vision, Some(true));
        assert_eq!(models[0].max_output, Some(4096));
        assert_eq!(models[0].quantization.as_deref(), Some("Q4_K_M"));
        assert_eq!(models[1].vision, Some(true));
        assert_eq!(models[1].reasoning, Some(true));
        assert_eq!(models[2].vision, None);
        assert_eq!(models[2].reasoning, None);
        assert_eq!(models[2].max_output, None);
        assert_eq!(models[2].quantization, None);

        let data = serde_json::json!({
            "data": [
                {
                    "id": "text-only",
                    "input_modalities": ["text"],
                    "quantization": { "name": "Q8_0" }
                },
                {
                    "id": "image-model",
                    "input_modalities": ["text", "image"],
                    "reasoning": { "default": "on" }
                }
            ]
        });
        let models = super::parse_models(&data);
        assert_eq!(models[0].vision, Some(false));
        assert_eq!(models[0].quantization.as_deref(), Some("Q8_0"));
        assert_eq!(models[1].vision, Some(true));
        assert_eq!(models[1].reasoning, Some(true));
    }

    #[test]
    fn finds_every_loaded_instance_for_a_key() {
        let body = serde_json::json!({
            "models": [
                {
                    "key": "gemma",
                    "loaded_instances": [{ "id": "gemma:1" }, { "id": "gemma:2" }]
                },
                {
                    "key": "other",
                    "loaded_instances": [{ "id": "other:1" }]
                }
            ]
        });
        assert_eq!(
            super::find_instance_ids(&body, "gemma"),
            vec!["gemma:1".to_string(), "gemma:2".to_string()]
        );
    }

    #[tokio::test]
    async fn maps_missing_model_and_backend_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/models"))
            .respond_with(ResponseTemplate::new(404).set_body_string("model not found"))
            .mount(&server)
            .await;
        let client = LmStudioClient::new(server.uri(), None).expect("client");
        let error = client.list_models().await.expect_err("missing");
        assert_eq!(error.category, crate::commands::ErrorCategory::ModelMissing);

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/models"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let client = LmStudioClient::new(server.uri(), None).expect("client");
        let error = client.list_models().await.expect_err("backend");
        assert_eq!(error.category, crate::commands::ErrorCategory::Runtime);
        assert!(error.retryable);
    }

    #[tokio::test]
    async fn maps_context_overflow() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_string("tokens exceed the model context window"),
            )
            .mount(&server)
            .await;

        let client = LmStudioClient::new(server.uri(), None).expect("client");
        let error = client
            .chat(&ChatRequest {
                model: "lfm".into(),
                messages: vec![ChatMessage::text("user", "hi")],
                temperature: 0.2,
                max_tokens: 32,
                json_schema: None,
                ttl_seconds: None,
                seed: None,
                cancel: None,
            })
            .await
            .expect_err("context");
        assert_eq!(
            error.category,
            crate::commands::ErrorCategory::ContextOverflow
        );
    }
}
