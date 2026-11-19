use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::db::Database;
use crate::db::repository::presets as repo;
use crate::preset::{self, CompileInput};

use super::{CommandError, Guidance, PromptWarning};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetKind {
    Image,
    Video,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetSummary {
    pub id: String,
    pub name: String,
    pub kind: PresetKind,
    pub version: String,
    pub description: String,
    pub builtin: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetDocument {
    pub id: String,
    pub name: String,
    pub kind: PresetKind,
    pub version: String,
    pub hash: String,
    pub builtin: bool,
    pub document: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileDocumentRequest {
    pub document: Value,
    pub guidance: Guidance,
    pub description: Option<Value>,
    pub locked: Vec<String>,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileRequest {
    pub preset_id: String,
    pub guidance: Guidance,
    pub description: Option<Value>,
    pub locked: Vec<String>,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledPrompt {
    pub prompt: String,
    pub negative: Option<String>,
    pub params: Value,
    pub warnings: Vec<PromptWarning>,
    pub char_count: usize,
    pub token_estimate: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePresetRequest {
    pub document: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePresetRequest {
    pub id: String,
    pub confirm_in_use: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetPathRequest {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetExportSummary {
    pub path: String,
    pub bytes: u64,
}

#[tauri::command]
pub async fn presets_list(
    database: State<'_, Mutex<Database>>,
) -> Result<Vec<PresetSummary>, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let rows = repo::list(guard.connection(), false)?;

    rows.into_iter()
        .map(|row| {
            let kind = kind_from_db(&row.kind)?;
            Ok(PresetSummary {
                id: row.id,
                name: row.name,
                kind,
                version: row.version,
                description: row.description,
                builtin: row.owner == "builtin",
            })
        })
        .collect()
}

#[tauri::command]
pub async fn presets_get(
    id: String,
    database: State<'_, Mutex<Database>>,
) -> Result<PresetDocument, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let row = repo::get(guard.connection(), &id)?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| CommandError::not_found("Preset not found"))?;
    let document: Value = serde_json::from_str(&row.document_json)
        .map_err(|error| CommandError::internal(format!("read preset document: {error}")))?;

    Ok(PresetDocument {
        id: row.id,
        name: row.name,
        kind: kind_from_db(&row.kind)?,
        version: row.version,
        hash: row.content_hash,
        builtin: row.owner == "builtin",
        document,
    })
}

#[tauri::command]
pub async fn presets_preview(
    request: CompileRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<CompiledPrompt, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let row = repo::get(guard.connection(), &request.preset_id)?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| CommandError::not_found("Preset not found"))?;
    let document: Value = serde_json::from_str(&row.document_json)
        .map_err(|error| CommandError::internal(format!("read preset document: {error}")))?;
    let spec = preset::parse(&document)?;
    preset::validate(&spec)?;

    let mut compiled = preset::compile(
        &spec,
        &CompileInput {
            guidance: request.guidance,
            description: request.description,
            brief: None,
            locked: request.locked,
            params: request.params,
            existing_prompt: None,
        },
    );
    compiled
        .warnings
        .extend(preset::preflight(&spec, &compiled));
    Ok(compiled)
}

#[tauri::command]
pub async fn presets_compile(
    request: CompileDocumentRequest,
) -> Result<CompiledPrompt, CommandError> {
    let spec = preset::parse(&request.document)?;
    preset::validate(&spec)?;
    let mut compiled = preset::compile(
        &spec,
        &CompileInput {
            guidance: request.guidance,
            description: request.description,
            brief: None,
            locked: request.locked,
            params: request.params,
            existing_prompt: None,
        },
    );
    compiled
        .warnings
        .extend(preset::preflight(&spec, &compiled));
    Ok(compiled)
}

#[tauri::command]
pub async fn presets_save(
    request: SavePresetRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<PresetDocument, CommandError> {
    let spec = preset::parse(&request.document)?;
    preset::validate(&spec)?;
    save_user_preset(&database, &spec, false)
}

#[tauri::command]
pub async fn presets_duplicate(
    id: String,
    database: State<'_, Mutex<Database>>,
) -> Result<PresetDocument, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let row = repo::get(guard.connection(), &id)?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| CommandError::not_found("Preset not found"))?;
    let document: Value = serde_json::from_str(&row.document_json)
        .map_err(|error| CommandError::internal(format!("read preset document: {error}")))?;
    let mut spec = preset::parse(&document)?;

    let mut candidate = format!("{}-copy", spec.id);
    let mut index = 2;
    while repo::get(guard.connection(), &candidate)?.is_some() {
        candidate = format!("{}-copy-{index}", spec.id);
        index += 1;
    }

    spec.id = candidate;
    spec.name = format!("{} (copy)", spec.name);
    let hash = preset::content_hash(&spec)?;

    repo::upsert(
        guard.connection(),
        &repo::NewPreset {
            id: spec.id.clone(),
            owner: "user".into(),
            name: spec.name.clone(),
            kind: kind_str(spec.kind).into(),
            version: spec.version.clone(),
            description: spec.description.clone(),
            document_json: serde_json::to_string(&spec)
                .map_err(|error| CommandError::internal(format!("serialize preset: {error}")))?,
            content_hash: hash,
        },
    )?;

    to_document(spec, false)
}

#[tauri::command]
pub async fn presets_delete(
    request: DeletePresetRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<(), CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let row = repo::get(guard.connection(), &request.id)?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| CommandError::not_found("Preset not found"))?;

    if row.owner == "builtin" {
        return Err(CommandError::forbidden(
            "Built-in presets cannot be deleted; duplicate one instead",
        ));
    }
    if repo::is_referenced(guard.connection(), &request.id)? && !request.confirm_in_use {
        return Err(CommandError::conflict(
            "This preset is referenced by history; confirm deletion to continue",
        ));
    }

    repo::delete(guard.connection(), &request.id)?;
    Ok(())
}

#[tauri::command]
pub async fn presets_import(
    request: PresetPathRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<PresetDocument, CommandError> {
    let path = std::path::PathBuf::from(&request.path);
    let metadata = std::fs::metadata(&path).map_err(|error| CommandError::from_io(&error))?;
    if metadata.len() > 256 * 1024 {
        return Err(CommandError::validation(
            "Preset files must be smaller than 256 KB",
        ));
    }
    let text = std::fs::read_to_string(&path).map_err(|error| CommandError::from_io(&error))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| CommandError::validation(format!("Preset is not valid JSON: {error}")))?;
    let document = value.get("document").cloned().unwrap_or(value);
    let mut spec = preset::parse(&document)?;
    preset::validate(&spec)?;

    // Imported presets are always user-owned; avoid clobbering built-ins.
    if let Ok(guard) = database.lock()
        && let Some(existing) = repo::get(guard.connection(), &spec.id)?
        && existing.owner == "builtin"
    {
        let mut candidate = format!("{}-imported", spec.id);
        let mut index = 2;
        while repo::get(guard.connection(), &candidate)?.is_some() {
            candidate = format!("{}-imported-{index}", spec.id);
            index += 1;
        }
        spec.id = candidate;
    }

    save_user_preset(&database, &spec, true)
}

#[tauri::command]
pub async fn presets_export(
    request: PresetPathRequest,
    database: State<'_, Mutex<Database>>,
) -> Result<PresetExportSummary, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
    let row = repo::get(guard.connection(), &request.id)?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| CommandError::not_found("Preset not found"))?;

    let path = std::path::PathBuf::from(&request.path);
    if let Some(parent) = path.parent()
        && !parent.is_dir()
    {
        return Err(CommandError::validation(format!(
            "Output folder does not exist: {}",
            parent.display()
        )));
    }

    let document: Value = serde_json::from_str(&row.document_json)
        .map_err(|error| CommandError::internal(format!("read preset document: {error}")))?;
    let json = serde_json::to_string_pretty(&document)
        .map_err(|error| CommandError::internal(format!("serialize preset: {error}")))?;
    let temporary = path.with_extension("partial");
    std::fs::write(&temporary, &json).map_err(|error| CommandError::from_io(&error))?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| CommandError::from_io(&error))?;
    }
    std::fs::rename(&temporary, &path).map_err(|error| CommandError::from_io(&error))?;

    Ok(PresetExportSummary {
        path: path.display().to_string(),
        bytes: json.len() as u64,
    })
}

fn save_user_preset(
    database: &State<'_, Mutex<Database>>,
    spec: &preset::PresetSpec,
    allow_existing_user: bool,
) -> Result<PresetDocument, CommandError> {
    let guard = database
        .lock()
        .map_err(|_| CommandError::internal("Database lock is poisoned"))?;

    if let Some(existing) = repo::get(guard.connection(), &spec.id)? {
        if existing.owner == "builtin" {
            return Err(CommandError::conflict(
                "That id belongs to a built-in preset; duplicate it instead",
            ));
        }
        if !allow_existing_user && existing.deleted_at.is_some() {
            return Err(CommandError::conflict(
                "A deleted preset uses that id; restore it or choose another id",
            ));
        }
    }

    let hash = preset::content_hash(spec)?;
    repo::upsert(
        guard.connection(),
        &repo::NewPreset {
            id: spec.id.clone(),
            owner: "user".into(),
            name: spec.name.clone(),
            kind: kind_str(spec.kind).into(),
            version: spec.version.clone(),
            description: spec.description.clone(),
            document_json: serde_json::to_string(spec)
                .map_err(|error| CommandError::internal(format!("serialize preset: {error}")))?,
            content_hash: hash,
        },
    )?;

    to_document(spec.clone(), false)
}

fn to_document(spec: preset::PresetSpec, builtin: bool) -> Result<PresetDocument, CommandError> {
    Ok(PresetDocument {
        id: spec.id.clone(),
        name: spec.name.clone(),
        kind: spec.kind,
        version: spec.version.clone(),
        hash: preset::content_hash(&spec)?,
        builtin,
        document: serde_json::to_value(&spec)
            .map_err(|error| CommandError::internal(format!("serialize preset: {error}")))?,
    })
}

fn kind_from_db(value: &str) -> Result<PresetKind, CommandError> {
    match value {
        "image" => Ok(PresetKind::Image),
        "video" => Ok(PresetKind::Video),
        "edit" => Ok(PresetKind::Edit),
        other => Err(CommandError::internal(format!(
            "Unknown preset kind in database: {other}"
        ))),
    }
}

fn kind_str(kind: PresetKind) -> &'static str {
    match kind {
        PresetKind::Image => "image",
        PresetKind::Video => "video",
        PresetKind::Edit => "edit",
    }
}
