# Design

See `proposal.md` for motivation and `specs/` for the behavior contract.

## Context

The ten v1 capabilities are specified and implemented. LM Studio already returns
per-model metadata through `/api/v1/models`, the dataset layer already validates
and stores a single `media_path`, the library already supports multi-select,
two-way field diffs, and preset re-runs, and `library_backup` already wraps
SQLite's `VACUUM INTO` with a pre-restore snapshot. This change builds on those
paths instead of adding parallel ones.

Current database schema version is 5; `dataset_rows` has `data_json`, `status`,
`generation_id`, and `error`, but nowhere to persist a resolved asset id.

## Goals / Non-Goals

**Goals**

- Persist row media provenance so reruns do not re-import, and exports carry it.
- Treat LM Studio capability fields as data: unknown when absent, never inferred.
- Compare independent generations, not only lineage versions.
- Make backup one file the user can move, while keeping legacy restores working.
- Keep every new behavior testable through existing Rust and Vitest patterns.

**Non-Goals**

- Reference roles/slots and storyboard (explicitly out of scope).
- Automatic role changes or model lifecycle actions.
- Downloading models, cloud storage, or cross-machine token portability.
- Changing the existing reference-limit decision (soft warning above four).

## Decisions

### D1: Persist row media ids in a new column

Add migration 6: `ALTER TABLE dataset_rows ADD COLUMN media_asset_ids_json TEXT`.
A row may have several media paths, so a JSON array of asset ids is the smallest
representation that keeps order. Import writes the array after resolving assets;
the run payload reads it. Rejected alternative: rewriting `data_json`, which
would mix derived state into the source row and complicate dedupe merges.

`MappedRow.media_path` stays the source of truth for the declared path. Parsing
splits on `|`, and each segment may end with `@<weight>`; weights are clamped to
0.0–2.0 with the same warning codes as compose references. Text-only rows keep
`media_asset_ids_json` null.

### D2: Import media lazily inside the run, before enqueue or during item execution

Resolve and import at `datasets_run` time for rows that have media, but only for
unfinished rows, and reuse assets already recorded. This keeps the job payload
self-contained and gives one progress surface. Import failures mark the row
failed with the reason and do not prevent other rows from queueing.

Alternative considered: import during dataset import. Rejected because import is
network-free but I/O heavy, and users often map a media column before deciding to
run; lazy import keeps preview fast and honours the existing "valid rows while
listing rejected" behavior.

Export adds `mediaAssetId`, `generationId`, and `status` columns after the
original fields, so existing column selections still work.

### D3: Capability fields are optional and carried through the existing model DTO

Extend the model DTO with `vision`, `reasoning`, `maxOutput`, and
`quantization` as options, parsed from whatever LM Studio reports. Absent means
unknown. Do not cache beyond the current list response, matching the existing
"cached metadata is marked stale" behavior. Suggestions rank known
vision-capable models for the describer first, then fall back to the existing
name matcher; applying a suggestion is an explicit user action. Preflight
warnings live in the workspace so they appear where the run starts, and use the
same model list the runtime panel shows.

Budgeting: the pipeline receives the composer model's reported context (already
available from the manager status) and uses `min(reported, configured)`; the
trimming record names which value was used.

### D4: Library compare is a read-only batch detail query

Add one command that returns full generation details for two to four ids,
rejecting other counts. The UI renders aligned columns and reuses the existing
field-diff output for prompt highlighting; no new diff algorithm. Compare is
independent of lineage, so version compare keeps its current two-item behavior.

Reference-aware rerun already forwards `params.references`; this change adds
tests that lock the behavior rather than a new path.

### D5: Backup becomes a zip bundle with a manifest

Bundle format:

```
manifest.json   { format: "prompt-studio-backup", version: 1, createdAt, appVersion }
prompt-studio.db
settings.json   (token excluded)
token.bin       (when present)
workspace.json  (when present)
```

Restore detects a zip by its leading bytes; a raw SQLite file uses the legacy
path. The pre-restore snapshot is written before either path. The manifest never
contains prompt or description text. `settings.json` keeps its existing schema.

Trade-off: DPAPI-encrypted `token.bin` is bound to the Windows user and machine,
so a bundle restored elsewhere cannot decrypt the token. The app treats a failed
token decryption as "no token" and asks the user to re-enter it; the design does
not attempt to export plaintext tokens.

### D6: Job details resolve references through the asset repository

Job item DTOs gain a reference list (asset id, name, weight, thumbnail path,
missing flag). The queue stores the payload as-is; the command enriches it on
read. Missing assets render with a marker rather than hiding the row.

### D7: Shortcuts live at the app shell, not in each panel

One keydown listener in `App.tsx` dispatches to the active workspace action.
`Ctrl+Enter` runs the current mode, `Ctrl+Shift+Enter` describes, `Esc` cancels.
The listener ignores events from open dialogs and from inputs unless the target
is a textarea (where `Ctrl+Enter` is the expected submit gesture).

### D8: Verification is evidence, not new behavior

Drag-out uses the documented manual checklist; the 10k-generation measurement
uses a dev-only seeding script against a temporary database, never the user's
data. Per-target preset limits are researched and written into the built-in
preset JSON with their source recorded in `docs/presets.md`; changed hashes
trigger the existing reseed. Packaging reuses the NSIS and portable flows.

## Risks / Trade-offs

- **Importing media for large datasets is slow** → import only unfinished rows,
  reuse stored ids, and keep the existing cancellation and progress channel.
- **LM Studio capability shapes vary by version** → parse opportunistically,
  treat missing as unknown, and cover reported/absent/malformed fixtures.
- **A backup bundle can be large** → stream the zip entries and reuse the
  existing atomic output pattern; do not load the database into memory.
- **Migration 6 changes row shape** → keep the column nullable and read it with
  `COALESCE`; the pre-migration backup runs first.
- **Shortcuts can conflict with typing** → opt in only for textareas and ignore
  dialogs; document the bindings.

## Migration Plan

1. Ship migration 6 with a nullable column; existing rows read as no stored
   media ids and resolve on their next run.
2. Bundle restore accepts legacy database files indefinitely.
3. Rollback is code-only: the added column is ignored by older builds because
   they never select it; backups remain readable.
4. Rebuild NSIS and portable packages after gates pass; re-run the Avast
   exception procedure from `docs/troubleshooting.md`.

## Open Questions

- Exact per-target limits for mmh3, krea2, krea2-edit, and qwen2-edit depend on
  current vendor documentation. They are data-only and can be corrected without
  a code change; record the source beside each value.
