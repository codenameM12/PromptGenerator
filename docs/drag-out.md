# Drag-out spike

## Result

Native drag-out is available on Windows through
[`tauri-plugin-drag`](https://crates.io/crates/tauri-plugin-drag) `2.1.1`. The
plugin is registered in `src-tauri/src/lib.rs` and the renderer calls its
`plugin:drag|start_drag` command directly because the plugin does not publish a
separate npm guest package.

- The drag uses `mode: "copy"`; source files and stored outputs are never moved,
  renamed, or deleted.
- Files are materialized first under `<cache>/drag/<session>/`, then the native
  drag starts. This keeps the async preparation out of the OLE drag loop.
- The drag icon is a 1x1 transparent PNG data URL.
- The capability allowlist includes `drag:default`.

## Fallbacks

Every draggable item also exposes:

- **Save as** — a native save dialog opens with the template-expanded name and
  copies the prepared file atomically.
- **Reveal** — Explorer opens with the prepared file selected.

Both are in the library detail panel (`src/features/library/LibraryPanel.tsx`).

## Verification

The Rust side is covered by tests for template expansion, Windows-illegal
character sanitization, reserved names, collision suffixes, missing sources,
atomic Save-as failures, and stale-session cleanup. The build is validated with
`cargo check`, `cargo clippy`, and `npm run typecheck`. If native drag is
unavailable on a specific Windows/WebView2 combination, the UI reports the
failure and directs the user to **Save as**; no partial file is left behind.

## Retention

Prepared drag sessions live for 24 hours. `drag_out::cleanup` runs on startup and
before each preparation, removing older session folders without touching asset
sources or edit outputs.

## Interactive result

- **Interactive Explorer drop passed on the target machine**: dragging from the
  Library **Drag out** handle into an Explorer window produced the prepared files
  with the expected template-expanded names and byte content. The drag uses copy
  mode, so the session folder and the stored original remain in place.
- Save-as and Reveal remain the documented fallbacks and stay covered by tests.
