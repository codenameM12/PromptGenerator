# Design

## Error text (`src/lib/errors.ts`)

`errorText(cause)` returns, in order: `Error.message`, `String(cause)` when the
string is a real message, structured `CommandError` fields
(`user_message` → `message`, camelCase for TS), nested `data.message`, then
`JSON.stringify(cause)`. One helper, no per-panel variants; all catch sites swap
`cause instanceof Error ? cause.message : String(cause)` for `errorText(cause)`
via a codemod-style global replace, with an AST-light regex over source files.

## Crop drag-to-draw

The existing preview `<img>` sits inside `.editor__preview` together with the
crop overlay. Handlers:

- `onPointerDown` (left button, no crop-forbidden states): capture pointer,
  record start point in source coordinates (`%` via bounding rect).
- `onPointerMove` while dragging: update a live `dragRect` state (screen-side
  overlay, independent of history).
- `onPointerUp`: convert the rect to percentages → `cropFromEdges` clamp →
  `update({ crop })` through the standard history entry, mirroring the slider.

The overlay keeps rendering from `ops.crop`; the live drag rect renders only
while dragging, so undo/redo and Reset work unchanged. Video assets also get
the drag surface (crop is a geometric op on the first frame pipeline).

## Picker removal

`removeAsset` drops the `window.confirm` and calls `assets_import`'s removal
path directly (same `library_trash` command already behind the dialog). The
button title keeps stating the trash destination.

## Composer images

- DTO: `ComposeRequest.include_images: Option<bool>` (serde default),
  `ComposeInput { include_images: bool, images: Vec<String> }` — the command
  layer owns resolution; the pipeline only appends parts.
- Resolution helper in `commands/pipeline.rs`: for each reference, resolve edit
  output (`edit_repo`) → else asset source → `still_frame` conversion for
  non-PNG/JPEG → for video, one frame via the existing `frame_sampler`
  (`max_images: 1`). Cap 4 images, encode as `data:image/png;base64`.
- Guard: `manager.status()` composer vision is `Some(false)` → clear error
  ("Composer model ... does not accept images"); `Some(true)`/unknown →
  proceed.
- Cache key: add `"images": blake3 hash of the joined data URLs` so text-only
  and image runs never share a cache entry.
- Message assembly: images append to the final user message after all text
  sections, before the system-defined JSON shape instruction stays untouched.
- Batch/job executor paths do not set images; `BatchRequest` unchanged.
- UI: checkbox in the Compose (and Refine) run options; enabled only when the
  panel's existing preflight sees composer vision === true; label explains the
  requirement. Reset clears the flag; it is not persisted.

## Risks

- Data URLs enlarge requests; capped at 4 images and pinned to PNG/first-frame.
- Regex catch-site replacement may miss an inline unique form — verified by
  ripgrep after the pass and by typecheck; behavior-neutral either way.
- Drag on the preview region must not conflict with the existing overlay div,
  which has `pointer-events: none`.
