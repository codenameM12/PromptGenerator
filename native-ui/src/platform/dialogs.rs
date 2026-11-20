//! Native file/folder dialogs via `rfd` (replacement for `tauri-plugin-dialog`).
//! Thin wrappers so views never depend on the dialog crate directly.

use std::path::PathBuf;

const MEDIA_FILTER: &[&str] = &[
    "png", "jpg", "jpeg", "jfif", "jpe", "webp", "gif", "bmp", "avif", "mp4", "m4v", "webm", "mov",
];

pub fn pick_import_files() -> Option<Vec<PathBuf>> {
    rfd::FileDialog::new()
        .add_filter("Images and video", MEDIA_FILTER)
        .pick_files()
}

pub fn pick_import_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

/// Dataset sources (tabular, JSON, or plain text).
pub fn pick_import_dataset() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Datasets",
            &["csv", "tsv", "json", "jsonl", "ndjson", "txt"],
        )
        .pick_file()
}

/// Preset import payload (`.json`).
pub fn pick_import_json() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Prompt Studio preset", &["json"])
        .pick_file()
}

/// File-save fallback used by the drag-out divergence.
pub fn pick_save_path(default_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_file_name(default_name)
        .save_file()
}
