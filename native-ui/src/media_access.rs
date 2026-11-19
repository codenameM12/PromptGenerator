//! In-process replacement for the webview `media://` protocol's `MediaAccess`.
//!
//! The native UI loads image bytes directly, but only from the canonicalized
//! cache directory or from source folders explicitly approved after an import.
//! A path that resolves outside those roots (including via `..` or a junction)
//! is rejected before any file is read.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::backend::BackendError;

#[derive(Debug)]
pub struct AllowedRoots {
    cache_dir: PathBuf,
    roots: Mutex<Vec<PathBuf>>,
}

impl AllowedRoots {
    pub fn new(cache_dir: PathBuf) -> Self {
        // Portable builds may reach `data` through a junction; the stored path
        // is the resolved one, so canonicalize once at construction.
        let cache_dir = dunce::canonicalize(&cache_dir).unwrap_or(cache_dir);
        Self {
            cache_dir,
            roots: Mutex::new(Vec::new()),
        }
    }

    /// Approves the parent folder of `path` (an imported source file).
    pub fn allow_path(&self, path: &Path) {
        let Some(parent) = path.parent() else { return };
        let Ok(parent) = dunce::canonicalize(parent) else {
            return;
        };
        if let Ok(mut roots) = self.roots.lock() {
            if !roots.iter().any(|root| root == &parent) {
                roots.push(parent);
            }
        }
    }

    pub fn is_allowed(&self, path: &Path) -> bool {
        let Ok(canonical) = dunce::canonicalize(path) else {
            return false;
        };
        if canonical.starts_with(&self.cache_dir) {
            return true;
        }
        self.roots
            .lock()
            .map(|roots| roots.iter().any(|root| canonical.starts_with(root)))
            .unwrap_or(false)
    }

    pub fn approved_root_count(&self) -> usize {
        self.roots.lock().map(|roots| roots.len()).unwrap_or(0)
    }

    /// Reads bytes only when the path passes the allow check.
    pub fn load_bytes(&self, path: &Path) -> Result<Vec<u8>, BackendError> {
        if !self.is_allowed(path) {
            return Err(BackendError::forbidden(format!(
                "Media path is not approved: {}",
                path.display()
            )));
        }
        std::fs::read(path).map_err(|error| {
            BackendError::not_found(format!("Cannot read media {}: {error}", path.display()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AllowedRoots;

    struct Tree {
        dir: tempfile::TempDir,
    }

    impl Tree {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("temp dir");
            std::fs::create_dir_all(dir.path().join("cache")).expect("cache dir");
            std::fs::create_dir_all(dir.path().join("approved")).expect("approved dir");
            std::fs::create_dir_all(dir.path().join("outside")).expect("outside dir");
            std::fs::write(dir.path().join("approved/a.png"), b"approved").expect("write");
            std::fs::write(dir.path().join("outside/b.png"), b"outside").expect("write");
            Self { dir }
        }
    }

    #[test]
    fn rejects_out_of_root_path() {
        let tree = Tree::new();
        let access = AllowedRoots::new(tree.dir.path().join("cache"));
        let outside = tree.dir.path().join("outside/b.png");
        assert!(!access.is_allowed(&outside));
        let error = access.load_bytes(&outside).expect_err("must reject");
        assert_eq!(error.code, "forbidden");
    }

    #[test]
    fn accepts_allowed_source_after_allow_path() {
        let tree = Tree::new();
        let access = AllowedRoots::new(tree.dir.path().join("cache"));
        let source = tree.dir.path().join("approved/a.png");
        access.allow_path(&source);
        assert_eq!(access.approved_root_count(), 1);
        assert!(access.is_allowed(&source));
        assert_eq!(access.load_bytes(&source).expect("read"), b"approved");
    }

    #[test]
    fn cache_files_are_always_allowed() {
        let tree = Tree::new();
        let access = AllowedRoots::new(tree.dir.path().join("cache"));
        let thumb = tree.dir.path().join("cache/thumb.png");
        std::fs::write(&thumb, b"thumb").expect("write thumb");
        assert!(access.is_allowed(&thumb));
        assert_eq!(access.load_bytes(&thumb).expect("read"), b"thumb");
    }

    #[test]
    fn traversal_outside_root_is_rejected() {
        let tree = Tree::new();
        let access = AllowedRoots::new(tree.dir.path().join("cache"));
        let attack = tree
            .dir
            .path()
            .join("approved")
            .join("..")
            .join("outside")
            .join("b.png");
        assert!(!access.is_allowed(&attack));
    }
}
