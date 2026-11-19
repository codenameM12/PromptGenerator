use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::CommandError;

/// Rejects any path that resolves outside an explicit set of approved roots.
///
/// Roots are canonicalized once at construction. Every check canonicalizes the
/// candidate first, so `..` traversal, symlinks, and directory junctions cannot
/// escape a root.
#[derive(Debug, Clone)]
pub struct PathGuard {
    roots: Vec<PathBuf>,
}

impl PathGuard {
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Result<Self, CommandError> {
        let mut canonical_roots = Vec::new();
        for root in roots {
            let canonical = canonicalize(&root)?;
            if !canonical.is_dir() {
                return Err(CommandError::validation(format!(
                    "Approved root is not a directory: {}",
                    canonical.display()
                )));
            }
            canonical_roots.push(canonical);
        }

        if canonical_roots.is_empty() {
            return Err(CommandError::validation(
                "At least one approved root is required",
            ));
        }

        Ok(Self {
            roots: canonical_roots,
        })
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn ensure_existing_file(&self, path: &Path) -> Result<PathBuf, CommandError> {
        require_absolute(path)?;
        let canonical = canonicalize(path)?;
        if !canonical.is_file() {
            return Err(CommandError::validation(format!(
                "Not a file: {}",
                canonical.display()
            )));
        }
        self.ensure_inside(&canonical)?;
        Ok(canonical)
    }

    pub fn ensure_existing_dir(&self, path: &Path) -> Result<PathBuf, CommandError> {
        require_absolute(path)?;
        let canonical = canonicalize(path)?;
        if !canonical.is_dir() {
            return Err(CommandError::validation(format!(
                "Not a directory: {}",
                canonical.display()
            )));
        }
        self.ensure_inside(&canonical)?;
        Ok(canonical)
    }

    /// Allows an output file that does not exist yet, but requires its parent
    /// directory to resolve inside an approved root.
    pub fn ensure_output_file(&self, path: &Path) -> Result<PathBuf, CommandError> {
        require_absolute(path)?;
        let file_name = path.file_name().ok_or_else(|| {
            CommandError::validation(format!("Path has no file name: {}", path.display()))
        })?;

        let parent = path.parent().ok_or_else(|| {
            CommandError::validation(format!("Path has no parent: {}", path.display()))
        })?;
        let parent = canonicalize(parent)?;
        self.ensure_inside(&parent)?;

        Ok(parent.join(file_name))
    }

    fn ensure_inside(&self, canonical: &Path) -> Result<(), CommandError> {
        if self.roots.iter().any(|root| canonical.starts_with(root)) {
            Ok(())
        } else {
            Err(CommandError::forbidden(format!(
                "Path is outside the approved roots: {}",
                canonical.display()
            )))
        }
    }
}

fn require_absolute(path: &Path) -> Result<(), CommandError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(CommandError::validation(format!(
            "Path must be absolute: {}",
            path.display()
        )))
    }
}

fn canonicalize(path: &Path) -> Result<PathBuf, CommandError> {
    dunce::canonicalize(path).map_err(|error| CommandError::from_io(&error))
}

/// Resolved storage locations for a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub root: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config_dir: PathBuf,
    pub portable: bool,
    pub fallback_reason: Option<String>,
}

/// Portable mode is requested by `PROMPT_STUDIO_PORTABLE=1` or a `portable.txt`
/// marker beside the executable.
pub fn portable_requested(exe_dir: &Path) -> bool {
    std::env::var("PROMPT_STUDIO_PORTABLE").ok().as_deref() == Some("1")
        || exe_dir.join("portable.txt").exists()
}

/// Resolves data, cache, and config directories. Portable mode stores everything
/// under `<exe>/data`; if that location is not writable it falls back to the
/// per-user directories and reports why.
pub fn resolve_app_paths(
    exe_dir: &Path,
    per_user_data: &Path,
    per_user_cache: &Path,
    portable: bool,
) -> Result<AppPaths, CommandError> {
    if portable {
        let root = exe_dir.join("data");
        match prepare_writable(&root) {
            Ok(()) => {
                let cache = root.join("cache");
                prepare_writable(&cache).map_err(|reason| {
                    CommandError::io(format!("Cannot prepare portable cache: {reason}"))
                })?;
                return Ok(AppPaths {
                    data_dir: root.clone(),
                    cache_dir: cache,
                    config_dir: root.clone(),
                    root,
                    portable: true,
                    fallback_reason: None,
                });
            }
            Err(reason) => {
                prepare_writable(per_user_data).map_err(|error| {
                    CommandError::io(format!("Cannot prepare data directory: {error}"))
                })?;
                prepare_writable(per_user_cache).map_err(|error| {
                    CommandError::io(format!("Cannot prepare cache directory: {error}"))
                })?;
                return Ok(AppPaths {
                    root: per_user_data.to_path_buf(),
                    data_dir: per_user_data.to_path_buf(),
                    cache_dir: per_user_cache.to_path_buf(),
                    config_dir: per_user_data.to_path_buf(),
                    portable: false,
                    fallback_reason: Some(format!(
                        "Portable folder is not writable ({reason}); using per-user storage"
                    )),
                });
            }
        }
    }

    prepare_writable(per_user_data)
        .map_err(|error| CommandError::io(format!("Cannot prepare data directory: {error}")))?;
    prepare_writable(per_user_cache)
        .map_err(|error| CommandError::io(format!("Cannot prepare cache directory: {error}")))?;

    Ok(AppPaths {
        root: per_user_data.to_path_buf(),
        data_dir: per_user_data.to_path_buf(),
        cache_dir: per_user_cache.to_path_buf(),
        config_dir: per_user_data.to_path_buf(),
        portable: false,
        fallback_reason: None,
    })
}

/// Sum of regular-file sizes below a directory, bounded by depth.
pub fn directory_size(root: &Path) -> u64 {
    walkdir::WalkDir::new(root)
        .max_depth(8)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

fn prepare_writable(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let probe = dir.join(format!(".write-probe-{}", std::process::id()));
    fs::write(&probe, b"ok").map_err(|error| error.to_string())?;
    fs::remove_file(&probe).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod app_path_tests {
    use std::fs;
    use std::path::Path;

    use super::{portable_requested, resolve_app_paths};

    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "prompt-studio-app-paths-{label}-{}-{}",
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

    #[test]
    fn portable_mode_uses_data_beside_the_executable() {
        let temp = TempDir::new("portable");
        let exe_dir = temp.path.join("app");
        fs::create_dir_all(&exe_dir).expect("exe dir");

        let paths = resolve_app_paths(
            &exe_dir,
            &temp.path.join("per-user-data"),
            &temp.path.join("per-user-cache"),
            true,
        )
        .expect("resolve");

        assert!(paths.portable);
        assert_eq!(paths.root, exe_dir.join("data"));
        assert_eq!(paths.cache_dir, exe_dir.join("data").join("cache"));
        assert!(paths.data_dir.is_dir());
        assert!(paths.cache_dir.is_dir());
        assert!(paths.fallback_reason.is_none());
    }

    #[test]
    fn unwritable_portable_location_falls_back_to_per_user() {
        let temp = TempDir::new("fallback");
        let fake_exe_dir = temp.path.join("not-a-directory");
        fs::write(&fake_exe_dir, b"file").expect("create blocking file");

        let per_user_data = temp.path.join("per-user-data");
        let per_user_cache = temp.path.join("per-user-cache");
        let paths = resolve_app_paths(&fake_exe_dir, &per_user_data, &per_user_cache, true)
            .expect("resolve with fallback");

        assert!(!paths.portable);
        assert_eq!(paths.data_dir, per_user_data);
        assert_eq!(paths.cache_dir, per_user_cache);
        assert!(paths.fallback_reason.is_some());
        assert!(paths.data_dir.is_dir());
    }

    #[test]
    fn per_user_mode_uses_supplied_directories() {
        let temp = TempDir::new("per-user");
        let exe_dir = temp.path.join("app");
        fs::create_dir_all(&exe_dir).expect("exe dir");
        let data = temp.path.join("roaming");
        let cache = temp.path.join("local-cache");

        let paths = resolve_app_paths(&exe_dir, &data, &cache, false).expect("resolve per-user");
        assert!(!paths.portable);
        assert_eq!(paths.data_dir, data);
        assert_eq!(paths.cache_dir, cache);
        assert_eq!(paths.config_dir, paths.data_dir);
        assert!(paths.fallback_reason.is_none());
    }

    #[test]
    fn portable_marker_file_enables_portable_mode() {
        let temp = TempDir::new("marker");
        assert!(!portable_requested(&temp.path));
        fs::write(temp.path.join("portable.txt"), b"").expect("write marker");
        assert!(portable_requested(&temp.path));
        assert!(Path::new(&temp.path).join("portable.txt").exists());
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    use super::PathGuard;

    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new(label: &str) -> Self {
            let unique = format!(
                "prompt-studio-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            );
            let root = std::env::temp_dir().join(unique);
            fs::create_dir_all(root.join("approved")).expect("create approved root");
            fs::create_dir_all(root.join("outside")).expect("create outside root");
            fs::write(root.join("approved").join("inside.txt"), "inside").expect("write inside");
            fs::write(root.join("outside").join("outside.txt"), "outside").expect("write outside");
            Self { root }
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn accepts_files_inside_an_approved_root() {
        let tree = TempTree::new("accepts");
        let guard =
            PathGuard::new([tree.root.join("approved")]).expect("build guard for approved root");
        let resolved = guard
            .ensure_existing_file(&tree.root.join("approved").join("inside.txt"))
            .expect("inside file is accepted");
        assert!(resolved.is_file());
    }

    #[test]
    fn rejects_parent_traversal() {
        let tree = TempTree::new("traversal");
        let guard =
            PathGuard::new([tree.root.join("approved")]).expect("build guard for approved root");
        let attack = tree
            .root
            .join("approved")
            .join("..")
            .join("outside")
            .join("outside.txt");
        let error = guard
            .ensure_existing_file(&attack)
            .expect_err("traversal must be rejected");
        assert_eq!(error.code, "forbidden");
    }

    #[test]
    fn rejects_absolute_path_outside_root() {
        let tree = TempTree::new("absolute");
        let guard =
            PathGuard::new([tree.root.join("approved")]).expect("build guard for approved root");
        let error = guard
            .ensure_existing_file(&tree.root.join("outside").join("outside.txt"))
            .expect_err("outside file must be rejected");
        assert_eq!(error.code, "forbidden");
    }

    #[test]
    fn rejects_sibling_with_shared_prefix() {
        let tree = TempTree::new("prefix");
        let sibling = tree.root.join("approved-sibling");
        fs::create_dir_all(&sibling).expect("create sibling");
        fs::write(sibling.join("file.txt"), "nope").expect("write sibling file");

        let guard =
            PathGuard::new([tree.root.join("approved")]).expect("build guard for approved root");
        let error = guard
            .ensure_existing_file(&sibling.join("file.txt"))
            .expect_err("prefix sibling must be rejected");
        assert_eq!(error.code, "forbidden");
    }

    #[test]
    fn rejects_relative_paths() {
        let tree = TempTree::new("relative");
        let guard =
            PathGuard::new([tree.root.join("approved")]).expect("build guard for approved root");
        let error = guard
            .ensure_existing_file(PathBuf::from("inside.txt").as_path())
            .expect_err("relative path must be rejected");
        assert_eq!(error.code, "validation");
    }

    #[test]
    fn allows_new_output_inside_root() {
        let tree = TempTree::new("output");
        let guard =
            PathGuard::new([tree.root.join("approved")]).expect("build guard for approved root");
        let output = guard
            .ensure_output_file(&tree.root.join("approved").join("new.txt"))
            .expect("new output inside root is allowed");
        assert!(output.ends_with("new.txt"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_output_outside_root() {
        let tree = TempTree::new("output-outside");
        let guard =
            PathGuard::new([tree.root.join("approved")]).expect("build guard for approved root");
        let error = guard
            .ensure_output_file(&tree.root.join("outside").join("new.txt"))
            .expect_err("output outside root must be rejected");
        assert_eq!(error.code, "forbidden");
    }

    #[test]
    fn rejects_directory_junction_escape() {
        let tree = TempTree::new("junction");
        let link = tree.root.join("approved").join("escape");
        let target = tree.root.join("outside");

        let status = Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                link.to_str().expect("link path"),
                target.to_str().expect("target path"),
            ])
            .status();

        let Ok(status) = status else {
            return;
        };
        if !status.success() {
            // Junctions can be unavailable in restricted environments.
            return;
        }

        let guard =
            PathGuard::new([tree.root.join("approved")]).expect("build guard for approved root");
        let escaped = link.join("outside.txt");
        let error = guard
            .ensure_existing_file(&escaped)
            .expect_err("junction escape must be rejected");
        assert_eq!(error.code, "forbidden");
    }
}
