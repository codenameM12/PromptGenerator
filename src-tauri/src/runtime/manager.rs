use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::commands::CommandError;
use crate::commands::models::{
    KeepWarm, LoadModelRequest, LoadModelResult, ModelInfo, ModelRole, RuntimeStatus,
};

use super::client::{ChatRequest, ChatResponse, LmStudioClient};
use super::lms_cli::LmsCli;

#[async_trait]
pub trait RuntimeBackend: Send + Sync {
    async fn list_models(&self) -> Result<Vec<ModelInfo>, CommandError>;
    async fn load_model(&self, request: &LoadModelRequest)
    -> Result<LoadModelResult, CommandError>;
    async fn unload_model(&self, model: &str) -> Result<(), CommandError>;

    async fn chat(&self, _request: &ChatRequest) -> Result<ChatResponse, CommandError> {
        Err(CommandError::not_implemented("runtime chat"))
    }

    async fn stream_chat<'a>(
        &'a self,
        _request: &'a ChatRequest,
        _on_token: &'a mut (dyn for<'b> FnMut(&'b str) + Send),
    ) -> Result<ChatResponse, CommandError> {
        Err(CommandError::not_implemented("runtime stream_chat"))
    }
}

#[async_trait]
impl RuntimeBackend for LmStudioClient {
    async fn list_models(&self) -> Result<Vec<ModelInfo>, CommandError> {
        LmStudioClient::list_models(self).await
    }

    async fn load_model(
        &self,
        request: &LoadModelRequest,
    ) -> Result<LoadModelResult, CommandError> {
        LmStudioClient::load_model(self, request).await
    }

    async fn unload_model(&self, model: &str) -> Result<(), CommandError> {
        LmStudioClient::unload_model(self, model).await
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, CommandError> {
        LmStudioClient::chat(self, request).await
    }

    async fn stream_chat<'a>(
        &'a self,
        request: &'a ChatRequest,
        on_token: &'a mut (dyn for<'b> FnMut(&'b str) + Send),
    ) -> Result<ChatResponse, CommandError> {
        LmStudioClient::stream_chat(self, request, on_token).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleConfig {
    pub describer: String,
    pub composer: String,
}

impl Default for RoleConfig {
    fn default() -> Self {
        Self {
            describer: "lfm3-vl-2b".into(),
            composer: "gemma4-12b-qat-uncensored-hauhaucs-balanced".into(),
        }
    }
}

impl RoleConfig {
    pub fn model(&self, role: ModelRole) -> &str {
        match role {
            ModelRole::Describer => &self.describer,
            ModelRole::Composer => &self.composer,
        }
    }

    fn set(&mut self, role: ModelRole, model: String) {
        match role {
            ModelRole::Describer => self.describer = model,
            ModelRole::Composer => self.composer = model,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManagerConfig {
    pub roles: RoleConfig,
    pub keep_warm: KeepWarm,
    pub ttl: Duration,
    pub describer_concurrency: u8,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            roles: RoleConfig::default(),
            keep_warm: KeepWarm::None,
            ttl: Duration::from_secs(600),
            describer_concurrency: 2,
        }
    }
}

struct State {
    loaded: HashSet<String>,
    busy: HashMap<String, u32>,
    last_used: HashMap<String, Instant>,
    roles: RoleConfig,
    keep_warm: KeepWarm,
    ttl: Duration,
}

pub struct ModelManager {
    backend: Arc<dyn RuntimeBackend>,
    lms: Option<LmsCli>,
    state: Mutex<State>,
    gate: tokio::sync::Mutex<()>,
    describer: Mutex<Arc<Semaphore>>,
    composer: Arc<Semaphore>,
}

impl ModelManager {
    pub fn new(
        backend: Arc<dyn RuntimeBackend>,
        lms: Option<LmsCli>,
        config: ManagerConfig,
    ) -> Arc<Self> {
        let concurrency = config.describer_concurrency.clamp(1, 4) as usize;
        Arc::new(Self {
            backend,
            lms,
            state: Mutex::new(State {
                loaded: HashSet::new(),
                busy: HashMap::new(),
                last_used: HashMap::new(),
                roles: config.roles,
                keep_warm: config.keep_warm,
                ttl: config.ttl,
            }),
            gate: tokio::sync::Mutex::new(()),
            describer: Mutex::new(Arc::new(Semaphore::new(concurrency))),
            composer: Arc::new(Semaphore::new(1)),
        })
    }

    pub fn roles(&self) -> RoleConfig {
        self.state
            .lock()
            .map(|state| state.roles.clone())
            .unwrap_or_default()
    }

    pub fn set_roles(&self, describer: String, composer: String) {
        if let Ok(mut state) = self.state.lock() {
            state.roles.describer = describer;
            state.roles.composer = composer;
        }
    }

    pub fn keep_warm(&self) -> KeepWarm {
        self.state
            .lock()
            .map(|state| state.keep_warm)
            .unwrap_or(KeepWarm::None)
    }

    pub fn set_keep_warm(&self, keep_warm: KeepWarm) {
        if let Ok(mut state) = self.state.lock() {
            state.keep_warm = keep_warm;
        }
    }

    pub fn set_ttl(&self, ttl: Duration) {
        if let Ok(mut state) = self.state.lock() {
            state.ttl = ttl;
        }
    }

    pub fn set_describer_concurrency(&self, concurrency: u8) {
        if let Ok(mut guard) = self.describer.lock() {
            *guard = Arc::new(Semaphore::new(concurrency.clamp(1, 4) as usize));
        }
    }

    pub async fn list_models(&self) -> Result<Vec<ModelInfo>, CommandError> {
        self.backend.list_models().await
    }

    pub async fn status(&self) -> RuntimeStatus {
        let (reachable, models) = match self.backend.list_models().await {
            Ok(models) => (true, models),
            Err(_) => (false, Vec::new()),
        };

        let (loaded, describer_model, composer_model, keep_warm) = match self.state.lock() {
            Ok(state) => {
                let mut loaded: Vec<String> = state.loaded.iter().cloned().collect();
                loaded.sort();
                (
                    loaded,
                    Some(state.roles.describer.clone()),
                    Some(state.roles.composer.clone()),
                    state.keep_warm,
                )
            }
            Err(_) => (Vec::new(), None, None, KeepWarm::None),
        };

        RuntimeStatus {
            reachable,
            models,
            loaded,
            describer_model,
            composer_model,
            keep_warm,
        }
    }

    pub async fn load(
        &self,
        role: ModelRole,
        request: &LoadModelRequest,
    ) -> Result<LoadModelResult, CommandError> {
        let _gate = self.gate.lock().await;

        if let Ok(mut state) = self.state.lock() {
            state.roles.set(role, request.model.clone());
        }

        let desired = self.desired_for(role)?;
        self.apply_desired(&desired).await?;

        let result = if request.gpu_offload.is_some() {
            if let Some(lms) = &self.lms {
                lms.load(
                    &request.model,
                    request.gpu_offload,
                    request.context_length,
                    request.ttl_seconds,
                )?;
                LoadModelResult {
                    model: request.model.clone(),
                    context_length: request.context_length,
                    load_time_ms: 0,
                    effective_gpu_offload: request.gpu_offload,
                }
            } else {
                self.backend.load_model(request).await?
            }
        } else {
            self.backend.load_model(request).await?
        };

        if let Ok(mut state) = self.state.lock() {
            state.loaded.insert(result.model.clone());
            state.last_used.insert(result.model.clone(), Instant::now());
        }

        Ok(result)
    }

    pub async fn chat(
        self: &Arc<Self>,
        role: ModelRole,
        mut request: ChatRequest,
        on_token: Option<&mut (dyn for<'b> FnMut(&'b str) + Send)>,
    ) -> Result<ChatResponse, CommandError> {
        let lease = self.acquire(role).await?;
        request.model = lease.model().to_string();

        let result = match on_token {
            Some(callback) => self.backend.stream_chat(&request, callback).await,
            None => self.backend.chat(&request).await,
        };

        drop(lease);
        result
    }

    pub async fn acquire(self: &Arc<Self>, role: ModelRole) -> Result<ModelLease, CommandError> {
        let semaphore = match role {
            ModelRole::Describer => self
                .describer
                .lock()
                .map_err(|_| CommandError::internal("Describer semaphore is poisoned"))?
                .clone(),
            ModelRole::Composer => self.composer.clone(),
        };

        let permit = semaphore
            .acquire_owned()
            .await
            .map_err(|_| CommandError::internal("Runtime concurrency semaphore closed"))?;

        let model = match self.ensure_loaded(role).await {
            Ok(model) => model,
            Err(error) => {
                drop(permit);
                return Err(error);
            }
        };

        if let Ok(mut state) = self.state.lock() {
            *state.busy.entry(model.clone()).or_insert(0) += 1;
            state.last_used.insert(model.clone(), Instant::now());
        }

        Ok(ModelLease {
            model,
            manager: Arc::clone(self),
            _permit: permit,
        })
    }

    pub async fn unload(&self, model: &str) -> Result<(), CommandError> {
        let _gate = self.gate.lock().await;
        self.unload_one(model, false).await
    }

    pub async fn unload_all(&self, force: bool) -> Result<(), CommandError> {
        let _gate = self.gate.lock().await;

        if !force
            && let Ok(state) = self.state.lock()
            && state.busy.values().any(|count| *count > 0)
        {
            return Err(CommandError::conflict(
                "A model is busy; wait for the active request or force unload",
            ));
        }

        let loaded: Vec<String> = self
            .state
            .lock()
            .map(|state| state.loaded.iter().cloned().collect())
            .unwrap_or_default();

        for model in loaded {
            self.unload_one(&model, true).await?;
        }

        if let Ok(mut state) = self.state.lock() {
            state.loaded.clear();
            state.busy.clear();
            state.last_used.clear();
        }

        Ok(())
    }

    /// Unloads models whose TTL expired and that are not in use.
    pub async fn unload_idle(&self, now: Instant) -> Result<Vec<String>, CommandError> {
        let _gate = self.gate.lock().await;

        let candidates = {
            let state = self
                .state
                .lock()
                .map_err(|_| CommandError::internal("Runtime state is poisoned"))?;
            let keep: HashSet<String> = match state.keep_warm {
                KeepWarm::Both => [state.roles.describer.clone(), state.roles.composer.clone()]
                    .into_iter()
                    .collect(),
                KeepWarm::Describer => [state.roles.describer.clone()].into_iter().collect(),
                KeepWarm::Composer => [state.roles.composer.clone()].into_iter().collect(),
                KeepWarm::None => HashSet::new(),
            };

            state
                .loaded
                .iter()
                .filter(|model| !keep.contains(*model))
                .filter(|model| state.busy.get(*model).copied().unwrap_or_default() == 0)
                .filter(|model| {
                    state
                        .last_used
                        .get(*model)
                        .map(|last| *last + state.ttl <= now)
                        .unwrap_or(true)
                })
                .cloned()
                .collect::<Vec<_>>()
        };

        for model in &candidates {
            self.unload_one(model, false).await?;
        }

        Ok(candidates)
    }

    fn desired_for(&self, role: ModelRole) -> Result<HashSet<String>, CommandError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CommandError::internal("Runtime state is poisoned"))?;

        let describer = state.roles.describer.clone();
        let composer = state.roles.composer.clone();

        let desired = match state.keep_warm {
            KeepWarm::Both => [describer, composer].into_iter().collect(),
            KeepWarm::Describer => {
                if role == ModelRole::Describer {
                    [describer].into_iter().collect()
                } else {
                    [describer, composer].into_iter().collect()
                }
            }
            KeepWarm::Composer => {
                if role == ModelRole::Composer {
                    [composer].into_iter().collect()
                } else {
                    [describer, composer].into_iter().collect()
                }
            }
            KeepWarm::None => [state.roles.model(role).to_string()].into_iter().collect(),
        };

        Ok(desired)
    }

    async fn ensure_loaded(&self, role: ModelRole) -> Result<String, CommandError> {
        let _gate = self.gate.lock().await;
        let model = self
            .state
            .lock()
            .map_err(|_| CommandError::internal("Runtime state is poisoned"))?
            .roles
            .model(role)
            .to_string();

        if model.is_empty() {
            return Err(CommandError::validation(format!(
                "No model is configured for the {role:?} role"
            )));
        }

        // The app may have restarted while LM Studio still had a model loaded,
        // or the user may have loaded it by hand. Mirror LM Studio's state
        // before deciding to load again so we do not create a second instance.
        // The reported set replaces ours, both ways: a model LM Studio unloaded
        // on its own (idle TTL, manual unload) must be loaded again, and one it
        // still holds must not be loaded a second time.
        if let Ok(models) = self.backend.list_models().await
            && let Ok(mut state) = self.state.lock()
        {
            state.loaded = models
                .into_iter()
                .filter(|entry| entry.loaded)
                .map(|entry| entry.id)
                .collect();
        }

        let desired = self.desired_for(role)?;
        self.apply_desired(&desired).await?;

        let loaded = self
            .state
            .lock()
            .map(|state| state.loaded.contains(&model))
            .unwrap_or(false);

        if !loaded {
            let ttl = self
                .state
                .lock()
                .map(|state| state.ttl.as_secs())
                .unwrap_or(600);
            let result = self
                .backend
                .load_model(&LoadModelRequest {
                    role,
                    model: model.clone(),
                    context_length: None,
                    ttl_seconds: Some(ttl),
                    gpu_offload: None,
                })
                .await?;

            if let Ok(mut state) = self.state.lock() {
                state.loaded.insert(result.model);
                state.last_used.insert(model.clone(), Instant::now());
            }
        }

        Ok(model)
    }

    async fn apply_desired(&self, desired: &HashSet<String>) -> Result<(), CommandError> {
        let loaded: Vec<String> = self
            .state
            .lock()
            .map(|state| state.loaded.iter().cloned().collect())
            .unwrap_or_default();

        for model in loaded {
            if !desired.contains(&model) {
                self.unload_one(&model, false).await?;
            }
        }
        Ok(())
    }

    async fn unload_one(&self, model: &str, force: bool) -> Result<(), CommandError> {
        if !force {
            let busy = self
                .state
                .lock()
                .map(|state| state.busy.get(model).copied().unwrap_or_default())
                .unwrap_or_default();
            if busy > 0 {
                return Err(CommandError::conflict(format!(
                    "Model {model} is busy and cannot be unloaded"
                )));
            }
        }

        match self.backend.unload_model(model).await {
            Ok(()) => {}
            Err(error) => {
                if let Some(lms) = &self.lms {
                    lms.unload(model)?;
                } else {
                    return Err(error);
                }
            }
        }

        if let Ok(mut state) = self.state.lock() {
            state.loaded.remove(model);
            state.last_used.remove(model);
            if force {
                state.busy.remove(model);
            }
        }

        Ok(())
    }

    fn release(&self, model: &str) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(count) = state.busy.get_mut(model) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    state.busy.remove(model);
                }
            }
            state.last_used.insert(model.to_string(), Instant::now());
        }
    }
}

pub struct ModelLease {
    model: String,
    manager: Arc<ModelManager>,
    _permit: OwnedSemaphorePermit,
}

impl std::fmt::Debug for ModelLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModelLease")
            .field("model", &self.model)
            .finish()
    }
}

impl ModelLease {
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl Drop for ModelLease {
    fn drop(&mut self) {
        self.manager.release(&self.model);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use async_trait::async_trait;

    use super::{ManagerConfig, ModelManager, RoleConfig, RuntimeBackend};
    use crate::commands::CommandError;
    use crate::commands::models::{
        KeepWarm, LoadModelRequest, LoadModelResult, ModelInfo, ModelRole,
    };

    struct FakeBackend {
        loaded: Mutex<HashSet<String>>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                loaded: Mutex::new(HashSet::new()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }

        fn loaded(&self) -> HashSet<String> {
            self.loaded.lock().expect("loaded").clone()
        }
    }

    #[async_trait]
    impl RuntimeBackend for FakeBackend {
        async fn list_models(&self) -> Result<Vec<ModelInfo>, CommandError> {
            Ok(self
                .loaded()
                .into_iter()
                .map(|id| ModelInfo {
                    id,
                    kind: "llm".into(),
                    loaded: true,
                    context_length: Some(8192),
                    path: None,
                    vision: None,
                    reasoning: None,
                    max_output: None,
                    quantization: None,
                })
                .collect())
        }

        async fn load_model(
            &self,
            request: &LoadModelRequest,
        ) -> Result<LoadModelResult, CommandError> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("load {}", request.model));
            self.loaded
                .lock()
                .expect("loaded")
                .insert(request.model.clone());
            Ok(LoadModelResult {
                model: request.model.clone(),
                context_length: request.context_length,
                load_time_ms: 1,
                effective_gpu_offload: request.gpu_offload,
            })
        }

        async fn unload_model(&self, model: &str) -> Result<(), CommandError> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("unload {model}"));
            self.loaded.lock().expect("loaded").remove(model);
            Ok(())
        }
    }

    fn manager(
        backend: Arc<FakeBackend>,
        keep_warm: KeepWarm,
        concurrency: u8,
    ) -> Arc<ModelManager> {
        ModelManager::new(
            backend,
            None,
            ManagerConfig {
                roles: RoleConfig::default(),
                keep_warm,
                ttl: Duration::from_secs(60),
                describer_concurrency: concurrency,
            },
        )
    }

    #[tokio::test]
    async fn none_policy_switches_models() {
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend.clone(), KeepWarm::None, 2);

        drop(
            manager
                .clone()
                .acquire(ModelRole::Describer)
                .await
                .expect("describer"),
        );
        drop(
            manager
                .clone()
                .acquire(ModelRole::Composer)
                .await
                .expect("composer"),
        );

        let calls = backend.calls();
        assert!(calls.contains(&"load lfm3-vl-2b".to_string()));
        assert!(calls.contains(&"unload lfm3-vl-2b".to_string()));
        assert!(calls.contains(&"load gemma4-12b-qat-uncensored-hauhaucs-balanced".to_string()));
        assert!(!backend.loaded().contains("lfm3-vl-2b"));
    }

    #[tokio::test]
    async fn reuses_a_model_already_loaded_in_lm_studio() {
        let backend = Arc::new(FakeBackend::new());
        backend
            .loaded
            .lock()
            .expect("loaded")
            .insert("gemma4-12b-qat-uncensored-hauhaucs-balanced".into());
        let manager = manager(backend.clone(), KeepWarm::None, 2);

        drop(
            manager
                .clone()
                .acquire(ModelRole::Composer)
                .await
                .expect("composer"),
        );

        let calls = backend.calls();
        assert!(
            !calls
                .iter()
                .any(|call| call == "load gemma4-12b-qat-uncensored-hauhaucs-balanced"),
            "the manager loaded an already-loaded model again: {calls:?}"
        );
    }

    fn load_count(backend: &FakeBackend, model: &str) -> usize {
        backend
            .calls()
            .iter()
            .filter(|call| *call == &format!("load {model}"))
            .count()
    }

    #[tokio::test]
    async fn keep_both_loads_each_model_once_across_many_runs() {
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend.clone(), KeepWarm::Both, 2);
        for _ in 0..5 {
            drop(manager.clone().acquire(ModelRole::Describer).await.expect("describer"));
            drop(manager.clone().acquire(ModelRole::Composer).await.expect("composer"));
        }
        assert_eq!(load_count(&backend, "lfm3-vl-2b"), 1);
        assert_eq!(
            load_count(&backend, "gemma4-12b-qat-uncensored-hauhaucs-balanced"),
            1
        );
        assert!(!backend.calls().iter().any(|call| call.starts_with("unload")));
    }

    #[tokio::test]
    async fn repeated_runs_of_one_role_never_reload() {
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend.clone(), KeepWarm::None, 2);
        for _ in 0..5 {
            drop(manager.clone().acquire(ModelRole::Composer).await.expect("composer"));
        }
        assert_eq!(
            load_count(&backend, "gemma4-12b-qat-uncensored-hauhaucs-balanced"),
            1
        );
    }

    #[tokio::test]
    async fn a_model_lm_studio_unloaded_by_itself_is_loaded_again() {
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend.clone(), KeepWarm::Both, 2);
        drop(manager.clone().acquire(ModelRole::Composer).await.expect("first"));
        // LM Studio's own idle TTL (or the user) unloads it.
        backend.loaded.lock().expect("loaded").clear();
        drop(manager.clone().acquire(ModelRole::Composer).await.expect("second"));
        assert_eq!(
            load_count(&backend, "gemma4-12b-qat-uncensored-hauhaucs-balanced"),
            2
        );
    }

    #[tokio::test]
    async fn none_policy_swaps_models_when_roles_differ() {
        // Documents the VRAM-saving behaviour: with different describer and
        // composer models, "keep none" unloads one to load the other each time.
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend.clone(), KeepWarm::None, 2);
        for _ in 0..2 {
            drop(manager.clone().acquire(ModelRole::Describer).await.expect("describer"));
            drop(manager.clone().acquire(ModelRole::Composer).await.expect("composer"));
        }
        assert_eq!(load_count(&backend, "lfm3-vl-2b"), 2);
    }

    #[tokio::test]
    async fn both_policy_keeps_models_resident() {
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend.clone(), KeepWarm::Both, 2);

        drop(
            manager
                .clone()
                .acquire(ModelRole::Describer)
                .await
                .expect("describer"),
        );
        drop(
            manager
                .clone()
                .acquire(ModelRole::Composer)
                .await
                .expect("composer"),
        );

        assert!(
            !backend
                .calls()
                .iter()
                .any(|call| call.starts_with("unload"))
        );
        assert_eq!(backend.loaded().len(), 2);
    }

    #[tokio::test]
    async fn busy_model_blocks_a_switch() {
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend.clone(), KeepWarm::None, 2);

        let composer = manager
            .clone()
            .acquire(ModelRole::Composer)
            .await
            .expect("composer");
        let error = manager
            .clone()
            .acquire(ModelRole::Describer)
            .await
            .expect_err("switch must fail");
        assert_eq!(error.category, crate::commands::ErrorCategory::Conflict);
        drop(composer);
    }

    #[tokio::test]
    async fn enforces_the_describer_concurrency_cap() {
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend, KeepWarm::None, 2);

        let first = manager
            .clone()
            .acquire(ModelRole::Describer)
            .await
            .expect("first");
        let second = manager
            .clone()
            .acquire(ModelRole::Describer)
            .await
            .expect("second");

        let pending = tokio::time::timeout(
            Duration::from_millis(60),
            manager.clone().acquire(ModelRole::Describer),
        )
        .await;
        assert!(pending.is_err(), "third acquire should wait");

        drop(first);
        let third = tokio::time::timeout(
            Duration::from_millis(500),
            manager.clone().acquire(ModelRole::Describer),
        )
        .await
        .expect("third should proceed")
        .expect("third");

        drop(second);
        drop(third);
    }

    #[tokio::test]
    async fn ttl_unloads_idle_models_but_not_kept_ones() {
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend.clone(), KeepWarm::None, 2);

        drop(
            manager
                .clone()
                .acquire(ModelRole::Describer)
                .await
                .expect("describer"),
        );
        let unloaded = manager
            .unload_idle(Instant::now() + Duration::from_secs(120))
            .await
            .expect("idle");
        assert_eq!(unloaded.len(), 1);
        assert!(backend.loaded().is_empty());

        let backend = Arc::new(FakeBackend::new());
        let manager = self::manager(backend.clone(), KeepWarm::Both, 2);
        drop(
            manager
                .clone()
                .acquire(ModelRole::Describer)
                .await
                .expect("describer"),
        );
        let unloaded = manager
            .unload_idle(Instant::now() + Duration::from_secs(120))
            .await
            .expect("idle");
        assert!(unloaded.is_empty());
        assert!(!backend.loaded().is_empty());
    }

    #[tokio::test]
    async fn unload_all_respects_busy_models() {
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend.clone(), KeepWarm::None, 2);
        let lease = manager
            .clone()
            .acquire(ModelRole::Describer)
            .await
            .expect("describer");

        let error = manager.unload_all(false).await.expect_err("busy");
        assert_eq!(error.category, crate::commands::ErrorCategory::Conflict);

        manager.unload_all(true).await.expect("force");
        assert!(backend.loaded().is_empty());
        drop(lease);
    }

    #[tokio::test]
    async fn status_reports_loaded_models() {
        let backend = Arc::new(FakeBackend::new());
        let manager = manager(backend, KeepWarm::None, 2);
        drop(
            manager
                .clone()
                .acquire(ModelRole::Describer)
                .await
                .expect("describer"),
        );

        let status = manager.status().await;
        assert!(status.reachable);
        assert!(status.loaded.contains(&"lfm3-vl-2b".to_string()));
        assert_eq!(status.keep_warm, KeepWarm::None);
    }
}
