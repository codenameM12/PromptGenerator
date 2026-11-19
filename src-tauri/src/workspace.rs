use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::CommandError;
use crate::commands::workspace::WorkspaceDraft;

const DRAFT_FILE: &str = "workspace.json";
const CORRUPT_DRAFT_FILE: &str = "workspace.corrupt.json";

/// Persists the last workspace selection and any unapplied edit draft.
pub struct WorkspaceStore {
    dir: PathBuf,
    draft: Option<WorkspaceDraft>,
}

impl WorkspaceStore {
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, CommandError> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).map_err(|error| CommandError::from_io(&error))?;

        let path = dir.join(DRAFT_FILE);
        let draft = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<WorkspaceDraft>(&text) {
                Ok(draft) if !draft.asset_id.trim().is_empty() => Some(draft),
                Ok(_) => {
                    backup(&path, &dir);
                    None
                }
                Err(error) => {
                    eprintln!("[workspace] ignoring unreadable draft: {error}");
                    backup(&path, &dir);
                    None
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(CommandError::from_io(&error)),
        };

        Ok(Self { dir, draft })
    }

    pub fn get(&self) -> Option<WorkspaceDraft> {
        self.draft.clone()
    }

    pub fn save(&mut self, mut draft: WorkspaceDraft) -> Result<WorkspaceDraft, CommandError> {
        if draft.asset_id.trim().is_empty() {
            return Err(CommandError::validation("A workspace draft needs an asset"));
        }
        draft.updated_at_ms = now_ms();
        write_draft(&self.dir, &draft)?;
        self.draft = Some(draft.clone());
        Ok(draft)
    }

    pub fn clear(&mut self) -> Result<(), CommandError> {
        let path = self.dir.join(DRAFT_FILE);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(CommandError::from_io(&error)),
        }
        self.draft = None;
        Ok(())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn write_draft(dir: &Path, draft: &WorkspaceDraft) -> Result<(), CommandError> {
    let json = serde_json::to_string_pretty(draft)
        .map_err(|error| CommandError::internal(format!("serialize workspace draft: {error}")))?;
    let target = dir.join(DRAFT_FILE);
    let temporary = dir.join(format!("{DRAFT_FILE}.tmp"));
    fs::write(&temporary, json).map_err(|error| CommandError::from_io(&error))?;
    if target.exists() {
        fs::remove_file(&target).map_err(|error| CommandError::from_io(&error))?;
    }
    fs::rename(&temporary, &target).map_err(|error| CommandError::from_io(&error))
}

fn backup(path: &Path, dir: &Path) {
    let _ = fs::rename(path, dir.join(CORRUPT_DRAFT_FILE));
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::WorkspaceStore;
    use crate::commands::Guidance;
    use crate::commands::edits::{Crop, EditOps, FitMode};
    use crate::commands::workspace::WorkspaceDraft;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "prompt-studio-workspace-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn draft_with_crop() -> WorkspaceDraft {
        WorkspaceDraft {
            asset_id: "asset-1".into(),
            edit_ops: Some(EditOps {
                crop: Some(Crop {
                    x: 10,
                    y: 20,
                    width: 300,
                    height: 200,
                }),
                rotate: 0,
                flip_h: false,
                flip_v: false,
                fit: FitMode::Fit,
                trim: None,
                fps: None,
                remove_audio: false,
                mask_path: None,
                format: None,
                loop_video: false,
                pad_color: None,
            }),
            selected_preset_id: Some("krea2".into()),
            guidance: Guidance {
                global: None,
                preset: None,
                run: Some("keep the coat".into()),
            },
            params: None,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn restores_an_unapplied_crop_after_reload() {
        let dir = TempDir::new("restore");
        {
            let mut store = WorkspaceStore::load(&dir.path).expect("load");
            let saved = store.save(draft_with_crop()).expect("save draft");
            assert!(saved.updated_at_ms > 0);
        }

        let store = WorkspaceStore::load(&dir.path).expect("reload");
        let restored = store.get().expect("draft restored");
        assert_eq!(restored.asset_id, "asset-1");
        let crop = restored
            .edit_ops
            .and_then(|ops| ops.crop)
            .expect("crop restored");
        assert_eq!(crop.width, 300);
        assert_eq!(crop.height, 200);
        assert_eq!(restored.selected_preset_id.as_deref(), Some("krea2"));
    }

    #[test]
    fn rejects_a_draft_without_an_asset() {
        let dir = TempDir::new("no-asset");
        let mut store = WorkspaceStore::load(&dir.path).expect("load");
        let mut draft = draft_with_crop();
        draft.asset_id = " ".into();

        let error = store.save(draft).expect_err("empty asset must fail");
        assert_eq!(error.code, "validation");
        assert!(store.get().is_none());
    }

    #[test]
    fn clearing_removes_the_file() {
        let dir = TempDir::new("clear");
        let mut store = WorkspaceStore::load(&dir.path).expect("load");
        store.save(draft_with_crop()).expect("save");

        store.clear().expect("clear");
        assert!(store.get().is_none());
        assert!(!dir.path.join("workspace.json").exists());
    }

    #[test]
    fn corrupt_draft_is_ignored() {
        let dir = TempDir::new("corrupt");
        fs::write(dir.path.join("workspace.json"), "{nope").expect("write corrupt");

        let store = WorkspaceStore::load(&dir.path).expect("load");
        assert!(store.get().is_none());
        assert!(dir.path.join("workspace.corrupt.json").exists());
    }
}
