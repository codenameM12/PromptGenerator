use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::CommandError;
use crate::commands::jobs::{Job, JobItem, JobState};
use crate::commands::pipeline::ComposeRequest;
use crate::db::Database;
use crate::db::repository::datasets as dataset_repo;
use crate::db::repository::jobs as repo;
use crate::db::repository::presets as preset_repo;
use crate::pipeline::{ComposeInput, Pipeline};
use crate::preset;
use crate::runtime::manager::ModelManager;
use crate::settings::SettingsStore;

/// How many attempts an item gets before it becomes a permanent failure.
pub const MAX_ATTEMPTS: u32 = 3;
const DEFAULT_BACKOFF_MS: u64 = 750;
const MAX_BACKOFF_MS: u64 = 8_000;

/// Parses a queued compose payload, accepting legacy structured descriptions
/// by flattening them to prose before deserialization.
fn parse_compose_payload(text: &str) -> Result<ComposeRequest, serde_json::Error> {
    let mut value: Value = serde_json::from_str(text)?;
    if let Some(description) = value.get_mut("description")
        && description.is_object()
    {
        *description = Value::String(crate::pipeline::flatten_legacy_description(description));
    }
    if let Some(references) = value.get_mut("references").and_then(Value::as_array_mut) {
        for reference in references {
            if let Some(description) = reference.get_mut("description")
                && description.is_object()
            {
                *description =
                    Value::String(crate::pipeline::flatten_legacy_description(description));
            }
        }
    }
    serde_json::from_value(value)
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub max_attempts: u32,
    pub base_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_attempts: MAX_ATTEMPTS,
            base_backoff: Duration::from_millis(DEFAULT_BACKOFF_MS),
            max_backoff: Duration::from_millis(MAX_BACKOFF_MS),
        }
    }
}

impl RunOptions {
    pub fn immediate() -> Self {
        Self {
            base_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
            ..Self::default()
        }
    }

    fn backoff(&self, attempt: u32) -> Duration {
        if self.base_backoff.is_zero() {
            return Duration::ZERO;
        }
        let factor = 1u32 << attempt.min(5);
        (self.base_backoff * factor).min(self.max_backoff)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgressEvent {
    pub job_id: String,
    pub state: JobState,
    pub progress: f64,
    pub total: i64,
    pub completed: i64,
    pub failed: i64,
    pub current_item: Option<i64>,
    pub item_state: Option<JobState>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueSpec {
    pub kind: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub items: Vec<Value>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub dedupe_key: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueResult {
    pub job: Job,
    pub reused: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderRequest {
    pub id: String,
    pub direction: String,
}

pub fn parse_state(value: &str) -> Result<JobState, CommandError> {
    match value {
        "queued" => Ok(JobState::Queued),
        "running" => Ok(JobState::Running),
        "waiting" => Ok(JobState::Waiting),
        "succeeded" => Ok(JobState::Succeeded),
        "failed" => Ok(JobState::Failed),
        "cancelled" => Ok(JobState::Cancelled),
        "interrupted" => Ok(JobState::Interrupted),
        other => Err(CommandError::internal(format!(
            "Unknown job state in database: {other}"
        ))),
    }
}

pub fn state_str(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Running => "running",
        JobState::Waiting => "waiting",
        JobState::Succeeded => "succeeded",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
        JobState::Interrupted => "interrupted",
    }
}

/// Legal job transitions. A job that already reached a terminal state can only
/// move again through an explicit new attempt (manual retry), which re-queues
/// it from `failed`/`interrupted`/`cancelled`.
pub fn can_transition(from: JobState, to: JobState) -> bool {
    use JobState::*;
    match from {
        Queued => matches!(to, Running | Waiting | Cancelled),
        Waiting => matches!(to, Queued | Cancelled | Failed),
        Running => matches!(to, Succeeded | Failed | Cancelled | Interrupted),
        Interrupted => matches!(to, Queued | Cancelled),
        Failed => matches!(to, Queued),
        Succeeded | Cancelled => false,
    }
}

pub fn transition(
    connection: &rusqlite::Connection,
    id: &str,
    to: JobState,
) -> Result<(), CommandError> {
    let row = repo::get(connection, id)?.ok_or_else(|| CommandError::not_found("Job not found"))?;
    let from = parse_state(&row.state)?;
    if !can_transition(from, to) {
        return Err(CommandError::conflict(format!(
            "Job {id} cannot move from {} to {}",
            state_str(from),
            state_str(to)
        )));
    }
    repo::update_state(
        connection,
        id,
        state_str(to),
        row.progress,
        row.error.as_deref(),
    )?;
    Ok(())
}

pub fn enqueue(
    database: &Mutex<Database>,
    spec: EnqueueSpec,
) -> Result<EnqueueResult, CommandError> {
    if spec.kind.trim().is_empty() {
        return Err(CommandError::validation("Job kind is required"));
    }
    if spec.items.is_empty() {
        return Err(CommandError::validation(
            "A job needs at least one item; use its payload for single work",
        ));
    }

    let dedupe_key = Some(spec.dedupe_key.clone().unwrap_or_else(|| {
        let mut hasher = blake3::Hasher::new();
        hasher.update(spec.kind.as_bytes());
        hasher.update(&serde_json::to_vec(&spec.payload).unwrap_or_default());
        for item in &spec.items {
            hasher.update(&serde_json::to_vec(item).unwrap_or_default());
        }
        hasher.finalize().to_hex().to_string()
    }));

    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();

    if let Some(key) = &dedupe_key
        && let Some(existing) = repo::find_active_by_dedupe(connection, key)?
    {
        return Ok(EnqueueResult {
            job: to_job(connection, existing)?,
            reused: true,
        });
    }

    let id = uuid::Uuid::now_v7().to_string();
    let payload_json = serde_json::to_string(&spec.payload)
        .map_err(|error| CommandError::internal(format!("serialize job payload: {error}")))?;
    let row = repo::insert(
        connection,
        &repo::NewJob {
            id: id.clone(),
            kind: spec.kind,
            state: state_str(JobState::Queued).into(),
            payload_json,
            priority: spec.priority,
            dedupe_key,
            note: spec.note,
        },
    )?;

    for (index, item) in spec.items.iter().enumerate() {
        repo::insert_item(
            connection,
            &repo::NewJobItem {
                id: uuid::Uuid::now_v7().to_string(),
                job_id: id.clone(),
                item_index: index as i64,
                state: state_str(JobState::Queued).into(),
                payload_json: serde_json::to_string(item).map_err(|error| {
                    CommandError::internal(format!("serialize job item: {error}"))
                })?,
                idempotency_key: None,
            },
        )?;
    }

    repo::update_progress(connection, &id, 0.0)?;
    Ok(EnqueueResult {
        job: to_job(connection, row)?,
        reused: false,
    })
}

pub fn to_job(connection: &rusqlite::Connection, row: repo::JobRow) -> Result<Job, CommandError> {
    let counts = repo::item_counts(connection, &row.id)?;
    let items = repo::list_items(connection, &row.id)?;
    let current_item = items
        .iter()
        .find(|item| item.state == "running")
        .map(|item| item.item_index);

    Ok(Job {
        id: row.id,
        kind: row.kind,
        state: parse_state(&row.state)?,
        progress: row.progress as f32,
        total_items: counts.total,
        completed_items: counts.succeeded,
        failed_items: counts.failed,
        cancelled_items: counts.cancelled,
        queued_items: counts.queued + counts.waiting,
        attempts: row.attempts,
        priority: row.priority,
        note: row.note,
        current_item,
        created_at: row.created_at,
        started_at: row.started_at,
        finished_at: row.finished_at,
        error: row.error,
        items: items
            .into_iter()
            .map(|item| JobItem {
                id: item.id,
                index: item.item_index,
                state: parse_state(&item.state).unwrap_or(JobState::Queued),
                error: item.error,
                attempts: item.attempts,
                started_at: item.started_at,
                finished_at: item.finished_at,
                has_result: item.result_json.is_some(),
                references: crate::commands::jobs::job_references(connection, &item.payload_json),
            })
            .collect(),
    })
}

pub fn list(database: &Mutex<Database>, limit: i64) -> Result<Vec<Job>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    repo::list(connection, limit.clamp(1, 500))?
        .into_iter()
        .map(|row| to_job(connection, row))
        .collect()
}

pub fn get(database: &Mutex<Database>, id: &str) -> Result<Job, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let row = repo::get(connection, id)?.ok_or_else(|| CommandError::not_found("Job not found"))?;
    to_job(connection, row)
}

/// Marks rows left running by an unclean exit as interrupted. Called once at
/// startup before the runner starts.
pub fn recover(database: &Mutex<Database>) -> Result<usize, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    repo::mark_interrupted(guard.connection())
}

pub fn retry(database: &Mutex<Database>, id: &str) -> Result<Job, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let row = repo::get(connection, id)?.ok_or_else(|| CommandError::not_found("Job not found"))?;
    let state = parse_state(&row.state)?;
    if !matches!(
        state,
        JobState::Failed | JobState::Interrupted | JobState::Cancelled
    ) {
        return Err(CommandError::conflict(
            "Only failed, interrupted, or cancelled jobs can be retried",
        ));
    }
    repo::requeue(connection, id)?;
    let row = repo::get(connection, id)?.expect("job exists");
    to_job(connection, row)
}

pub fn cancel_waiting(database: &Mutex<Database>) -> Result<usize, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    repo::cancel_waiting(guard.connection())
}

/// Cancels a job that has not started yet. Running jobs are cancelled through
/// their shared cancellation flag instead.
pub fn cancel(database: &Mutex<Database>, id: &str) -> Result<(), CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let row = repo::get(connection, id)?.ok_or_else(|| CommandError::not_found("Job not found"))?;
    let state = parse_state(&row.state)?;
    if !matches!(
        state,
        JobState::Queued | JobState::Waiting | JobState::Interrupted
    ) {
        return Err(CommandError::conflict(
            "Only a job that has not started can be cancelled here",
        ));
    }
    repo::cancel_job(connection, id)?;
    Ok(())
}

pub fn reorder(
    database: &Mutex<Database>,
    request: &ReorderRequest,
) -> Result<Vec<Job>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();

    let mut active: Vec<repo::JobRow> = repo::list_active(connection)?
        .into_iter()
        .filter(|row| row.state != "running")
        .collect();
    let index = active
        .iter()
        .position(|row| row.id == request.id)
        .ok_or_else(|| CommandError::not_found("Job is not waiting in the queue"))?;

    let target = match request.direction.as_str() {
        "up" if index > 0 => index - 1,
        "down" if index + 1 < active.len() => index + 1,
        "up" | "down" => return Err(CommandError::conflict("Job is already at the edge")),
        other => {
            return Err(CommandError::validation(format!(
                "Unknown reorder direction: {other}"
            )));
        }
    };

    active.swap(index, target);
    for (position, row) in active.iter().enumerate() {
        let priority = ((active.len() - position) * 10) as i64;
        repo::set_priority(connection, &row.id, priority)?;
    }

    active
        .into_iter()
        .map(|row| {
            let row = repo::get(connection, &row.id)?.expect("job exists");
            to_job(connection, row)
        })
        .collect::<Result<Vec<_>, CommandError>>()
}

#[async_trait]
pub trait JobExecutor: Send + Sync {
    /// Runs one item. Returning `Err` with `retryable == true` schedules a
    /// retry; any other error becomes a permanent failure.
    async fn execute(
        &self,
        job: &repo::JobRow,
        item: &repo::JobItemRow,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Value, CommandError>;
}

pub async fn run_job(
    database: &Mutex<Database>,
    job: &repo::JobRow,
    executor: &dyn JobExecutor,
    cancel: &Arc<AtomicBool>,
    options: &RunOptions,
    on_event: &mut (dyn FnMut(&JobProgressEvent) + Send),
) -> Result<(), CommandError> {
    let job_id = job.id.clone();

    if cancel.load(Ordering::Relaxed) {
        finish_cancelled(database, &job_id, on_event)?;
        return Ok(());
    }

    emit(
        database,
        on_event,
        &job_id,
        JobState::Running,
        None,
        None,
        Some("Job started".into()),
    )?;

    loop {
        if cancel.load(Ordering::Relaxed) {
            finish_cancelled(database, &job_id, on_event)?;
            return Ok(());
        }

        let item = {
            let guard = lock(database)?;
            repo::next_item(guard.connection(), &job_id)?
        };
        let Some(item) = item else {
            break;
        };

        let claimed = {
            let guard = lock(database)?;
            repo::claim_item(guard.connection(), &item.id)?
        };
        if !claimed {
            continue;
        }

        let item_index = item.item_index;
        emit(
            database,
            on_event,
            &job_id,
            JobState::Running,
            Some(item_index),
            Some(JobState::Running),
            None,
        )?;

        let mut attempt = 1u32;
        loop {
            let result = executor.execute(job, &item, cancel).await;
            match result {
                Ok(value) => {
                    let result_json = serde_json::to_string(&value).map_err(|error| {
                        CommandError::internal(format!("serialize result: {error}"))
                    })?;
                    let guard = lock(database)?;
                    repo::complete_item(guard.connection(), &item.id, &result_json)?;
                    refresh_progress(guard.connection(), &job_id)?;
                    drop(guard);
                    emit(
                        database,
                        on_event,
                        &job_id,
                        JobState::Running,
                        Some(item_index),
                        Some(JobState::Succeeded),
                        None,
                    )?;
                    break;
                }
                Err(error) if error.code == "cancelled" => {
                    finish_cancelled(database, &job_id, on_event)?;
                    return Ok(());
                }
                Err(error) => {
                    let retry = error.retryable && attempt < options.max_attempts;
                    if retry {
                        let delay = options.backoff(attempt - 1);
                        if !delay.is_zero() {
                            tokio::time::sleep(delay).await;
                        }
                        if cancel.load(Ordering::Relaxed) {
                            finish_cancelled(database, &job_id, on_event)?;
                            return Ok(());
                        }
                        attempt += 1;
                        let guard = lock(database)?;
                        repo::retry_item(guard.connection(), &item.id)?;
                        continue;
                    }

                    let message = format!("[{:?}] {}", error.category, error.message);
                    let guard = lock(database)?;
                    repo::fail_item(guard.connection(), &item.id, &message)?;
                    refresh_progress(guard.connection(), &job_id)?;
                    drop(guard);
                    emit(
                        database,
                        on_event,
                        &job_id,
                        JobState::Running,
                        Some(item_index),
                        Some(JobState::Failed),
                        Some(message),
                    )?;
                    break;
                }
            }
        }
    }

    finish(database, &job_id, on_event)?;
    Ok(())
}

fn finish(
    database: &Mutex<Database>,
    job_id: &str,
    on_event: &mut (dyn FnMut(&JobProgressEvent) + Send),
) -> Result<(), CommandError> {
    let guard = lock(database)?;
    let connection = guard.connection();
    repo::get(connection, job_id)?.ok_or_else(|| CommandError::not_found("Job not found"))?;
    let counts = repo::item_counts(connection, job_id)?;
    let progress = if counts.total == 0 {
        1.0
    } else {
        counts.finished() as f64 / counts.total as f64
    };

    let (state, error) = if counts.failed > 0 {
        (
            JobState::Failed,
            Some(format!(
                "{} of {} item(s) failed",
                counts.failed, counts.total
            )),
        )
    } else if counts.cancelled > 0 {
        (
            JobState::Cancelled,
            Some("Some items were cancelled".to_string()),
        )
    } else {
        (JobState::Succeeded, None)
    };
    repo::update_state(
        connection,
        job_id,
        state_str(state),
        progress,
        error.as_deref(),
    )?;
    drop(guard);

    emit(
        database,
        on_event,
        job_id,
        state,
        None,
        None,
        error.or_else(|| {
            Some(format!(
                "Job {} after {} item(s)",
                state_str(state),
                counts.total
            ))
        }),
    )?;
    Ok(())
}

fn finish_cancelled(
    database: &Mutex<Database>,
    job_id: &str,
    on_event: &mut (dyn FnMut(&JobProgressEvent) + Send),
) -> Result<(), CommandError> {
    let guard = lock(database)?;
    let connection = guard.connection();
    for item in repo::list_items(connection, job_id)? {
        if !matches!(item.state.as_str(), "succeeded" | "failed") {
            repo::cancel_item(connection, &item.id)?;
        }
    }
    repo::update_state(
        connection,
        job_id,
        "cancelled",
        0.0,
        Some("Cancelled by the user"),
    )?;
    drop(guard);
    emit(
        database,
        on_event,
        job_id,
        JobState::Cancelled,
        None,
        None,
        Some("Job cancelled".into()),
    )
}

fn refresh_progress(connection: &rusqlite::Connection, job_id: &str) -> Result<(), CommandError> {
    let counts = repo::item_counts(connection, job_id)?;
    let progress = if counts.total == 0 {
        0.0
    } else {
        counts.finished() as f64 / counts.total as f64
    };
    repo::update_progress(connection, job_id, progress)?;
    Ok(())
}

fn emit(
    database: &Mutex<Database>,
    on_event: &mut (dyn FnMut(&JobProgressEvent) + Send),
    job_id: &str,
    state: JobState,
    current_item: Option<i64>,
    item_state: Option<JobState>,
    message: Option<String>,
) -> Result<(), CommandError> {
    let (progress, total, completed, failed) = {
        let guard = lock(database)?;
        let connection = guard.connection();
        let job = repo::get(connection, job_id)?
            .ok_or_else(|| CommandError::not_found("Job not found"))?;
        let counts = repo::item_counts(connection, job_id)?;
        (job.progress, counts.total, counts.succeeded, counts.failed)
    };

    on_event(&JobProgressEvent {
        job_id: job_id.to_string(),
        state,
        progress,
        total,
        completed,
        failed,
        current_item,
        item_state,
        message,
    });
    Ok(())
}

fn lock(database: &Mutex<Database>) -> Result<std::sync::MutexGuard<'_, Database>, CommandError> {
    database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))
}

/// Shared queue state managed by Tauri. It owns the pause flag and the
/// cancellation handles for running jobs.
#[derive(Default)]
pub struct JobQueue {
    paused: AtomicBool,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl JobQueue {
    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    pub fn cancel_flag(&self, id: &str) -> Arc<AtomicBool> {
        let mut cancels = self.cancels.lock().expect("job cancels");
        cancels
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    pub fn request_cancel(&self, id: &str) {
        self.cancel_flag(id).store(true, Ordering::Relaxed);
    }

    pub fn release(&self, id: &str) {
        if let Ok(mut cancels) = self.cancels.lock() {
            cancels.remove(id);
        }
    }

    /// Starts the single background runner. Composer concurrency is one, so a
    /// single runner already serializes compositions; describe-stage limits are
    /// enforced by the model manager.
    pub fn start(self: &Arc<Self>, app: AppHandle) {
        let queue = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let executor = TauriExecutor { app: app.clone() };
            loop {
                tokio::time::sleep(Duration::from_millis(150)).await;
                if queue.paused() {
                    continue;
                }

                let database = app.state::<Mutex<Database>>();
                let claimed = {
                    let Ok(guard) = database.lock() else {
                        continue;
                    };
                    repo::claim_next(guard.connection()).ok().flatten()
                };
                let Some(job) = claimed else {
                    continue;
                };

                let cancel = queue.cancel_flag(&job.id);
                if cancel.load(Ordering::Relaxed) {
                    if let Ok(guard) = database.lock() {
                        let _ = repo::update_state(
                            guard.connection(),
                            &job.id,
                            "cancelled",
                            0.0,
                            Some("Cancelled before it started"),
                        );
                    }
                    queue.release(&job.id);
                    continue;
                }

                let emit_handle = app.clone();
                let mut on_event = move |event: &JobProgressEvent| {
                    let _ = emit_handle.emit("jobs://progress", event);
                    if let Ok(database) = emit_handle.state::<Mutex<Database>>().lock()
                        && let Ok(row) = repo::get(database.connection(), &event.job_id)
                        && let Some(row) = row
                        && let Ok(job) = to_job(database.connection(), row)
                    {
                        let _ = emit_handle.emit("jobs://changed", &job);
                    }
                };

                let result = run_job(
                    database.inner(),
                    &job,
                    &executor,
                    &cancel,
                    &RunOptions::default(),
                    &mut on_event,
                )
                .await;
                if let Err(error) = result
                    && let Ok(guard) = database.lock()
                {
                    let _ = repo::update_state(
                        guard.connection(),
                        &job.id,
                        "failed",
                        0.0,
                        Some(&format!("[{:?}] {}", error.category, error.message)),
                    );
                }
                queue.release(&job.id);
            }
        });
    }
}

/// Executes one queued compose item. Shared by the Tauri runner and the
/// native runner so both behave identically (dataset row status included).
pub async fn run_compose_item(
    database: &Mutex<Database>,
    manager: &Arc<ModelManager>,
    composer_prompt: Option<String>,
    payload_json: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<Value, CommandError> {
    let request: ComposeRequest = parse_compose_payload(payload_json).map_err(|error| {
        CommandError::validation(format!("Invalid compose job payload: {error}"))
    })?;

    let spec = {
        let guard = database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let row = preset_repo::get(guard.connection(), &request.preset_id)?
            .filter(|row| row.deleted_at.is_none())
            .ok_or_else(|| CommandError::not_found("Preset not found"))?;
        let document: Value = serde_json::from_str(&row.document_json)
            .map_err(|error| CommandError::internal(format!("read preset: {error}")))?;
        let spec = preset::parse(&document)?;
        preset::validate(&spec)?;
        spec
    };

    let pipeline = Pipeline { manager, database };
    let cancel_flag = cancel.clone();
    let dataset_row_id = request.dataset_row_id.clone();
    let primary_asset_id = request
        .asset_id
        .clone()
        .or_else(|| request.references.first().and_then(|item| item.asset_id.clone()));
    let primary_edit_id = request
        .edit_id
        .clone()
        .or_else(|| request.references.first().and_then(|item| item.edit_id.clone()));
    let primary_description = request.description.clone().or_else(|| {
        request
            .references
            .first()
            .and_then(|item| item.description.clone())
    });
    let result = pipeline
        .compose(
            &ComposeInput {
                preset_id: request.preset_id.clone(),
                spec,
                asset_id: primary_asset_id,
                edit_id: primary_edit_id,
                references: request.references.clone(),
                parent_id: request.parent_id.clone(),
                description: primary_description,
                brief: request.brief.clone(),
                existing_prompt: request.existing_prompt.clone(),
                instruction: request.instruction.clone(),
                guidance: request.guidance.clone(),
                locked: request.locked.clone(),
                params: request.params.clone(),
                variations: request.variations.max(1),
                seed: request.seed,
                output_language: request.output_language.clone(),
                bypass_cache: request.bypass_cache,
                cancel: Some(cancel_flag),
                system_prompt: composer_prompt,
                images: Vec::new(),
                images_key: None,
            },
            None,
        )
        .await;

    match result {
        Ok(generations) => {
            if let (Some(row_id), Some(first)) = (dataset_row_id.as_deref(), generations.first())
                && let Ok(guard) = database.lock()
            {
                let _ = dataset_repo::update_item_status(
                    guard.connection(),
                    row_id,
                    "succeeded",
                    Some(&first.id),
                    None,
                );
            }
            serde_json::to_value(&generations)
                .map_err(|error| CommandError::internal(format!("serialize generations: {error}")))
        }
        Err(error) => {
            if let Some(row_id) = dataset_row_id.as_deref()
                && let Ok(guard) = database.lock()
            {
                let message = format!("[{:?}] {}", error.category, error.message);
                let _ = dataset_repo::update_item_status(
                    guard.connection(),
                    row_id,
                    "failed",
                    None,
                    Some(&message),
                );
            }
            Err(error)
        }
    }
}

struct TauriExecutor {
    app: AppHandle,
}

#[async_trait]
impl JobExecutor for TauriExecutor {
    async fn execute(
        &self,
        job: &repo::JobRow,
        item: &repo::JobItemRow,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Value, CommandError> {
        match job.kind.as_str() {
            "compose" => {
                let database = self.app.state::<Mutex<Database>>();
                let manager = self.app.state::<Arc<ModelManager>>();
                let composer_prompt = self
                    .app
                    .state::<Mutex<SettingsStore>>()
                    .lock()
                    .ok()
                    .and_then(|store| store.get().composer_system_prompt);
                run_compose_item(
                    database.inner(),
                    manager.inner(),
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use serde_json::json;

    use crate::commands::CommandError;
    use crate::commands::jobs::JobState;
    use crate::db::Database;
    use crate::db::repository::jobs as repo;

    use super::{
        EnqueueSpec, JobExecutor, JobProgressEvent, ReorderRequest, RunOptions, can_transition,
        enqueue, recover, reorder, retry, run_job, state_str, to_job,
    };

    struct FakeExecutor {
        outcomes: Mutex<VecDeque<Result<serde_json::Value, CommandError>>>,
        ran: Mutex<Vec<String>>,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
    }

    impl FakeExecutor {
        fn new(outcomes: Vec<Result<serde_json::Value, CommandError>>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes.into()),
                ran: Mutex::new(Vec::new()),
                in_flight: AtomicUsize::new(0),
                max_in_flight: AtomicUsize::new(0),
            }
        }

        fn always_ok() -> Self {
            Self::new(Vec::new())
        }

        fn ran(&self) -> Vec<String> {
            self.ran.lock().expect("ran").clone()
        }
    }

    #[async_trait]
    impl JobExecutor for FakeExecutor {
        async fn execute(
            &self,
            _job: &repo::JobRow,
            item: &repo::JobItemRow,
            cancel: &Arc<AtomicBoolSentinel>,
        ) -> Result<serde_json::Value, CommandError> {
            self.in_flight.fetch_add(1, Ordering::SeqCst);
            let current = self.in_flight.load(Ordering::SeqCst);
            self.max_in_flight.fetch_max(current, Ordering::SeqCst);
            self.ran
                .lock()
                .expect("ran")
                .push(item.payload_json.clone());
            tokio::time::sleep(Duration::from_millis(1)).await;
            let outcome = self
                .outcomes
                .lock()
                .expect("outcomes")
                .pop_front()
                .unwrap_or_else(|| Ok(json!({ "ok": true })));
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            if cancel.load(Ordering::Relaxed) {
                return Err(CommandError::cancelled());
            }
            outcome
        }
    }

    type AtomicBoolSentinel = std::sync::atomic::AtomicBool;

    fn database() -> Mutex<Database> {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        Mutex::new(database)
    }

    fn spec(kind: &str, count: usize, marker: &str) -> EnqueueSpec {
        EnqueueSpec {
            kind: kind.into(),
            payload: json!({ "marker": marker }),
            items: (0..count)
                .map(|index| json!({ "index": index, "marker": marker }))
                .collect(),
            priority: 0,
            dedupe_key: None,
            note: Some(marker.into()),
        }
    }

    fn enqueue_ok(database: &Mutex<Database>, spec: EnqueueSpec) -> repo::JobRow {
        let result = enqueue(database, spec).expect("enqueue");
        assert!(!result.reused);
        let guard = database.lock().expect("db");
        repo::get(guard.connection(), &result.job.id)
            .expect("get")
            .expect("present")
    }

    async fn run_all(
        database: &Mutex<Database>,
        executor: &dyn JobExecutor,
        options: &RunOptions,
    ) -> Vec<JobProgressEvent> {
        let mut events = Vec::new();
        loop {
            let claimed = {
                let guard = database.lock().expect("db");
                repo::claim_next(guard.connection()).expect("claim")
            };
            let Some(job) = claimed else {
                break;
            };
            let cancel = Arc::new(AtomicBoolSentinel::new(false));
            let mut on_event = |event: &JobProgressEvent| events.push(event.clone());
            run_job(database, &job, executor, &cancel, options, &mut on_event)
                .await
                .expect("run");
        }
        events
    }

    #[test]
    fn state_machine_rejects_illegal_transitions() {
        assert!(can_transition(JobState::Queued, JobState::Running));
        assert!(can_transition(JobState::Running, JobState::Succeeded));
        assert!(can_transition(JobState::Running, JobState::Cancelled));
        assert!(can_transition(JobState::Interrupted, JobState::Queued));
        assert!(can_transition(JobState::Failed, JobState::Queued));

        assert!(!can_transition(JobState::Queued, JobState::Succeeded));
        assert!(!can_transition(JobState::Succeeded, JobState::Running));
        assert!(!can_transition(JobState::Cancelled, JobState::Running));
        assert!(!can_transition(JobState::Running, JobState::Running));
    }

    #[test]
    fn persisted_transitions_are_validated() {
        let database = database();
        let job = enqueue_ok(&database, spec("compose", 1, "one"));
        let guard = database.lock().expect("db");

        let rejected = super::transition(guard.connection(), &job.id, JobState::Succeeded)
            .expect_err("queued cannot jump to succeeded");
        assert_eq!(rejected.category, crate::commands::ErrorCategory::Conflict);

        super::transition(guard.connection(), &job.id, JobState::Running).expect("start");
        let rejected = super::transition(guard.connection(), &job.id, JobState::Running)
            .expect_err("running cannot restart");
        assert_eq!(rejected.category, crate::commands::ErrorCategory::Conflict);

        let row = repo::get(guard.connection(), &job.id)
            .expect("get")
            .expect("present");
        assert_eq!(row.state, state_str(JobState::Running));
        assert!(row.started_at.is_some());
    }

    #[tokio::test]
    async fn fifty_queued_jobs_run_one_at_a_time() {
        let database = database();
        for index in 0..50 {
            enqueue_ok(&database, spec("compose", 1, &format!("job-{index}")));
        }
        let executor = FakeExecutor::always_ok();
        let events = run_all(&database, &executor, &RunOptions::immediate()).await;

        assert_eq!(executor.ran().len(), 50);
        assert_eq!(executor.max_in_flight.load(Ordering::SeqCst), 1);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.state == JobState::Succeeded)
                .count(),
            50
        );
        let guard = database.lock().expect("db");
        assert_eq!(repo::count_queued(guard.connection()).expect("queued"), 0);
    }

    #[tokio::test]
    async fn progress_events_report_each_item_and_the_aggregate() {
        let database = database();
        enqueue_ok(&database, spec("compose", 3, "batch"));
        let executor = FakeExecutor::always_ok();
        let events = run_all(&database, &executor, &RunOptions::immediate()).await;

        assert!(!events.is_empty());
        assert_eq!(events[0].state, JobState::Running);
        assert_eq!(events[0].total, 3);

        let item_events: Vec<_> = events
            .iter()
            .filter(|event| event.item_state.is_some())
            .collect();
        assert_eq!(item_events.len(), 6);
        assert_eq!(item_events[0].item_state, Some(JobState::Running));
        assert_eq!(item_events[1].item_state, Some(JobState::Succeeded));
        assert_eq!(item_events[0].current_item, Some(0));
        assert_eq!(item_events[5].current_item, Some(2));

        let last = events.last().expect("last");
        assert_eq!(last.state, JobState::Succeeded);
        assert_eq!(last.completed, 3);
        assert_eq!(last.total, 3);
        assert_eq!(last.progress, 1.0);
    }

    #[tokio::test]
    async fn retries_retryable_failures_with_increasing_attempts() {
        let database = database();
        enqueue_ok(&database, spec("compose", 1, "retry"));
        let executor = FakeExecutor::new(vec![
            Err(CommandError::runtime("backend hiccup")),
            Err(CommandError::runtime("backend hiccup again")),
            Ok(json!({ "ok": true })),
        ]);
        let events = run_all(&database, &executor, &RunOptions::immediate()).await;

        assert_eq!(executor.ran().len(), 3);
        assert_eq!(events.last().expect("last").state, JobState::Succeeded);

        let guard = database.lock().expect("db");
        let items = repo::list_items(guard.connection(), &events[0].job_id).expect("items");
        assert_eq!(items[0].attempts, 3);
        assert_eq!(items[0].state, "succeeded");
    }

    #[tokio::test]
    async fn permanent_failures_are_not_retried() {
        let database = database();
        enqueue_ok(&database, spec("compose", 1, "permanent"));
        let executor = FakeExecutor::new(vec![Err(CommandError::not_found(
            "input file no longer exists",
        ))]);
        let events = run_all(&database, &executor, &RunOptions::immediate()).await;

        assert_eq!(executor.ran().len(), 1);
        assert_eq!(events.last().expect("last").state, JobState::Failed);

        let guard = database.lock().expect("db");
        let items = repo::list_items(guard.connection(), &events[0].job_id).expect("items");
        assert_eq!(items[0].attempts, 1);
        assert_eq!(items[0].state, "failed");
        assert!(
            items[0]
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("input file")
        );
    }

    #[tokio::test]
    async fn manual_retry_requeues_failed_items_only() {
        let database = database();
        let job = enqueue_ok(&database, spec("compose", 2, "manual"));
        let guard = database.lock().expect("db");
        let items = repo::list_items(guard.connection(), &job.id).expect("items");
        repo::complete_item(guard.connection(), &items[0].id, "{}").expect("complete");
        repo::fail_item(guard.connection(), &items[1].id, "[Runtime] boom").expect("fail");
        repo::update_state(guard.connection(), &job.id, "failed", 0.5, Some("boom"))
            .expect("failed");
        drop(guard);

        let retried = retry(&database, &job.id).expect("retry");
        assert_eq!(retried.state, JobState::Queued);

        let executor = FakeExecutor::always_ok();
        let events = run_all(&database, &executor, &RunOptions::immediate()).await;
        assert_eq!(executor.ran().len(), 1, "only the failed item reruns");
        assert_eq!(events.last().expect("last").state, JobState::Succeeded);
    }

    #[tokio::test]
    async fn cancellation_marks_the_job_cancelled_without_results() {
        let database = database();
        let job = enqueue_ok(&database, spec("compose", 2, "cancel"));
        let executor = FakeExecutor::always_ok();
        let cancel = Arc::new(AtomicBoolSentinel::new(true));
        let mut events = Vec::new();
        let mut on_event = |event: &JobProgressEvent| events.push(event.clone());
        run_job(
            &database,
            &job,
            &executor,
            &cancel,
            &RunOptions::immediate(),
            &mut on_event,
        )
        .await
        .expect("cancelled run");

        assert!(executor.ran().is_empty());
        assert_eq!(events.last().expect("last").state, JobState::Cancelled);
        let guard = database.lock().expect("db");
        let items = repo::list_items(guard.connection(), &job.id).expect("items");
        assert!(items.iter().all(|item| item.state == "cancelled"));
        assert!(items.iter().all(|item| item.result_json.is_none()));
    }

    #[tokio::test]
    async fn crash_recovery_reprocesses_only_incomplete_items() {
        let database = database();
        let job = enqueue_ok(&database, spec("compose", 3, "recover"));
        {
            let guard = database.lock().expect("db");
            let connection = guard.connection();
            let items = repo::list_items(connection, &job.id).expect("items");
            repo::claim_next(connection).expect("claim");
            repo::complete_item(connection, &items[0].id, "{}").expect("complete one");
            repo::claim_item(connection, &items[1].id).expect("start two");
            repo::update_state(connection, &job.id, "running", 0.33, None).expect("running");
        }

        assert_eq!(recover(&database).expect("recover"), 1);
        let recovered = retry(&database, &job.id).expect("resume");
        assert_eq!(recovered.state, JobState::Queued);

        let executor = FakeExecutor::always_ok();
        let events = run_all(&database, &executor, &RunOptions::immediate()).await;
        assert_eq!(executor.ran().len(), 2, "succeeded item is skipped");
        assert_eq!(events.last().expect("last").state, JobState::Succeeded);

        let guard = database.lock().expect("db");
        let items = repo::list_items(guard.connection(), &job.id).expect("items");
        assert_eq!(items[0].state, "succeeded");
        assert_eq!(items[1].state, "succeeded");
        assert_eq!(items[2].state, "succeeded");
    }

    #[test]
    fn duplicate_enqueue_reuses_the_active_job() {
        let database = database();
        let first = enqueue(&database, spec("compose", 2, "duplicate")).expect("first");
        let second = enqueue(&database, spec("compose", 2, "duplicate")).expect("second");
        assert!(!first.reused);
        assert!(second.reused);
        assert_eq!(first.job.id, second.job.id);

        let guard = database.lock().expect("db");
        assert_eq!(
            repo::list_active(guard.connection()).expect("active").len(),
            1
        );
    }

    #[test]
    fn reorder_changes_the_claim_order() {
        let database = database();
        let first = enqueue_ok(&database, spec("compose", 1, "a"));
        let second = enqueue_ok(&database, spec("compose", 1, "b"));
        let third = enqueue_ok(&database, spec("compose", 1, "c"));

        reorder(
            &database,
            &ReorderRequest {
                id: third.id.clone(),
                direction: "up".into(),
            },
        )
        .expect("move c up");
        reorder(
            &database,
            &ReorderRequest {
                id: third.id.clone(),
                direction: "up".into(),
            },
        )
        .expect("move c to the top");

        let guard = database.lock().expect("db");
        let claimed = repo::claim_next(guard.connection())
            .expect("claim")
            .expect("job");
        assert_eq!(claimed.id, third.id);
        assert_ne!(claimed.id, first.id);
        assert_ne!(claimed.id, second.id);
    }

    #[test]
    fn cancel_all_waiting_stops_queued_jobs() {
        let database = database();
        let first = enqueue_ok(&database, spec("compose", 1, "waiting-a"));
        let second = enqueue_ok(&database, spec("compose", 1, "waiting-b"));
        assert_eq!(super::cancel_waiting(&database).expect("cancel"), 2);

        let guard = database.lock().expect("db");
        for id in [&first.id, &second.id] {
            let row = repo::get(guard.connection(), id)
                .expect("get")
                .expect("present");
            assert_eq!(row.state, "cancelled");
        }
        assert_eq!(repo::count_queued(guard.connection()).expect("queued"), 0);
    }

    #[test]
    fn dto_reports_counts_and_current_item() {
        let database = database();
        let job = enqueue_ok(&database, spec("compose", 2, "dto"));
        let guard = database.lock().expect("db");
        let row = repo::get(guard.connection(), &job.id)
            .expect("get")
            .expect("present");
        let dto = to_job(guard.connection(), row).expect("dto");
        assert_eq!(dto.total_items, 2);
        assert_eq!(dto.queued_items, 2);
        assert_eq!(dto.state, JobState::Queued);
        assert_eq!(dto.items.len(), 2);
        assert!(dto.note.as_deref() == Some("dto"));
    }
}
