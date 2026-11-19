# Proposal

## Why

Two user-reported gaps from using crop and mask in the rebuilt app:

1. Applying an edit can fail with a bare "Access is denied. (os error 5)". The
   message does not name the folder, whether the app or Windows blocked the
   write, or that the output folder can be moved in settings.
2. Applying a crop or mask in the Media editor does not change what describe
   and compose use or show. The Compose media picker still displays the source
   thumbnail and sends `editId: null`, so the user cannot see or trust that the
   cropped, masked version is what reaches the prompts.

## What Changes

- Write-failures and mask/export failures that hit a permission error produce
  an error naming the folder and the way out (alternate or per-run output
  folder, or allowing the app in Windows ransomware protection).
- A new `edits_latest` command returns the newest applied edit per asset.
- The Compose panel loads that map and, for each selected reference, sends the
  latest `editId` to describe and compose, so prompts are generated from the
  applied (cropped/masked) output.
- The Compose media tiles render the latest applied edit output with an
  "edited" badge instead of the raw source thumbnail, and the tiles refresh
  right after the user applies an edit in the Media view.
- Composer image resolution (Send media to composer) prefers the latest
  applied edit output when a reference has no explicit edit.

## Capabilities

### Modified Capabilities

- `media-editing`: permission failures name the folder and the path forward.
- `multi-reference-compose`: describe/compose use the latest applied edit and
  tiles preview it.
- `prompt-pipeline`: composer images derive from the latest applied edit.

## Impact

- Backend: `db/repository.rs` (latest-edit queries), `commands/edits.rs`
  (`edits_latest` + registration), `media/render.rs` and `media/mask.rs` (error
  context), `commands/pipeline.rs` (image resolution preference).
- Renderer: `src/lib/ipc` (type/fixtures), `ComposePanel.tsx` (edit map, tile
  previews, `editId` in requests), `App.tsx` (refresh bump after apply).
- No database migration.
