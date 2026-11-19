use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use crate::commands::pipeline::{ComposeReference, ComposeRequest};
use crate::dataset::{
    self, DatasetMapping, DedupeMode, MAX_IMPORT_BYTES, MappedRow, PREVIEW_ROWS, RowIssue,
    parse_media_segments,
};
use crate::db::Database;
use crate::db::repository::assets as asset_repo;
use crate::db::repository::datasets as repo;
use crate::db::repository::generations as gen_repo;
use crate::db::repository::import_profiles as profile_repo;
use crate::db::repository::presets as preset_repo;
use crate::job_queue::{self, EnqueueSpec};
use crate::media::ffmpeg::MediaTools;
use crate::media::import::ImportService;
use crate::media::protocol::MediaAccess;
use crate::paths::AppPaths;

use super::{CommandError, Guidance};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dataset {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub row_count: i64,
    pub created_at: String,
    pub mapping: DatasetMapping,
    pub export_columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProfile {
    pub name: String,
    pub mapping: DatasetMapping,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetImportRequest {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mapping: Option<DatasetMapping>,
    #[serde(default)]
    pub dedupe: Option<String>,
    #[serde(default)]
    pub profile_name: Option<String>,
    #[serde(default)]
    pub save_profile: bool,
    #[serde(default)]
    pub preset_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetPreview {
    pub path: String,
    pub format: String,
    pub encoding: String,
    pub delimiter: String,
    pub headers: Vec<String>,
    pub rows: Vec<MappedRow>,
    pub total_rows: usize,
    pub mapping: DatasetMapping,
    pub profiles: Vec<ImportProfile>,
    pub issues: Vec<RowIssue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetImportSummary {
    pub dataset: Dataset,
    pub imported: usize,
    pub duplicates: usize,
    pub skipped: usize,
    pub media_missing: usize,
    pub rejected: Vec<RowIssue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetPreviewRequest {
    pub path: String,
    #[serde(default)]
    pub mapping: Option<DatasetMapping>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetRowsRequest {
    pub dataset_id: String,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetRowView {
    pub id: String,
    pub index: i64,
    pub status: String,
    pub generation_id: Option<String>,
    pub error: Option<String>,
    pub prompt: String,
    pub media_path: Option<String>,
    pub media_missing: bool,
    pub media_asset_ids: Vec<String>,
    pub media_status: String,
    pub media_thumbnails: Vec<String>,
    pub fields: BTreeMap<String, String>,
    pub params: Value,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetRows {
    pub dataset: Dataset,
    pub counts: BTreeMap<String, i64>,
    pub rows: Vec<DatasetRowView>,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetRunRequest {
    pub dataset_id: String,
    pub preset_id: String,
    pub guidance: Guidance,
    #[serde(default)]
    pub concurrency: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetStarted {
    pub job_id: String,
    #[serde(default)]
    pub item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetExportRequest {
    pub dataset_id: String,
    pub output_path: String,
    pub format: String,
    #[serde(default)]
    pub columns: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetExportSummary {
    pub output_path: String,
    pub format: String,
    pub count: usize,
    pub bytes: u64,
    pub columns: Vec<String>,
}

#[tauri::command]
pub async fn datasets_list(
    database: State<'_, Mutex<Database>>,
) -> Result<Vec<Dataset>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    repo::list(guard.connection())?
        .into_iter()
        .map(to_dataset)
        .collect()
}

#[tauri::command]
pub async fn datasets_preview(
    request: DatasetPreviewRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<DatasetPreview, CommandError> {
    let path = PathBuf::from(&request.path);
    if !path.exists() {
        return Err(CommandError::not_found("Dataset file or folder not found"));
    }
    let info = dataset::inspect(&path)?;
    let headers = dataset::headers(&path)?;
    let mapping = request
        .mapping
        .unwrap_or_else(|| dataset::auto_mapping(&headers));
    let mapping = if mapping.is_empty() {
        dataset::auto_mapping(&headers)
    } else {
        mapping
    };
    if mapping.source_for("prompt").is_none() {
        return Err(CommandError::validation(
            "Map a source field to the prompt before importing",
        ));
    }

    let mut rows = Vec::new();
    let summary = dataset::stream(&path, &mapping, Some(PREVIEW_ROWS), |row| {
        rows.push(row);
        Ok(())
    })?;

    let profiles = {
        let guard = database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        profile_repo::list(guard.connection())?
            .into_iter()
            .map(profile_from_row)
            .collect::<Result<Vec<_>, _>>()?
    };

    Ok(DatasetPreview {
        path: request.path,
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

/// Plain entry point shared by the Tauri command and the native facade.
pub fn import(
    request: DatasetImportRequest,
    database: &Mutex<Database>,
) -> Result<DatasetImportSummary, CommandError> {
    let path = PathBuf::from(&request.path);
    let metadata = std::fs::metadata(&path).map_err(|error| CommandError::from_io(&error))?;
    if metadata.is_file() && metadata.len() > MAX_IMPORT_BYTES {
        return Err(CommandError::validation(format!(
            "Dataset is larger than {} MB; split the file and import it in parts",
            MAX_IMPORT_BYTES / (1024 * 1024)
        )));
    }

    let headers = dataset::headers(&path)?;
    let mapping = request
        .mapping
        .clone()
        .filter(|mapping| !mapping.is_empty())
        .unwrap_or_else(|| dataset::auto_mapping(&headers));
    if mapping.source_for("prompt").is_none() {
        return Err(CommandError::validation(
            "Map a source field to the prompt before importing",
        ));
    }
    let dedupe = DedupeMode::parse(request.dedupe.as_deref().unwrap_or("skip"))?;

    let dataset_id = Uuid::now_v7().to_string();
    let name = request.name.clone().unwrap_or_else(|| {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dataset")
            .to_string()
    });

    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    repo::insert(
        connection,
        &repo::NewDataset {
            id: dataset_id.clone(),
            name,
            source_path: request.path.clone(),
            mapping_json: Some(
                serde_json::to_string(&mapping)
                    .map_err(|error| CommandError::internal(format!("mapping json: {error}")))?,
            ),
            export_columns_json: None,
            row_count: 0,
        },
    )?;

    let mut seen: HashMap<String, String> = HashMap::new();
    let mut imported = 0usize;
    let mut duplicates = 0usize;
    let mut media_missing = 0usize;
    let mut rejected = Vec::new();

    let summary = dataset::stream(&path, &mapping, None, |mapped| {
        if mapped.media_missing {
            media_missing += 1;
        }
        let key = dataset::dedupe_key(&mapped);
        match decide(dedupe, &seen, &key) {
            DedupeDecision::Skip => {
                duplicates += 1;
                return Ok(());
            }
            DedupeDecision::Merge(existing_id) => {
                duplicates += 1;
                merge_into_row(connection, &existing_id, &mapped)?;
                return Ok(());
            }
            DedupeDecision::Insert => {}
        }

        let item_id = Uuid::now_v7().to_string();
        let data_json = serde_json::to_string(&mapped)
            .map_err(|error| CommandError::internal(format!("serialize row: {error}")))?;
        repo::insert_item(
            connection,
            &repo::NewDatasetItem {
                id: item_id.clone(),
                dataset_id: dataset_id.clone(),
                row_index: imported as i64,
                data_json,
                status: "pending".into(),
            },
        )?;
        seen.insert(key, item_id);
        imported += 1;
        Ok(())
    })?;
    let rejected_count = summary.issues.len();
    rejected.extend(summary.issues);

    repo::set_row_count(connection, &dataset_id, imported as i64)?;
    if request.save_profile
        && let Some(profile_name) = request
            .profile_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
    {
        let mapping_json = serde_json::to_string(&mapping)
            .map_err(|error| CommandError::internal(format!("mapping json: {error}")))?;
        profile_repo::upsert(
            connection,
            &Uuid::now_v7().to_string(),
            profile_name,
            &mapping_json,
        )?;
    }

    let row = repo::get(connection, &dataset_id)?
        .ok_or_else(|| CommandError::internal("dataset insert vanished"))?;
    Ok(DatasetImportSummary {
        dataset: to_dataset(row)?,
        imported,
        duplicates,
        skipped: rejected_count,
        media_missing,
        rejected,
    })
}

#[tauri::command]
pub async fn datasets_import(
    request: DatasetImportRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<DatasetImportSummary, CommandError> {
    import(request, database.inner())
}

#[tauri::command]
pub async fn datasets_rows(
    request: DatasetRowsRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<DatasetRows, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let dataset = repo::get(connection, &request.dataset_id)?
        .ok_or_else(|| CommandError::not_found("Dataset not found"))?;
    let offset = request.offset.unwrap_or(0).max(0) as usize;
    let limit = request.limit.unwrap_or(200).clamp(1, 1000) as usize;
    let items = repo::list_items(connection, &request.dataset_id)?;
    let total = items.len() as i64;
    let rows = items
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|item| {
            let data: Option<MappedRow> = serde_json::from_str(&item.data_json).ok();
            let media_status = media_status(&item.media_asset_ids, data.as_ref()).to_string();
            let media_thumbnails = item
                .media_asset_ids
                .iter()
                .filter_map(|id| asset_repo::get(connection, id).ok().flatten())
                .map(|asset| asset.thumbnail_path.unwrap_or(asset.source_path))
                .collect::<Vec<_>>();
            DatasetRowView {
                id: item.id,
                index: item.row_index,
                status: item.status,
                generation_id: item.generation_id,
                error: item.error,
                prompt: data
                    .as_ref()
                    .map(|row| row.prompt.clone())
                    .unwrap_or_default(),
                media_path: data.as_ref().and_then(|row| row.media_path.clone()),
                media_missing: data.as_ref().is_some_and(|row| row.media_missing),
                media_asset_ids: item.media_asset_ids,
                media_status,
                media_thumbnails,
                fields: data
                    .as_ref()
                    .map(|row| row.fields.clone())
                    .unwrap_or_default(),
                params: data
                    .as_ref()
                    .map(|row| row.params.clone())
                    .unwrap_or_else(|| serde_json::json!({})),
                tags: data.map(|row| row.tags).unwrap_or_default(),
            }
        })
        .collect();
    let counts = repo::status_counts(connection, &request.dataset_id)?
        .into_iter()
        .collect();

    Ok(DatasetRows {
        dataset: to_dataset(dataset)?,
        counts,
        rows,
        total,
    })
}

fn dataset_root(source: &std::path::Path) -> PathBuf {
    if source.is_dir() {
        source.to_path_buf()
    } else {
        source
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

fn unfinished(items: Vec<repo::DatasetItemRow>) -> Vec<repo::DatasetItemRow> {
    items
        .into_iter()
        .filter(|item| item.status != "succeeded")
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn prepare_dataset_payloads(
    importer: &ImportService<'_>,
    access: &MediaAccess,
    selected: &[repo::DatasetItemRow],
    preset_id: &str,
    guidance: &Guidance,
    source_path: &std::path::Path,
) -> Result<Vec<Value>, CommandError> {
    let connection = importer.connection;
    let root = dataset_root(source_path);
    let cancel = AtomicBool::new(false);
    let mut imported: HashMap<String, Result<String, String>> = HashMap::new();
    let mut payloads = Vec::with_capacity(selected.len());

    for item in selected {
        let data: MappedRow = serde_json::from_str(&item.data_json).map_err(|error| {
            CommandError::validation(format!(
                "Dataset row {} is unreadable: {error}",
                item.row_index + 1
            ))
        })?;

        if data.media_missing {
            repo::update_item_status(
                connection,
                &item.id,
                "failed",
                None,
                Some("Media file is missing under the dataset root"),
            )?;
            continue;
        }

        let segments = data
            .media_path
            .as_deref()
            .map(parse_media_segments)
            .unwrap_or_default();
        let mut asset_ids = Vec::with_capacity(segments.len());
        let mut weights = Vec::with_capacity(segments.len());
        let mut failure: Option<String> = None;

        for segment in &segments {
            let (resolved, missing) = match dataset::resolve_media(&root, &segment.path) {
                Ok(value) => value,
                Err(reason) => {
                    failure.get_or_insert(reason);
                    continue;
                }
            };
            if missing {
                failure.get_or_insert_with(|| format!("Media file is missing: {resolved}"));
                continue;
            }

            let value = match imported.get(&resolved) {
                Some(value) => value.clone(),
                None => {
                    let results = importer.import(std::slice::from_ref(&resolved), &cancel, |_| {});
                    let value = match results.into_iter().next() {
                        Some(result) => match result.error {
                            Some(error) => Err(error),
                            None => match result.asset_id {
                                Some(asset_id) => Ok(asset_id),
                                None => Err("Media import returned no asset".to_string()),
                            },
                        },
                        None => Err("Media import returned no result".to_string()),
                    };
                    if value.is_ok() {
                        access.allow_path(std::path::Path::new(&resolved));
                    }
                    imported.insert(resolved.clone(), value.clone());
                    value
                }
            };

            match value {
                Ok(asset_id) => {
                    asset_ids.push(asset_id);
                    weights.push(segment.weight);
                }
                Err(reason) => {
                    failure.get_or_insert(format!("Could not import {resolved}: {reason}"));
                }
            }
        }

        if let Some(reason) = failure {
            repo::update_item_status(connection, &item.id, "failed", None, Some(&reason))?;
            continue;
        }

        if !asset_ids.is_empty() {
            repo::set_item_media_assets(connection, &item.id, &asset_ids)?;
        }
        let references = asset_ids
            .iter()
            .zip(weights.iter())
            .map(|(asset_id, weight)| ComposeReference {
                asset_id: Some(asset_id.clone()),
                edit_id: None,
                weight: Some(*weight),
                name: None,
                description: None,
            })
            .collect::<Vec<_>>();

        payloads.push(
            serde_json::to_value(ComposeRequest {
                preset_id: preset_id.to_string(),
                asset_id: asset_ids.first().cloned(),
                edit_id: None,
                references,
                parent_id: None,
                dataset_row_id: Some(item.id.clone()),
                description: None,
                brief: Some(data.prompt),
                guidance: guidance.clone(),
                locked: Vec::new(),
                params: data.params,
                existing_prompt: None,
                instruction: None,
                seed: None,
                variations: 1,
                bypass_cache: true,
                output_language: None,
                include_images: false,
            })
            .map_err(|error| CommandError::internal(format!("serialize row: {error}")))?,
        );
        repo::update_item_status(connection, &item.id, "queued", None, None)?;
    }

    Ok(payloads)
}

/// Plain entry point shared by the Tauri command and the native facade.
///
/// Resumes the queue runner in the Tauri wrapper only; the native facade has
/// no background runner, so enqueued rows stay queued until one is started.
pub fn enqueue_run(
    request: DatasetRunRequest,
    database: &Mutex<Database>,
    tools: &MediaTools,
    app_paths: &AppPaths,
    access: &MediaAccess,
) -> Result<DatasetStarted, CommandError> {
    let items = {
        let guard = database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let connection = guard.connection();
        let preset = preset_repo::get(connection, &request.preset_id)?
            .filter(|row| row.deleted_at.is_none())
            .ok_or_else(|| CommandError::not_found("Preset not found"))?;
        if preset.deleted_at.is_some() {
            return Err(CommandError::not_found("Preset not found"));
        }
        let items = repo::list_items(connection, &request.dataset_id)?;
        let selected = unfinished(items);
        if selected.is_empty() {
            return Err(CommandError::conflict(
                "Every row has already succeeded; nothing to run",
            ));
        }

        let dataset_row = repo::get(connection, &request.dataset_id)?
            .ok_or_else(|| CommandError::not_found("Dataset not found"))?;
        let importer = ImportService {
            tools,
            connection,
            paths: app_paths,
        };
        let payloads = prepare_dataset_payloads(
            &importer,
            access,
            &selected,
            &request.preset_id,
            &request.guidance,
            std::path::Path::new(&dataset_row.source_path),
        )?;
        if payloads.is_empty() {
            return Err(CommandError::conflict(
                "No dataset row could be prepared; check the reported media errors",
            ));
        }
        payloads
    };

    let result = job_queue::enqueue(
        database,
        EnqueueSpec {
            kind: "compose".into(),
            payload: serde_json::json!({
                "datasetId": request.dataset_id,
                "presetId": request.preset_id,
            }),
            items,
            priority: 0,
            dedupe_key: None,
            note: Some("Dataset batch".into()),
        },
    )?;
    Ok(DatasetStarted {
        job_id: result.job.id,
        item_count: result.job.total_items as usize,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn datasets_run(
    request: DatasetRunRequest,
    database: State<'_, Mutex<Database>>,
    queue: State<'_, std::sync::Arc<job_queue::JobQueue>>,
    app_paths: State<'_, AppPaths>,
    tools: State<'_, MediaTools>,
    access: State<'_, MediaAccess>,
) -> Result<DatasetStarted, CommandError> {
    let result = enqueue_run(
        request,
        database.inner(),
        tools.inner(),
        app_paths.inner(),
        access.inner(),
    )?;
    queue.resume();
    Ok(result)
}

/// Plain entry point shared by the Tauri command and the native facade.
pub fn export(
    request: DatasetExportRequest,
    database: &Mutex<Database>,
) -> Result<DatasetExportSummary, CommandError> {
    let path = PathBuf::from(&request.output_path);
    if let Some(parent) = path.parent()
        && !parent.is_dir()
    {
        return Err(CommandError::validation(format!(
            "Output folder does not exist: {}",
            parent.display()
        )));
    }

    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let dataset = repo::get(connection, &request.dataset_id)?
        .ok_or_else(|| CommandError::not_found("Dataset not found"))?;
    let items = repo::list_items(connection, &request.dataset_id)?;
    if items.is_empty() {
        return Err(CommandError::validation("Dataset has no rows to export"));
    }

    let original_columns = items
        .iter()
        .find_map(|item| {
            serde_json::from_str::<MappedRow>(&item.data_json)
                .ok()
                .map(|row| row.fields.keys().cloned().collect::<Vec<_>>())
        })
        .unwrap_or_default();
    let generated_columns = [
        "mediaAssetId",
        "generatedPrompt",
        "generatedNegative",
        "generatedParams",
        "generatedStatus",
        "generatedError",
        "generationId",
        "status",
    ];
    let columns = request
        .columns
        .clone()
        .or_else(|| {
            dataset
                .export_columns_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
        })
        .filter(|columns| !columns.is_empty())
        .unwrap_or_else(|| {
            original_columns
                .iter()
                .cloned()
                .chain(generated_columns.iter().map(|column| column.to_string()))
                .collect()
        });

    let rows = items.into_iter().map(|item| {
        let generation = item
            .generation_id
            .as_deref()
            .and_then(|id| gen_repo::get(connection, id).ok().flatten());
        export_row(item, generation.as_ref())
    });

    let (count, bytes) = dataset::write_rows(&path, &request.format, &columns, rows)?;
    let columns_json = serde_json::to_string(&columns)
        .map_err(|error| CommandError::internal(format!("serialize columns: {error}")))?;
    repo::set_export_columns(connection, &request.dataset_id, &columns_json)?;

    Ok(DatasetExportSummary {
        output_path: request.output_path,
        format: request.format,
        count,
        bytes,
        columns,
    })
}

#[tauri::command]
pub async fn datasets_export(
    request: DatasetExportRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<DatasetExportSummary, CommandError> {
    export(request, database.inner())
}

#[tauri::command]
pub async fn datasets_delete(
    dataset_id: String,
    database: State<'_, Mutex<Database>>,
) -> Result<(), CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    if !repo::delete(guard.connection(), &dataset_id)? {
        return Err(CommandError::not_found("Dataset not found"));
    }
    Ok(())
}

#[tauri::command]
pub async fn datasets_profiles(
    database: State<'_, Mutex<Database>>,
) -> Result<Vec<ImportProfile>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    profile_repo::list(guard.connection())?
        .into_iter()
        .map(profile_from_row)
        .collect()
}

fn export_row(
    item: repo::DatasetItemRow,
    generation: Option<&gen_repo::GenerationRow>,
) -> BTreeMap<String, String> {
    let mut values: BTreeMap<String, String> = serde_json::from_str::<MappedRow>(&item.data_json)
        .map(|row| row.fields)
        .unwrap_or_default();
    values.insert("generatedStatus".into(), item.status.clone());
    values.insert("status".into(), item.status.clone());
    values.insert("mediaAssetId".into(), item.media_asset_ids.join("|"));
    values.insert("generatedError".into(), item.error.unwrap_or_default());
    if let Some(generation) = generation {
        values.insert("generationId".into(), generation.id.clone());
        values.insert("generatedPrompt".into(), generation.prompt.clone());
        values.insert(
            "generatedNegative".into(),
            generation.negative.clone().unwrap_or_default(),
        );
        values.insert("generatedParams".into(), generation.params_json.clone());
    } else {
        values.insert(
            "generationId".into(),
            item.generation_id.unwrap_or_default(),
        );
    }
    values
}

fn media_status(ids: &[String], data: Option<&MappedRow>) -> &'static str {
    if !ids.is_empty() {
        return "imported";
    }
    match data {
        Some(row) if row.media_missing => "missing",
        Some(row) if row.media_path.is_some() => "pending",
        _ => "none",
    }
}

fn to_dataset(row: repo::DatasetRow) -> Result<Dataset, CommandError> {
    Ok(Dataset {
        id: row.id,
        name: row.name,
        source_path: row.source_path,
        row_count: row.row_count,
        created_at: row.created_at,
        mapping: row
            .mapping_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default(),
        export_columns: row
            .export_columns_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default(),
    })
}

fn profile_from_row(row: profile_repo::ImportProfileRow) -> Result<ImportProfile, CommandError> {
    let mapping = serde_json::from_str(&row.mapping_json)
        .map_err(|error| CommandError::internal(format!("read import profile: {error}")))?;
    Ok(ImportProfile {
        name: row.name,
        mapping,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DedupeDecision {
    Insert,
    Skip,
    Merge(String),
}

fn decide(mode: DedupeMode, seen: &HashMap<String, String>, key: &str) -> DedupeDecision {
    let Some(existing) = seen.get(key) else {
        return DedupeDecision::Insert;
    };
    match mode {
        DedupeMode::Skip => DedupeDecision::Skip,
        DedupeMode::Merge => DedupeDecision::Merge(existing.clone()),
        DedupeMode::Keep => DedupeDecision::Insert,
    }
}

fn merge_into_row(
    connection: &rusqlite::Connection,
    item_id: &str,
    duplicate: &MappedRow,
) -> Result<(), CommandError> {
    let Some(item) = repo::get_item(connection, item_id)? else {
        return Ok(());
    };
    let Ok(mut existing) = serde_json::from_str::<MappedRow>(&item.data_json) else {
        return Ok(());
    };
    for tag in &duplicate.tags {
        if !existing.tags.iter().any(|current| current == tag) {
            existing.tags.push(tag.clone());
        }
    }
    if let (Some(target), Some(source)) = (
        existing.params.as_object_mut(),
        duplicate.params.as_object(),
    ) {
        for (key, value) in source {
            target.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    if existing.negative.is_none() {
        existing.negative = duplicate.negative.clone();
    }
    if existing.media_path.is_none() {
        existing.media_path = duplicate.media_path.clone();
        existing.media_missing = duplicate.media_missing;
    }
    for (key, value) in &duplicate.fields {
        existing
            .fields
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
    let json = serde_json::to_string(&existing)
        .map_err(|error| CommandError::internal(format!("serialize merged row: {error}")))?;
    repo::update_item_data(connection, item_id, &json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use serde_json::json;

    use crate::dataset::{DatasetMapping, MappedRow};
    use crate::db::Database;
    use crate::db::repository::assets as asset_repo;
    use crate::db::repository::datasets as repo;
    use crate::media::import::ImportService;
    use crate::media::protocol::MediaAccess;
    use crate::media::test_support::{make_image, temp_dir, tools};
    use crate::paths::resolve_app_paths;

    use super::{
        export_row, media_status, merge_into_row, prepare_dataset_payloads, to_dataset, unfinished,
    };

    fn database() -> Database {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        database
    }

    #[test]
    fn media_status_covers_text_single_and_multi_rows() {
        let text = MappedRow {
            row: 0,
            fields: Default::default(),
            prompt: "x".into(),
            negative: None,
            params: serde_json::json!({}),
            media_path: None,
            media_missing: false,
            tags: Vec::new(),
            identifier: None,
        };
        assert_eq!(media_status(&[], Some(&text)), "none");

        let single = MappedRow {
            media_path: Some("a.png".into()),
            ..text.clone()
        };
        assert_eq!(media_status(&[], Some(&single)), "pending");
        assert_eq!(
            media_status(&["asset-1".to_string()], Some(&single)),
            "imported"
        );

        let missing = MappedRow {
            media_path: Some("gone.png".into()),
            media_missing: true,
            ..text.clone()
        };
        assert_eq!(media_status(&[], Some(&missing)), "missing");

        assert_eq!(
            media_status(
                &["asset-1".to_string(), "asset-2".to_string()],
                Some(&single)
            ),
            "imported"
        );
    }

    #[test]
    fn dataset_run_prepares_media_and_isolates_failures() {
        let source_dir = temp_dir("dataset-run-media");
        let cache_dir = temp_dir("dataset-run-cache");
        let tools = tools();
        let image_a = source_dir.join("a.png");
        let image_b = source_dir.join("b.png");
        make_image(&tools, &image_a, "32x24");
        make_image(&tools, &image_b, "48x32");

        let database = database();
        let connection = database.connection();
        let paths = resolve_app_paths(&source_dir, &source_dir, &cache_dir, false).expect("paths");
        let access = MediaAccess::new(cache_dir.clone());
        let importer = ImportService {
            tools: &tools,
            connection,
            paths: &paths,
        };
        let source = source_dir.join("rows.csv");

        repo::insert(
            connection,
            &repo::NewDataset {
                id: "ds-run".into(),
                name: "rows".into(),
                source_path: source.display().to_string(),
                mapping_json: None,
                export_columns_json: None,
                row_count: 5,
            },
        )
        .expect("dataset");

        let row = |index: i64, media: Option<String>, missing: bool| MappedRow {
            row: index as usize,
            fields: Default::default(),
            prompt: "a fox".into(),
            negative: None,
            params: json!({}),
            media_path: media,
            media_missing: missing,
            tags: Vec::new(),
            identifier: None,
        };
        let rows = [
            ("row-a", 0, Some(image_a.display().to_string()), false),
            ("row-b", 1, Some(image_a.display().to_string()), false),
            (
                "row-c",
                2,
                Some(format!("{}@0.8|{}", image_a.display(), image_b.display())),
                false,
            ),
            (
                "row-d",
                3,
                Some(source_dir.join("missing.png").display().to_string()),
                true,
            ),
            ("row-e", 4, Some("../outside.png".to_string()), false),
        ];
        for (id, index, media, missing) in rows {
            repo::insert_item(
                connection,
                &repo::NewDatasetItem {
                    id: id.into(),
                    dataset_id: "ds-run".into(),
                    row_index: index,
                    data_json: serde_json::to_string(&row(index, media, missing)).expect("json"),
                    status: "pending".into(),
                },
            )
            .expect("row");
        }

        // A succeeded row is skipped by the run selection.
        repo::update_item_status(connection, "row-b", "succeeded", None, None).expect("succeed");
        let selected = unfinished(repo::list_items(connection, "ds-run").expect("rows"));
        assert_eq!(selected.len(), 4);

        let payloads = prepare_dataset_payloads(
            &importer,
            &access,
            &selected,
            "mmh3",
            &crate::commands::Guidance::default(),
            &source,
        )
        .expect("prepare");

        // row-b is excluded, row-d (missing) and row-e (traversal) fail, leaving a and c.
        assert_eq!(payloads.len(), 2);
        assert!(payloads[0]["references"][0]["assetId"].as_str().is_some());
        let refs = payloads[1]["references"].as_array().expect("refs");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0]["weight"], json!(0.8));
        assert_eq!(refs[1]["weight"], json!(1.0));

        let items = repo::list_items(connection, "ds-run").expect("rows");
        assert_eq!(items[3].status, "failed");
        assert!(items[3].error.is_some());
        assert_eq!(items[4].status, "failed");
        assert!(
            items[4]
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("escapes")
        );

        let assets = asset_repo::list(connection, 50, 0).expect("assets");
        assert_eq!(assets.len(), 2, "media should be imported once each");

        // Re-running reuses the stored assets instead of importing again.
        let again = prepare_dataset_payloads(
            &importer,
            &access,
            &unfinished(items),
            "mmh3",
            &crate::commands::Guidance::default(),
            &source,
        )
        .expect("prepare again");
        assert!(!again.is_empty());
        assert_eq!(
            asset_repo::list(connection, 50, 0).expect("assets").len(),
            2
        );
    }

    #[test]
    fn export_rows_carry_media_and_generation_provenance() {
        let fields = BTreeMap::from([("caption".to_string(), "first".to_string())]);
        let data = MappedRow {
            row: 0,
            fields,
            prompt: "a fox".into(),
            negative: None,
            params: json!({}),
            media_path: Some("a.png".into()),
            media_missing: false,
            tags: Vec::new(),
            identifier: None,
        };
        let item = repo::DatasetItemRow {
            id: "row-1".into(),
            dataset_id: "ds-1".into(),
            row_index: 0,
            data_json: serde_json::to_string(&data).expect("json"),
            status: "succeeded".into(),
            generation_id: Some("gen-1".into()),
            error: None,
            media_asset_ids: vec!["asset-1".into(), "asset-2".into()],
        };

        let values = export_row(item, None);
        assert_eq!(values.get("caption").map(String::as_str), Some("first"));
        assert_eq!(
            values.get("mediaAssetId").map(String::as_str),
            Some("asset-1|asset-2")
        );
        assert_eq!(
            values.get("generationId").map(String::as_str),
            Some("gen-1")
        );
        assert_eq!(values.get("status").map(String::as_str), Some("succeeded"));
    }

    #[test]
    fn dataset_rows_round_trip_through_the_repository() {
        let database = database();
        let connection = database.connection();
        let dataset = repo::insert(
            connection,
            &repo::NewDataset {
                id: "ds-1".into(),
                name: "test".into(),
                source_path: "C:\\data\\rows.csv".into(),
                mapping_json: Some(json!({"fields": {"caption": "prompt"}}).to_string()),
                export_columns_json: Some(json!(["caption"]).to_string()),
                row_count: 1,
            },
        )
        .expect("dataset");
        let summary = to_dataset(dataset).expect("dto");
        assert_eq!(summary.mapping.source_for("prompt"), Some("caption"));
        assert_eq!(summary.export_columns, vec!["caption"]);

        let mapped = MappedRow {
            row: 1,
            fields: [("caption".into(), "a fox".into())].into_iter().collect(),
            prompt: "a fox".into(),
            negative: None,
            params: json!({"aspect_ratio": "1:1"}),
            media_path: None,
            media_missing: false,
            tags: vec!["neon".into()],
            identifier: None,
        };
        repo::insert_item(
            connection,
            &repo::NewDatasetItem {
                id: "row-1".into(),
                dataset_id: "ds-1".into(),
                row_index: 0,
                data_json: serde_json::to_string(&mapped).expect("json"),
                status: "pending".into(),
            },
        )
        .expect("item");

        assert_eq!(repo::queue_items(connection, "ds-1").expect("queue"), 1);
        assert_eq!(
            repo::status_counts(connection, "ds-1").expect("counts"),
            vec![("queued".to_string(), 1)]
        );
        repo::update_item_status(connection, "row-1", "succeeded", None, None).expect("update");
        let items = repo::list_items(connection, "ds-1").expect("items");
        assert_eq!(items[0].status, "succeeded");
        assert_eq!(items[0].generation_id, None);
    }

    #[test]
    fn merge_combines_tags_params_and_missing_fields() {
        let database = database();
        let connection = database.connection();
        repo::insert(
            connection,
            &repo::NewDataset {
                id: "ds-1".into(),
                name: "test".into(),
                source_path: "C:\\data\\rows.csv".into(),
                mapping_json: None,
                export_columns_json: None,
                row_count: 1,
            },
        )
        .expect("dataset");
        let first = MappedRow {
            row: 1,
            fields: [("prompt".into(), "a fox".into())].into_iter().collect(),
            prompt: "a fox".into(),
            negative: None,
            params: json!({"aspect_ratio": "1:1"}),
            media_path: None,
            media_missing: false,
            tags: vec!["neon".into()],
            identifier: None,
        };
        repo::insert_item(
            connection,
            &repo::NewDatasetItem {
                id: "row-1".into(),
                dataset_id: "ds-1".into(),
                row_index: 0,
                data_json: serde_json::to_string(&first).expect("json"),
                status: "pending".into(),
            },
        )
        .expect("item");

        let second = MappedRow {
            row: 2,
            fields: [
                ("prompt".into(), "a fox".into()),
                ("extra".into(), "x".into()),
            ]
            .into_iter()
            .collect(),
            prompt: "a fox".into(),
            negative: Some("blurry".into()),
            params: json!({"seed": 7}),
            media_path: Some("C:\\data\\fox.png".into()),
            media_missing: false,
            tags: vec!["neon".into(), "snow".into()],
            identifier: None,
        };
        merge_into_row(connection, "row-1", &second).expect("merge");
        let item = repo::get_item(connection, "row-1")
            .expect("get")
            .expect("present");
        let merged: MappedRow = serde_json::from_str(&item.data_json).expect("parse");
        assert_eq!(merged.tags, vec!["neon", "snow"]);
        assert_eq!(merged.params["aspect_ratio"], "1:1");
        assert_eq!(merged.params["seed"], 7);
        assert_eq!(merged.negative.as_deref(), Some("blurry"));
        assert_eq!(merged.fields["extra"], "x");
    }

    #[test]
    fn csv_export_round_trips_through_the_same_profile() {
        let directory = temp_dir("dataset-roundtrip");
        let input = directory.join("rows.csv");
        fs::write(&input, b"caption,tags\nfirst,neon\nsecond,snow\n").expect("csv");
        let headers = crate::dataset::headers(&input).expect("headers");
        let mapping = crate::dataset::auto_mapping(&headers);
        assert_eq!(mapping.source_for("prompt"), Some("caption"));

        let mut rows = Vec::new();
        crate::dataset::stream(&input, &mapping, None, |row| {
            rows.push(row);
            Ok(())
        })
        .expect("stream");
        assert_eq!(rows.len(), 2);

        let output = directory.join("out.csv");
        let columns = vec![
            "caption".to_string(),
            "tags".to_string(),
            "generatedPrompt".to_string(),
        ];
        let table = rows.iter().map(|row| {
            let mut values = row.fields.clone();
            values.insert(
                "generatedPrompt".into(),
                format!("generated {}", row.prompt),
            );
            values
        });
        crate::dataset::write_rows(&output, "csv", &columns, table).expect("export");
        let text = fs::read_to_string(&output).expect("read export");
        assert!(text.contains("generated first"));

        let headers = crate::dataset::headers(&output).expect("roundtrip headers");
        let remapped = crate::dataset::auto_mapping(&headers);
        assert_eq!(remapped.source_for("prompt"), Some("caption"));
        let _ = DatasetMapping::default();
    }
}

#[cfg(test)]
mod extra_tests {
    use std::collections::HashMap;
    use std::io::Write;

    use super::decide;
    use crate::dataset::{self, DedupeMode, MappedRow};
    use crate::db::Database;
    use crate::db::repository::datasets as repo;
    use crate::db::repository::import_profiles as profiles;
    use crate::media::test_support::temp_dir;

    fn database() -> Database {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        database
    }

    #[test]
    fn ninety_seven_of_one_hundred_rows_import() {
        let directory = temp_dir("dataset-97");
        let path = directory.join("rows.csv");
        let mut file = std::fs::File::create(&path).expect("create");
        writeln!(file, "caption,note").expect("header");
        for index in 1..=100 {
            if index % 33 == 0 {
                writeln!(file, ",missing").expect("blank prompt");
            } else {
                writeln!(file, "prompt {index},ok").expect("row");
            }
        }
        drop(file);

        let mapping = dataset::auto_mapping(&dataset::headers(&path).expect("headers"));
        let summary = dataset::stream(&path, &mapping, None, |_| Ok(())).expect("stream");
        assert_eq!(summary.total, 100);
        assert_eq!(summary.valid, 97);
        assert_eq!(summary.issues.len(), 3);
        assert_eq!(summary.issues[0].row, 33);
        assert!(summary.issues[0].message.contains("no prompt"));
    }

    #[test]
    fn hundred_thousand_rows_stream_without_collecting() {
        let directory = temp_dir("dataset-100k");
        let path = directory.join("rows.txt");
        let mut file = std::fs::File::create(&path).expect("create");
        for index in 0..100_000 {
            writeln!(file, "prompt number {index}").expect("row");
        }
        drop(file);

        let mapping = dataset::auto_mapping(&["text".into()]);
        let mut count = 0usize;
        let summary = dataset::stream(&path, &mapping, None, |_| {
            count += 1;
            Ok(())
        })
        .expect("stream");
        assert_eq!(summary.total, 100_000);
        assert_eq!(count, 100_000);
    }

    #[test]
    fn dedupe_modes_choose_skip_merge_or_keep() {
        let mut seen = HashMap::new();
        seen.insert("prompt:a fox".to_string(), "row-1".to_string());
        assert_eq!(
            decide(DedupeMode::Skip, &seen, "prompt:a fox"),
            super::DedupeDecision::Skip
        );
        assert_eq!(
            decide(DedupeMode::Merge, &seen, "prompt:a fox"),
            super::DedupeDecision::Merge("row-1".into())
        );
        assert_eq!(
            decide(DedupeMode::Keep, &seen, "prompt:a fox"),
            super::DedupeDecision::Insert
        );
        assert_eq!(
            decide(DedupeMode::Skip, &seen, "prompt:new"),
            super::DedupeDecision::Insert
        );
    }

    #[test]
    fn import_profiles_are_reusable_by_name() {
        let database = database();
        let connection = database.connection();
        let mapping_json = serde_json::json!({"fields": {"caption": "prompt"}}).to_string();
        profiles::upsert(connection, "profile-1", "captions", &mapping_json).expect("save");
        profiles::upsert(
            connection,
            "profile-2",
            "captions",
            &serde_json::json!({"fields": {"text": "prompt"}}).to_string(),
        )
        .expect("replace");
        let found = profiles::get_by_name(connection, "captions")
            .expect("get")
            .expect("present");
        assert_eq!(found.id, "profile-1");
        assert!(found.mapping_json.contains("text"));
        assert_eq!(profiles::list(connection).expect("list").len(), 1);
    }

    #[test]
    fn resume_skips_succeeded_rows() {
        let database = database();
        let connection = database.connection();
        repo::insert(
            connection,
            &repo::NewDataset {
                id: "ds-1".into(),
                name: "test".into(),
                source_path: "C:\\data\\rows.csv".into(),
                mapping_json: None,
                export_columns_json: None,
                row_count: 2,
            },
        )
        .expect("dataset");
        let make_row = |id: &str, index: i64, status: &str, data: &MappedRow| {
            repo::insert_item(
                connection,
                &repo::NewDatasetItem {
                    id: id.into(),
                    dataset_id: "ds-1".into(),
                    row_index: index,
                    data_json: serde_json::to_string(data).expect("json"),
                    status: status.into(),
                },
            )
            .expect("item");
        };
        let data = MappedRow {
            row: 1,
            fields: Default::default(),
            prompt: "a fox".into(),
            negative: None,
            params: serde_json::json!({}),
            media_path: None,
            media_missing: false,
            tags: Vec::new(),
            identifier: None,
        };
        make_row("row-1", 0, "succeeded", &data);
        make_row("row-2", 1, "pending", &data);

        assert_eq!(repo::queue_items(connection, "ds-1").expect("queue"), 1);
        let items = repo::list_items(connection, "ds-1").expect("items");
        assert_eq!(items[0].status, "succeeded");
        assert_eq!(items[1].status, "queued");
    }
}
