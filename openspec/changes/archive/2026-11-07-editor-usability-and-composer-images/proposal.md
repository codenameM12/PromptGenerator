# Proposal

## Why

Three linked usability problems confirmed against the running app:

1. IPC failures render as `[object Object]` because the renderer does `String(cause)`
   on the structured `CommandError` object Tauri rejects with. Users cannot act on
   that text (reported near the editor Apply button for crop and mask use).
2. Deleting an image from the Compose media picker shows the same blocking
   `window.confirm` popup every time even though removal is tash-recoverable.
3. The crop rectangle can only be shaped with percentage sliders; the readout text
   already promises "Drag an edge" but no drag exists. The editor also has no way
   to draw the crop directly on the preview.
4. The composer only ever receives text. Descriptions exist, but the user has no
   option to also send the reference images themselves to the composer model.

## What Changes

- Add one shared error-text helper that unwraps `CommandError` (`userMessage` /
  `message` / nested `data`) and stands in for every
  `cause instanceof Error ? cause.message : String(cause)` call site, so real
  messages show instead of `[object Object]`.
- Remove the delete confirmation popup in the Compose media picker; removal goes
  straight to the Library trash and stays recoverable.
- Add drag-to-draw crop on the editor preview: dragging a rectangle on the source
  image sets percentage-based crop, clamped inside the source, and keeps the
  existing sliders/aspect buttons in sync.
- Add a "Send media to composer" option for Compose/Refine. When enabled, the
  composer request includes the reference images as data URLs; video references
  contribute a sampled first-frame image. Requirement applies only when the
  composer model reports vision; the option is disabled otherwise. Direct
  compose/Refine runs honor it; batch and job queues keep text-only behavior.

## Capabilities

### Modified Capabilities

- `media-editing`: drag-to-draw crop rectangle on the preview.
- `interface-system`: readable IPC error text; no confirm popup for picker removal.
- `prompt-pipeline`: composer requests may include reference images.

## Impact

- Renderer: `src/lib/errors.ts` (new helper), catch sites across features,
  `MediaEditor.tsx` (crop drag), `ComposePanel.tsx` (confirm removal, option UI).
- Backend: `commands/pipeline.rs` (image part resolution), `pipeline.rs`
  (`ComposeInput.images`, message assembly, cache key hash).
- No database migration; no new dependencies.
