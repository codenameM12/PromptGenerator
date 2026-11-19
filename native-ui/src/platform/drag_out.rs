//! Drag-out fallback: prepare the payload under the cache and write it to a
//! user-chosen path with a native save dialog.
//!
//! DIVERGENCE: the Tauri app used `tauri-plugin-drag` to initiate an OS drag.
//! There is no drop-in eframe equivalent, so the native binary offers a
//! file-save fallback. This must be recorded in `PORTING.md` at integration.

use std::path::{Path, PathBuf};

use crate::backend::BackendError;

const DRAG_DIR: &str = "drag";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedDrag {
    pub files: Vec<PathBuf>,
    pub prepared_dir: PathBuf,
    pub suggested_name: String,
}

/// Writes a prepared prompt/JSON payload below `<cache>/drag/<session>/`.
pub fn prepare_payload(
    cache_dir: &Path,
    session: &str,
    suggested_name: &str,
    contents: &str,
) -> Result<PreparedDrag, BackendError> {
    let directory = cache_dir.join(DRAG_DIR).join(session);
    std::fs::create_dir_all(&directory).map_err(|error| {
        BackendError::validation(format!("Could not prepare drag folder: {error}"))
    })?;
    let safe_name = sanitize_name(suggested_name);
    let file = directory.join(format!("{safe_name}.txt"));
    std::fs::write(&file, contents).map_err(|error| {
        BackendError::validation(format!("Could not write drag payload: {error}"))
    })?;
    Ok(PreparedDrag {
        files: vec![file],
        prepared_dir: directory,
        suggested_name: format!("{safe_name}.txt"),
    })
}

/// Copies a prepared file to the path chosen in the native save dialog.
/// Returns the number of bytes written.
pub fn save_to_chosen_path(source: &Path, output: &Path) -> Result<u64, BackendError> {
    if !source.is_file() {
        return Err(BackendError::not_found(
            "Prepared drag file no longer exists",
        ));
    }
    std::fs::copy(source, output)
        .map_err(|error| BackendError::validation(format!("Could not save drag payload: {error}")))
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "prompt".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{prepare_payload, sanitize_name, save_to_chosen_path};

    #[test]
    fn prepares_and_saves_a_payload() {
        let dir = tempfile::tempdir().expect("temp dir");
        let prepared =
            prepare_payload(dir.path(), "session-1", "my:prompt", "hello world").expect("prepare");
        assert_eq!(prepared.files.len(), 1);
        assert_eq!(prepared.suggested_name, "my_prompt.txt");
        assert_eq!(
            std::fs::read_to_string(&prepared.files[0]).expect("read prepared"),
            "hello world"
        );

        let chosen = dir.path().join("chosen.txt");
        let bytes = save_to_chosen_path(&prepared.files[0], &chosen).expect("save");
        assert_eq!(bytes, 11);
        assert_eq!(
            std::fs::read_to_string(&chosen).expect("read chosen"),
            "hello world"
        );
    }

    #[test]
    fn rejects_missing_source() {
        let dir = tempfile::tempdir().expect("temp dir");
        let error = save_to_chosen_path(&dir.path().join("missing"), &dir.path().join("out"))
            .expect_err("missing source");
        assert_eq!(error.code, "not_found");
    }

    #[test]
    fn sanitizes_windows_names() {
        assert_eq!(sanitize_name("a/b:c*?"), "a_b_c__");
        assert_eq!(sanitize_name("   "), "prompt");
    }
}
