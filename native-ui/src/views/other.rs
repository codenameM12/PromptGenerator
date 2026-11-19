//! Shared native command glue. The panel views now render directly from
//! `app.rs`; this module keeps the one cross-panel named counterpart.

/// Named native counterpart of the source `assets_import_cancel` call site.
/// The import flow itself lives in `import.rs`; cancelling reaches the same
/// typed backend seam (`Backend::call`).
pub fn cancel_import(record: &dyn Fn(&str, serde_json::Value)) {
    record("assets_import_cancel", serde_json::json!({}));
}
