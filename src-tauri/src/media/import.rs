use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Serialize;
use uuid::Uuid;

use crate::commands::CommandError;
use crate::commands::assets::{AssetKind, ImportResult};
use crate::db::repository::assets::{self, NewAsset};
use crate::paths::AppPaths;

use super::ffmpeg::MediaTools;
use super::hash::hash_file;
use super::metadata;
use super::probe::{ProbeData, probe};
use super::proxy;
use super::still;
use super::thumbnail;

const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "avif", "gif", "bmp", "tif", "tiff",
];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "webm", "avi", "m4v"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub done: usize,
    pub total: usize,
    pub path: String,
    pub asset_id: Option<String>,
    pub duplicate: bool,
    pub error: Option<String>,
    pub cancelled: bool,
}

/// Holds the cancellation flag of the single active import.
#[derive(Default)]
pub struct ImportControl {
    pub current: Mutex<Option<Arc<AtomicBool>>>,
}

pub struct ImportService<'a> {
    pub tools: &'a MediaTools,
    pub connection: &'a Connection,
    pub paths: &'a AppPaths,
}

impl ImportService<'_> {
    pub fn import(
        &self,
        inputs: &[String],
        cancel: &AtomicBool,
        mut progress: impl FnMut(ImportProgress),
    ) -> Vec<ImportResult> {
        let files = collect_files(inputs);
        let total = files.len();
        let mut results = Vec::new();

        for (index, path) in files.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                progress(ImportProgress {
                    done: index,
                    total,
                    path: path.display().to_string(),
                    asset_id: None,
                    duplicate: false,
                    error: None,
                    cancelled: true,
                });
                break;
            }

            let result = self.import_one(path);
            progress(ImportProgress {
                done: index + 1,
                total,
                path: result.path.clone(),
                asset_id: result.asset_id.clone(),
                duplicate: result.duplicate,
                error: result.error.clone(),
                cancelled: false,
            });
            results.push(result);
        }

        results
    }

    fn import_one(&self, path: &Path) -> ImportResult {
        match self.try_import(path) {
            Ok(result) => result,
            Err(error) => ImportResult {
                path: path.display().to_string(),
                asset_id: None,
                duplicate: false,
                restored: false,
                error: Some(error.message),
            },
        }
    }

    fn try_import(&self, path: &Path) -> Result<ImportResult, CommandError> {
        if !path.is_file() {
            return Err(CommandError::not_found(format!(
                "File not found: {}",
                path.display()
            )));
        }
        if !is_supported(path) {
            return Err(CommandError::validation(format!(
                "Unsupported media format: {}",
                path.display()
            )));
        }

        let canonical = dunce::canonicalize(path).map_err(|error| CommandError::from_io(&error))?;
        let hash = hash_file(&canonical)?;

        if let Some(existing) = assets::find_by_hash(self.connection, &hash)? {
            let _ = assets::set_missing(self.connection, &existing.id, false);
            let restored = if assets::is_trashed(self.connection, &existing.id)? {
                assets::restore(self.connection, &existing.id)?
            } else {
                false
            };
            return Ok(ImportResult {
                path: canonical.display().to_string(),
                asset_id: Some(existing.id),
                duplicate: !restored,
                restored,
                error: None,
            });
        }

        let kind = kind_for_path(&canonical);
        let is_video = kind == AssetKind::Video;
        let prefix = &hash[..2];
        let thumbnail_path = self
            .paths
            .cache_dir
            .join("thumbnails")
            .join(prefix)
            .join(format!("{hash}.jpg"));

        // Common still formats are read in-process: it skips two ffprobe/ffmpeg
        // launches per image (slow, and each flashed a console window).
        let quick = if is_video {
            None
        } else {
            still::read(&canonical, &thumbnail_path)
        };
        let probe = match quick {
            Some(data) => data,
            None => {
                let probe = probe(self.tools, &canonical)?;
                if probe.display_width == 0 || probe.display_height == 0 {
                    return Err(CommandError::media("Probe returned zero dimensions"));
                }
                if !thumbnail_path.is_file() {
                    thumbnail::generate(self.tools, &canonical, &thumbnail_path, is_video)?;
                }
                probe
            }
        };

        let proxy_path = if is_video && proxy::needs_proxy(&probe) {
            let proxy_path = self
                .paths
                .cache_dir
                .join("proxies")
                .join(prefix)
                .join(format!("{hash}.mp4"));
            if !proxy_path.is_file() {
                proxy::generate(self.tools, &canonical, &proxy_path, probe.has_audio)?;
            }
            Some(proxy_path)
        } else {
            None
        };

        let embedded = metadata::extract(&canonical)
            .map(|metadata| serde_json::to_string(&metadata))
            .transpose()
            .map_err(|error| CommandError::internal(format!("serialize metadata: {error}")))?;

        let id = Uuid::now_v7().to_string();
        let asset = assets::insert(
            self.connection,
            &NewAsset {
                id,
                hash,
                kind: kind_str(kind).into(),
                source_path: canonical.display().to_string(),
                display_name: canonical
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("media")
                    .to_string(),
                width: Some(probe.display_width),
                height: Some(probe.display_height),
                fps: probe.fps,
                duration_ms: probe.duration_ms.map(|value| value as i64),
                codec: probe.codec,
                has_audio: probe.has_audio,
                color: probe.color_space,
                rotation: probe.rotation,
                thumbnail_path: Some(thumbnail_path.display().to_string()),
                proxy_path: proxy_path.map(|path| path.display().to_string()),
                embedded_metadata: embedded,
            },
        )?;

        Ok(ImportResult {
            path: canonical.display().to_string(),
            asset_id: Some(asset.id),
            duplicate: false,
            restored: false,
            error: None,
        })
    }
}

pub fn collect_files(inputs: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for input in inputs {
        let path = PathBuf::from(input);
        if path.is_dir() {
            for entry in walkdir::WalkDir::new(&path)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
            {
                if entry.file_type().is_file() && is_supported(entry.path()) {
                    files.push(entry.into_path());
                }
            }
        } else {
            files.push(path);
        }
    }

    files.sort();
    files.dedup();
    files
}

pub fn is_supported(path: &Path) -> bool {
    extension(path).is_some_and(|extension| {
        IMAGE_EXTENSIONS
            .iter()
            .chain(VIDEO_EXTENSIONS.iter())
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
    })
}

pub fn kind_for_path(path: &Path) -> AssetKind {
    match extension(path) {
        Some(extension)
            if VIDEO_EXTENSIONS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension)) =>
        {
            AssetKind::Video
        }
        _ => AssetKind::Image,
    }
}

pub fn kind_str(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Image => "image",
        AssetKind::Video => "video",
    }
}

fn extension(path: &Path) -> Option<&str> {
    path.extension().and_then(|extension| extension.to_str())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::{ImportService, collect_files};
    use crate::db::Database;
    use crate::db::repository::assets;
    use crate::media::test_support::{make_image, make_video, temp_dir, tools};
    use crate::paths::resolve_app_paths;

    #[test]
    fn reimporting_a_trashed_asset_restores_it() {
        let source_dir = temp_dir("import-restore-source");
        let cache_dir = temp_dir("import-restore-cache");
        let tools = tools();
        let image = source_dir.join("restore.png");
        make_image(&tools, &image, "40x30");

        let mut database = Database::open_in_memory().expect("db");
        database.migrate().expect("migrate");
        let paths = resolve_app_paths(&source_dir, &source_dir, &cache_dir, false).expect("paths");
        let service = ImportService {
            tools: &tools,
            connection: database.connection(),
            paths: &paths,
        };
        let cancel = AtomicBool::new(false);

        let first = service.import(&[image.display().to_string()], &cancel, |_| {});
        assert!(!first[0].restored);
        assert!(!first[0].duplicate);
        let id = first[0].asset_id.clone().expect("asset id");

        assert!(assets::soft_delete(database.connection(), &id).expect("trash"));
        assert!(
            assets::list(database.connection(), 10, 0)
                .expect("list")
                .is_empty()
        );
        assert_eq!(
            assets::list_deleted(database.connection())
                .expect("trash list")
                .len(),
            1
        );

        let second = service.import(&[image.display().to_string()], &cancel, |_| {});
        assert_eq!(second[0].asset_id.as_deref(), Some(id.as_str()));
        assert!(second[0].restored);
        assert!(!second[0].duplicate);
        assert_eq!(
            assets::list(database.connection(), 10, 0)
                .expect("list")
                .len(),
            1
        );
        assert!(
            assets::list_deleted(database.connection())
                .expect("trash list")
                .is_empty()
        );
    }

    #[test]
    fn imports_images_videos_and_deduplicates() {
        let source_dir = temp_dir("import-source");
        let cache_dir = temp_dir("import-cache");
        let tools = tools();

        let image = source_dir.join("still.png");
        make_image(&tools, &image, "64x48");
        let video = source_dir.join("clip.mp4");
        make_video(&tools, &video, "96x54", 1.0, 24, true);

        let duplicate = source_dir.join("copy.png");
        std::fs::copy(&image, &duplicate).expect("copy duplicate");

        let text = source_dir.join("notes.txt");
        std::fs::write(&text, b"hello").expect("write text");

        let mut database = Database::open_in_memory().expect("db");
        database.migrate().expect("migrate");
        let paths = resolve_app_paths(&source_dir, &source_dir, &cache_dir, false).expect("paths");

        let service = ImportService {
            tools: &tools,
            connection: database.connection(),
            paths: &paths,
        };
        let cancel = AtomicBool::new(false);
        let mut progress_count = 0;
        let results = service.import(
            &[source_dir.display().to_string(), text.display().to_string()],
            &cancel,
            |_| progress_count += 1,
        );

        assert_eq!(results.len(), 4);
        assert!(results.iter().any(|result| result.duplicate));
        assert!(results.iter().any(|result| result.error.is_some()));
        assert!(progress_count >= 4);

        let assets = assets::list(database.connection(), 10, 0).expect("list");
        assert_eq!(assets.len(), 2);

        let image_asset = assets
            .iter()
            .find(|asset| asset.kind == "image")
            .expect("image asset");
        assert!(
            image_asset
                .thumbnail_path
                .as_deref()
                .is_some_and(|path| { std::path::Path::new(path).is_file() })
        );

        let video_asset = assets
            .iter()
            .find(|asset| asset.kind == "video")
            .expect("video asset");
        assert!(video_asset.has_audio);
        assert!(video_asset.duration_ms.unwrap_or_default() >= 900);

        let _ = std::fs::remove_dir_all(source_dir);
        let _ = std::fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn cancellation_stops_before_the_next_item() {
        let source_dir = temp_dir("import-cancel-source");
        let cache_dir = temp_dir("import-cancel-cache");
        let tools = tools();

        for index in 0..3 {
            make_image(&tools, &source_dir.join(format!("{index}.png")), "32x32");
        }

        let mut database = Database::open_in_memory().expect("db");
        database.migrate().expect("migrate");
        let paths = resolve_app_paths(&source_dir, &source_dir, &cache_dir, false).expect("paths");

        let service = ImportService {
            tools: &tools,
            connection: database.connection(),
            paths: &paths,
        };
        let cancel = AtomicBool::new(false);
        let results = service.import(&[source_dir.display().to_string()], &cancel, |progress| {
            if progress.done == 1 {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });

        assert_eq!(results.len(), 1);
        assert_eq!(
            assets::list(database.connection(), 10, 0)
                .expect("list")
                .len(),
            1
        );

        let _ = std::fs::remove_dir_all(source_dir);
        let _ = std::fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn collects_nested_supported_files() {
        let directory = temp_dir("collect");
        let nested = directory.join("nested");
        std::fs::create_dir_all(&nested).expect("nested");
        std::fs::write(nested.join("a.png"), b"x").expect("a");
        std::fs::write(nested.join("b.txt"), b"x").expect("b");

        let files = collect_files(&[directory.display().to_string()]);
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("a.png"));

        let _ = std::fs::remove_dir_all(directory);
    }
}
