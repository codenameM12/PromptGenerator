use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::State;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::commands::edits::EditOps;
use crate::commands::pipeline::{ComposeReference, ComposeRequest};
use crate::commands::workspace::WorkspaceDraft;
use crate::db::Database;
use crate::db::repository::assets as asset_repo;
use crate::db::repository::collections as collection_repo;
use crate::db::repository::edits as edit_repo;
use crate::db::repository::frames as frame_repo;
use crate::db::repository::generations::{self as gen_repo, GenerationFilter, GenerationRow};
use crate::db::repository::presets as preset_repo;
use crate::db::repository::tags as tag_repo;
use crate::job_queue::{self, EnqueueSpec};
use crate::paths::AppPaths;
use crate::workspace::WorkspaceStore;

use super::presets::PresetKind;
use super::{CommandError, Guidance, Page};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationQuery {
    pub text: Option<String>,
    pub preset_id: Option<String>,
    pub kind: Option<PresetKind>,
    pub model: Option<String>,
    pub favorite: Option<bool>,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub collection_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub ids: Vec<String>,
    #[serde(default)]
    pub include_deleted: bool,
    #[serde(default)]
    pub only_deleted: bool,
    pub offset: i64,
    pub limit: i64,
    pub sort: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationRecord {
    pub id: String,
    pub parent_id: Option<String>,
    pub asset_id: Option<String>,
    pub edit_id: Option<String>,
    pub prompt: String,
    pub negative: Option<String>,
    pub params: Value,
    pub guidance: Guidance,
    pub description: Option<Value>,
    pub preset_id: String,
    pub preset_version: String,
    pub preset_hash: String,
    pub models: Vec<String>,
    pub seed: Option<u64>,
    pub status: String,
    pub error: Option<String>,
    pub favorite: bool,
    pub rating: Option<u8>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub collections: Vec<String>,
    pub created_at: String,
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationReference {
    pub asset_id: Option<String>,
    pub edit_id: Option<String>,
    pub name: Option<String>,
    pub weight: Option<f64>,
    pub thumbnail_path: Option<String>,
    pub missing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationDetail {
    #[serde(flatten)]
    pub record: GenerationRecord,
    pub raw_response: Option<String>,
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
    pub latency_ms: Option<u64>,
    pub cache_key: Option<String>,
    pub references: Vec<GenerationReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteRequest {
    pub id: String,
    pub favorite: bool,
    #[serde(default)]
    pub asset: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGenerationRequest {
    pub ids: Vec<String>,
    pub favorite: Option<bool>,
    pub rating: Option<u8>,
    #[serde(default)]
    pub clear_rating: bool,
    pub notes: Option<String>,
    #[serde(default)]
    pub add_tags: Vec<String>,
    #[serde(default)]
    pub remove_tags: Vec<String>,
    pub collection_id: Option<String>,
    pub collection_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagSummary {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NameRequest {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionRequest {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionAssignRequest {
    pub collection_id: String,
    pub generation_ids: Vec<String>,
    pub add: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub prompt: String,
    pub negative: Option<String>,
    pub seed: Option<u64>,
    pub status: String,
    pub preset_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionTree {
    pub generations: Vec<VersionNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRequest {
    pub left_id: String,
    pub right_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDiff {
    pub field: String,
    pub left: Value,
    pub right: Value,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub draft: WorkspaceDraft,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashRequest {
    #[serde(default)]
    pub generation_ids: Vec<String>,
    #[serde(default)]
    pub asset_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashSummary {
    pub generations: usize,
    pub assets: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeRequest {
    #[serde(default)]
    pub generation_ids: Vec<String>,
    #[serde(default)]
    pub asset_ids: Vec<String>,
    pub confirm: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeSummary {
    pub generations: usize,
    pub assets: usize,
    pub bytes_reclaimed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetTrashEntry {
    pub id: String,
    pub display_name: String,
    pub kind: String,
    pub source_path: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub format: String,
    pub output_path: String,
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub output_path: String,
    pub format: String,
    pub count: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub path: String,
    pub format: String,
    pub imported: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RerunRequest {
    pub ids: Vec<String>,
    pub preset_id: String,
    pub guidance: Option<Guidance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RerunSummary {
    pub job_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRequest {
    pub output_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub output_path: String,
    pub bytes: u64,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRequest {
    pub backup_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSummary {
    pub backup_path: String,
    pub pre_restore_backup: Option<String>,
    pub bundle: bool,
    pub restart_required: bool,
    pub token_restored: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportRecord {
    id: String,
    parent_id: Option<String>,
    asset_id: Option<String>,
    edit_id: Option<String>,
    prompt: String,
    negative: Option<String>,
    params: Value,
    guidance: Guidance,
    description: Option<Value>,
    preset_id: String,
    preset_version: String,
    preset_hash: String,
    models: Vec<String>,
    seed: Option<u64>,
    status: String,
    error: Option<String>,
    favorite: bool,
    rating: Option<u8>,
    notes: Option<String>,
    tags: Vec<String>,
    created_at: String,
}

#[tauri::command]
pub async fn library_query(
    query: GenerationQuery,
    database: State<'_, Mutex<Database>>,
) -> Result<Page<GenerationRecord>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let filter = GenerationFilter {
        text: query.text,
        preset_id: query.preset_id,
        kind: query.kind.map(kind_str).map(ToOwned::to_owned),
        model: query.model,
        favorite: query.favorite,
        status: query.status,
        tags: query.tags,
        collection_id: query.collection_id,
        from: query.from,
        to: query.to,
        ids: if query.ids.is_empty() {
            None
        } else {
            Some(query.ids)
        },
        include_deleted: query.include_deleted,
        only_deleted: query.only_deleted,
        sort: query.sort,
        limit: query.limit,
        offset: query.offset,
    };
    let (rows, total) = gen_repo::query(connection, &filter)?;
    let items = rows
        .iter()
        .map(|row| record(connection, row))
        .collect::<Result<Vec<_>, CommandError>>()?;
    Ok(Page {
        items,
        total,
        offset: query.offset,
        limit: query.limit,
    })
}

#[tauri::command]
pub async fn library_get(
    id: String,
    database: State<'_, Mutex<Database>>,
) -> Result<GenerationDetail, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let row = gen_repo::get(guard.connection(), &id)?
        .ok_or_else(|| CommandError::not_found("Generation not found"))?;
    detail(guard.connection(), row)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRequest {
    pub ids: Vec<String>,
}

fn validate_compare_ids(ids: &[String]) -> Result<(), CommandError> {
    if ids.len() < 2 || ids.len() > 4 {
        return Err(CommandError::validation(
            "Compare needs between 2 and 4 generations",
        ));
    }
    let mut unique = ids.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != ids.len() {
        return Err(CommandError::validation("Compare ids must be distinct"));
    }
    Ok(())
}

fn compare_details(
    connection: &rusqlite::Connection,
    ids: &[String],
) -> Result<Vec<GenerationDetail>, CommandError> {
    let mut details = Vec::with_capacity(ids.len());
    for id in ids {
        let row = gen_repo::get(connection, id)?
            .ok_or_else(|| CommandError::not_found(format!("Generation {id} not found")))?;
        details.push(detail(connection, row)?);
    }
    Ok(details)
}

#[tauri::command]
pub async fn library_compare(
    request: CompareRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<Vec<GenerationDetail>, CommandError> {
    validate_compare_ids(&request.ids)?;
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    compare_details(guard.connection(), &request.ids)
}

#[tauri::command]
pub async fn library_favorite(
    request: FavoriteRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<(), CommandError> {
    let guard = database
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
    Ok(())
}

#[tauri::command]
pub async fn library_update(
    request: UpdateGenerationRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<usize, CommandError> {
    if request.ids.is_empty() {
        return Err(CommandError::validation("Select at least one generation"));
    }
    let guard = database
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
            let mut tags = tag_repo::list_for_generation(connection, id)?;
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
                "add" => collection_repo::add_item(connection, collection_id, id)?,
                "remove" => {
                    collection_repo::remove_item(connection, collection_id, id)?;
                }
                other => {
                    return Err(CommandError::validation(format!(
                        "Unknown collection action: {other}"
                    )));
                }
            }
        }
    }

    Ok(changed)
}

#[tauri::command]
pub async fn library_tags(
    database: State<'_, Mutex<Database>>,
) -> Result<Vec<TagSummary>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    Ok(tag_repo::list_counts(guard.connection())?
        .into_iter()
        .map(|(name, count)| TagSummary { name, count })
        .collect())
}

#[tauri::command]
pub async fn library_collections(
    database: State<'_, Mutex<Database>>,
) -> Result<Vec<CollectionSummary>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let counts: BTreeMap<String, i64> = collection_repo::counts(guard.connection())?
        .into_iter()
        .collect();
    Ok(collection_repo::list(guard.connection())?
        .into_iter()
        .map(|row| CollectionSummary {
            count: counts.get(&row.id).copied().unwrap_or_default(),
            id: row.id,
            name: row.name,
            created_at: row.created_at,
        })
        .collect())
}

#[tauri::command]
pub async fn library_collection_create(
    request: NameRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<CollectionSummary, CommandError> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(CommandError::validation("Collection name is required"));
    }
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let row = collection_repo::create(guard.connection(), &Uuid::now_v7().to_string(), name)?;
    Ok(CollectionSummary {
        id: row.id,
        name: row.name,
        created_at: row.created_at,
        count: 0,
    })
}

#[tauri::command]
pub async fn library_collection_rename(
    request: CollectionRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<CollectionSummary, CommandError> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(CommandError::validation("Collection name is required"));
    }
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    if !collection_repo::rename(connection, &request.id, name)? {
        return Err(CommandError::not_found("Collection not found"));
    }
    let row = collection_repo::get(connection, &request.id)?
        .ok_or_else(|| CommandError::not_found("Collection not found"))?;
    let count = collection_repo::list_items(connection, &row.id)?.len() as i64;
    Ok(CollectionSummary {
        id: row.id,
        name: row.name,
        created_at: row.created_at,
        count,
    })
}

#[tauri::command]
pub async fn library_collection_delete(
    id: String,
    database: State<'_, Mutex<Database>>,
) -> Result<(), CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    if !collection_repo::delete(guard.connection(), &id)? {
        return Err(CommandError::not_found("Collection not found"));
    }
    Ok(())
}

#[tauri::command]
pub async fn library_collection_assign(
    request: CollectionAssignRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<usize, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let mut changed = 0;
    for id in &request.generation_ids {
        if request.add {
            collection_repo::add_item(connection, &request.collection_id, id)?;
            changed += 1;
        } else {
            changed += usize::from(collection_repo::remove_item(
                connection,
                &request.collection_id,
                id,
            )?);
        }
    }
    Ok(changed)
}

#[tauri::command]
pub async fn library_versions(
    id: String,
    database: State<'_, Mutex<Database>>,
) -> Result<VersionTree, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let rows = gen_repo::version_tree(guard.connection(), &id)?;
    if rows.is_empty() {
        return Err(CommandError::not_found("Generation not found"));
    }
    Ok(VersionTree {
        generations: rows
            .into_iter()
            .map(|row| VersionNode {
                id: row.id,
                parent_id: row.parent_id,
                prompt: row.prompt,
                negative: row.negative,
                seed: row.seed.and_then(|seed| u64::try_from(seed).ok()),
                status: row.status,
                preset_id: row.preset_id,
                created_at: row.created_at,
            })
            .collect(),
    })
}

#[tauri::command]
pub async fn library_diff(
    request: DiffRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<Vec<FieldDiff>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let left = gen_repo::get(connection, &request.left_id)?
        .ok_or_else(|| CommandError::not_found("Left generation not found"))?;
    let right = gen_repo::get(connection, &request.right_id)?
        .ok_or_else(|| CommandError::not_found("Right generation not found"))?;
    Ok(diff(&left, &right))
}

#[tauri::command]
pub async fn library_restore_workspace(
    id: String,
    database: State<'_, Mutex<Database>>,
    workspace: State<'_, Mutex<WorkspaceStore>>,
) -> Result<RestoreResult, CommandError> {
    let (mut draft, params) = {
        let guard = database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let (draft, params) = restore_inputs(guard.connection(), &id)?;
        (draft, params)
    };

    let mut store = workspace
        .lock()
        .map_err(|_| CommandError::internal("Workspace lock is poisoned"))?;
    draft = store.save(draft)?;
    Ok(RestoreResult { draft, params })
}

fn restore_inputs(
    connection: &rusqlite::Connection,
    id: &str,
) -> Result<(WorkspaceDraft, Value), CommandError> {
    let row = gen_repo::get(connection, id)?
        .ok_or_else(|| CommandError::not_found("Generation not found"))?;
    let asset_id = row
        .asset_id
        .clone()
        .ok_or_else(|| CommandError::conflict("This generation has no source asset"))?;
    let edit_ops = row
        .edit_id
        .as_deref()
        .and_then(|edit_id| edit_repo::get(connection, edit_id).ok().flatten())
        .and_then(|edit| serde_json::from_str::<EditOps>(&edit.ops_json).ok());
    let guidance: Guidance = serde_json::from_str(&row.guidance_json).unwrap_or_default();
    let params: Value = serde_json::from_str(&row.params_json).unwrap_or_else(|_| json!({}));
    Ok((
        WorkspaceDraft {
            asset_id,
            edit_ops,
            selected_preset_id: Some(row.preset_id),
            guidance,
            params: Some(params.clone()),
            updated_at_ms: 0,
        },
        params,
    ))
}

#[tauri::command]
pub async fn library_trash(
    request: TrashRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<TrashSummary, CommandError> {
    if request.generation_ids.is_empty() && request.asset_ids.is_empty() {
        return Err(CommandError::validation("Select at least one item"));
    }
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let mut assets = 0;
    for id in &request.asset_ids {
        assets += usize::from(asset_repo::soft_delete(connection, id)?);
    }
    let generations = gen_repo::soft_delete_many(connection, &request.generation_ids)?;
    Ok(TrashSummary {
        generations,
        assets,
    })
}

#[tauri::command]
pub async fn library_restore_deleted(
    request: TrashRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<TrashSummary, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let mut assets = 0;
    for id in &request.asset_ids {
        assets += usize::from(asset_repo::restore(connection, id)?);
    }
    let generations = gen_repo::restore_many(connection, &request.generation_ids)?;
    Ok(TrashSummary {
        generations,
        assets,
    })
}

#[tauri::command]
pub async fn library_assets_trash(
    database: State<'_, Mutex<Database>>,
) -> Result<Vec<AssetTrashEntry>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    Ok(asset_repo::list_deleted(guard.connection())?
        .into_iter()
        .map(|row| AssetTrashEntry {
            id: row.id,
            display_name: row.display_name,
            kind: row.kind,
            source_path: row.source_path,
            deleted_at: None,
        })
        .collect())
}

#[tauri::command]
pub async fn library_purge(
    request: PurgeRequest,
    database: State<'_, Mutex<Database>>,
    paths: State<'_, AppPaths>,
) -> Result<PurgeSummary, CommandError> {
    if !request.confirm {
        return Err(CommandError::validation(
            "Permanent deletion needs explicit confirmation",
        ));
    }
    if request.generation_ids.is_empty() && request.asset_ids.is_empty() {
        return Err(CommandError::validation("Select at least one item"));
    }

    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let mut bytes_reclaimed = 0u64;
    let mut assets = 0;
    for id in &request.asset_ids {
        if let Some(asset) = asset_repo::get(connection, id)? {
            for path in [&asset.thumbnail_path, &asset.proxy_path]
                .into_iter()
                .flatten()
            {
                bytes_reclaimed += remove_cached_file(Path::new(path), &paths.cache_dir);
            }
            for frame in frame_repo::list_for_asset(connection, id)? {
                bytes_reclaimed += remove_cached_file(Path::new(&frame.path), &paths.cache_dir);
            }
        }
        assets += usize::from(asset_repo::hard_delete(connection, id)?);
    }
    let generations = gen_repo::hard_delete_many(connection, &request.generation_ids)?;
    Ok(PurgeSummary {
        generations,
        assets,
        bytes_reclaimed,
    })
}

/// Plain entry point shared by the Tauri command and the native facade.
pub fn export(
    request: &ExportRequest,
    database: &Mutex<Database>,
) -> Result<ExportSummary, CommandError> {
    let format = ExportFormat::from_name(&request.format)?;
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
    let rows = if request.ids.is_empty() {
        gen_repo::query(
            connection,
            &GenerationFilter {
                sort: "newest".into(),
                limit: 100_000,
                ..GenerationFilter::default()
            },
        )?
        .0
    } else {
        let mut rows = Vec::new();
        for id in &request.ids {
            if let Some(row) = gen_repo::get(connection, id)? {
                rows.push(row);
            }
        }
        rows
    };

    let records = rows
        .iter()
        .map(|row| export_record(connection, row))
        .collect::<Result<Vec<_>, CommandError>>()?;
    let text = format.render(&records)?;
    write_atomic(&path, &text)?;

    Ok(ExportSummary {
        output_path: request.output_path.clone(),
        format: format.as_str().into(),
        count: records.len(),
        bytes: text.len() as u64,
    })
}

#[tauri::command]
pub async fn library_export(
    request: ExportRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<ExportSummary, CommandError> {
    export(&request, database.inner())
}

/// Plain entry point shared by the Tauri command and the native facade.
pub fn import(
    request: &ImportRequest,
    database: &Mutex<Database>,
) -> Result<ImportSummary, CommandError> {
    let path = PathBuf::from(&request.path);
    let metadata = std::fs::metadata(&path).map_err(|error| CommandError::from_io(&error))?;
    if metadata.len() > 64 * 1024 * 1024 {
        return Err(CommandError::validation(
            "Library imports must be smaller than 64 MB",
        ));
    }
    let text = std::fs::read_to_string(&path).map_err(|error| CommandError::from_io(&error))?;
    let format = ExportFormat::infer(&path, &text);
    let mut records = format.parse_text(&text)?;

    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let connection = guard.connection();
    let mut imported = 0;
    let mut skipped = 0;

    for record in records.drain(..) {
        if record.prompt.trim().is_empty() {
            skipped += 1;
            continue;
        }
        let exists = gen_repo::get(connection, &record.id)?.is_some();
        let id = if exists {
            Uuid::now_v7().to_string()
        } else {
            record.id.clone()
        };
        let parent_id = record
            .parent_id
            .filter(|parent| gen_repo::get(connection, parent).ok().flatten().is_some());
        gen_repo::insert(
            connection,
            &gen_repo::NewGeneration {
                id: id.clone(),
                parent_id,
                asset_id: record.asset_id,
                edit_id: record.edit_id,
                description_id: None,
                project_id: None,
                shot_index: None,
                preset_id: record.preset_id,
                preset_version: record.preset_version,
                preset_hash: record.preset_hash,
                prompt: record.prompt,
                negative: record.negative,
                params_json: serde_json::to_string(&record.params).unwrap_or_else(|_| "{}".into()),
                guidance_json: serde_json::to_string(&record.guidance)
                    .unwrap_or_else(|_| "{}".into()),
                description_json: record
                    .description
                    .and_then(|value| serde_json::to_string(&value).ok()),
                seed: record.seed.and_then(|seed| i64::try_from(seed).ok()),
                models_json: serde_json::to_string(&record.models).unwrap_or_else(|_| "[]".into()),
                status: record.status,
                error: record.error,
                raw_response: None,
                tokens_in: None,
                tokens_out: None,
                latency_ms: None,
                cache_key: None,
            },
        )?;
        if !record.tags.is_empty() {
            gen_repo::set_tags(connection, &id, &record.tags)?;
        }
        imported += 1;
    }

    Ok(ImportSummary {
        path: request.path.clone(),
        format: format.as_str().into(),
        imported,
        skipped,
    })
}

#[tauri::command]
pub async fn library_import(
    request: ImportRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<ImportSummary, CommandError> {
    import(&request, database.inner())
}

fn references_from_params(params_json: &str) -> Vec<ComposeReference> {
    let Ok(value) = serde_json::from_str::<Value>(params_json) else {
        return Vec::new();
    };
    value
        .get("references")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<ComposeReference>(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn rerun_request(
    row: &gen_repo::GenerationRow,
    preset_id: &str,
    guidance: Option<Guidance>,
) -> ComposeRequest {
    let description = row
        .description_json
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .map(|value| crate::pipeline::flatten_legacy_description(&value))
        .filter(|text| !text.trim().is_empty());
    let guidance = guidance
        .or_else(|| serde_json::from_str::<Guidance>(&row.guidance_json).ok())
        .unwrap_or_default();
    let existing_prompt = if description.is_some() {
        None
    } else {
        Some(row.prompt.clone())
    };
    ComposeRequest {
        preset_id: preset_id.to_string(),
        asset_id: row.asset_id.clone(),
        edit_id: row.edit_id.clone(),
        references: references_from_params(&row.params_json),
        parent_id: Some(row.id.clone()),
        dataset_row_id: None,
        description,
        brief: None,
        guidance,
        locked: Vec::new(),
        params: serde_json::from_str::<Value>(&row.params_json).unwrap_or_else(|_| json!({})),
        existing_prompt,
        instruction: None,
        seed: row.seed.and_then(|seed| u64::try_from(seed).ok()),
        variations: 1,
        bypass_cache: true,
        output_language: None,
        include_images: false,
    }
}

/// Plain entry point shared by the Tauri command and the native facade.
///
/// The queue runner is resumed by the Tauri wrapper only; the native facade
/// enqueues the same compose jobs without starting a background runner.
pub fn enqueue_rerun(
    request: &RerunRequest,
    database: &Mutex<Database>,
) -> Result<RerunSummary, CommandError> {
    if request.ids.is_empty() {
        return Err(CommandError::validation("Select at least one generation"));
    }
    {
        let guard = database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let row = preset_repo::get(guard.connection(), &request.preset_id)?
            .filter(|row| row.deleted_at.is_none())
            .ok_or_else(|| CommandError::not_found("Preset not found"))?;
        if row.deleted_at.is_some() {
            return Err(CommandError::not_found("Preset not found"));
        }
    }

    let mut job_ids = Vec::new();
    for id in &request.ids {
        let compose = {
            let guard = database
                .lock()
                .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
            let row = gen_repo::get(guard.connection(), id)?
                .ok_or_else(|| CommandError::not_found("Generation not found"))?;
            rerun_request(&row, &request.preset_id, request.guidance.clone())
        };

        let result = job_queue::enqueue(
            database,
            EnqueueSpec {
                kind: "compose".into(),
                payload: json!({ "presetId": request.preset_id }),
                items: vec![serde_json::to_value(&compose).map_err(|error| {
                    CommandError::internal(format!("serialize rerun item: {error}"))
                })?],
                priority: 0,
                dedupe_key: None,
                note: Some(format!("Re-run with {}", request.preset_id)),
            },
        )?;
        job_ids.push(result.job.id);
    }

    Ok(RerunSummary { job_ids })
}

#[tauri::command]
pub async fn library_rerun(
    request: RerunRequest,
    database: State<'_, Mutex<Database>>,
    queue: State<'_, std::sync::Arc<job_queue::JobQueue>>,
) -> Result<RerunSummary, CommandError> {
    let summary = enqueue_rerun(&request, database.inner())?;
    queue.resume();
    Ok(summary)
}

const BACKUP_FORMAT: &str = "prompt-studio-backup";
const BACKUP_VERSION: u32 = 1;

fn is_zip_archive(path: &Path) -> bool {
    let mut header = [0_u8; 4];
    match File::open(path) {
        Ok(mut file) => file.read_exact(&mut header).is_ok() && header == [0x50, 0x4b, 0x03, 0x04],
        Err(_) => false,
    }
}

fn backup_bundle(
    database: &Database,
    paths: &AppPaths,
    output: &Path,
) -> Result<u64, CommandError> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| CommandError::from_io(&error))?;
    }
    let partial = output.with_extension("partial");
    let temp_db = output.with_extension("db.tmp");
    let _ = fs::remove_file(&temp_db);
    database.backup_to(&temp_db)?;

    let file = File::create(&partial).map_err(|error| CommandError::from_io(&error))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let manifest = json!({
        "format": BACKUP_FORMAT,
        "version": BACKUP_VERSION,
        "createdAtMs": created,
        "appVersion": env!("CARGO_PKG_VERSION"),
    });
    writer
        .start_file("manifest.json", options)
        .map_err(|error| CommandError::internal(format!("create manifest: {error}")))?;
    writer
        .write_all(manifest.to_string().as_bytes())
        .map_err(|error| CommandError::from_io(&error))?;

    writer
        .start_file("prompt-studio.db", options)
        .map_err(|error| CommandError::internal(format!("create database entry: {error}")))?;
    let mut database_file = File::open(&temp_db).map_err(|error| CommandError::from_io(&error))?;
    std::io::copy(&mut database_file, &mut writer)
        .map_err(|error| CommandError::from_io(&error))?;

    for (name, path) in [
        ("settings.json", paths.config_dir.join("settings.json")),
        ("token.bin", paths.config_dir.join("token.bin")),
        ("workspace.json", paths.data_dir.join("workspace.json")),
    ] {
        if !path.is_file() {
            continue;
        }
        writer
            .start_file(name, options)
            .map_err(|error| CommandError::internal(format!("create {name} entry: {error}")))?;
        let mut source = File::open(&path).map_err(|error| CommandError::from_io(&error))?;
        std::io::copy(&mut source, &mut writer).map_err(|error| CommandError::from_io(&error))?;
    }

    writer
        .finish()
        .map_err(|error| CommandError::internal(format!("finish backup bundle: {error}")))?;
    fs::rename(&partial, output).map_err(|error| CommandError::from_io(&error))?;
    let _ = fs::remove_file(&temp_db);
    fs::metadata(output)
        .map(|metadata| metadata.len())
        .map_err(|error| CommandError::from_io(&error))
}

fn restore_bundle(
    archive_path: &Path,
    database: &mut Database,
    paths: &AppPaths,
) -> Result<(Option<PathBuf>, bool), CommandError> {
    let file = File::open(archive_path).map_err(|error| CommandError::from_io(&error))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| CommandError::validation(format!("Unreadable backup bundle: {error}")))?;

    {
        let mut entry = archive
            .by_name("manifest.json")
            .map_err(|_| CommandError::validation("Backup bundle is missing its manifest"))?;
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|error| CommandError::from_io(&error))?;
        let manifest: Value = serde_json::from_str(&text).map_err(|error| {
            CommandError::validation(format!("Invalid backup manifest: {error}"))
        })?;
        if manifest.get("format").and_then(Value::as_str) != Some(BACKUP_FORMAT) {
            return Err(CommandError::validation(
                "Not a Prompt Studio backup bundle",
            ));
        }
        let version = manifest
            .get("version")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if version > u64::from(BACKUP_VERSION) {
            return Err(CommandError::validation(
                "Backup bundle is newer than this build supports",
            ));
        }
    }

    let temp = std::env::temp_dir().join(format!("prompt-studio-restore-{}.db", Uuid::now_v7()));
    {
        let mut entry = archive
            .by_name("prompt-studio.db")
            .map_err(|_| CommandError::validation("Backup bundle is missing its database"))?;
        let mut output = File::create(&temp).map_err(|error| CommandError::from_io(&error))?;
        std::io::copy(&mut entry, &mut output).map_err(|error| CommandError::from_io(&error))?;
    }
    let pre_restore = database.restore_from(&temp)?;
    let _ = fs::remove_file(&temp);

    let mut token_restored = false;
    for (name, destination) in [
        ("settings.json", paths.config_dir.join("settings.json")),
        ("token.bin", paths.config_dir.join("token.bin")),
        ("workspace.json", paths.data_dir.join("workspace.json")),
    ] {
        let Ok(mut entry) = archive.by_name(name) else {
            continue;
        };
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| CommandError::from_io(&error))?;
        }
        let partial = destination.with_extension("restore.tmp");
        {
            let mut output =
                File::create(&partial).map_err(|error| CommandError::from_io(&error))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|error| CommandError::from_io(&error))?;
        }
        fs::rename(&partial, &destination).map_err(|error| CommandError::from_io(&error))?;
        if name == "token.bin" {
            token_restored = true;
        }
    }

    Ok((pre_restore, token_restored))
}

/// Plain entry point shared by the Tauri command and the native facade.
pub fn backup(
    request: &BackupRequest,
    database: &Mutex<Database>,
    paths: &AppPaths,
) -> Result<BackupSummary, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let output = PathBuf::from(&request.output_path);
    let bytes = backup_bundle(&guard, paths, &output)?;
    Ok(BackupSummary {
        output_path: request.output_path.clone(),
        bytes,
        format: BACKUP_FORMAT.to_string(),
    })
}

#[tauri::command]
pub async fn library_backup(
    request: BackupRequest,
    database: State<'_, Mutex<Database>>,
    paths: State<'_, AppPaths>,
) -> Result<BackupSummary, CommandError> {
    backup(&request, database.inner(), paths.inner())
}

/// Plain entry point shared by the Tauri command and the native facade.
pub fn restore(
    request: &RestoreRequest,
    database: &Mutex<Database>,
    paths: &AppPaths,
) -> Result<RestoreSummary, CommandError> {
    let mut guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let backup = PathBuf::from(&request.backup_path);

    if is_zip_archive(&backup) {
        let (pre_restore, token_restored) = restore_bundle(&backup, &mut guard, paths)?;
        return Ok(RestoreSummary {
            backup_path: request.backup_path.clone(),
            pre_restore_backup: pre_restore.map(|path| path.display().to_string()),
            bundle: true,
            restart_required: true,
            token_restored,
        });
    }

    let pre_restore = guard.restore_from(&backup)?;
    Ok(RestoreSummary {
        backup_path: request.backup_path.clone(),
        pre_restore_backup: pre_restore.map(|path| path.display().to_string()),
        bundle: false,
        restart_required: false,
        token_restored: false,
    })
}

#[tauri::command]
pub async fn library_restore(
    request: RestoreRequest,
    database: State<'_, Mutex<Database>>,
    paths: State<'_, AppPaths>,
) -> Result<RestoreSummary, CommandError> {
    restore(&request, database.inner(), paths.inner())
}

fn kind_str(kind: PresetKind) -> &'static str {
    match kind {
        PresetKind::Image => "image",
        PresetKind::Video => "video",
        PresetKind::Edit => "edit",
    }
}

fn record(
    connection: &rusqlite::Connection,
    row: &GenerationRow,
) -> Result<GenerationRecord, CommandError> {
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
        tags: tag_repo::list_for_generation(connection, &row.id)?,
        collections: gen_repo::collections_for_generation(connection, &row.id)?,
        created_at: row.created_at.clone(),
        deleted_at: row.deleted_at.clone(),
        title: None,
    })
}

fn detail(
    connection: &rusqlite::Connection,
    row: GenerationRow,
) -> Result<GenerationDetail, CommandError> {
    let record = record(connection, &row)?;
    let references = reference_details(connection, &row.params_json);
    Ok(GenerationDetail {
        record,
        raw_response: row.raw_response,
        tokens_in: row.tokens_in.and_then(|value| u64::try_from(value).ok()),
        tokens_out: row.tokens_out.and_then(|value| u64::try_from(value).ok()),
        latency_ms: row.latency_ms.and_then(|value| u64::try_from(value).ok()),
        cache_key: row.cache_key,
        references,
    })
}

fn reference_details(
    connection: &rusqlite::Connection,
    params_json: &str,
) -> Vec<GenerationReference> {
    let Ok(value) = serde_json::from_str::<Value>(params_json) else {
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
                    let edit_id = item
                        .get("editId")
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
                    GenerationReference {
                        asset_id,
                        edit_id,
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

fn export_record(
    connection: &rusqlite::Connection,
    row: &GenerationRow,
) -> Result<ExportRecord, CommandError> {
    Ok(ExportRecord {
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
        tags: tag_repo::list_for_generation(connection, &row.id)?,
        created_at: row.created_at.clone(),
    })
}

fn diff(left: &GenerationRow, right: &GenerationRow) -> Vec<FieldDiff> {
    let fields: Vec<(&str, Value, Value)> = vec![
        ("prompt", json!(left.prompt), json!(right.prompt)),
        ("negative", json!(left.negative), json!(right.negative)),
        (
            "params",
            serde_json::from_str(&left.params_json).unwrap_or(Value::Null),
            serde_json::from_str(&right.params_json).unwrap_or(Value::Null),
        ),
        (
            "guidance",
            serde_json::from_str(&left.guidance_json).unwrap_or(Value::Null),
            serde_json::from_str(&right.guidance_json).unwrap_or(Value::Null),
        ),
        ("seed", json!(left.seed), json!(right.seed)),
        ("presetId", json!(left.preset_id), json!(right.preset_id)),
        ("models", json!(left.models_json), json!(right.models_json)),
        ("status", json!(left.status), json!(right.status)),
    ];
    fields
        .into_iter()
        .map(|(field, left, right)| FieldDiff {
            field: field.into(),
            changed: left != right,
            left,
            right,
        })
        .collect()
}

fn remove_cached_file(path: &Path, cache_dir: &Path) -> u64 {
    let cached = path.starts_with(cache_dir);
    if !cached {
        return 0;
    }
    let bytes = std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    match std::fs::remove_file(path) {
        Ok(()) => bytes,
        Err(_) => 0,
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), CommandError> {
    let temporary = path.with_extension(format!(
        "{}.partial",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("tmp")
    ));
    std::fs::write(&temporary, text).map_err(|error| CommandError::from_io(&error))?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| CommandError::from_io(&error))?;
    }
    std::fs::rename(&temporary, path).map_err(|error| CommandError::from_io(&error))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportFormat {
    Json,
    Jsonl,
    Csv,
    Text,
}

impl ExportFormat {
    fn from_name(value: &str) -> Result<Self, CommandError> {
        match value.to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "jsonl" | "ndjson" => Ok(Self::Jsonl),
            "csv" => Ok(Self::Csv),
            "txt" | "text" => Ok(Self::Text),
            other => Err(CommandError::validation(format!(
                "Unsupported export format: {other}"
            ))),
        }
    }

    fn infer(path: &Path, text: &str) -> Self {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("json") => Self::Json,
            Some("jsonl") | Some("ndjson") => Self::Jsonl,
            Some("csv") => Self::Csv,
            Some("txt") | Some("text") => Self::Text,
            _ => {
                let trimmed = text.trim_start();
                if trimmed.starts_with('[') {
                    Self::Json
                } else if trimmed.starts_with('{') {
                    Self::Jsonl
                } else {
                    Self::Text
                }
            }
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Csv => "csv",
            Self::Text => "txt",
        }
    }

    fn render(self, records: &[ExportRecord]) -> Result<String, CommandError> {
        match self {
            Self::Json => serde_json::to_string_pretty(records)
                .map_err(|error| CommandError::internal(format!("serialize export: {error}"))),
            Self::Jsonl => records
                .iter()
                .map(|record| {
                    serde_json::to_string(record).map_err(|error| {
                        CommandError::internal(format!("serialize export row: {error}"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|lines| {
                    let mut text = lines.join("\n");
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text
                }),
            Self::Csv => Ok(render_csv(records)),
            Self::Text => Ok(records
                .iter()
                .map(|record| record.prompt.replace('\n', " ").trim().to_string())
                .collect::<Vec<_>>()
                .join("\n")),
        }
    }

    fn parse_text(self, text: &str) -> Result<Vec<ExportRecord>, CommandError> {
        match self {
            Self::Json => serde_json::from_str::<Vec<ExportRecord>>(text)
                .map_err(|error| CommandError::validation(format!("Invalid JSON export: {error}"))),
            Self::Jsonl => text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    serde_json::from_str::<ExportRecord>(line).map_err(|error| {
                        CommandError::validation(format!("Invalid JSONL export: {error}"))
                    })
                })
                .collect(),
            Self::Csv => parse_csv(text),
            Self::Text => Ok(text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(text_record)
                .collect()),
        }
    }
}

fn text_record(prompt: &str) -> ExportRecord {
    ExportRecord {
        id: Uuid::now_v7().to_string(),
        parent_id: None,
        asset_id: None,
        edit_id: None,
        prompt: prompt.trim().to_string(),
        negative: None,
        params: json!({}),
        guidance: Guidance::default(),
        description: None,
        preset_id: "imported".into(),
        preset_version: "1.0.0".into(),
        preset_hash: String::new(),
        models: Vec::new(),
        seed: None,
        status: "succeeded".into(),
        error: None,
        favorite: false,
        rating: None,
        notes: None,
        tags: Vec::new(),
        created_at: String::new(),
    }
}

fn render_csv(records: &[ExportRecord]) -> String {
    let headers = [
        "id",
        "parentId",
        "assetId",
        "editId",
        "prompt",
        "negative",
        "params",
        "presetId",
        "presetVersion",
        "presetHash",
        "models",
        "guidance",
        "description",
        "seed",
        "status",
        "error",
        "favorite",
        "notes",
        "rating",
        "tags",
        "createdAt",
    ];
    let mut lines = vec![headers.join(",")];
    for record in records {
        let cells = [
            record.id.clone(),
            record.parent_id.clone().unwrap_or_default(),
            record.asset_id.clone().unwrap_or_default(),
            record.edit_id.clone().unwrap_or_default(),
            record.prompt.clone(),
            record.negative.clone().unwrap_or_default(),
            serde_json::to_string(&record.params).unwrap_or_else(|_| "{}".into()),
            record.preset_id.clone(),
            record.preset_version.clone(),
            record.preset_hash.clone(),
            serde_json::to_string(&record.models).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&record.guidance).unwrap_or_else(|_| "{}".into()),
            record
                .description
                .as_ref()
                .and_then(|value| serde_json::to_string(value).ok())
                .unwrap_or_default(),
            record.seed.map(|seed| seed.to_string()).unwrap_or_default(),
            record.status.clone(),
            record.error.clone().unwrap_or_default(),
            record.favorite.to_string(),
            record.notes.clone().unwrap_or_default(),
            record
                .rating
                .map(|rating| rating.to_string())
                .unwrap_or_default(),
            record.tags.join("|"),
            record.created_at.clone(),
        ];
        lines.push(cells.map(|cell| csv_cell(&cell)).join(","));
    }
    lines.join("\n")
}

fn csv_cell(value: &str) -> String {
    if value.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn parse_csv(text: &str) -> Result<Vec<ExportRecord>, CommandError> {
    let rows = parse_csv_rows(text);
    let mut rows = rows.into_iter();
    let Some(headers) = rows.next() else {
        return Ok(Vec::new());
    };
    let index: BTreeMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(index, header)| (header.trim().to_string(), index))
        .collect();
    let prompt_column = index
        .get("prompt")
        .copied()
        .ok_or_else(|| CommandError::validation("CSV export is missing the prompt column"))?;

    let mut records = Vec::new();
    for row in rows {
        if row.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }
        let cell = |name: &str| {
            index
                .get(name)
                .and_then(|position| row.get(*position))
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        records.push(ExportRecord {
            id: cell("id").unwrap_or_else(|| Uuid::now_v7().to_string()),
            parent_id: cell("parentId"),
            asset_id: cell("assetId"),
            edit_id: cell("editId"),
            prompt: row
                .get(prompt_column)
                .cloned()
                .unwrap_or_default()
                .trim()
                .to_string(),
            negative: cell("negative"),
            params: cell("params")
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_else(|| json!({})),
            guidance: cell("guidance")
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default(),
            description: cell("description")
                .and_then(|value| serde_json::from_str::<Value>(&value).ok()),
            preset_id: cell("presetId").unwrap_or_else(|| "imported".into()),
            preset_version: cell("presetVersion").unwrap_or_else(|| "1.0.0".into()),
            preset_hash: cell("presetHash").unwrap_or_default(),
            models: cell("models")
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default(),
            seed: cell("seed").and_then(|value| value.parse().ok()),
            status: cell("status").unwrap_or_else(|| "succeeded".into()),
            error: cell("error"),
            favorite: cell("favorite").as_deref() == Some("true"),
            rating: cell("rating").and_then(|value| value.parse().ok()),
            notes: cell("notes"),
            tags: cell("tags")
                .map(|value| {
                    value
                        .split('|')
                        .map(str::trim)
                        .filter(|tag| !tag.is_empty())
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            created_at: cell("createdAt").unwrap_or_default(),
        });
    }
    Ok(records)
}

fn parse_csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();

    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cell.push(character);
            }
            continue;
        }
        match character {
            '"' if cell.is_empty() => quoted = true,
            ',' => {
                row.push(std::mem::take(&mut cell));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            other => cell.push(other),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;
    use std::time::Instant;

    use serde_json::json;

    use crate::commands::Guidance;
    use crate::db::Database;
    use crate::db::repository::assets;
    use crate::db::repository::collections;
    use crate::db::repository::edits;
    use crate::db::repository::generations::{self as gen_repo, GenerationFilter};
    use crate::db::repository::tags;
    use crate::media::test_support::temp_dir;

    use super::{
        ExportFormat, backup_bundle, compare_details, detail, diff, export_record, record,
        rerun_request, restore_bundle, restore_inputs, validate_compare_ids,
    };
    use crate::paths::resolve_app_paths;

    fn database() -> Database {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        database
    }

    fn generation_row(
        id: &str,
        parent_id: Option<&str>,
        preset: &str,
        prompt: &str,
        status: &str,
    ) -> gen_repo::NewGeneration {
        gen_repo::NewGeneration {
            id: id.into(),
            parent_id: parent_id.map(ToOwned::to_owned),
            asset_id: None,
            edit_id: None,
            description_id: None,
            project_id: None,
            shot_index: None,
            preset_id: preset.into(),
            preset_version: "1.0.0".into(),
            preset_hash: "hash".into(),
            prompt: prompt.into(),
            negative: Some("blurry".into()),
            params_json: json!({ "aspect_ratio": "1:1" }).to_string(),
            guidance_json: json!({ "global": null, "preset": null, "run": "neon" }).to_string(),
            description_json: Some(json!({ "subject": "a fox" }).to_string()),
            seed: Some(7),
            models_json: json!(["gemma"]).to_string(),
            status: status.into(),
            error: None,
            raw_response: None,
            tokens_in: Some(100),
            tokens_out: Some(50),
            latency_ms: Some(900),
            cache_key: None,
        }
    }

    fn insert_asset(connection: &rusqlite::Connection, id: &str) {
        assets::insert(
            connection,
            &assets::NewAsset {
                id: id.into(),
                hash: format!("hash-{id}"),
                kind: "image".into(),
                source_path: format!("C:\\media\\{id}.png"),
                display_name: format!("{id}.png"),
                width: Some(64),
                height: Some(64),
                fps: None,
                duration_ms: None,
                codec: Some("png".into()),
                has_audio: false,
                color: None,
                rotation: 0,
                thumbnail_path: None,
                proxy_path: None,
                embedded_metadata: None,
            },
        )
        .expect("asset");
    }

    #[test]
    fn rerun_preserves_references_and_relinks_parent() {
        let database = database();
        let connection = database.connection();
        let mut row = generation_row("gen-rerun", None, "mmh3", "a fox", "succeeded");
        row.params_json = json!({
            "references": [
                { "assetId": "asset-a", "weight": 0.8, "name": "subject" },
                { "assetId": "asset-b", "weight": 1.2 }
            ]
        })
        .to_string();
        gen_repo::insert(connection, &row).expect("insert");

        let stored = gen_repo::get(connection, "gen-rerun")
            .expect("get")
            .expect("present");
        let request = rerun_request(&stored, "krea2", None);
        assert_eq!(request.parent_id.as_deref(), Some("gen-rerun"));
        assert_eq!(request.preset_id, "krea2");
        assert_eq!(request.references.len(), 2);
        assert_eq!(request.references[0].asset_id.as_deref(), Some("asset-a"));
        assert_eq!(request.references[0].weight, Some(0.8));
        assert_eq!(request.references[1].asset_id.as_deref(), Some("asset-b"));
        assert_eq!(request.references[1].weight, Some(1.2));
        assert_eq!(request.seed, Some(7));
        assert!(request.bypass_cache);
    }

    #[test]
    fn detail_lists_reference_thumbnails_and_missing_assets() {
        let database = database();
        let connection = database.connection();
        insert_asset(connection, "asset-a");

        let mut row = generation_row("gen-refs", None, "mmh3", "a fox", "succeeded");
        row.params_json = json!({
            "references": [
                { "assetId": "asset-a", "weight": 0.8, "name": "subject" },
                { "assetId": "gone", "weight": 1.0 }
            ]
        })
        .to_string();
        gen_repo::insert(connection, &row).expect("insert");

        let stored = gen_repo::get(connection, "gen-refs")
            .expect("get")
            .expect("present");
        let details = detail(connection, stored).expect("detail");
        assert_eq!(details.references.len(), 2);
        assert_eq!(details.references[0].weight, Some(0.8));
        assert_eq!(
            details.references[0].thumbnail_path.as_deref(),
            Some("C:\\media\\asset-a.png")
        );
        assert!(!details.references[0].missing);
        assert!(details.references[1].missing);

        let text_only = generation_row("gen-text", None, "mmh3", "a fox", "succeeded");
        gen_repo::insert(connection, &text_only).expect("text");
        let stored = gen_repo::get(connection, "gen-text")
            .expect("get")
            .expect("present");
        assert!(
            detail(connection, stored)
                .expect("detail")
                .references
                .is_empty()
        );
    }

    #[test]
    fn backup_bundle_round_trips_database_settings_token_and_workspace() {
        use zip::ZipArchive;

        let temp = temp_dir("backup-bundle");
        let data = temp.join("data");
        let cache = temp.join("cache");
        fs::create_dir_all(&data).expect("data dir");
        let db_path = data.join("prompt-studio.db");
        let mut database = Database::open(&db_path).expect("open");
        database.migrate().expect("migrate");

        fs::write(
            data.join("settings.json"),
            "{\"endpoint\":\"http://127.0.0.1:1234\"}",
        )
        .expect("settings");
        fs::write(data.join("token.bin"), b"foreign-token").expect("token");
        fs::write(data.join("workspace.json"), "{\"assetId\":null}").expect("workspace");

        let paths = resolve_app_paths(&temp, &data, &cache, false).expect("paths");
        let output = temp.join("backup.zip");
        let bytes = backup_bundle(&database, &paths, &output).expect("backup");
        assert!(bytes > 0);

        let mut archive = ZipArchive::new(fs::File::open(&output).expect("open")).expect("zip");
        let names: Vec<&str> = archive.file_names().collect();
        for expected in [
            "manifest.json",
            "prompt-studio.db",
            "settings.json",
            "token.bin",
            "workspace.json",
        ] {
            assert!(names.contains(&expected), "missing {expected}: {names:?}");
        }
        let mut manifest = String::new();
        archive
            .by_name("manifest.json")
            .expect("manifest")
            .read_to_string(&mut manifest)
            .expect("read manifest");
        assert!(manifest.contains("prompt-studio-backup"));
        assert!(!manifest.contains("prompt text"));
        drop(archive);

        fs::write(
            data.join("settings.json"),
            "{\"endpoint\":\"http://changed\"}",
        )
        .expect("change settings");
        let (pre_restore, token_restored) =
            restore_bundle(&output, &mut database, &paths).expect("restore");
        assert!(pre_restore.is_some());
        assert!(token_restored);
        let restored = fs::read_to_string(data.join("settings.json")).expect("settings");
        assert!(restored.contains("1234"));
        assert!(!restored.contains("changed"));
    }

    #[test]
    fn restore_rejects_a_bundle_without_its_database() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let temp = temp_dir("backup-corrupt");
        let data = temp.join("data");
        let cache = temp.join("cache");
        fs::create_dir_all(&data).expect("data dir");
        let db_path = data.join("prompt-studio.db");
        let mut database = Database::open(&db_path).expect("open");
        database.migrate().expect("migrate");
        let paths = resolve_app_paths(&temp, &data, &cache, false).expect("paths");

        let bad = temp.join("bad.zip");
        let file = fs::File::create(&bad).expect("create");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("manifest.json", SimpleFileOptions::default())
            .expect("manifest entry");
        writer
            .write_all(br#"{"format":"prompt-studio-backup","version":1}"#)
            .expect("manifest write");
        writer.finish().expect("finish");

        let error = restore_bundle(&bad, &mut database, &paths).expect_err("reject");
        assert!(error.message.contains("database"));
    }

    #[test]
    fn compare_accepts_unrelated_generations_and_bounds_the_count() {
        let database = database();
        let connection = database.connection();
        gen_repo::insert(
            connection,
            &generation_row("gen-a", None, "mmh3", "a fox", "succeeded"),
        )
        .expect("a");
        gen_repo::insert(
            connection,
            &generation_row("gen-b", None, "krea2", "a wolf", "succeeded"),
        )
        .expect("b");

        let details = compare_details(connection, &["gen-a".into(), "gen-b".into()])
            .expect("compare unrelated");
        assert_eq!(details.len(), 2);
        assert_ne!(details[0].record.preset_id, details[1].record.preset_id);

        assert!(validate_compare_ids(&["a".into()]).is_err());
        assert!(validate_compare_ids(&["a".into(), "a".into()]).is_err());
        assert!(validate_compare_ids(&["a".into(), "b".into()]).is_ok());
        assert!(validate_compare_ids(&["a".into(), "b".into(), "c".into(), "d".into()]).is_ok());
        assert!(
            validate_compare_ids(&["a".into(), "b".into(), "c".into(), "d".into(), "e".into()])
                .is_err()
        );
    }

    #[test]
    fn failed_generation_keeps_raw_response_and_error() {
        let database = database();
        let connection = database.connection();
        let mut row = generation_row("failed-1", None, "mmh3", "raw text", "failed");
        row.raw_response = Some("not json".into());
        row.error = Some("[InvalidResponse] no JSON object".into());
        gen_repo::insert(connection, &row).expect("insert");

        let (rows, total) = gen_repo::query(
            connection,
            &GenerationFilter {
                status: Some("failed".into()),
                sort: "newest".into(),
                limit: 25,
                ..GenerationFilter::default()
            },
        )
        .expect("query");
        assert_eq!(total, 1);
        assert_eq!(rows[0].status, "failed");
        assert_eq!(rows[0].raw_response.as_deref(), Some("not json"));
        assert!(
            rows[0]
                .error
                .as_deref()
                .unwrap()
                .contains("InvalidResponse")
        );
    }

    #[test]
    fn ten_thousand_rows_search_and_page_within_budget() {
        let database = database();
        let connection = database.connection();
        connection
            .execute_batch(
                "WITH RECURSIVE seq(n) AS (
                     SELECT 1 UNION ALL SELECT n + 1 FROM seq WHERE n < 10000
                 )
                 INSERT INTO generations (
                     id, parent_id, preset_id, preset_version, preset_hash, prompt, negative,
                     params_json, guidance_json, description_json, seed, models_json, status,
                     error, raw_response, tokens_in, tokens_out, latency_ms, cache_key, favorite,
                     rating, notes, created_at
                 )
                 SELECT 'g' || n, NULL, 'mmh3', '1.0.0', 'hash',
                        'prompt ' || n || CASE WHEN n = 7777 THEN ' needlephrase' ELSE '' END,
                        NULL, '{}', '{}', NULL, n, '[]', 'succeeded', NULL, NULL, NULL, NULL,
                        NULL, NULL, 0, NULL, NULL, '2026-11-05T12:00:00Z'
                 FROM seq;",
            )
            .expect("bulk insert");

        let started = Instant::now();
        let (rows, total) = gen_repo::query(
            connection,
            &GenerationFilter {
                text: Some("needlephrase".into()),
                sort: "newest".into(),
                limit: 25,
                ..GenerationFilter::default()
            },
        )
        .expect("search");
        let elapsed = started.elapsed();

        assert_eq!(total, 1);
        assert_eq!(rows[0].id, "g7777");
        assert!(elapsed.as_millis() < 300, "search took {elapsed:?}");

        let started = Instant::now();
        let (page, total) = gen_repo::query(
            connection,
            &GenerationFilter {
                sort: "newest".into(),
                limit: 25,
                ..GenerationFilter::default()
            },
        )
        .expect("first page");
        assert_eq!(page.len(), 25);
        assert_eq!(total, 10_000);
        assert!(started.elapsed().as_millis() < 300, "first page too slow");
    }

    #[test]
    fn generation_and_asset_favorites_are_independent() {
        let database = database();
        let connection = database.connection();
        gen_repo::insert(
            connection,
            &generation_row("gen-1", None, "mmh3", "a fox", "succeeded"),
        )
        .expect("generation");
        insert_asset(connection, "asset-1");
        gen_repo::insert(
            connection,
            &generation_row("gen-2", None, "mmh3", "a wolf", "succeeded"),
        )
        .expect("generation");

        gen_repo::set_favorite(connection, "gen-1", true).expect("favorite gen");
        assets::set_favorite(connection, "asset-1", true).expect("favorite asset");

        let (favorites, total) = gen_repo::query(
            connection,
            &GenerationFilter {
                favorite: Some(true),
                sort: "newest".into(),
                limit: 25,
                ..GenerationFilter::default()
            },
        )
        .expect("favorites");
        assert_eq!(total, 1);
        assert_eq!(favorites[0].id, "gen-1");
        assert_eq!(
            assets::list_favorites(connection)
                .expect("asset favorites")
                .len(),
            1
        );
    }

    #[test]
    fn tags_collections_notes_and_ratings_filter_and_rename() {
        let database = database();
        let connection = database.connection();
        gen_repo::insert(
            connection,
            &generation_row("gen-1", None, "mmh3", "a fox", "succeeded"),
        )
        .expect("generation");

        gen_repo::set_tags(connection, "gen-1", &["neon".into(), "snow".into()]).expect("tags");
        gen_repo::set_rating(connection, "gen-1", Some(4)).expect("rating");
        gen_repo::set_notes(connection, "gen-1", Some("keep this one")).expect("notes");

        let collection =
            collections::create(connection, "col-1", "city spots").expect("collection");
        collections::add_item(connection, &collection.id, "gen-1").expect("add");

        let (rows, total) = gen_repo::query(
            connection,
            &GenerationFilter {
                tags: vec!["neon".into()],
                sort: "rating".into(),
                limit: 25,
                ..GenerationFilter::default()
            },
        )
        .expect("tag filter");
        assert_eq!(total, 1);
        assert_eq!(rows[0].rating, Some(4));

        let (rows, total) = gen_repo::query(
            connection,
            &GenerationFilter {
                collection_id: Some("col-1".into()),
                sort: "newest".into(),
                limit: 25,
                ..GenerationFilter::default()
            },
        )
        .expect("collection filter");
        assert_eq!(total, 1);
        assert_eq!(rows[0].notes.as_deref(), Some("keep this one"));

        collections::rename(connection, "col-1", "renamed").expect("rename");
        assert_eq!(
            collections::list_items(connection, "col-1")
                .expect("items")
                .len(),
            1
        );
        assert_eq!(
            collections::get(connection, "col-1")
                .expect("get")
                .expect("present")
                .name,
            "renamed"
        );
        assert_eq!(tags::list_counts(connection).expect("tag counts").len(), 2);
    }

    #[test]
    fn versions_link_parents_and_diff_changed_fields() {
        let database = database();
        let connection = database.connection();
        gen_repo::insert(
            connection,
            &generation_row("parent", None, "mmh3", "a red fox", "succeeded"),
        )
        .expect("parent");
        gen_repo::insert(
            connection,
            &generation_row(
                "child",
                Some("parent"),
                "krea2",
                "a red fox at night",
                "succeeded",
            ),
        )
        .expect("child");

        let tree = gen_repo::version_tree(connection, "child").expect("tree");
        assert_eq!(tree.len(), 2);
        assert!(tree.iter().any(|row| row.id == "parent"));
        assert!(tree.iter().any(|row| row.id == "child"));

        let parent = gen_repo::get(connection, "parent")
            .expect("get")
            .expect("present");
        let child = gen_repo::get(connection, "child")
            .expect("get")
            .expect("present");
        let diffs = diff(&parent, &child);
        let prompt = diffs
            .iter()
            .find(|diff| diff.field == "prompt")
            .expect("prompt diff");
        assert!(prompt.changed);
        assert_ne!(prompt.left, prompt.right);
        let seed = diffs
            .iter()
            .find(|diff| diff.field == "seed")
            .expect("seed diff");
        assert!(!seed.changed);
    }

    #[test]
    fn restores_asset_edit_preset_guidance_and_params() {
        let database = database();
        let connection = database.connection();
        insert_asset(connection, "asset-1");
        edits::insert(
            connection,
            &edits::NewEdit {
                id: "edit-1".into(),
                asset_id: "asset-1".into(),
                ops_json: json!({
                    "crop": { "x": 1, "y": 2, "width": 30, "height": 20 },
                    "rotate": 0, "flipH": false, "flipV": false, "fit": "fit",
                    "trim": null, "fps": null, "removeAudio": false, "maskPath": null,
                    "format": null, "loopVideo": false, "padColor": null
                })
                .to_string(),
                target_json: "{}".into(),
                output_path: "C:\\media\\edited\\asset-1.png".into(),
                cache_key: "edit-cache".into(),
                width: Some(30),
                height: Some(20),
                fps: None,
                duration_ms: None,
            },
        )
        .expect("edit");

        let mut row = generation_row("gen-1", None, "krea2", "a fox", "succeeded");
        row.asset_id = Some("asset-1".into());
        row.edit_id = Some("edit-1".into());
        row.preset_id = "krea2".into();
        gen_repo::insert(connection, &row).expect("generation");

        let (draft, params) = restore_inputs(connection, "gen-1").expect("restore");
        assert_eq!(draft.asset_id, "asset-1");
        assert_eq!(draft.selected_preset_id.as_deref(), Some("krea2"));
        assert!(draft.edit_ops.is_some());
        assert_eq!(draft.guidance.run.as_deref(), Some("neon"));
        assert_eq!(params["aspect_ratio"], "1:1");
        assert_eq!(
            draft.params.as_ref().expect("params")["aspect_ratio"],
            "1:1"
        );

        let unchanged = gen_repo::get(connection, "gen-1")
            .expect("get")
            .expect("present");
        assert_eq!(unchanged.prompt, "a fox");
    }

    #[test]
    fn trash_restore_and_purge_keep_sources_and_clean_cache() {
        let database = database();
        let connection = database.connection();
        gen_repo::insert(
            connection,
            &generation_row("gen-1", None, "mmh3", "a fox", "succeeded"),
        )
        .expect("generation");
        insert_asset(connection, "asset-1");

        assert_eq!(
            gen_repo::soft_delete_many(connection, &["gen-1".into()]).expect("soft"),
            1
        );
        let (visible, total) = gen_repo::query(
            connection,
            &GenerationFilter {
                limit: 25,
                sort: "newest".into(),
                ..Default::default()
            },
        )
        .expect("visible");
        assert_eq!(visible.len(), 0);
        assert_eq!(total, 0);

        let (trash, total) = gen_repo::query(
            connection,
            &GenerationFilter {
                only_deleted: true,
                limit: 25,
                sort: "newest".into(),
                ..Default::default()
            },
        )
        .expect("trash");
        assert_eq!(total, 1);
        assert!(trash[0].deleted_at.is_some());

        gen_repo::restore_many(connection, &["gen-1".into()]).expect("restore");
        assert!(gen_repo::get(connection, "gen-1").expect("get").is_some());

        assets::soft_delete(connection, "asset-1").expect("asset trash");
        assert_eq!(
            assets::list_deleted(connection)
                .expect("deleted assets")
                .len(),
            1
        );
        assets::restore(connection, "asset-1").expect("asset restore");
        assert!(
            assets::list_deleted(connection)
                .expect("deleted assets")
                .is_empty()
        );

        let cache = temp_dir("library-purge-cache");
        let inside = cache.join("thumb.png");
        fs::write(&inside, b"1234").expect("inside");
        let outside = temp_dir("library-purge-source").join("source.png");
        fs::write(&outside, b"1234").expect("outside");

        assert_eq!(super::remove_cached_file(&inside, &cache), 4);
        assert!(!inside.exists());
        assert_eq!(super::remove_cached_file(&outside, &cache), 0);
        assert!(outside.exists());

        assert_eq!(
            gen_repo::hard_delete_many(connection, &["gen-1".into()]).expect("purge"),
            1
        );
        assert!(gen_repo::get(connection, "gen-1").expect("get").is_none());
        let _ = fs::remove_dir_all(cache);
    }

    #[test]
    fn export_and_import_round_trip_every_format() {
        let database = database();
        let connection = database.connection();
        let mut row = generation_row("gen-1", None, "mmh3", "a red fox", "succeeded");
        row.negative = Some("blurry, low contrast".into());
        gen_repo::insert(connection, &row).expect("generation");
        gen_repo::set_favorite(connection, "gen-1", true).expect("favorite");
        gen_repo::set_tags(connection, "gen-1", &["neon".into(), "snow".into()]).expect("tags");

        let source = gen_repo::get(connection, "gen-1")
            .expect("get")
            .expect("present");
        let exported = export_record(connection, &source).expect("export record");

        for format in [
            ExportFormat::Json,
            ExportFormat::Jsonl,
            ExportFormat::Csv,
            ExportFormat::Text,
        ] {
            let text = format
                .render(std::slice::from_ref(&exported))
                .expect("render");
            let parsed = format.parse_text(&text).expect("parse");
            assert_eq!(parsed.len(), 1, "{format:?}");
            if format == ExportFormat::Text {
                assert_eq!(parsed[0].prompt, "a red fox");
                assert!(parsed[0].negative.is_none());
            } else {
                assert_eq!(parsed[0].prompt, "a red fox");
                assert_eq!(parsed[0].negative.as_deref(), Some("blurry, low contrast"));
                assert_eq!(parsed[0].params["aspect_ratio"], "1:1");
                assert_eq!(parsed[0].tags, vec!["neon", "snow"]);
                assert_eq!(parsed[0].guidance.run.as_deref(), Some("neon"));
                assert!(parsed[0].favorite);
            }
        }
    }

    #[test]
    fn text_export_writes_one_prompt_per_line() {
        let records = vec![
            super::text_record("first prompt"),
            super::text_record("second prompt"),
        ];
        let text = ExportFormat::Text.render(&records).expect("render");
        assert_eq!(text, "first prompt\nsecond prompt");
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn record_exposes_provenance_and_tags() {
        let database = database();
        let connection = database.connection();
        gen_repo::insert(
            connection,
            &generation_row("gen-1", None, "mmh3", "a fox", "succeeded"),
        )
        .expect("generation");
        tags::attach(
            connection,
            "gen-1",
            &tags::ensure(connection, "tag-1", "neon").expect("tag"),
        )
        .expect("attach");

        let row = gen_repo::get(connection, "gen-1")
            .expect("get")
            .expect("present");
        let record = record(connection, &row).expect("record");
        assert_eq!(record.preset_version, "1.0.0");
        assert_eq!(record.models, vec!["gemma"]);
        assert_eq!(record.seed, Some(7));
        assert_eq!(record.tags, vec!["neon"]);
        assert!(record.description.is_some());
        assert_eq!(
            record.guidance,
            Guidance {
                global: None,
                preset: None,
                run: Some("neon".into()),
            }
        );
    }
}
