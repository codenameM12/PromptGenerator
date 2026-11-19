# Proposal

## Why

The v1 workbench is functionally complete, but four documented gaps stop it from
being finished in daily use: dataset rows that carry media still compose
text-only; the runtime relies on manual roles and ignores the capabilities and
context length LM Studio reports; the library can only diff two versions of the
same lineage; and the release checklist (drag-out, large-library performance,
per-target preset limits, portable packaging) was never verified on the target
machine. Closing these now makes the tool dependable for batch work and removes
the last known limitations in `docs/release-notes.md`.

## What Changes

- **Dataset media composition**: resolve and validate each row's media path,
  import it lazily with hash deduplication, compose with it as a reference,
  isolate failures per row, resume only unfinished rows, expose media status in
  the preview, and export media asset and generation identifiers. Rows may list
  several media paths (stretch).
- **Runtime capability detection**: read vision, reasoning, context length, and
  maximum output from LM Studio where reported; show capability badges; suggest
  describer and composer roles by capability; warn before a run when the
  describer is not vision-capable or the composer budget is below the preset's
  input; use the loaded model's reported context for budgeting; include a
  capability snapshot in diagnostics. No automatic load, unload, or role change.
- **Library comparison**: select two to four generations, including unrelated
  ones, and compare prompts side by side with a diff, metadata, reference
  thumbnails, and per-column copy. Re-running selected generations preserves
  their ordered references and weights.
- **Complete backup**: a backup bundle that includes the database, settings,
  encrypted token, and workspace draft, with restore accepting both the bundle
  and legacy database backups; the existing pre-restore snapshot is kept.
- **Job visibility**: job details list each item's ordered reference thumbnails
  and weights.
- **Keyboard operation**: compose, describe, and cancel shortcuts with documented
  bindings.
- **Target verification**: Explorer drag-out checklist, 10,000-generation
  performance measurement, confirmed per-target preset limits, and portable
  packaging re-verification.

## Capabilities

### New Capabilities

None. All changes extend existing capabilities.

### Modified Capabilities

- `dataset-import-export`: row media import, media-aware runs, multi-media rows,
  and media identifiers in previews and exports.
- `model-runtime`: capability discovery, capability-aware role suggestions and
  preflight warnings, and capability-based context budgeting and reporting.
- `generation-library`: multi-generation comparison and a complete backup bundle.
- `job-orchestration`: reference thumbnails and weights in job details.
- `desktop-shell`: keyboard shortcuts for the primary actions.

## Impact

- **Rust**: `commands/datasets.rs` and `dataset.rs` (media resolution and run
  payloads), a database migration for per-row media asset ids, `runtime/client.rs`
  and `commands/models.rs` (capability fields), `runtime/roles.rs` (capability
  ranking), `commands/library.rs` and `db/repository.rs` (compare query, backup
  bundle), `diagnostics.rs` (capability snapshot), `job_queue.rs` (reference DTOs).
- **Renderer**: `DatasetPanel`, `RuntimePanel`, `LibraryPanel`, `JobPanel`,
  `App.tsx` shortcuts, IPC types and mock fixtures.
- **Data**: one forward migration; backups remain backward compatible.
- **Docs**: user guide, release notes, presets, troubleshooting, AGENTS.
- **Out of scope**: reference roles/slots, storyboard/multi-shot, image or video
  generation, cloud inference, auto-update, macOS/Linux packaging.
