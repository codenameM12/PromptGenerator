//! Import flow: native dialogs/clipboard → typed backend → allowed-roots guard.
//! Produces assets with the same metadata shape as the source import path.

use std::path::Path;

use crate::backend::{Backend, ImportResult};
use crate::media_access::AllowedRoots;
use crate::platform::clipboard::ClipboardImage;

/// Imports disk paths and approves each source folder for in-process display.
pub fn import_paths(
    backend: &dyn Backend,
    access: &AllowedRoots,
    paths: &[String],
) -> Result<Vec<ImportResult>, crate::backend::BackendError> {
    let results = backend.import_paths(paths)?;
    for path in paths {
        access.allow_path(Path::new(path));
    }
    Ok(results)
}

/// Imports a clipboard image. The PNG bytes match
/// `src-tauri::commands::assets::ClipboardImportRequest`.
pub fn import_clipboard(
    backend: &dyn Backend,
    image: &ClipboardImage,
) -> Result<Vec<ImportResult>, crate::backend::BackendError> {
    backend.import_clipboard(Some(&image.file_name), &image.bytes)
}

#[cfg(test)]
mod tests {
    use super::{import_clipboard, import_paths};
    use crate::backend::{Backend, StubBackend};
    use crate::media_access::AllowedRoots;
    use crate::platform::clipboard::ClipboardImage;

    #[test]
    fn disk_import_approves_source_and_lists_asset() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("photo.png");
        std::fs::write(&file, b"png").expect("write file");
        let access = AllowedRoots::new(dir.path().join("cache"));
        let backend = StubBackend::default();
        let path = file.to_string_lossy().to_string();

        let results = import_paths(&backend, &access, &[path.clone()]).expect("import");
        assert_eq!(results[0].path, path);
        assert!(results[0].asset_id.is_some());
        assert!(access.is_allowed(&file));
        let assets = backend.list_assets().expect("list");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].display_name, "photo.png");
    }

    #[test]
    fn clipboard_import_uses_png_payload() {
        let backend = StubBackend::default();
        let image = ClipboardImage {
            file_name: "clipboard.png".into(),
            width: 1,
            height: 1,
            bytes: vec![1, 2, 3, 4],
        };
        let results = import_clipboard(&backend, &image).expect("import");
        assert_eq!(results.len(), 1);
        assert!(results[0].path.starts_with("clipboard://"));
    }
}
