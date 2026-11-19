use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::db::Database;
use crate::db::repository::assets as asset_repo;
use crate::db::repository::jobs as repo;
use crate::job_queue::{self, EnqueueResult, EnqueueSpec, JobQueue, ReorderRequest};

use super::CommandError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Queued,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobReference {
    pub asset_id: Option<String>,
    pub name: Option<String>,
    pub weight: Option<f64>,
    pub thumbnail_path: Option<String>,
    pub missing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobItem {
    pub id: String,
    pub index: i64,
    pub state: JobState,
    pub error: Option<String>,
    pub attempts: i64,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub has_result: bool,
    #[serde(default)]
    pub references: Vec<JobReference>,
}

/// Reads ordered references out of a queued compose payload and resolves
/// thumbnails for the job detail view.
pub fn job_references(connection: &rusqlite::Connection, payload_json: &str) -> Vec<JobReference> {
    let Ok(value) = serde_json::from_str::<Value>(payload_json) else {
        return Vec::new();
    };
    value
        .get("references")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let asset_id = item
                        .get("assetId")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    let (thumbnail_path, missing) = match asset_id.as_deref() {
                        Some(id) => match asset_repo::get(connection, id) {
                            Ok(Some(asset)) => (
                                Some(asset.thumbnail_path.unwrap_or(asset.source_path)),
                                false,
                            ),
                            _ => (None, true),
                        },
                        None => (None, false),
                    };
                    JobReference {
                        asset_id,
                        name,
                        weight: item.get("weight").and_then(Value::as_f64),
                        thumbnail_path,
                        missing,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub state: JobState,
    pub progress: f32,
    pub total_items: i64,
    pub completed_items: i64,
    pub failed_items: i64,
    pub cancelled_items: i64,
    pub queued_items: i64,
    pub attempts: i64,
    pub priority: i64,
    pub note: Option<String>,
    pub current_item: Option<i64>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error: Option<String>,
    pub items: Vec<JobItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueStatus {
    pub paused: bool,
    pub queued: i64,
    pub running: i64,
    pub waiting: i64,
}

#[tauri::command]
pub async fn jobs_list(database: State<'_, Mutex<Database>>) -> Result<Vec<Job>, CommandError> {
    job_queue::list(database.inner(), 200)
}

#[tauri::command]
pub async fn jobs_get(
    id: String,
    database: State<'_, Mutex<Database>>,
) -> Result<Job, CommandError> {
    job_queue::get(database.inner(), &id)
}

#[tauri::command]
pub async fn jobs_status(
    database: State<'_, Mutex<Database>>,
    queue: State<'_, std::sync::Arc<JobQueue>>,
) -> Result<QueueStatus, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let mut queued = 0;
    let mut running = 0;
    let mut waiting = 0;
    for row in repo::list_active(connection)? {
        match row.state.as_str() {
            "queued" => queued += 1,
            "running" => running += 1,
            "waiting" => waiting += 1,
            _ => {}
        }
    }
    Ok(QueueStatus {
        paused: queue.paused(),
        queued,
        running,
        waiting,
    })
}

#[tauri::command]
pub async fn jobs_enqueue(
    request: EnqueueSpec,
    database: State<'_, Mutex<Database>>,
) -> Result<EnqueueResult, CommandError> {
    job_queue::enqueue(database.inner(), request)
}

#[tauri::command]
pub async fn jobs_pause(queue: State<'_, std::sync::Arc<JobQueue>>) -> Result<(), CommandError> {
    queue.pause();
    Ok(())
}

#[tauri::command]
pub async fn jobs_resume(queue: State<'_, std::sync::Arc<JobQueue>>) -> Result<(), CommandError> {
    queue.resume();
    Ok(())
}

#[tauri::command]
pub async fn jobs_retry(
    id: String,
    database: State<'_, Mutex<Database>>,
    queue: State<'_, std::sync::Arc<JobQueue>>,
) -> Result<Job, CommandError> {
    let job = job_queue::retry(database.inner(), &id)?;
    queue.release(&id);
    Ok(job)
}

#[tauri::command]
pub async fn jobs_cancel(
    id: String,
    database: State<'_, Mutex<Database>>,
    queue: State<'_, std::sync::Arc<JobQueue>>,
) -> Result<Job, CommandError> {
    let job = job_queue::get(database.inner(), &id)?;
    match job.state {
        JobState::Running => queue.request_cancel(&id),
        JobState::Queued | JobState::Waiting | JobState::Interrupted => {
            job_queue::cancel(&database, &id)?;
            queue.release(&id);
        }
        JobState::Succeeded | JobState::Failed | JobState::Cancelled => {
            return Err(CommandError::conflict("That job has already finished"));
        }
    }
    job_queue::get(database.inner(), &id)
}

#[tauri::command]
pub async fn jobs_cancel_waiting(
    database: State<'_, Mutex<Database>>,
    queue: State<'_, std::sync::Arc<JobQueue>>,
) -> Result<usize, CommandError> {
    let cancelled = job_queue::cancel_waiting(database.inner())?;
    if cancelled > 0 {
        queue.resume();
    }
    Ok(cancelled)
}

#[tauri::command]
pub async fn jobs_reorder(
    request: ReorderRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<Vec<Job>, CommandError> {
    job_queue::reorder(database.inner(), &request)
}

#[cfg(test)]
mod tests {
    use super::job_references;
    use crate::db::Database;
    use crate::db::repository::assets;

    #[test]
    fn resolves_reference_thumbnails_and_missing_assets() {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        let connection = database.connection();

        assets::insert(
            connection,
            &assets::NewAsset {
                id: "asset-a".into(),
                hash: "hash-a".into(),
                kind: "image".into(),
                source_path: "C:\\media\\a.png".into(),
                display_name: "a.png".into(),
                width: Some(64),
                height: Some(64),
                fps: None,
                duration_ms: None,
                codec: Some("png".into()),
                has_audio: false,
                color: None,
                rotation: 0,
                thumbnail_path: Some("C:\\cache\\a.jpg".into()),
                proxy_path: None,
                embedded_metadata: None,
            },
        )
        .expect("asset");

        let payload = serde_json::json!({
            "references": [
                { "assetId": "asset-a", "weight": 0.8, "name": "subject" },
                { "assetId": "gone", "weight": 1.0 }
            ]
        })
        .to_string();
        let references = job_references(connection, &payload);
        assert_eq!(references.len(), 2);
        assert_eq!(references[0].name.as_deref(), Some("subject"));
        assert_eq!(references[0].weight, Some(0.8));
        assert_eq!(
            references[0].thumbnail_path.as_deref(),
            Some("C:\\cache\\a.jpg")
        );
        assert!(!references[0].missing);
        assert!(references[1].missing);
        assert!(job_references(connection, "{}").is_empty());
    }
}
