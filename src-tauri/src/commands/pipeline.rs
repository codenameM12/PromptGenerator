use base64::Engine as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::db::Database;
use crate::db::repository::assets as asset_repo;
use crate::db::repository::edits as edit_repo;
use crate::db::repository::presets as preset_repo;
use crate::media::ffmpeg::{MediaTools, run_checked};
use crate::media::frames::{self as frame_sampler, SampleRequest, SamplingStrategy};
use crate::paths::AppPaths;
use crate::pipeline::{BatchItemInput, BatchReferenceInput, ComposeInput, DescribeInput, Pipeline};
use crate::preset;
use crate::runtime::manager::ModelManager;
use crate::settings::SettingsStore;

use super::{CommandError, Guidance, PromptWarning};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DescribeMode {
    Image,
    Video,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribeRequest {
    pub asset_id: Option<String>,
    pub edit_id: Option<String>,
    pub frame_paths: Vec<String>,
    pub mode: DescribeMode,
    pub hint: Option<String>,
    pub force_refresh: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Description {
    pub id: String,
    pub mode: DescribeMode,
    /// Verbose prose returned by the describer.
    pub text: String,
    pub cached: bool,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeReference {
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub edit_id: Option<String>,
    /// Requested reference weight. Missing values default to 1.0 and are
    /// clamped to the supported 0.0-2.0 range by the pipeline.
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub name: Option<String>,
    /// Verbose text description for this reference.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeRequest {
    pub preset_id: String,
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub edit_id: Option<String>,
    /// Ordered reference list for multi-reference targets. Old callers that
    /// only send `asset_id`/`description` keep working because the field is
    /// omitted from their payload.
    #[serde(default)]
    pub references: Vec<ComposeReference>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub dataset_row_id: Option<String>,
    pub description: Option<String>,
    pub brief: Option<String>,
    pub guidance: Guidance,
    pub locked: Vec<String>,
    #[serde(default)]
    pub params: Value,
    pub existing_prompt: Option<String>,
    #[serde(default)]
    pub instruction: Option<String>,
    pub seed: Option<u64>,
    pub variations: u8,
    pub bypass_cache: bool,
    #[serde(default)]
    pub output_language: Option<String>,
    /// Compose/Refine option: send the reference images to the composer model.
    /// `None`/`false` keeps the established text-only behavior.
    #[serde(default)]
    pub include_images: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Generation {
    pub id: String,
    pub prompt: String,
    pub negative: Option<String>,
    pub params: Value,
    pub warnings: Vec<PromptWarning>,
    pub seed: u64,
    pub model: String,
    pub preset_id: String,
    pub preset_version: String,
    pub cached: bool,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItem {
    pub asset_id: Option<String>,
    pub brief: Option<String>,
    #[serde(default)]
    pub references: Vec<ComposeReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {
    pub items: Vec<BatchItem>,
    pub preset_id: String,
    pub guidance: Guidance,
    pub concurrency: u8,
    #[serde(default)]
    pub output_language: Option<String>,
}

/// Cancellation flag for the active composition.
#[derive(Default)]
pub struct PipelineControl {
    pub current: Mutex<Option<Arc<AtomicBool>>>,
}

/// Shared state for resolving composer image parts.
struct ComposerImages<'a> {
    database: &'a Mutex<Database>,
    paths: &'a AppPaths,
    tools: &'a MediaTools,
    manager: &'a ModelManager,
}

/// Resolves data-URL images for the composer option: each applicable reference
/// contributes its saved edit output, else a still frame of the source.
async fn composer_images(
    context: ComposerImages<'_>,
    references: &[ComposeReference],
) -> Result<(Vec<String>, Option<String>), CommandError> {
    let ComposerImages {
        database,
        paths,
        tools,
        manager,
    } = context;
    if references.is_empty() {
        return Ok((Vec::new(), None));
    }
    let composer = manager.roles().composer;
    if manager
        .status()
        .await
        .models
        .iter()
        .any(|model| model.id == composer && model.vision == Some(false))
    {
        return Err(CommandError::validation(format!(
            "Composer model {composer} does not accept images; disable Send media to composer or choose a vision model"
        )));
    }

    // Compact reference identities hash into the cache key without embedding
    // image bytes in it.
    struct Resolved {
        identity: String,
        path: PathBuf,
    }
    let mut resolved = Vec::new();
    {
        let guard = database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        let connection = guard.connection();
        for reference in references.iter().take(4) {
            let item = match (&reference.edit_id, &reference.asset_id) {
                (Some(edit_id), _) => {
                    let Some(edit) = edit_repo::get(connection, edit_id)? else {
                        continue;
                    };
                    Resolved {
                        identity: format!("edit:{}", edit.cache_key),
                        path: PathBuf::from(edit.output_path),
                    }
                }
                (None, Some(asset_id)) => {
                    let Some(asset) = asset_repo::get(connection, asset_id)? else {
                        continue;
                    };
                    // A plain reference follows the same rule as the picker: use
                    // the newest applied edit when one exists.
                    if let Some(edit) = edit_repo::latest_for_asset(connection, asset_id)? {
                        let edited = PathBuf::from(&edit.output_path);
                        if asset.kind == "video" {
                            let sampled = frame_sampler::sample(
                                tools,
                                &SampleRequest {
                                    source: &edited,
                                    duration_ms: edit.duration_ms.unwrap_or_default().max(0) as u64,
                                    is_video: true,
                                    strategy: SamplingStrategy::Even,
                                    count: 1,
                                    max_images: 1,
                                    want_contact_sheet: false,
                                    cache_dir: &paths.cache_dir,
                                    asset_hash: &edit.cache_key,
                                },
                            )?;
                            let Some(frame) = sampled.frames.into_iter().next() else {
                                continue;
                            };
                            Resolved {
                                identity: format!("edit:{}:frame", edit.cache_key),
                                path: frame.path,
                            }
                        } else {
                            Resolved {
                                identity: format!("edit:{}", edit.cache_key),
                                path: still_frame(
                                    tools,
                                    paths,
                                    &edit.output_path,
                                    &edit.cache_key,
                                )?,
                            }
                        }
                    } else if asset.kind == "video" {
                        let sampled = frame_sampler::sample(
                            tools,
                            &SampleRequest {
                                source: std::path::Path::new(&asset.source_path),
                                duration_ms: asset.duration_ms.unwrap_or_default().max(0) as u64,
                                is_video: true,
                                strategy: SamplingStrategy::Even,
                                count: 1,
                                max_images: 1,
                                want_contact_sheet: false,
                                cache_dir: &paths.cache_dir,
                                asset_hash: &asset.hash,
                            },
                        )?;
                        let Some(frame) = sampled.frames.into_iter().next() else {
                            continue;
                        };
                        Resolved {
                            identity: format!("asset:{}:frame", asset.hash),
                            path: frame.path,
                        }
                    } else {
                        Resolved {
                            identity: format!("asset:{}", asset.hash),
                            path: still_frame(tools, paths, &asset.source_path, &asset.hash)?,
                        }
                    }
                }
                (None, None) => continue,
            };
            resolved.push(item);
        }
    }

    if resolved.is_empty() {
        return Ok((Vec::new(), None));
    }
    let mut images = Vec::new();
    for item in &resolved {
        let bytes = std::fs::read(&item.path).map_err(|error| {
            CommandError::media(format!(
                "read composer image {}: {error}",
                item.path.display()
            ))
        })?;
        let mime = match item
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("png") => "image/png",
            Some("gif") => "image/gif",
            _ => "image/jpeg",
        };
        images.push(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ));
    }

    let mut hash = blake3::Hasher::new();
    for item in &resolved {
        hash.update(item.identity.as_bytes());
    }
    Ok((images, Some(hash.finalize().to_hex().to_string())))
}

/// Plain entry point shared by the Tauri command and the native facade.
///
/// Keeps frame resolution and role/system-prompt selection in this module; the
/// native caller supplies its own state instead of Tauri `State` guards.
pub async fn describe(
    request: DescribeRequest,
    database: &Mutex<Database>,
    paths: &AppPaths,
    tools: &MediaTools,
    manager: &Arc<ModelManager>,
    settings: &Mutex<SettingsStore>,
) -> Result<Description, CommandError> {
    let describer_prompt = role_prompt(settings, true);
    let (asset_hash, frames) = {
        let guard = database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        resolve_frames(&guard, paths, tools, &request)?
    };
    let pipeline = Pipeline { manager, database };

    pipeline
        .describe(&DescribeInput {
            asset_id: request.asset_id,
            edit_id: request.edit_id,
            asset_hash,
            mode: request.mode,
            frames,
            hint: request.hint,
            force_refresh: request.force_refresh,
            system_prompt: describer_prompt,
        })
        .await
}

#[tauri::command]
pub async fn pipeline_describe(
    request: DescribeRequest,
    database: State<'_, Mutex<Database>>,
    paths: State<'_, AppPaths>,
    tools: State<'_, MediaTools>,
    manager: State<'_, Arc<ModelManager>>,
    settings: State<'_, Mutex<SettingsStore>>,
) -> Result<Description, CommandError> {
    describe(
        request,
        database.inner(),
        paths.inner(),
        tools.inner(),
        manager.inner(),
        settings.inner(),
    )
    .await
}

/// Plain entry point for composition. `on_token` lets the caller decide what
/// to do with streamed tokens; the native facade passes a no-op callback
/// because there is no webview to emit `pipeline://token` to.
#[allow(clippy::too_many_arguments)]
pub async fn compose(
    request: ComposeRequest,
    database: &Mutex<Database>,
    paths: &AppPaths,
    tools: &MediaTools,
    manager: &Arc<ModelManager>,
    control: &PipelineControl,
    settings: &Mutex<SettingsStore>,
    on_token: &mut (dyn for<'b> FnMut(&'b str) + Send),
) -> Result<Vec<Generation>, CommandError> {
    let composer_prompt = role_prompt(settings, false);
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut current = control
            .current
            .lock()
            .map_err(|_| CommandError::internal("Pipeline lock is poisoned"))?;
        if current.is_some() {
            return Err(CommandError::conflict("A generation is already running"));
        }
        *current = Some(cancel.clone());
    }

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

    let mut references = request.references.clone();
    {
        let guard = database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        for reference in &mut references {
            if reference.name.is_none()
                && let Some(asset_id) = &reference.asset_id
                && let Ok(Some(asset)) = asset_repo::get(guard.connection(), asset_id)
            {
                reference.name = Some(asset.display_name);
            }
        }
    }

    let primary_asset_id = request.asset_id.clone().or_else(|| {
        references
            .first()
            .and_then(|reference| reference.asset_id.clone())
    });
    let primary_edit_id = request.edit_id.clone().or_else(|| {
        references
            .first()
            .and_then(|reference| reference.edit_id.clone())
    });
    let primary_description = request.description.clone().or_else(|| {
        references
            .first()
            .and_then(|reference| reference.description.clone())
    });

    let (images, images_key) = if request.include_images {
        composer_images(
            ComposerImages {
                database,
                paths,
                tools,
                manager,
            },
            &references,
        )
        .await?
    } else {
        (Vec::new(), None)
    };

    let outcome = pipeline
        .compose(
            &ComposeInput {
                preset_id: request.preset_id,
                spec,
                asset_id: primary_asset_id,
                edit_id: primary_edit_id,
                references,
                parent_id: request.parent_id,
                description: primary_description,
                brief: request.brief,
                existing_prompt: request.existing_prompt,
                instruction: request.instruction,
                guidance: request.guidance,
                locked: request.locked,
                params: request.params,
                variations: request.variations,
                seed: request.seed,
                output_language: request.output_language,
                bypass_cache: request.bypass_cache,
                cancel: Some(cancel.clone()),
                system_prompt: composer_prompt,
                images,
                images_key,
            },
            Some(on_token),
        )
        .await;

    if let Ok(mut current) = control.current.lock() {
        *current = None;
    }
    outcome
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn pipeline_compose(
    request: ComposeRequest,
    app: AppHandle,
    database: State<'_, Mutex<Database>>,
    paths: State<'_, AppPaths>,
    tools: State<'_, MediaTools>,
    manager: State<'_, Arc<ModelManager>>,
    control: State<'_, PipelineControl>,
    settings: State<'_, Mutex<SettingsStore>>,
) -> Result<Vec<Generation>, CommandError> {
    let app_handle = app.clone();
    let mut on_token = move |token: &str| {
        let _ = app_handle.emit("pipeline://token", token);
    };
    compose(
        request,
        database.inner(),
        paths.inner(),
        tools.inner(),
        manager.inner(),
        control.inner(),
        settings.inner(),
        &mut on_token,
    )
    .await
}

/// Plain entry point shared by the Tauri command and the native facade.
#[allow(clippy::too_many_arguments)]
pub async fn batch(
    request: BatchRequest,
    database: &Mutex<Database>,
    paths: &AppPaths,
    tools: &MediaTools,
    manager: &Arc<ModelManager>,
    settings: &Mutex<SettingsStore>,
) -> Result<Vec<Vec<Generation>>, CommandError> {
    let composer_prompt = role_prompt(settings, false);
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

    let mut items = Vec::new();
    {
        let guard = database
            .lock()
            .map_err(|_| CommandError::internal("Database lock is poisoned"))?;
        for item in &request.items {
            let mut references = Vec::new();
            for reference in &item.references {
                if let Some(resolved) = resolve_reference(&guard, paths, tools, reference)? {
                    references.push(resolved);
                }
            }
            match &item.asset_id {
                Some(asset_id) => {
                    let (asset_hash, frames) =
                        resolve_asset_frames(&guard, paths, tools, asset_id, None)?;
                    items.push(BatchItemInput {
                        asset_id: Some(asset_id.clone()),
                        edit_id: None,
                        asset_hash,
                        frames,
                        brief: item.brief.clone(),
                        references,
                        system_prompt: composer_prompt.clone(),
                    });
                }
                None => items.push(BatchItemInput {
                    asset_id: None,
                    edit_id: None,
                    asset_hash: String::new(),
                    frames: Vec::new(),
                    brief: item.brief.clone(),
                    references,
                    system_prompt: composer_prompt.clone(),
                }),
            }
        }
    }

    let pipeline = Pipeline { manager, database };
    pipeline
        .batch(
            &items,
            &spec,
            &request.guidance,
            request.output_language.as_deref(),
        )
        .await
}

#[tauri::command]
pub async fn pipeline_batch(
    request: BatchRequest,
    database: State<'_, Mutex<Database>>,
    paths: State<'_, AppPaths>,
    tools: State<'_, MediaTools>,
    manager: State<'_, Arc<ModelManager>>,
    settings: State<'_, Mutex<SettingsStore>>,
) -> Result<Vec<Vec<Generation>>, CommandError> {
    batch(
        request,
        database.inner(),
        paths.inner(),
        tools.inner(),
        manager.inner(),
        settings.inner(),
    )
    .await
}

#[tauri::command]
pub async fn pipeline_cancel(control: State<'_, PipelineControl>) -> Result<(), CommandError> {
    if let Some(flag) = control
        .current
        .lock()
        .map_err(|_| CommandError::internal("Pipeline lock is poisoned"))?
        .as_ref()
    {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

fn role_prompt(store: &Mutex<SettingsStore>, describer: bool) -> Option<String> {
    let guard = store.lock().ok()?;
    let settings = guard.get();
    if describer {
        settings.describer_system_prompt
    } else {
        settings.composer_system_prompt
    }
}

fn resolve_frames(
    database: &Database,
    paths: &AppPaths,
    tools: &MediaTools,
    request: &DescribeRequest,
) -> Result<(String, Vec<PathBuf>), CommandError> {
    if !request.frame_paths.is_empty() {
        let hash = request
            .asset_id
            .as_deref()
            .and_then(|id| asset_repo::get(database.connection(), id).ok().flatten())
            .map(|asset| asset.hash)
            .unwrap_or_else(|| "frames".into());
        return Ok((
            hash,
            request.frame_paths.iter().map(PathBuf::from).collect(),
        ));
    }

    if let Some(edit_id) = &request.edit_id
        && let Some(edit) = edit_repo::get(database.connection(), edit_id)?
    {
        return Ok((edit.cache_key, vec![PathBuf::from(edit.output_path)]));
    }

    if let Some(asset_id) = &request.asset_id {
        return resolve_asset_frames(database, paths, tools, asset_id, Some(request.mode));
    }

    Err(CommandError::validation(
        "Describe needs an asset, an edit, or explicit frames",
    ))
}

fn resolve_reference(
    database: &Database,
    paths: &AppPaths,
    tools: &MediaTools,
    reference: &ComposeReference,
) -> Result<Option<BatchReferenceInput>, CommandError> {
    if let Some(edit_id) = &reference.edit_id {
        let edit = edit_repo::get(database.connection(), edit_id)?
            .ok_or_else(|| CommandError::not_found("Edit not found"))?;
        return Ok(Some(BatchReferenceInput {
            asset_id: reference.asset_id.clone(),
            edit_id: Some(edit_id.clone()),
            asset_hash: edit.cache_key,
            frames: vec![PathBuf::from(edit.output_path)],
            weight: reference.weight,
            name: reference.name.clone(),
            description: reference.description.clone(),
        }));
    }
    if let Some(asset_id) = &reference.asset_id {
        let (asset_hash, frames) = resolve_asset_frames(database, paths, tools, asset_id, None)?;
        let name = reference.name.clone().or_else(|| {
            asset_repo::get(database.connection(), asset_id)
                .ok()
                .flatten()
                .map(|asset| asset.display_name)
        });
        return Ok(Some(BatchReferenceInput {
            asset_id: Some(asset_id.clone()),
            edit_id: None,
            asset_hash,
            frames,
            weight: reference.weight,
            name,
            description: reference.description.clone(),
        }));
    }
    Ok(None)
}

/// Still images reach the vision model as PNG or JPEG. Other formats are
/// converted once into the cache, so WebP/AVIF/TIFF/GIF describe correctly.
fn still_frame(
    tools: &MediaTools,
    paths: &AppPaths,
    source_path: &str,
    hash: &str,
) -> Result<PathBuf, CommandError> {
    let source = PathBuf::from(source_path);
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
        return Ok(source);
    }

    let directory = paths.cache_dir.join("describe");
    std::fs::create_dir_all(&directory).map_err(|error| CommandError::from_io(&error))?;
    let target = directory.join(format!("{hash}.png"));
    if target.is_file() {
        return Ok(target);
    }

    let mut command = tools.ffmpeg_command();
    command
        .arg("-y")
        .arg("-i")
        .arg(&source)
        .arg("-frames:v")
        .arg("1")
        .arg(&target);
    run_checked(command, &format!("convert {source_path} for the describer"))?;
    Ok(target)
}

fn resolve_asset_frames(
    database: &Database,
    paths: &AppPaths,
    tools: &MediaTools,
    asset_id: &str,
    mode: Option<DescribeMode>,
) -> Result<(String, Vec<PathBuf>), CommandError> {
    let asset = asset_repo::get(database.connection(), asset_id)?
        .ok_or_else(|| CommandError::not_found("Asset not found"))?;

    if asset.kind != "video" {
        let frame = still_frame(tools, paths, &asset.source_path, &asset.hash)?;
        return Ok((asset.hash, vec![frame]));
    }

    if mode == Some(DescribeMode::Edit) {
        return Ok((asset.hash, vec![PathBuf::from(asset.source_path)]));
    }

    let duration = asset.duration_ms.unwrap_or_default().max(0) as u64;
    let sampled = frame_sampler::sample(
        tools,
        &SampleRequest {
            source: std::path::Path::new(&asset.source_path),
            duration_ms: duration,
            is_video: true,
            strategy: SamplingStrategy::Even,
            count: 8,
            max_images: 9,
            want_contact_sheet: false,
            cache_dir: &paths.cache_dir,
            asset_hash: &asset.hash,
        },
    )?;

    Ok((
        asset.hash,
        sampled.frames.into_iter().map(|frame| frame.path).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::still_frame;
    use crate::media::ffmpeg::run_checked;
    use crate::media::test_support::{make_image, temp_dir, tools};
    use crate::paths::resolve_app_paths;

    #[test]
    fn converts_webp_once_and_reuses_the_png() {
        let source_dir = temp_dir("webp-describe-source");
        let cache_dir = temp_dir("webp-describe-cache");
        let tools = tools();
        let png = source_dir.join("still.png");
        make_image(&tools, &png, "40x30");
        let webp = source_dir.join("still.webp");
        let mut command = tools.ffmpeg_command();
        command.arg("-y").arg("-i").arg(&png).arg(&webp);
        run_checked(command, "make webp").expect("webp");

        let paths = resolve_app_paths(&source_dir, &source_dir, &cache_dir, false).expect("paths");
        let converted =
            still_frame(&tools, &paths, &webp.display().to_string(), "hash-webp").expect("convert");
        let bytes = std::fs::read(&converted).expect("read converted");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert!(
            converted
                .file_name()
                .is_some_and(|name| name == "hash-webp.png")
        );

        let again =
            still_frame(&tools, &paths, &webp.display().to_string(), "hash-webp").expect("reuse");
        assert_eq!(again, converted);
    }
}
