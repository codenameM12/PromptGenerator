# Proposal

## Why

Removing an asset is currently one-way in practice: re-importing the same file hits hash dedupe, which returns the trashed record and leaves it hidden. The Compose media picker also has no remove action, the mask canvas is opaque so users draw blind, and crop requires typing exact pixel values that easily fail validation.

## What Changes

- Re-importing a file whose hash matches a **trashed** asset restores that asset and reports it as restored instead of leaving it hidden.
- The Compose media picker gains **Remove**, with the same trash semantics and confirmation as the Media view.
- The mask canvas renders the source image underneath the strokes, with a transparent mask layer and an eraser that cuts through rather than painting black.
- Crop becomes edge-based: four percentage handles plus aspect presets, with bounds clamped to the source so an edit cannot fail on out-of-range pixels. Pixel entry is removed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `media-ingest`: re-import restores trashed assets; removal is available in the Compose picker.
- `media-editing`: mask canvas shows the source; crop uses percentage edges.

## Impact

- Backend: `media/import.rs` (restore on match), `ImportResult` gains `restored`.
- Renderer: `ComposePanel.tsx`, `MediaEditor.tsx` and styles, IPC types and fixture.
- Docs: user guide and release notes.
- No database migration.
