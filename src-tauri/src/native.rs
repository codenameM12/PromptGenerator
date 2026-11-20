//! Plain-library facade over the existing backend for the native UI.
//!
//! The Tauri `#[tauri::command]` wrappers own `State`/`AppHandle`, which a
//! native binary cannot provide. This module reuses the *same* repository and
//! media services with plain state, so the native UI reaches real logic rather
//! than a mock. Event emission is intentionally omitted: there is no webview.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::commands::CommandError;
use crate::commands::assets::{Asset, ImportResult};
use crate::commands::drag::{
    DragControl, DragPayload, DragPrepareRequest, DragRevealRequest, DragSaveRequest,
};
use crate::commands::edits::{EditControl, EditRequest};
use crate::commands::pipeline::{BatchRequest, ComposeRequest, DescribeRequest, PipelineControl};
use crate::db::Database;
use crate::db::repository::{assets as asset_repo, edits as edit_repo, generations as gen_repo};
use crate::drag_out::{self, Transfer};
use crate::job_queue;
use crate::media::edit::{self as edit_math, SourceInfo};
use crate::media::ffmpeg::MediaTools;
use crate::media::import::{ImportControl, ImportService};
use crate::media::protocol::MediaAccess;
use crate::paths::{self, AppPaths};
use crate::runtime::client::LmStudioClient;
use crate::runtime::lms_cli::LmsCli;
use crate::runtime::manager::ModelManager;
use crate::settings::SettingsStore;
use crate::workspace::WorkspaceStore;

/// Plain, non-Tauri state shared by the native UI's backend calls.
pub struct NativeState {
    pub paths: AppPaths,
    pub database: Mutex<Database>,
    pub tools: MediaTools,
    pub import_control: ImportControl,
    pub access: MediaAccess,
    pub settings: Mutex<SettingsStore>,
    pub workspace: Mutex<WorkspaceStore>,
    pub edit_control: EditControl,
    pub drag_control: DragControl,
    pub pipeline_control: PipelineControl,
    /// Native-only queue pause flag; the Tauri queue owns its own flag.
    pub jobs_paused: AtomicBool,
    /// Cancellation handles for running jobs (the native runner's counterpart
    /// of the Tauri-managed queue).
    pub queue: job_queue::JobQueue,
    pub manager: Arc<ModelManager>,
}

/// Executes queued items in-process, mirroring the Tauri runner.
struct StateExecutor {
    state: Arc<NativeState>,
}

#[async_trait::async_trait]
impl job_queue::JobExecutor for StateExecutor {
    async fn execute(
        &self,
        job: &crate::db::repository::jobs::JobRow,
        item: &crate::db::repository::jobs::JobItemRow,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Value, CommandError> {
        match job.kind.as_str() {
            "compose" => {
                let composer_prompt = self
                    .state
                    .settings
                    .lock()
                    .ok()
                    .and_then(|store| store.get().composer_system_prompt);
                job_queue::run_compose_item(
                    &self.state.database,
                    &self.state.manager,
                    composer_prompt,
                    &item.payload_json,
                    cancel,
                )
                .await
            }
            other => Err(CommandError::validation(format!(
                "Job kind '{other}' cannot be executed by this runner"
            ))),
        }
    }
}

fn per_user_dirs() -> (PathBuf, PathBuf) {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("prompt-studio-local"));
    let root = base.join("Prompt Studio");
    (root.join("data"), root.join("cache"))
}

impl NativeState {
    /// Open the same on-disk state the Tauri app uses.
    pub fn open() -> Result<Self, CommandError> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let (data, cache) = per_user_dirs();
        let paths =
            paths::resolve_app_paths(&exe_dir, &data, &cache, paths::portable_requested(&exe_dir))?;

        let mut database = Database::open(&paths.data_dir.join("prompt-studio.db"))?;
        database.migrate()?;
        crate::builtin_presets::seed(database.connection())?;

        let settings = SettingsStore::load(&paths.config_dir)?;
        let workspace = WorkspaceStore::load(&paths.data_dir)?;
        let settings_snapshot = settings.get();
        let token = settings.token().map(ToOwned::to_owned);
        let client = LmStudioClient::new(settings_snapshot.endpoint.clone(), token)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let manager = ModelManager::new(
            Arc::new(client),
            LmsCli::detect(),
            crate::commands::models::manager_config(&settings_snapshot),
        );
        let tools = MediaTools::resolve()?;
        let access = MediaAccess::new(paths.cache_dir.clone());
        if let Ok(assets) = asset_repo::list(database.connection(), 100_000, 0) {
            for asset in assets {
                access.allow_path(Path::new(&asset.source_path));
            }
        }

        Ok(Self {
            paths,
            database: Mutex::new(database),
            tools,
            import_control: ImportControl::default(),
            access,
            settings: Mutex::new(settings),
            workspace: Mutex::new(workspace),
            edit_control: EditControl::default(),
            drag_control: DragControl::default(),
            pipeline_control: PipelineControl::default(),
            jobs_paused: AtomicBool::new(false),
            queue: job_queue::JobQueue::default(),
            manager,
        })
    }

    /// Starts the thread that gives every generation a short title. The
    /// MiniLM model loads once on that thread; without the model files a plain
    /// word-picking fallback is used.
    pub fn start_title_worker(self: &Arc<Self>) {
        use crate::db::repository::titles;
        let state = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("title-worker".into())
            .spawn(move || {
                loop {
                    let pending = state
                        .database
                        .lock()
                        .ok()
                        .and_then(|guard| titles::missing(guard.connection(), 64).ok())
                        .unwrap_or_default();
                    if pending.is_empty() {
                        std::thread::sleep(Duration::from_secs(2));
                        continue;
                    }
                    for (id, prompt) in pending {
                        let (title, source) =
                            crate::title::shared_title(&prompt, &Default::default());
                        if let Ok(guard) = state.database.lock() {
                            let _ = titles::set(guard.connection(), &id, &title, source);
                        }
                    }
                }
            });
    }

    /// Starts the background runner that executes queued jobs. Without it the
    /// native shell can enqueue work (batches, dataset runs) but nothing ever
    /// runs it. Call once after the state is wrapped in an `Arc`.
    pub fn start_job_runner(self: &Arc<Self>) {
        use crate::db::repository::jobs as repo;
        let state = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("job-runner".into())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                let executor = StateExecutor {
                    state: Arc::clone(&state),
                };
                runtime.block_on(async move {
                    loop {
                        tokio::time::sleep(Duration::from_millis(150)).await;
                        if state.jobs_paused.load(Ordering::Relaxed) {
                            continue;
                        }
                        let claimed = {
                            let Ok(guard) = state.database.lock() else {
                                continue;
                            };
                            repo::claim_next(guard.connection()).ok().flatten()
                        };
                        let Some(job) = claimed else {
                            continue;
                        };
                        let cancel = state.queue.cancel_flag(&job.id);
                        if cancel.load(Ordering::Relaxed) {
                            if let Ok(guard) = state.database.lock() {
                                let _ = repo::update_state(
                                    guard.connection(),
                                    &job.id,
                                    "cancelled",
                                    0.0,
                                    Some("Cancelled before it started"),
                                );
                            }
                            state.queue.release(&job.id);
                            continue;
                        }
                        let mut on_event = |_: &job_queue::JobProgressEvent| {};
                        let result = job_queue::run_job(
                            &state.database,
                            &job,
                            &executor,
                            &cancel,
                            &job_queue::RunOptions::default(),
                            &mut on_event,
                        )
                        .await;
                        if let Err(error) = result
                            && let Ok(guard) = state.database.lock()
                        {
                            let _ = repo::update_state(
                                guard.connection(),
                                &job.id,
                                "failed",
                                0.0,
                                Some(&format!("[{:?}] {}", error.category, error.message)),
                            );
                        }
                        state.queue.release(&job.id);
                    }
                });
            });
        let _ = spawned;
    }

    /// Same behaviour as `commands::assets::assets_list`, without Tauri state.
    pub fn list_assets(&self) -> Result<Vec<Asset>, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        asset_repo::refresh_missing(guard.connection())?;
        let rows = asset_repo::list(guard.connection(), 1000, 0)?;
        Ok(rows.into_iter().map(to_asset).collect())
    }

    /// Same behaviour as `commands::assets::run_import`, minus progress events.
    pub fn import_paths(&self, inputs: &[String]) -> Result<Vec<ImportResult>, CommandError> {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut current = self
                .import_control
                .current
                .lock()
                .map_err(|_| CommandError::internal("Import lock is poisoned"))?;
            if current.is_some() {
                return Err(CommandError::conflict("An import is already running"));
            }
            *current = Some(cancel.clone());
        }

        let results = {
            let guard = self
                .database
                .lock()
                .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
            let service = ImportService {
                tools: &self.tools,
                connection: guard.connection(),
                paths: &self.paths,
            };
            service.import(inputs, &cancel, |_| {})
        };

        if let Ok(mut current) = self.import_control.current.lock() {
            *current = None;
        }
        for result in &results {
            if result.asset_id.is_some() {
                self.access.allow_path(Path::new(&result.path));
            }
        }
        Ok(results)
    }

    /// Same validation as `commands::assets::assets_import_clipboard`.
    pub fn import_clipboard(
        &self,
        file_name: Option<&str>,
        bytes: &[u8],
    ) -> Result<Vec<ImportResult>, CommandError> {
        if bytes.is_empty() {
            return Err(CommandError::validation("Clipboard image is empty"));
        }
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(CommandError::validation(
                "Clipboard image is larger than 64 MB",
            ));
        }

        let directory = self.paths.cache_dir.join("imports");
        std::fs::create_dir_all(&directory).map_err(|error| CommandError::from_io(&error))?;

        let requested = file_name.unwrap_or("clipboard.png");
        let stem = Path::new(requested)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("clipboard");
        let safe_stem: String = stem
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                    character
                } else {
                    '_'
                }
            })
            .collect();

        let file = directory.join(format!("{}-{safe_stem}.png", Uuid::now_v7()));
        std::fs::write(&file, bytes).map_err(|error| CommandError::from_io(&error))?;
        self.import_paths(&[file.display().to_string()])
    }

    /// Cancels the active import flag, mirroring `assets_import_cancel`.
    pub fn cancel_import(&self) {
        if let Ok(guard) = self.import_control.current.lock()
            && let Some(flag) = guard.as_ref()
        {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// Generic command dispatch. Each arm reimplements only the thin glue of the
// Tauri wrapper; domain logic stays in the existing modules.
// ---------------------------------------------------------------------------

fn parse<T: DeserializeOwned>(value: &Value) -> Result<T, CommandError> {
    serde_json::from_value(value.clone())
        .map_err(|error| CommandError::validation(format!("Invalid command arguments: {error}")))
}

fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T, CommandError> {
    parse(args.get(key).unwrap_or(&Value::Null))
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, CommandError> {
    serde_json::to_value(value)
        .map_err(|error| CommandError::internal(format!("serialize result: {error}")))
}

impl NativeState {
    fn db<T, F>(&self, f: F) -> Result<Value, CommandError>
    where
        T: serde::Serialize,
        F: FnOnce(&rusqlite::Connection) -> Result<T, CommandError>,
    {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        to_value(f(guard.connection())?)
    }

    /// Dispatches a command name using the same JSON shapes as the Tauri IPC
    /// surface. Unknown/unwired commands return a structured not-implemented
    /// error — never a fabricated success.
    pub fn call(&self, command: &str, args: Value) -> Result<Value, CommandError> {
        let request = args.get("request").cloned().unwrap_or(Value::Null);
        match command {
            // assets
            "assets_list" => to_value(self.list_assets()?),
            "assets_import" => {
                let request: crate::commands::assets::ImportRequest = parse(&request)?;
                to_value(self.import_paths(&request.paths)?)
            }
            "assets_import_clipboard" => {
                let request: crate::commands::assets::ClipboardImportRequest = parse(&request)?;
                to_value(self.import_clipboard(request.file_name.as_deref(), &request.bytes)?)
            }
            "assets_import_cancel" => {
                self.cancel_import();
                to_value(())
            }
            "assets_sample_frames" => self.assets_sample_frames(&request),

            // edits
            "edits_latest" => {
                let ids = arg::<Vec<String>>(&args, "assetIds")?;
                self.db(|connection| {
                    Ok(
                        crate::db::repository::edits::latest_by_assets(connection, &ids)?
                            .into_iter()
                            .map(|edit| crate::commands::edits::LatestEdit {
                                asset_id: edit.asset_id,
                                edit_id: edit.id,
                                output_path: edit.output_path,
                                width: edit.width,
                                height: edit.height,
                            })
                            .collect::<Vec<_>>(),
                    )
                })
            }
            "edits_preview" => {
                let request: EditRequest = parse(&request)?;
                self.edits_preview(&request)
            }
            "edits_save" => {
                let request: EditRequest = parse(&request)?;
                self.edits_save(&request)
            }
            "edits_cancel" => {
                if let Ok(guard) = self.edit_control.current.lock()
                    && let Some(flag) = guard.as_ref()
                {
                    flag.store(true, Ordering::Relaxed);
                }
                to_value(())
            }
            "edits_export_mask" => self.edits_export_mask(&request),

            // presets
            "presets_list" => self.presets_list(),
            "presets_get" => self.presets_get(&arg::<String>(&args, "id")?),
            "presets_preview" => {
                let request: crate::commands::presets::CompileRequest = parse(&request)?;
                self.presets_preview(&request)
            }
            "presets_compile" => {
                let request: crate::commands::presets::CompileDocumentRequest = parse(&request)?;
                to_value(presets_compile(&request)?)
            }
            "presets_save" => {
                let request: crate::commands::presets::SavePresetRequest = parse(&request)?;
                self.presets_save(&request)
            }
            "presets_duplicate" => self.presets_duplicate(&arg::<String>(&args, "id")?),
            "presets_delete" => {
                let request: crate::commands::presets::DeletePresetRequest = parse(&request)?;
                self.presets_delete(&request)
            }
            "presets_import" => {
                let request: crate::commands::presets::PresetPathRequest = parse(&request)?;
                self.presets_import(&request)
            }
            "presets_export" => {
                let request: crate::commands::presets::PresetPathRequest = parse(&request)?;
                self.presets_export(&request)
            }

            // jobs
            "jobs_list" => to_value(job_queue::list(&self.database, 200)?),
            "jobs_get" => to_value(job_queue::get(
                &self.database,
                &arg::<String>(&args, "id")?,
            )?),
            "jobs_status" => self.jobs_status(),
            "jobs_enqueue" => {
                let spec: job_queue::EnqueueSpec = parse(&request)?;
                to_value(job_queue::enqueue(&self.database, spec)?)
            }
            "jobs_pause" => {
                self.jobs_paused.store(true, Ordering::Relaxed);
                to_value(())
            }
            "jobs_resume" => {
                self.jobs_paused.store(false, Ordering::Relaxed);
                to_value(())
            }
            "jobs_retry" => {
                let id = arg::<String>(&args, "id")?;
                to_value(job_queue::retry(&self.database, &id)?)
            }
            "jobs_cancel" => self.jobs_cancel(&arg::<String>(&args, "id")?),
            "jobs_cancel_waiting" => to_value(job_queue::cancel_waiting(&self.database)?),
            "jobs_reorder" => {
                let request: job_queue::ReorderRequest = parse(&request)?;
                to_value(job_queue::reorder(&self.database, &request)?)
            }

            // settings / workspace
            "settings_get" => {
                let store = self
                    .settings
                    .lock()
                    .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
                to_value(store.get())
            }
            "settings_update" => {
                // Tauri passes this DTO as the named `patch` argument.
                let patch: crate::commands::settings::SettingsPatch = arg(&args, "patch")?;
                let mut store = self
                    .settings
                    .lock()
                    .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
                to_value(store.update(patch)?)
            }
            "workspace_get_draft" => {
                let store = self
                    .workspace
                    .lock()
                    .map_err(|_| CommandError::internal("Workspace lock is poisoned"))?;
                to_value(store.get())
            }
            "workspace_save_draft" => {
                let request: crate::commands::workspace::SaveDraftRequest = parse(&request)?;
                let mut store = self
                    .workspace
                    .lock()
                    .map_err(|_| CommandError::internal("Workspace lock is poisoned"))?;
                to_value(store.save(request.draft)?)
            }
            "workspace_clear_draft" => {
                let mut store = self
                    .workspace
                    .lock()
                    .map_err(|_| CommandError::internal("Workspace lock is poisoned"))?;
                store.clear()?;
                to_value(())
            }

            // library
            "library_query" => {
                // Tauri passes this DTO as the named `query` argument.
                let query: crate::commands::library::GenerationQuery = arg(&args, "query")?;
                self.library_query(&query)
            }
            "library_get" => self.library_get(&arg::<String>(&args, "id")?),
            "library_compare" => {
                let request: crate::commands::library::CompareRequest = parse(&request)?;
                self.library_compare(&request)
            }
            "characters_list" => self.db(crate::characters::list),
            "characters_get" => {
                let id: String = arg(&args, "id")?;
                let data_dir = self.paths.data_dir.clone();
                self.db(|connection| {
                    let mut value = crate::characters::get(connection, &id)?;
                    crate::characters::absolutize(&mut value, &data_dir);
                    Ok(value)
                })
            }
            "characters_create" => {
                let name: String = arg(&args, "name")?;
                self.db(|connection| crate::characters::create(connection, &name))
            }
            "characters_update" => {
                let id: String = arg(&args, "id")?;
                let name: String = arg(&args, "name")?;
                let description: String = arg(&args, "description")?;
                let data_dir = self.paths.data_dir.clone();
                self.db(|connection| {
                    let mut value =
                        crate::characters::update(connection, &id, &name, &description)?;
                    crate::characters::absolutize(&mut value, &data_dir);
                    Ok(value)
                })
            }
            "characters_delete" => {
                let id: String = arg(&args, "id")?;
                let data_dir = self.paths.data_dir.clone();
                self.db(|connection| crate::characters::delete(connection, &data_dir, &id))
            }
            "characters_add_images" => {
                let id: String = arg(&args, "id")?;
                let paths: Vec<String> = arg(&args, "paths")?;
                let data_dir = self.paths.data_dir.clone();
                self.db(|connection| {
                    let mut value =
                        crate::characters::add_images(connection, &data_dir, &id, &paths)?;
                    crate::characters::absolutize(&mut value, &data_dir);
                    Ok(value)
                })
            }
            "characters_image_update" => {
                let image_id: String = arg(&args, "imageId")?;
                let file_name: String = arg(&args, "fileName")?;
                let role: String = arg(&args, "role")?;
                let description: String = arg(&args, "description")?;
                let is_default: bool = arg(&args, "isDefault")?;
                self.db(|connection| {
                    crate::characters::update_image(
                        connection,
                        &image_id,
                        &file_name,
                        &role,
                        &description,
                        is_default,
                    )
                })
            }
            "characters_image_remove" => {
                let image_id: String = arg(&args, "imageId")?;
                let data_dir = self.paths.data_dir.clone();
                self.db(|connection| {
                    crate::characters::remove_image(connection, &data_dir, &image_id)
                })
            }
            "characters_load" => self.characters_load(&args),
            "characters_image_autoname" => {
                let image_id: String = arg(&args, "imageId")?;
                let data_dir = self.paths.data_dir.clone();
                let renamed = self.db(|connection| {
                    crate::characters::autoname_image(
                        connection,
                        &data_dir,
                        &image_id,
                        &|text, exclude| crate::title::shared_title(text, exclude).0,
                    )
                })?;
                if let Some(path) = renamed.get("path").and_then(Value::as_str) {
                    self.access.allow_path(Path::new(path));
                }
                Ok(renamed)
            }
            "characters_link_generations" => {
                let characters: Vec<String> = arg(&args, "characterIds")?;
                let generations: Vec<String> = arg(&args, "generationIds")?;
                self.db(|connection| {
                    crate::characters::link_generations(connection, &characters, &generations)
                })
            }
            "characters_generation_ids" => {
                let id: String = arg(&args, "id")?;
                self.db(|connection| crate::characters::generation_ids(connection, &id))
            }
            "library_render_meta_get" => {
                let id: String = arg(&args, "id")?;
                self.db(|connection| crate::db::repository::render_meta::get(connection, &id))
            }
            "library_render_meta_set" => {
                let id: String = arg(&args, "id")?;
                let meta: Value = arg(&args, "meta")?;
                self.db(|connection| {
                    crate::db::repository::render_meta::set(connection, &id, &meta)?;
                    crate::db::repository::render_meta::get(connection, &id)
                })
            }
            "library_favorite" => {
                let request: crate::commands::library::FavoriteRequest = parse(&request)?;
                self.library_favorite(&request)
            }
            "library_update" => {
                let request: crate::commands::library::UpdateGenerationRequest = parse(&request)?;
                self.library_update(&request)
            }
            "library_tags" => self.library_tags(),
            "library_collections" => self.library_collections(),
            "library_collection_create" => {
                let request: crate::commands::library::NameRequest = parse(&request)?;
                self.library_collection_create(&request)
            }
            "library_collection_rename" => {
                let request: crate::commands::library::CollectionRequest = parse(&request)?;
                self.library_collection_rename(&request)
            }
            "library_collection_delete" => {
                self.library_collection_delete(&arg::<String>(&args, "id")?)
            }
            "library_collection_assign" => {
                let request: crate::commands::library::CollectionAssignRequest = parse(&request)?;
                self.library_collection_assign(&request)
            }
            "library_versions" => self.library_versions(&arg::<String>(&args, "id")?),
            "library_diff" => {
                let request: crate::commands::library::DiffRequest = parse(&request)?;
                self.library_diff(&request)
            }
            "library_restore_workspace" => {
                self.library_restore_workspace(&arg::<String>(&args, "id")?)
            }
            "library_trash" => {
                let request: crate::commands::library::TrashRequest = parse(&request)?;
                self.library_trash(&request, false)
            }
            "library_restore_deleted" => {
                let request: crate::commands::library::TrashRequest = parse(&request)?;
                self.library_trash(&request, true)
            }
            "library_assets_trash" => self.library_assets_trash(),
            "library_purge" => {
                let request: crate::commands::library::PurgeRequest = parse(&request)?;
                self.library_purge(&request)
            }
            "library_export" => {
                let request: crate::commands::library::ExportRequest = parse(&request)?;
                self.library_export(&request)
            }
            "library_import" => {
                let request: crate::commands::library::ImportRequest = parse(&request)?;
                self.library_import(&request)
            }
            "library_rerun" => {
                let request: crate::commands::library::RerunRequest = parse(&request)?;
                self.library_rerun(&request)
            }
            "library_backup" => {
                let request: crate::commands::library::BackupRequest = parse(&request)?;
                self.library_backup(&request)
            }
            "library_restore" => {
                let request: crate::commands::library::RestoreRequest = parse(&request)?;
                self.library_restore(&request)
            }

            // datasets
            "datasets_list" => self.datasets_list(),
            "datasets_preview" => {
                let request: crate::commands::datasets::DatasetPreviewRequest = parse(&request)?;
                self.datasets_preview(&request)
            }
            "datasets_rows" => {
                let request: crate::commands::datasets::DatasetRowsRequest = parse(&request)?;
                self.datasets_rows(&request)
            }
            "datasets_delete" => self.datasets_delete(&arg::<String>(&args, "datasetId")?),
            "datasets_profiles" => self.datasets_profiles(),

            // models / pipeline / drag / diagnostics
            "models_list" => self.models_list(),
            "models_status" => self.models_status(),
            "models_load" => self.models_load(&request),
            "models_unload" => {
                let model = arg::<String>(&args, "model")?;
                self.with_runtime(|runtime| runtime.block_on(self.manager.unload(&model)))??;
                to_value(())
            }
            "models_unload_all" => {
                let force = arg::<bool>(&args, "force").unwrap_or(false);
                self.with_runtime(|runtime| runtime.block_on(self.manager.unload_all(force)))??;
                to_value(())
            }
            "models_set_roles" => self.models_set_roles(&request),
            "models_set_policy" => self.models_set_policy(&request),
            "pipeline_cancel" => {
                if let Ok(guard) = self.pipeline_control.current.lock()
                    && let Some(flag) = guard.as_ref()
                {
                    flag.store(true, Ordering::Relaxed);
                }
                to_value(())
            }
            // pipeline (no webview: token emission is a no-op callback)
            "pipeline_describe" => {
                let request: DescribeRequest = parse(&request)?;
                self.pipeline_describe(request)
            }
            "pipeline_compose" => {
                let request: ComposeRequest = parse(&request)?;
                self.pipeline_compose(request)
            }
            "pipeline_batch" => {
                let request: BatchRequest = parse(&request)?;
                self.pipeline_batch(request)
            }

            // drag-out
            "drag_prepare" => {
                let request: DragPrepareRequest = parse(&request)?;
                self.drag_prepare(&request)
            }
            "drag_cancel" => {
                if let Ok(guard) = self.drag_control.current.lock()
                    && let Some(transfer) = guard.as_ref()
                {
                    transfer.cancel.store(true, Ordering::Relaxed);
                }
                to_value(())
            }
            "drag_save" => {
                let request: DragSaveRequest = parse(&request)?;
                let bytes = drag_out::save_as(
                    Path::new(&request.source),
                    Path::new(&request.output_path),
                    &self.paths.cache_dir,
                )?;
                to_value(bytes)
            }
            "drag_reveal" => {
                let request: DragRevealRequest = parse(&request)?;
                drag_out::reveal(Path::new(&request.path))?;
                to_value(())
            }

            // datasets
            "datasets_import" => {
                let request: crate::commands::datasets::DatasetImportRequest = parse(&request)?;
                to_value(crate::commands::datasets::import(request, &self.database)?)
            }
            "datasets_run" => {
                let request: crate::commands::datasets::DatasetRunRequest = parse(&request)?;
                to_value(crate::commands::datasets::enqueue_run(
                    request,
                    &self.database,
                    &self.tools,
                    &self.paths,
                    &self.access,
                )?)
            }
            "datasets_export" => {
                let request: crate::commands::datasets::DatasetExportRequest = parse(&request)?;
                to_value(crate::commands::datasets::export(request, &self.database)?)
            }

            // status / app info
            "status_snapshot" => self.status_snapshot(),
            "app_info" => self.app_info(),
            "diagnostics_export" => self.diagnostics_export(&request),

            other => Err(CommandError::not_implemented(other)),
        }
    }
}

// ---- command helpers -------------------------------------------------------

fn source_info(asset: &asset_repo::AssetRow) -> SourceInfo {
    SourceInfo {
        width: asset.width.unwrap_or(0).max(0) as u32,
        height: asset.height.unwrap_or(0).max(0) as u32,
        duration_ms: asset.duration_ms.map(|value| value.max(0) as u64),
        fps: asset.fps,
        is_video: asset.kind == "video",
    }
}

fn presets_compile(
    request: &crate::commands::presets::CompileDocumentRequest,
) -> Result<crate::commands::presets::CompiledPrompt, CommandError> {
    let spec = crate::preset::parse(&request.document)?;
    crate::preset::validate(&spec)?;
    let mut compiled = crate::preset::compile(
        &spec,
        &crate::preset::CompileInput {
            guidance: request.guidance.clone(),
            description: request.description.clone(),
            brief: None,
            locked: request.locked.clone(),
            params: request.params.clone(),
            existing_prompt: None,
        },
    );
    compiled
        .warnings
        .extend(crate::preset::preflight(&spec, &compiled));
    Ok(compiled)
}

impl NativeState {
    fn assets_sample_frames(&self, request: &Value) -> Result<Value, CommandError> {
        let request: crate::commands::assets::SampleFramesRequest = parse(request)?;
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let asset = asset_repo::get(guard.connection(), &request.asset_id)?
            .ok_or_else(|| CommandError::not_found("Asset not found"))?;
        let sampled = crate::media::frames::sample(
            &self.tools,
            &crate::media::frames::SampleRequest {
                source: Path::new(&asset.source_path),
                duration_ms: asset.duration_ms.unwrap_or_default().max(0) as u64,
                is_video: asset.kind == "video",
                strategy: request.strategy,
                count: request.count,
                max_images: request.max_images,
                want_contact_sheet: request.contact_sheet,
                cache_dir: &self.paths.cache_dir,
                asset_hash: &asset.hash,
            },
        )?;
        let rows: Vec<crate::db::repository::frames::FrameRow> = sampled
            .frames
            .iter()
            .map(|frame| crate::db::repository::frames::FrameRow {
                frame_index: frame.index as i64,
                timestamp_ms: frame.timestamp_ms as i64,
                path: frame.path.display().to_string(),
            })
            .collect();
        crate::db::repository::frames::replace_for_asset(
            guard.connection(),
            &request.asset_id,
            &rows,
        )?;
        to_value(crate::commands::assets::SampleFramesResult {
            frames: sampled
                .frames
                .into_iter()
                .map(|frame| crate::commands::assets::FrameDto {
                    index: frame.index,
                    timestamp_ms: frame.timestamp_ms,
                    path: frame.path.display().to_string(),
                })
                .collect(),
            contact_sheet: sampled.contact_sheet.map(|path| path.display().to_string()),
            scene_count: sampled.scene_count,
            capped: sampled.capped,
        })
    }

    fn edits_preview(&self, request: &EditRequest) -> Result<Value, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let asset = asset_repo::get(guard.connection(), &request.asset_id)?
            .ok_or_else(|| CommandError::not_found("Asset not found"))?;
        to_value(edit_math::plan(
            &request.ops,
            &request.target,
            &source_info(&asset),
        )?)
    }

    fn edits_save(&self, request: &EditRequest) -> Result<Value, CommandError> {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut current = self
                .edit_control
                .current
                .lock()
                .map_err(|_| CommandError::internal("Edit lock is poisoned"))?;
            if current.is_some() {
                return Err(CommandError::conflict("An edit is already running"));
            }
            *current = Some(cancel.clone());
        }
        let outcome = self.render_edit(request, &cancel);
        if let Ok(mut current) = self.edit_control.current.lock() {
            *current = None;
        }
        to_value(outcome?)
    }

    fn render_edit(
        &self,
        request: &EditRequest,
        cancel: &Arc<AtomicBool>,
    ) -> Result<crate::commands::edits::EditResult, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let asset = asset_repo::get(guard.connection(), &request.asset_id)?
            .ok_or_else(|| CommandError::not_found("Asset not found"))?;
        let source = source_info(&asset);
        let ops_json = serde_json::to_string(&request.ops)
            .map_err(|error| CommandError::internal(format!("serialize edit ops: {error}")))?;
        let target_json = serde_json::to_string(&request.target)
            .map_err(|error| CommandError::internal(format!("serialize target: {error}")))?;
        let cache_key = edit_math::cache_key(&asset.hash, &request.ops, &request.target)?;
        if let Some(existing) = edit_repo::find_by_cache_key(guard.connection(), &cache_key)?
            && Path::new(&existing.output_path).is_file()
        {
            return Ok(crate::commands::edits::EditResult {
                edit_id: existing.id,
                output_path: existing.output_path,
                width: existing.width.unwrap_or_default(),
                height: existing.height.unwrap_or_default(),
                fps: existing.fps,
                duration_ms: existing.duration_ms.map(|value| value.max(0) as u64),
                warnings: Vec::new(),
            });
        }
        let output_root = self
            .settings
            .lock()
            .map_err(|_| CommandError::internal("Settings lock is poisoned"))?
            .get()
            .output_root
            .map(PathBuf::from);
        let render_into = |root: Option<&Path>| {
            crate::media::render::render(
                &self.tools,
                &crate::media::render::RenderRequest {
                    source: Path::new(&asset.source_path),
                    source_hash: &asset.hash,
                    source_info: source_info(&asset),
                    ops: &request.ops,
                    target: &request.target,
                    output_root: root,
                },
                cancel,
                |_| {},
            )
        };
        let rendered = match render_into(output_root.as_deref()) {
            // Windows "controlled folder access" blocks writes beside sources in
            // Pictures/Documents. Fall back to the app's own data folder so the
            // edit still completes.
            Err(error) if error.code == "permission_denied" && output_root.is_none() => {
                render_into(Some(self.paths.data_dir.as_path())).map_err(|_| error)?
            }
            other => other?,
        };
        let _ = source;
        let mut warnings = rendered.plan.warnings.clone();
        let actual = crate::media::probe::probe(&self.tools, &rendered.output_path).ok();
        if let Some(actual) = &actual {
            warnings.extend(edit_math::verification_warnings(
                &rendered.plan,
                actual.display_width,
                actual.display_height,
                actual.fps,
            ));
        }
        let width = actual
            .as_ref()
            .map(|v| v.display_width)
            .unwrap_or(rendered.plan.resolution.width);
        let height = actual
            .as_ref()
            .map(|v| v.display_height)
            .unwrap_or(rendered.plan.resolution.height);
        let fps = actual.as_ref().and_then(|v| v.fps);
        let duration_ms = actual.as_ref().and_then(|v| v.duration_ms);
        let edit_id = Uuid::now_v7().to_string();
        edit_repo::insert(
            guard.connection(),
            &edit_repo::NewEdit {
                id: edit_id.clone(),
                asset_id: request.asset_id.clone(),
                ops_json,
                target_json,
                output_path: rendered.output_path.display().to_string(),
                cache_key,
                width: Some(width),
                height: Some(height),
                fps,
                duration_ms: duration_ms.map(|value| value as i64),
            },
        )?;
        Ok(crate::commands::edits::EditResult {
            edit_id,
            output_path: rendered.output_path.display().to_string(),
            width,
            height,
            fps,
            duration_ms,
            warnings,
        })
    }

    fn edits_export_mask(&self, request: &Value) -> Result<Value, CommandError> {
        let request: crate::commands::edits::ExportMaskRequest = parse(request)?;
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let asset = asset_repo::get(guard.connection(), &request.asset_id)?
            .ok_or_else(|| CommandError::not_found("Asset not found"))?;
        let path = crate::media::mask::export(&crate::media::mask::MaskExportRequest {
            source: Path::new(&asset.source_path),
            alternate_root: None,
            mask: &request.mask,
            mask_width: request.mask_width,
            mask_height: request.mask_height,
            target_width: request.target_width,
            target_height: request.target_height,
            feather: request.feather,
            invert: request.invert,
            polarity: request.polarity,
        })?;
        to_value(crate::commands::edits::ExportMaskResult {
            path: path.display().to_string(),
        })
    }
}

impl NativeState {
    fn presets_list(&self) -> Result<Value, CommandError> {
        self.db(|connection| {
            crate::db::repository::presets::list(connection, false)?
                .into_iter()
                .map(|row| {
                    Ok(crate::commands::presets::PresetSummary {
                        id: row.id,
                        name: row.name,
                        kind: preset_kind(&row.kind)?,
                        version: row.version,
                        description: row.description,
                        builtin: row.owner == "builtin",
                    })
                })
                .collect::<Result<Vec<_>, CommandError>>()
        })
    }

    fn preset_document(
        &self,
        id: &str,
    ) -> Result<crate::commands::presets::PresetDocument, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let row = crate::db::repository::presets::get(guard.connection(), id)?
            .filter(|row| row.deleted_at.is_none())
            .ok_or_else(|| CommandError::not_found("Preset not found"))?;
        let document: Value = serde_json::from_str(&row.document_json)
            .map_err(|error| CommandError::internal(format!("read preset document: {error}")))?;
        Ok(crate::commands::presets::PresetDocument {
            id: row.id,
            name: row.name,
            kind: preset_kind(&row.kind)?,
            version: row.version,
            hash: row.content_hash,
            builtin: row.owner == "builtin",
            document,
        })
    }

    fn presets_get(&self, id: &str) -> Result<Value, CommandError> {
        to_value(self.preset_document(id)?)
    }

    fn presets_save(
        &self,
        request: &crate::commands::presets::SavePresetRequest,
    ) -> Result<Value, CommandError> {
        let spec = crate::preset::parse(&request.document)?;
        crate::preset::validate(&spec)?;
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        if let Some(existing) = crate::db::repository::presets::get(guard.connection(), &spec.id)? {
            if existing.owner == "builtin" {
                return Err(CommandError::conflict(
                    "That id belongs to a built-in preset; duplicate it instead",
                ));
            }
            if existing.deleted_at.is_some() {
                return Err(CommandError::conflict(
                    "A deleted preset uses that id; restore it or choose another id",
                ));
            }
        }
        let hash = crate::preset::content_hash(&spec)?;
        crate::db::repository::presets::upsert(
            guard.connection(),
            &crate::db::repository::presets::NewPreset {
                id: spec.id.clone(),
                owner: "user".into(),
                name: spec.name.clone(),
                kind: preset_kind_str(spec.kind).into(),
                version: spec.version.clone(),
                description: spec.description.clone(),
                document_json: serde_json::to_string(&spec).map_err(|error| {
                    CommandError::internal(format!("serialize preset: {error}"))
                })?,
                content_hash: hash,
            },
        )?;
        to_value(crate::commands::presets::PresetDocument {
            id: spec.id.clone(),
            name: spec.name.clone(),
            kind: spec.kind,
            version: spec.version.clone(),
            hash: crate::preset::content_hash(&spec)?,
            builtin: false,
            document: to_value(&spec)?,
        })
    }

    fn presets_preview(
        &self,
        request: &crate::commands::presets::CompileRequest,
    ) -> Result<Value, CommandError> {
        let document = self.preset_document(&request.preset_id)?.document;
        let spec = crate::preset::parse(&document)?;
        crate::preset::validate(&spec)?;
        let mut compiled = crate::preset::compile(
            &spec,
            &crate::preset::CompileInput {
                guidance: request.guidance.clone(),
                description: request.description.clone(),
                brief: None,
                locked: request.locked.clone(),
                params: request.params.clone(),
                existing_prompt: None,
            },
        );
        compiled
            .warnings
            .extend(crate::preset::preflight(&spec, &compiled));
        to_value(compiled)
    }

    fn presets_duplicate(&self, id: &str) -> Result<Value, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let row = crate::db::repository::presets::get(guard.connection(), id)?
            .filter(|row| row.deleted_at.is_none())
            .ok_or_else(|| CommandError::not_found("Preset not found"))?;
        let document: Value = serde_json::from_str(&row.document_json)
            .map_err(|error| CommandError::internal(format!("read preset document: {error}")))?;
        let mut spec = crate::preset::parse(&document)?;
        let mut candidate = format!("{}-copy", spec.id);
        let mut index = 2;
        while crate::db::repository::presets::get(guard.connection(), &candidate)?.is_some() {
            candidate = format!("{}-copy-{index}", spec.id);
            index += 1;
        }
        spec.id = candidate;
        spec.name = format!("{} (copy)", spec.name);
        let hash = crate::preset::content_hash(&spec)?;
        crate::db::repository::presets::upsert(
            guard.connection(),
            &crate::db::repository::presets::NewPreset {
                id: spec.id.clone(),
                owner: "user".into(),
                name: spec.name.clone(),
                kind: preset_kind_str(spec.kind).into(),
                version: spec.version.clone(),
                description: spec.description.clone(),
                document_json: serde_json::to_string(&spec).map_err(|error| {
                    CommandError::internal(format!("serialize preset: {error}"))
                })?,
                content_hash: hash,
            },
        )?;
        to_value(crate::commands::presets::PresetDocument {
            id: spec.id.clone(),
            name: spec.name.clone(),
            kind: spec.kind,
            version: spec.version.clone(),
            hash: crate::preset::content_hash(&spec)?,
            builtin: false,
            document: to_value(&spec)?,
        })
    }

    fn presets_delete(
        &self,
        request: &crate::commands::presets::DeletePresetRequest,
    ) -> Result<Value, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let row = crate::db::repository::presets::get(guard.connection(), &request.id)?
            .filter(|row| row.deleted_at.is_none())
            .ok_or_else(|| CommandError::not_found("Preset not found"))?;
        if row.owner == "builtin" {
            return Err(CommandError::forbidden(
                "Built-in presets cannot be deleted; duplicate one instead",
            ));
        }
        if crate::db::repository::presets::is_referenced(guard.connection(), &request.id)?
            && !request.confirm_in_use
        {
            return Err(CommandError::conflict(
                "This preset is referenced by history; confirm deletion to continue",
            ));
        }
        crate::db::repository::presets::delete(guard.connection(), &request.id)?;
        to_value(())
    }

    fn presets_import(
        &self,
        request: &crate::commands::presets::PresetPathRequest,
    ) -> Result<Value, CommandError> {
        let path = PathBuf::from(&request.path);
        let metadata = std::fs::metadata(&path).map_err(|error| CommandError::from_io(&error))?;
        if metadata.len() > 256 * 1024 {
            return Err(CommandError::validation(
                "Preset files must be smaller than 256 KB",
            ));
        }
        let text = std::fs::read_to_string(&path).map_err(|error| CommandError::from_io(&error))?;
        let value: Value = serde_json::from_str(&text).map_err(|error| {
            CommandError::validation(format!("Preset is not valid JSON: {error}"))
        })?;
        let document = value.get("document").cloned().unwrap_or(value);
        let mut spec = crate::preset::parse(&document)?;
        crate::preset::validate(&spec)?;
        {
            let guard = self
                .database
                .lock()
                .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
            if let Some(existing) =
                crate::db::repository::presets::get(guard.connection(), &spec.id)?
                && existing.owner == "builtin"
            {
                let mut candidate = format!("{}-imported", spec.id);
                let mut index = 2;
                while crate::db::repository::presets::get(guard.connection(), &candidate)?.is_some()
                {
                    candidate = format!("{}-imported-{index}", spec.id);
                    index += 1;
                }
                spec.id = candidate;
            }
        }
        self.presets_save(&crate::commands::presets::SavePresetRequest {
            document: to_value(&spec)?,
        })
    }

    fn presets_export(
        &self,
        request: &crate::commands::presets::PresetPathRequest,
    ) -> Result<Value, CommandError> {
        let document = self.preset_document(&request.id)?.document;
        let path = PathBuf::from(&request.path);
        if let Some(parent) = path.parent()
            && !parent.is_dir()
        {
            return Err(CommandError::validation(format!(
                "Output folder does not exist: {}",
                parent.display()
            )));
        }
        let json = serde_json::to_string_pretty(&document)
            .map_err(|error| CommandError::internal(format!("serialize preset: {error}")))?;
        let temporary = path.with_extension("partial");
        std::fs::write(&temporary, &json).map_err(|error| CommandError::from_io(&error))?;
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| CommandError::from_io(&error))?;
        }
        std::fs::rename(&temporary, &path).map_err(|error| CommandError::from_io(&error))?;
        to_value(crate::commands::presets::PresetExportSummary {
            path: path.display().to_string(),
            bytes: json.len() as u64,
        })
    }

    fn jobs_status(&self) -> Result<Value, CommandError> {
        self.db(|connection| {
            let mut queued = 0;
            let mut running = 0;
            let mut waiting = 0;
            for row in crate::db::repository::jobs::list_active(connection)? {
                match row.state.as_str() {
                    "queued" => queued += 1,
                    "running" => running += 1,
                    "waiting" => waiting += 1,
                    _ => {}
                }
            }
            Ok(crate::commands::jobs::QueueStatus {
                paused: self.jobs_paused.load(Ordering::Relaxed),
                queued,
                running,
                waiting,
            })
        })
    }

    fn jobs_cancel(&self, id: &str) -> Result<Value, CommandError> {
        let job = job_queue::get(&self.database, id)?;
        match job.state {
            crate::commands::jobs::JobState::Running => {
                // The runner checks this flag between items and aborts the
                // active compose call.
                self.queue.request_cancel(id);
                return to_value(job_queue::get(&self.database, id)?);
            }
            crate::commands::jobs::JobState::Succeeded
            | crate::commands::jobs::JobState::Failed
            | crate::commands::jobs::JobState::Cancelled => {
                return Err(CommandError::conflict("That job has already finished"));
            }
            _ => {}
        }
        job_queue::cancel(&self.database, id)?;
        to_value(job_queue::get(&self.database, id)?)
    }

    fn status_snapshot(&self) -> Result<Value, CommandError> {
        let (endpoint, token) = {
            let store = self
                .settings
                .lock()
                .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
            (store.get().endpoint, store.token().map(ToOwned::to_owned))
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CommandError::internal(format!("runtime: {error}")))?;
        let probe = runtime.block_on(crate::commands::status::probe_endpoint(
            &endpoint,
            token.as_deref(),
        ));
        let cache_bytes = crate::paths::directory_size(&self.paths.cache_dir);
        let data_bytes = crate::paths::directory_size(&self.paths.data_dir);
        let active_jobs = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE state IN ('queued','running','waiting')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0);
        to_value(crate::commands::status::StatusSnapshot {
            endpoint,
            reachable: probe.reachable,
            latency_ms: probe.latency_ms,
            loaded_models: probe.loaded_models,
            active_jobs,
            cache_bytes,
            data_bytes,
            portable: self.paths.portable,
            storage_note: self.paths.fallback_reason.clone(),
            checked_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        })
    }

    fn app_info(&self) -> Result<Value, CommandError> {
        to_value(crate::commands::diagnostics::AppInfo {
            name: "Prompt Studio".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            data_dir: self.paths.data_dir.display().to_string(),
            cache_dir: self.paths.cache_dir.display().to_string(),
        })
    }

    fn diagnostics_export(&self, request: &Value) -> Result<Value, CommandError> {
        let request: crate::commands::diagnostics::DiagnosticsRequest = parse(request)?;
        let (settings_value, endpoint, token) = {
            let store = self
                .settings
                .lock()
                .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
            let snapshot = store.get();
            (
                to_value(&snapshot)?,
                snapshot.endpoint,
                store.token().map(ToOwned::to_owned),
            )
        };
        let app_info = crate::commands::diagnostics::AppInfo {
            name: "Prompt Studio".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            data_dir: self.paths.data_dir.display().to_string(),
            cache_dir: self.paths.cache_dir.display().to_string(),
        };
        let bundle = self.with_runtime(|runtime| {
            runtime.block_on(crate::diagnostics::collect(
                &app_info,
                settings_value,
                &endpoint,
                token.as_deref(),
                &self.paths,
                request.include_prompt_text,
            ))
        });
        let bundle = bundle?;
        let bytes = crate::diagnostics::export(&bundle, Path::new(&request.output_path))
            .map_err(CommandError::internal)?;
        to_value(crate::commands::diagnostics::DiagnosticsSummary {
            output_path: request.output_path,
            bytes,
        })
    }
}

// ---- library helpers -------------------------------------------------------

fn generation_record(
    connection: &rusqlite::Connection,
    row: &crate::db::repository::generations::GenerationRow,
) -> Result<crate::commands::library::GenerationRecord, CommandError> {
    use crate::commands::library::GenerationRecord;
    Ok(GenerationRecord {
        id: row.id.clone(),
        parent_id: row.parent_id.clone(),
        asset_id: row.asset_id.clone(),
        edit_id: row.edit_id.clone(),
        prompt: row.prompt.clone(),
        negative: row.negative.clone(),
        params: serde_json::from_str(&row.params_json).unwrap_or_else(|_| json!({})),
        guidance: serde_json::from_str(&row.guidance_json).unwrap_or_default(),
        description: row
            .description_json
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok()),
        preset_id: row.preset_id.clone(),
        preset_version: row.preset_version.clone(),
        preset_hash: row.preset_hash.clone(),
        models: serde_json::from_str(&row.models_json).unwrap_or_default(),
        seed: row.seed.and_then(|seed| u64::try_from(seed).ok()),
        status: row.status.clone(),
        error: row.error.clone(),
        favorite: row.favorite,
        rating: row.rating.and_then(|rating| u8::try_from(rating).ok()),
        notes: row.notes.clone(),
        tags: crate::db::repository::tags::list_for_generation(connection, &row.id)?,
        collections: gen_repo::collections_for_generation(connection, &row.id)?,
        created_at: row.created_at.clone(),
        deleted_at: row.deleted_at.clone(),
        title: crate::db::repository::titles::get(connection, &row.id)?,
    })
}

impl NativeState {
    fn library_query(
        &self,
        query: &crate::commands::library::GenerationQuery,
    ) -> Result<Value, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let connection = guard.connection();
        let filter = crate::db::repository::generations::GenerationFilter {
            text: query.text.clone(),
            preset_id: query.preset_id.clone(),
            kind: query
                .kind
                .as_ref()
                .map(|kind| match kind {
                    crate::commands::presets::PresetKind::Image => "image",
                    crate::commands::presets::PresetKind::Video => "video",
                    crate::commands::presets::PresetKind::Edit => "edit",
                })
                .map(ToOwned::to_owned),
            model: query.model.clone(),
            favorite: query.favorite,
            status: query.status.clone(),
            tags: query.tags.clone(),
            collection_id: query.collection_id.clone(),
            from: query.from.clone(),
            to: query.to.clone(),
            ids: if query.ids.is_empty() {
                None
            } else {
                Some(query.ids.clone())
            },
            include_deleted: query.include_deleted,
            only_deleted: query.only_deleted,
            sort: query.sort.clone(),
            limit: query.limit,
            offset: query.offset,
        };
        let (rows, total) = gen_repo::query(connection, &filter)?;
        let items = rows
            .iter()
            .map(|row| generation_record(connection, row))
            .collect::<Result<Vec<_>, _>>()?;
        to_value(crate::commands::Page {
            items,
            total,
            offset: query.offset,
            limit: query.limit,
        })
    }

    fn library_get(&self, id: &str) -> Result<Value, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let row = gen_repo::get(guard.connection(), id)?
            .ok_or_else(|| CommandError::not_found("Generation not found"))?;
        to_value(crate::commands::library::GenerationDetail {
            record: generation_record(guard.connection(), &row)?,
            raw_response: row.raw_response,
            tokens_in: row.tokens_in.and_then(|value| u64::try_from(value).ok()),
            tokens_out: row.tokens_out.and_then(|value| u64::try_from(value).ok()),
            latency_ms: row.latency_ms.and_then(|value| u64::try_from(value).ok()),
            cache_key: row.cache_key,
            references: Vec::new(),
        })
    }

    fn library_compare(
        &self,
        request: &crate::commands::library::CompareRequest,
    ) -> Result<Value, CommandError> {
        if request.ids.len() < 2 || request.ids.len() > 4 {
            return Err(CommandError::validation(
                "Compare needs between 2 and 4 generations",
            ));
        }
        let mut unique = request.ids.clone();
        unique.sort();
        unique.dedup();
        if unique.len() != request.ids.len() {
            return Err(CommandError::validation("Compare ids must be distinct"));
        }
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let mut details = Vec::new();
        for id in &request.ids {
            let row = gen_repo::get(guard.connection(), id)?
                .ok_or_else(|| CommandError::not_found(format!("Generation {id} not found")))?;
            details.push(crate::commands::library::GenerationDetail {
                record: generation_record(guard.connection(), &row)?,
                raw_response: row.raw_response,
                tokens_in: row.tokens_in.and_then(|value| u64::try_from(value).ok()),
                tokens_out: row.tokens_out.and_then(|value| u64::try_from(value).ok()),
                latency_ms: row.latency_ms.and_then(|value| u64::try_from(value).ok()),
                cache_key: row.cache_key,
                references: Vec::new(),
            });
        }
        to_value(details)
    }

    fn library_favorite(
        &self,
        request: &crate::commands::library::FavoriteRequest,
    ) -> Result<Value, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let changed = if request.asset {
            asset_repo::set_favorite(guard.connection(), &request.id, request.favorite)?
        } else {
            gen_repo::set_favorite(guard.connection(), &request.id, request.favorite)?
        };
        if !changed {
            return Err(CommandError::not_found("Record not found"));
        }
        to_value(())
    }

    fn library_update(
        &self,
        request: &crate::commands::library::UpdateGenerationRequest,
    ) -> Result<Value, CommandError> {
        if request.ids.is_empty() {
            return Err(CommandError::validation("Select at least one generation"));
        }
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let connection = guard.connection();
        let mut changed = 0;
        for id in &request.ids {
            if let Some(favorite) = request.favorite {
                changed += usize::from(gen_repo::set_favorite(connection, id, favorite)?);
            }
            if request.clear_rating {
                gen_repo::set_rating(connection, id, None)?;
            } else if let Some(rating) = request.rating {
                gen_repo::set_rating(connection, id, Some(i64::from(rating)))?;
            }
            if let Some(notes) = &request.notes {
                let notes = if notes.trim().is_empty() {
                    None
                } else {
                    Some(notes.as_str())
                };
                gen_repo::set_notes(connection, id, notes)?;
            }
            if !request.add_tags.is_empty() || !request.remove_tags.is_empty() {
                let mut tags = crate::db::repository::tags::list_for_generation(connection, id)?;
                for tag in &request.add_tags {
                    if !tags
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(tag))
                    {
                        tags.push(tag.clone());
                    }
                }
                tags.retain(|tag| {
                    !request
                        .remove_tags
                        .iter()
                        .any(|removed| removed.eq_ignore_ascii_case(tag))
                });
                gen_repo::set_tags(connection, id, &tags)?;
            }
            if let Some(collection_id) = &request.collection_id {
                match request.collection_action.as_deref().unwrap_or("add") {
                    "add" => {
                        crate::db::repository::collections::add_item(
                            connection,
                            collection_id,
                            id,
                        )?;
                        changed += 1;
                    }
                    "remove" => {
                        crate::db::repository::collections::remove_item(
                            connection,
                            collection_id,
                            id,
                        )?;
                        changed += 1;
                    }
                    other => {
                        return Err(CommandError::validation(format!(
                            "Unknown collection action: {other}"
                        )));
                    }
                }
            }
        }
        to_value(changed)
    }

    fn library_tags(&self) -> Result<Value, CommandError> {
        self.db(|connection| {
            Ok(crate::db::repository::tags::list_counts(connection)?
                .into_iter()
                .map(|(name, count)| crate::commands::library::TagSummary { name, count })
                .collect::<Vec<_>>())
        })
    }

    fn library_collections(&self) -> Result<Value, CommandError> {
        use std::collections::BTreeMap;
        self.db(|connection| {
            let counts: BTreeMap<String, i64> =
                crate::db::repository::collections::counts(connection)?
                    .into_iter()
                    .collect();
            Ok(crate::db::repository::collections::list(connection)?
                .into_iter()
                .map(|row| crate::commands::library::CollectionSummary {
                    id: row.id.clone(),
                    name: row.name,
                    created_at: row.created_at,
                    count: counts.get(&row.id).copied().unwrap_or_default(),
                })
                .collect::<Vec<_>>())
        })
    }

    fn library_collection_create(
        &self,
        request: &crate::commands::library::NameRequest,
    ) -> Result<Value, CommandError> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err(CommandError::validation("Collection name is required"));
        }
        self.db(|connection| {
            let row = crate::db::repository::collections::create(
                connection,
                &Uuid::now_v7().to_string(),
                name,
            )?;
            Ok(crate::commands::library::CollectionSummary {
                id: row.id,
                name: row.name,
                created_at: row.created_at,
                count: 0,
            })
        })
    }

    fn library_collection_rename(
        &self,
        request: &crate::commands::library::CollectionRequest,
    ) -> Result<Value, CommandError> {
        self.db(|connection| {
            if !crate::db::repository::collections::rename(connection, &request.id, &request.name)?
            {
                return Err(CommandError::not_found("Collection not found"));
            }
            let row = crate::db::repository::collections::get(connection, &request.id)?
                .ok_or_else(|| CommandError::not_found("Collection not found"))?;
            Ok(crate::commands::library::CollectionSummary {
                id: row.id,
                name: row.name,
                created_at: row.created_at,
                count: 0,
            })
        })
    }

    fn library_collection_delete(&self, id: &str) -> Result<Value, CommandError> {
        self.db(|connection| {
            if !crate::db::repository::collections::delete(connection, id)? {
                return Err(CommandError::not_found("Collection not found"));
            }
            Ok(())
        })
    }

    fn library_collection_assign(
        &self,
        request: &crate::commands::library::CollectionAssignRequest,
    ) -> Result<Value, CommandError> {
        self.db(|connection| {
            let mut changed = 0usize;
            for id in &request.generation_ids {
                if request.add {
                    crate::db::repository::collections::add_item(
                        connection,
                        &request.collection_id,
                        id,
                    )?;
                } else {
                    crate::db::repository::collections::remove_item(
                        connection,
                        &request.collection_id,
                        id,
                    )?;
                }
                changed += 1;
            }
            Ok(changed)
        })
    }

    fn library_versions(&self, id: &str) -> Result<Value, CommandError> {
        self.db(|connection| {
            let rows = gen_repo::version_tree(connection, id)?;
            if rows.is_empty() {
                return Err(CommandError::not_found("Generation not found"));
            }
            Ok(crate::commands::library::VersionTree {
                generations: rows
                    .into_iter()
                    .map(|row| crate::commands::library::VersionNode {
                        id: row.id,
                        parent_id: row.parent_id,
                        prompt: row.prompt,
                        negative: row.negative,
                        seed: row.seed.and_then(|value| u64::try_from(value).ok()),
                        status: row.status,
                        preset_id: row.preset_id,
                        created_at: row.created_at,
                    })
                    .collect(),
            })
        })
    }

    fn library_diff(
        &self,
        request: &crate::commands::library::DiffRequest,
    ) -> Result<Value, CommandError> {
        let left = self.library_get(&request.left_id)?;
        let right = self.library_get(&request.right_id)?;
        let left_obj = left
            .as_object()
            .ok_or_else(|| CommandError::internal("detail is not an object"))?;
        let right_obj = right
            .as_object()
            .ok_or_else(|| CommandError::internal("detail is not an object"))?;
        let fields = ["prompt", "negative", "seed", "presetId", "status"];
        let mut diffs = Vec::new();
        for field in fields {
            let l = left_obj.get(field).cloned().unwrap_or(Value::Null);
            let r = right_obj.get(field).cloned().unwrap_or(Value::Null);
            diffs.push(crate::commands::library::FieldDiff {
                field: field.to_string(),
                left: l.clone(),
                right: r.clone(),
                changed: l != r,
            });
        }
        to_value(diffs)
    }

    fn library_restore_workspace(&self, id: &str) -> Result<Value, CommandError> {
        let detail = self.library_get(id)?;
        let record = detail.get("record").cloned().unwrap_or(detail);
        let guidance: crate::commands::Guidance =
            serde_json::from_value(record.get("guidance").cloned().unwrap_or_else(|| json!({})))
                .unwrap_or_default();
        to_value(crate::commands::library::RestoreResult {
            draft: crate::commands::workspace::WorkspaceDraft {
                asset_id: record
                    .get("assetId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                edit_ops: None,
                selected_preset_id: record
                    .get("presetId")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                guidance,
                params: None,
                updated_at_ms: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            },
            params: record.get("params").cloned().unwrap_or_else(|| json!({})),
        })
    }

    fn library_trash(
        &self,
        request: &crate::commands::library::TrashRequest,
        restore: bool,
    ) -> Result<Value, CommandError> {
        let mut generations = 0usize;
        let mut assets = 0usize;
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let connection = guard.connection();
        // One transaction: thousands of single-row commits are very slow.
        begin_batch(connection)?;
        let batch = (|| -> Result<(), CommandError> {
            for id in &request.generation_ids {
                let changed = if restore {
                    gen_repo::restore(connection, id)?
                } else {
                    gen_repo::soft_delete(connection, id)?
                };
                generations += usize::from(changed);
            }
            for id in &request.asset_ids {
                let changed = if restore {
                    asset_repo::restore(connection, id)?
                } else {
                    asset_repo::soft_delete(connection, id)?
                };
                assets += usize::from(changed);
            }
            Ok(())
        })();
        finish_batch(connection, batch)?;
        to_value(crate::commands::library::TrashSummary {
            generations,
            assets,
        })
    }

    /// Picks a set's images (requested, defaults, else one random), registers
    /// them as normal media (deduplicated by hash) and returns them with asset ids.
    fn characters_load(&self, args: &Value) -> Result<Value, CommandError> {
        let id: String = arg(args, "id")?;
        let requested: Vec<String> = args
            .get("imageIds")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        let avoid: Option<String> = args
            .get("avoid")
            .and_then(Value::as_str)
            .map(str::to_string);
        let (head, picked) = {
            let guard = self
                .database
                .lock()
                .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
            let connection = guard.connection();
            let head = crate::characters::get(connection, &id)?;
            let picked =
                crate::characters::pick_images(connection, &id, &requested, avoid.as_deref())?;
            (head, picked)
        };
        let data_dir = self.paths.data_dir.clone();
        let mut picked = picked;
        picked
            .iter_mut()
            .for_each(|image| crate::characters::absolutize(image, &data_dir));
        let paths: Vec<String> = picked
            .iter()
            .filter_map(|image| image["path"].as_str().map(str::to_string))
            .collect();
        let imported = if paths.is_empty() {
            Vec::new()
        } else {
            self.import_paths(&paths)?
        };
        let mut loaded = Vec::new();
        for mut image in picked {
            let path = image["path"].as_str().unwrap_or_default().to_string();
            let asset = imported
                .iter()
                .find(|result| same_path(&result.path, &path))
                .and_then(|result| result.asset_id.clone());
            if let Some(asset_id) = asset {
                image["assetId"] = json!(asset_id);
                loaded.push(image);
            }
        }
        Ok(json!({
            "id": head["id"],
            "name": head["name"],
            "description": head["description"],
            "images": loaded,
        }))
    }

    fn library_assets_trash(&self) -> Result<Value, CommandError> {
        self.db(|connection| {
            Ok(asset_repo::list_deleted(connection)?
                .into_iter()
                .map(|row| crate::commands::library::AssetTrashEntry {
                    id: row.id,
                    display_name: row.display_name,
                    kind: row.kind,
                    source_path: row.source_path,
                    deleted_at: None,
                })
                .collect::<Vec<_>>())
        })
    }

    fn library_purge(
        &self,
        request: &crate::commands::library::PurgeRequest,
    ) -> Result<Value, CommandError> {
        if !request.confirm {
            return Err(CommandError::validation(
                "Confirm purge before deleting data",
            ));
        }
        let mut generations = 0usize;
        let mut assets = 0usize;
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let connection = guard.connection();
        begin_batch(connection)?;
        let batch = (|| -> Result<(), CommandError> {
            for id in &request.generation_ids {
                generations += usize::from(gen_repo::hard_delete(connection, id)?);
            }
            for id in &request.asset_ids {
                assets += usize::from(asset_repo::hard_delete(connection, id)?);
            }
            Ok(())
        })();
        finish_batch(connection, batch)?;
        to_value(crate::commands::library::PurgeSummary {
            generations,
            assets,
            bytes_reclaimed: 0,
        })
    }

    fn library_export(
        &self,
        request: &crate::commands::library::ExportRequest,
    ) -> Result<Value, CommandError> {
        to_value(crate::commands::library::export(request, &self.database)?)
    }

    fn library_import(
        &self,
        request: &crate::commands::library::ImportRequest,
    ) -> Result<Value, CommandError> {
        to_value(crate::commands::library::import(request, &self.database)?)
    }

    fn library_rerun(
        &self,
        request: &crate::commands::library::RerunRequest,
    ) -> Result<Value, CommandError> {
        to_value(crate::commands::library::enqueue_rerun(
            request,
            &self.database,
        )?)
    }

    fn library_backup(
        &self,
        request: &crate::commands::library::BackupRequest,
    ) -> Result<Value, CommandError> {
        to_value(crate::commands::library::backup(
            request,
            &self.database,
            &self.paths,
        )?)
    }

    fn library_restore(
        &self,
        request: &crate::commands::library::RestoreRequest,
    ) -> Result<Value, CommandError> {
        to_value(crate::commands::library::restore(
            request,
            &self.database,
            &self.paths,
        )?)
    }
}

// ---- runtime helpers -------------------------------------------------------

impl NativeState {
    fn models_list(&self) -> Result<Value, CommandError> {
        let (endpoint, token) = {
            let store = self
                .settings
                .lock()
                .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
            (store.get().endpoint, store.token().map(ToOwned::to_owned))
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CommandError::internal(format!("runtime: {error}")))?;
        let models = runtime.block_on(
            LmStudioClient::new(endpoint, token)
                .map_err(|error| CommandError::internal(error.to_string()))?
                .list_models(),
        )?;
        to_value(models)
    }

    fn with_runtime<T, F>(&self, f: F) -> Result<T, CommandError>
    where
        F: FnOnce(&tokio::runtime::Runtime) -> T,
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CommandError::internal(format!("runtime: {error}")))?;
        Ok(f(&runtime))
    }

    fn models_load(&self, request: &Value) -> Result<Value, CommandError> {
        let request: crate::commands::models::LoadModelRequest = parse(request)?;
        let (endpoint, token) = {
            let store = self
                .settings
                .lock()
                .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
            (store.get().endpoint, store.token().map(ToOwned::to_owned))
        };
        let client = LmStudioClient::new(endpoint, token)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let available = self
            .with_runtime(|runtime| runtime.block_on(client.list_models()))
            .ok()
            .and_then(|result| result.ok());
        if let Some(models) = &available
            && !models.iter().any(|model| model.id == request.model)
        {
            return Err(CommandError::model_missing(format!(
                "Model '{}' is not available in LM Studio",
                request.model
            )));
        }
        let result = self
            .with_runtime(|runtime| runtime.block_on(self.manager.load(request.role, &request)))?;
        to_value(result)
    }

    fn models_set_roles(&self, request: &Value) -> Result<Value, CommandError> {
        let request: crate::commands::models::SetRolesRequest = parse(request)?;
        {
            let mut store = self
                .settings
                .lock()
                .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
            store.update(crate::commands::settings::SettingsPatch {
                describer_model: Some(request.describer.clone()),
                composer_model: Some(request.composer.clone()),
                ..Default::default()
            })?;
        }
        self.manager.set_roles(request.describer, request.composer);
        self.models_status()
    }

    fn models_set_policy(&self, request: &Value) -> Result<Value, CommandError> {
        let request: crate::commands::models::SetPolicyRequest = parse(request)?;
        {
            let mut store = self
                .settings
                .lock()
                .map_err(|_| CommandError::internal("Settings lock is poisoned"))?;
            store.update(crate::commands::settings::SettingsPatch {
                keep_warm: Some(request.keep_warm),
                describer_concurrency: Some(request.describer_concurrency),
                ttl_seconds: Some(request.ttl_seconds),
                ..Default::default()
            })?;
        }
        self.manager.set_keep_warm(request.keep_warm);
        self.manager
            .set_ttl(Duration::from_secs(request.ttl_seconds));
        self.manager
            .set_describer_concurrency(request.describer_concurrency);
        self.models_status()
    }

    fn models_status(&self) -> Result<Value, CommandError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CommandError::internal(format!("runtime: {error}")))?;
        to_value(runtime.block_on(self.manager.status()))
    }
}

// ---- dataset helpers -------------------------------------------------------

impl NativeState {
    fn datasets_list(&self) -> Result<Value, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        to_value(
            crate::db::repository::datasets::list(guard.connection())?
                .into_iter()
                .map(|row| crate::commands::datasets::Dataset {
                    id: row.id,
                    name: row.name,
                    source_path: row.source_path,
                    row_count: row.row_count,
                    created_at: row.created_at,
                    mapping: row
                        .mapping_json
                        .as_deref()
                        .and_then(|text| serde_json::from_str(text).ok())
                        .unwrap_or_default(),
                    export_columns: row
                        .export_columns_json
                        .as_deref()
                        .and_then(|text| serde_json::from_str(text).ok())
                        .unwrap_or_default(),
                })
                .collect::<Vec<_>>(),
        )
    }

    fn datasets_profiles(&self) -> Result<Value, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let profiles = crate::db::repository::import_profiles::list(guard.connection())?
            .into_iter()
            .map(|row| {
                Ok(crate::commands::datasets::ImportProfile {
                    name: row.name,
                    mapping: serde_json::from_str(&row.mapping_json).map_err(|error| {
                        CommandError::internal(format!("read import profile: {error}"))
                    })?,
                })
            })
            .collect::<Result<Vec<_>, CommandError>>()?;
        to_value(profiles)
    }

    fn datasets_delete(&self, id: &str) -> Result<Value, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        if !crate::db::repository::datasets::delete(guard.connection(), id)? {
            return Err(CommandError::not_found("Dataset not found"));
        }
        to_value(())
    }

    fn datasets_preview(
        &self,
        request: &crate::commands::datasets::DatasetPreviewRequest,
    ) -> Result<Value, CommandError> {
        const PREVIEW_ROWS: usize = 50;
        let path = PathBuf::from(&request.path);
        if !path.exists() {
            return Err(CommandError::not_found("Dataset file or folder not found"));
        }
        let info = crate::dataset::inspect(&path)?;
        let headers = crate::dataset::headers(&path)?;
        let mut mapping = request.mapping.clone().unwrap_or_default();
        if mapping.is_empty() {
            mapping = crate::dataset::auto_mapping(&headers);
        }
        if mapping.source_for("prompt").is_none() {
            return Err(CommandError::validation(
                "Map a source field to the prompt before importing",
            ));
        }
        let mut rows = Vec::new();
        let summary = crate::dataset::stream(&path, &mapping, Some(PREVIEW_ROWS), |row| {
            rows.push(row);
            Ok(())
        })?;
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let profiles = crate::db::repository::import_profiles::list(guard.connection())?
            .into_iter()
            .map(|row| {
                Ok(crate::commands::datasets::ImportProfile {
                    name: row.name,
                    mapping: serde_json::from_str(&row.mapping_json).map_err(|error| {
                        CommandError::internal(format!("read import profile: {error}"))
                    })?,
                })
            })
            .collect::<Result<Vec<_>, CommandError>>()?;
        to_value(crate::commands::datasets::DatasetPreview {
            path: request.path.clone(),
            format: info.format.as_str().into(),
            encoding: info.encoding,
            delimiter: (info.delimiter as char).to_string(),
            headers,
            rows,
            total_rows: summary.total,
            mapping,
            profiles,
            issues: summary.issues,
        })
    }

    fn datasets_rows(
        &self,
        request: &crate::commands::datasets::DatasetRowsRequest,
    ) -> Result<Value, CommandError> {
        let guard = self
            .database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let connection = guard.connection();
        let row = crate::db::repository::datasets::get(connection, &request.dataset_id)?
            .ok_or_else(|| CommandError::not_found("Dataset not found"))?;
        let offset = request.offset.unwrap_or(0).max(0) as usize;
        let limit = request.limit.unwrap_or(200).clamp(1, 1000) as usize;
        let items = crate::db::repository::datasets::list_items(connection, &request.dataset_id)?;
        let total = items.len() as i64;
        let rows = items
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|item| {
                let data: Option<crate::dataset::MappedRow> =
                    serde_json::from_str(&item.data_json).ok();
                let media_status = if item.media_asset_ids.is_empty() {
                    "none"
                } else {
                    "imported"
                };
                let media_thumbnails = item
                    .media_asset_ids
                    .iter()
                    .filter_map(|id| asset_repo::get(connection, id).ok().flatten())
                    .map(|asset| asset.thumbnail_path.unwrap_or(asset.source_path))
                    .collect::<Vec<_>>();
                crate::commands::datasets::DatasetRowView {
                    id: item.id,
                    index: item.row_index,
                    status: item.status,
                    generation_id: item.generation_id,
                    error: item.error,
                    prompt: data
                        .as_ref()
                        .map(|value| value.prompt.clone())
                        .unwrap_or_default(),
                    media_path: data.as_ref().and_then(|value| value.media_path.clone()),
                    media_missing: data.as_ref().is_some_and(|value| value.media_missing),
                    media_asset_ids: item.media_asset_ids,
                    media_status: media_status.into(),
                    media_thumbnails,
                    fields: data
                        .as_ref()
                        .map(|value| value.fields.clone())
                        .unwrap_or_default(),
                    params: data
                        .as_ref()
                        .map(|value| value.params.clone())
                        .unwrap_or_else(|| json!({})),
                    tags: data.map(|value| value.tags).unwrap_or_default(),
                }
            })
            .collect();
        let counts =
            crate::db::repository::datasets::status_counts(connection, &request.dataset_id)?
                .into_iter()
                .collect();
        to_value(crate::commands::datasets::DatasetRows {
            dataset: crate::commands::datasets::Dataset {
                id: row.id,
                name: row.name,
                source_path: row.source_path,
                row_count: row.row_count,
                created_at: row.created_at,
                mapping: row
                    .mapping_json
                    .as_deref()
                    .and_then(|text| serde_json::from_str(text).ok())
                    .unwrap_or_default(),
                export_columns: row
                    .export_columns_json
                    .as_deref()
                    .and_then(|text| serde_json::from_str(text).ok())
                    .unwrap_or_default(),
            },
            counts,
            rows,
            total,
        })
    }
}

// ---- pipeline / drag helpers ------------------------------------------------

impl NativeState {
    fn pipeline_describe(&self, request: DescribeRequest) -> Result<Value, CommandError> {
        let outcome = self.with_runtime(|runtime| {
            runtime.block_on(crate::commands::pipeline::describe(
                request,
                &self.database,
                &self.paths,
                &self.tools,
                &self.manager,
                &self.settings,
            ))
        })?;
        to_value(outcome?)
    }

    fn pipeline_compose(&self, request: ComposeRequest) -> Result<Value, CommandError> {
        // No webview in the native shell: streamed tokens are discarded.
        let mut noop = |_: &str| {};
        let outcome = self.with_runtime(|runtime| {
            runtime.block_on(crate::commands::pipeline::compose(
                request,
                &self.database,
                &self.paths,
                &self.tools,
                &self.manager,
                &self.pipeline_control,
                &self.settings,
                &mut noop,
            ))
        })?;
        to_value(outcome?)
    }

    fn pipeline_batch(&self, request: BatchRequest) -> Result<Value, CommandError> {
        let outcome = self.with_runtime(|runtime| {
            runtime.block_on(crate::commands::pipeline::batch(
                request,
                &self.database,
                &self.paths,
                &self.tools,
                &self.manager,
                &self.settings,
            ))
        })?;
        to_value(outcome?)
    }

    fn drag_prepare(&self, request: &DragPrepareRequest) -> Result<Value, CommandError> {
        drag_out::cleanup(&self.paths.cache_dir, SystemTime::now());
        let transfer = Arc::new(Transfer::default());
        {
            let mut current = self
                .drag_control
                .current
                .lock()
                .map_err(|_| CommandError::internal("Drag lock is poisoned"))?;
            if current.is_some() {
                return Err(CommandError::conflict(
                    "Another drag file is still being prepared",
                ));
            }
            *current = Some(Arc::clone(&transfer));
        }
        let template = self
            .settings
            .lock()
            .map_err(|_| CommandError::internal("Settings lock is poisoned"))?
            .get()
            .naming_template;
        let result = {
            let guard = self
                .database
                .lock()
                .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
            let mut on_progress = |_progress: f64| {};
            drag_out::prepare(
                guard.connection(),
                request,
                &template,
                &self.paths.cache_dir,
                &transfer,
                &mut on_progress,
            )
        };
        if let Ok(mut current) = self.drag_control.current.lock() {
            *current = None;
        }
        match result {
            Ok(prepared) => to_value(DragPayload {
                files: prepared.files,
                prepared_dir: prepared.prepared_dir,
                suggested_name: prepared.suggested_name,
            }),
            Err(error) => {
                if transfer.cancel.load(Ordering::Relaxed) {
                    Err(CommandError::cancelled())
                } else {
                    Err(error)
                }
            }
        }
    }
}

fn preset_kind(value: &str) -> Result<crate::commands::presets::PresetKind, CommandError> {
    match value {
        "image" => Ok(crate::commands::presets::PresetKind::Image),
        "video" => Ok(crate::commands::presets::PresetKind::Video),
        "edit" => Ok(crate::commands::presets::PresetKind::Edit),
        other => Err(CommandError::internal(format!(
            "Unknown preset kind in database: {other}"
        ))),
    }
}

fn preset_kind_str(kind: crate::commands::presets::PresetKind) -> &'static str {
    match kind {
        crate::commands::presets::PresetKind::Image => "image",
        crate::commands::presets::PresetKind::Video => "video",
        crate::commands::presets::PresetKind::Edit => "edit",
    }
}

fn to_asset(row: asset_repo::AssetRow) -> Asset {
    Asset {
        id: row.id,
        kind: if row.kind == "video" {
            crate::commands::assets::AssetKind::Video
        } else {
            crate::commands::assets::AssetKind::Image
        },
        path: row.source_path,
        display_name: row.display_name,
        hash: row.hash,
        width: row.width,
        height: row.height,
        fps: row.fps,
        duration_ms: row.duration_ms.map(|value| value as u64),
        codec: row.codec,
        has_audio: row.has_audio,
        thumbnail_path: row.thumbnail_path,
        proxy_path: row.proxy_path,
        missing: row.missing,
        favorite: row.favorite,
        created_at: row.created_at,
    }
}

fn begin_batch(connection: &rusqlite::Connection) -> Result<(), CommandError> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| CommandError::internal(error.to_string()))
}

fn finish_batch(
    connection: &rusqlite::Connection,
    result: Result<(), CommandError>,
) -> Result<(), CommandError> {
    match result {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(|error| CommandError::internal(error.to_string())),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> Option<NativeState> {
        let mut database = Database::open_in_memory().ok()?;
        database.migrate().ok()?;
        crate::builtin_presets::seed(database.connection()).ok()?;
        let root = std::env::temp_dir().join(format!("native-call-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&root).ok()?;
        let paths = paths::resolve_app_paths(
            Path::new(&root),
            &root.join("data"),
            &root.join("cache"),
            false,
        )
        .ok()?;
        let settings = SettingsStore::load(&paths.config_dir).ok()?;
        let workspace = WorkspaceStore::load(&paths.data_dir).ok()?;
        let snapshot = settings.get();
        let token = settings.token().map(ToOwned::to_owned);
        let client = LmStudioClient::new(snapshot.endpoint.clone(), token).ok()?;
        let manager = ModelManager::new(
            Arc::new(client),
            None,
            crate::commands::models::manager_config(&snapshot),
        );
        Some(NativeState {
            paths,
            database: Mutex::new(database),
            tools: MediaTools::resolve().ok()?,
            import_control: ImportControl::default(),
            access: MediaAccess::new(root.join("cache")),
            settings: Mutex::new(settings),
            workspace: Mutex::new(workspace),
            edit_control: EditControl::default(),
            drag_control: DragControl::default(),
            pipeline_control: PipelineControl::default(),
            jobs_paused: AtomicBool::new(false),
            queue: job_queue::JobQueue::default(),
            manager,
        })
    }

    #[test]
    fn call_dispatches_real_repository_and_preset_logic() {
        let Some(state) = test_state() else {
            eprintln!("skipped: native test fixtures unavailable");
            return;
        };
        let presets = state.call("presets_list", json!({})).expect("presets_list");
        assert!(presets.as_array().is_some_and(|rows| !rows.is_empty()));
        let assets = state.call("assets_list", json!({})).expect("assets_list");
        assert_eq!(assets.as_array().map(Vec::len), Some(0));
        let settings = state.call("settings_get", json!({})).expect("settings_get");
        assert!(settings.get("endpoint").is_some());
        let jobs = state.call("jobs_list", json!({})).expect("jobs_list");
        assert!(jobs.as_array().is_some());
        let error = state
            .call("commands_that_does_not_exist", json!({}))
            .unwrap_err();
        assert_eq!(error.code, "not_implemented");
    }

    fn insert_generation(state: &NativeState, id: &str, prompt: &str) {
        let guard = state.database.lock().expect("db lock");
        crate::db::repository::generations::insert(
            guard.connection(),
            &crate::db::repository::generations::NewGeneration {
                id: id.to_string(),
                parent_id: None,
                asset_id: None,
                edit_id: None,
                description_id: None,
                project_id: None,
                shot_index: None,
                preset_id: "mmh3".into(),
                preset_version: "1.0.0".into(),
                preset_hash: "hash".into(),
                prompt: prompt.into(),
                negative: None,
                params_json: "{}".into(),
                guidance_json: "{}".into(),
                description_json: None,
                seed: None,
                models_json: "[]".into(),
                status: "succeeded".into(),
                error: None,
                raw_response: None,
                tokens_in: None,
                tokens_out: None,
                latency_ms: None,
                cache_key: None,
            },
        )
        .expect("generation");
    }

    #[test]
    fn call_uses_tauri_named_argument_shapes() {
        let Some(state) = test_state() else {
            return;
        };

        // Native startup must seed the same built-ins the webview startup seeds.
        let presets = state.call("presets_list", json!({})).expect("presets_list");
        assert!(
            presets
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["id"] == "mmh3")),
            "built-in mmh3 preset must be seeded"
        );

        insert_generation(&state, "gen-shape", "named argument shape");
        let page = state
            .call(
                "library_query",
                json!({ "query": {
                    "text": null, "presetId": null, "kind": null, "model": null,
                    "favorite": null, "status": null, "tags": [], "collectionId": null,
                    "from": null, "to": null, "ids": [], "includeDeleted": false,
                    "onlyDeleted": false, "offset": 0, "limit": 25, "sort": "newest"
                }}),
            )
            .expect("library_query");
        assert_eq!(page.get("total").and_then(Value::as_i64), Some(1));

        let wrong_shape = state
            .call(
                "library_query",
                json!({ "request": { "offset": 0, "limit": 25, "sort": "newest", "tags": [], "ids": [] }}),
            )
            .unwrap_err();
        assert_eq!(wrong_shape.code, "validation");

        let settings = state
            .call(
                "settings_update",
                json!({ "patch": { "endpoint": "http://127.0.0.1:1234" } }),
            )
            .expect("settings_update");
        assert_eq!(settings["endpoint"], "http://127.0.0.1:1234");

        let status = state.call("jobs_status", json!({})).expect("jobs_status");
        assert_eq!(status["paused"], false);
        state.call("jobs_pause", json!({})).expect("jobs_pause");
        let status = state.call("jobs_status", json!({})).expect("jobs_status");
        assert_eq!(status["paused"], true);
        state.call("jobs_resume", json!({})).expect("jobs_resume");
        let status = state.call("jobs_status", json!({})).expect("jobs_status");
        assert_eq!(status["paused"], false);
    }

    #[test]
    fn wired_pipeline_commands_reach_real_logic() {
        let Some(state) = test_state() else {
            return;
        };
        let describe = state
            .call(
                "pipeline_describe",
                json!({ "request": {
                    "assetId": null, "editId": null, "framePaths": [], "mode": "image",
                    "hint": null, "forceRefresh": false
                }}),
            )
            .unwrap_err();
        assert_eq!(describe.code, "validation");

        let compose = state
            .call(
                "pipeline_compose",
                json!({ "request": {
                    "presetId": "missing", "references": [], "locked": [], "guidance": {},
                    "params": {}, "description": null, "brief": null,
                    "existingPrompt": null, "seed": null, "variations": 1,
                    "bypassCache": false
                }}),
            )
            .unwrap_err();
        assert_eq!(compose.code, "not_found");

        let batch = state
            .call(
                "pipeline_batch",
                json!({ "request": {
                    "items": [], "presetId": "missing", "guidance": {}, "concurrency": 1
                }}),
            )
            .unwrap_err();
        assert_eq!(batch.code, "not_found");
    }

    #[test]
    fn wired_drag_commands_materialize_and_save() {
        let Some(state) = test_state() else {
            return;
        };
        insert_generation(&state, "gen-drag", "a red fox");
        let prepared = state
            .call(
                "drag_prepare",
                json!({ "request": {
                    "generationId": "gen-drag", "assetId": null, "editId": null,
                    "payload": "plainPrompt", "sidecars": false
                }}),
            )
            .expect("drag_prepare");
        let files = prepared
            .get("files")
            .and_then(Value::as_array)
            .expect("files");
        assert_eq!(files.len(), 1);
        let source = files[0].as_str().expect("source path");
        assert_eq!(
            std::fs::read_to_string(source).expect("read prepared"),
            "a red fox"
        );

        let destination =
            std::env::temp_dir().join(format!("native-drag-save-{}.txt", Uuid::now_v7()));
        let saved = state
            .call(
                "drag_save",
                json!({ "request": {
                    "source": source,
                    "outputPath": destination.display().to_string()
                }}),
            )
            .expect("drag_save");
        assert_eq!(saved.as_u64(), Some(9));
        assert_eq!(
            std::fs::read_to_string(&destination).expect("destination"),
            "a red fox"
        );

        let missing = state
            .call(
                "drag_reveal",
                json!({ "request": {
                    "path": destination.with_extension("missing").display().to_string()
                }}),
            )
            .unwrap_err();
        assert_eq!(missing.code, "not_found");
        state.call("drag_cancel", json!({})).expect("drag_cancel");
        let _ = std::fs::remove_file(destination);
    }

    #[test]
    fn wired_library_export_import_and_rerun() {
        let Some(state) = test_state() else {
            return;
        };
        insert_generation(&state, "gen-lib", "a library prompt");
        let output =
            std::env::temp_dir().join(format!("native-library-export-{}.json", Uuid::now_v7()));
        let exported = state
            .call(
                "library_export",
                json!({ "request": {
                    "format": "json", "outputPath": output.display().to_string(),
                    "ids": ["gen-lib"]
                }}),
            )
            .expect("library_export");
        assert_eq!(exported.get("count").and_then(Value::as_u64), Some(1));
        assert!(
            std::fs::read_to_string(&output)
                .expect("exported file")
                .contains("a library prompt")
        );

        {
            let guard = state.database.lock().expect("db lock");
            crate::db::repository::generations::hard_delete(guard.connection(), "gen-lib")
                .expect("delete");
        }
        let imported = state
            .call(
                "library_import",
                json!({ "request": { "path": output.display().to_string() }}),
            )
            .expect("library_import");
        assert_eq!(imported.get("imported").and_then(Value::as_u64), Some(1));
        let detail = state
            .call("library_get", json!({ "id": "gen-lib" }))
            .expect("library_get");
        assert!(
            detail["prompt"]
                .as_str()
                .is_some_and(|text| text.contains("a library prompt"))
        );

        let missing = state
            .call(
                "library_rerun",
                json!({ "request": {
                    "ids": ["gen-lib"], "presetId": "missing-preset", "guidance": null
                }}),
            )
            .unwrap_err();
        assert_eq!(missing.code, "not_found");
        let _ = std::fs::remove_file(output);
    }

    #[test]
    fn wired_library_backup_and_restore() {
        let Some(state) = test_state() else {
            return;
        };
        let output =
            std::env::temp_dir().join(format!("native-library-backup-{}.zip", Uuid::now_v7()));
        let backed = state
            .call(
                "library_backup",
                json!({ "request": { "outputPath": output.display().to_string() }}),
            )
            .expect("library_backup");
        assert!(backed.get("bytes").and_then(Value::as_u64).unwrap_or(0) > 0);
        assert!(output.is_file());

        let restored = state
            .call(
                "library_restore",
                json!({ "request": { "backupPath": output.display().to_string() }}),
            )
            .expect("library_restore");
        assert_eq!(restored.get("bundle").and_then(Value::as_bool), Some(true));
        let _ = std::fs::remove_file(output);
    }

    #[test]
    fn wired_datasets_import_run_and_export() {
        let Some(state) = test_state() else {
            return;
        };
        let directory = std::env::temp_dir().join(format!("native-datasets-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&directory).expect("directory");
        let input = directory.join("rows.csv");
        std::fs::write(&input, b"caption\nfirst prompt\nsecond prompt\n").expect("csv");

        let imported = state
            .call(
                "datasets_import",
                json!({ "request": {
                    "path": input.display().to_string(), "name": "rows", "dedupe": "skip"
                }}),
            )
            .expect("datasets_import");
        assert_eq!(imported.get("imported").and_then(Value::as_u64), Some(2));
        let dataset_id = imported["dataset"]["id"]
            .as_str()
            .expect("dataset id")
            .to_string();

        let output = directory.join("out.csv");
        let exported = state
            .call(
                "datasets_export",
                json!({ "request": {
                    "datasetId": dataset_id,
                    "outputPath": output.display().to_string(),
                    "format": "csv"
                }}),
            )
            .expect("datasets_export");
        assert_eq!(exported.get("count").and_then(Value::as_u64), Some(2));
        assert!(
            std::fs::read_to_string(&output)
                .expect("dataset export")
                .contains("first prompt")
        );

        let presets = state.call("presets_list", json!({})).expect("presets_list");
        let preset_id = presets[0]["id"].as_str().expect("preset id").to_string();
        let started = state
            .call(
                "datasets_run",
                json!({ "request": {
                    "datasetId": dataset_id, "presetId": preset_id,
                    "guidance": {}, "concurrency": 1
                }}),
            )
            .expect("datasets_run");
        assert!(started.get("jobId").and_then(Value::as_str).is_some());
        let _ = std::fs::remove_dir_all(directory);
    }
}

/// Windows paths compare equal regardless of separator style or letter case.
fn same_path(a: &str, b: &str) -> bool {
    let norm = |value: &str| value.replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
}
