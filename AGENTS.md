# AGENTS.md

> **Port status (2026-11-19):** the renderer was ported to a native Rust UI under
> OpenSpec change `rust-native-ui-port`. `native-ui/` is the only launcher; the
> React/Vite frontend (`src/`, `index.html`, Node tooling) has been removed.
> `src-tauri` is now a backend library (its Tauri webview binary target is
> disabled). All 83 command names are wired in-process through
> `native-ui/src/backend_native.rs` and `src-tauri/src/native.rs`. Behaviour
> evidence is in `PARITY-CONTRACT.md`, `PHASES.md` and `PORTING.md`.
>
> The architecture notes below describe the **original v1 webview app** and are
> retained as historical context. Where they mention `src/`, npm gates, or the
> Tauri shell, read them as superseded by the native build commands in
> `README.md`. The pre-cut-over frontend backup is in `%TEMP%`.

## Repository state

- Native Windows desktop app: Tauri 2 (Rust) + React + TypeScript + Vite. The v1
  planning cycle is complete: every change is archived under
  `openspec/changes/archive/`, and the live contract is the 12 capability specs in
  `openspec/specs/`.
- Git repository, single `main` branch tracking `origin`
  (`https://github.com/codenameM12/PromptGenerator`). No CI. The first commit is
  the squashed import of the whole project; local history starts there.
- `src-tauri/binaries/*.exe` (bundled ffmpeg/ffprobe) are tracked with Git LFS
  via `.gitattributes` because each is just over GitHub's 100 MiB file limit. Run
  `git lfs install` before a fresh clone, and remember the repo consumes LFS
  storage/bandwidth quota. Never strip the LFS filter from `.gitattributes`
  without re-adding those files a different way.
- Builds: `npm run tauri build -- --bundles nsis` writes the installer to `src-tauri/target/release/bundle/nsis/`; `npm run package:portable` writes `dist-portable/`.
- Avast flags a freshly rebuilt `dist-portable\Prompt Studio\Prompt Studio.exe` as `IDP.HELU.PSD11` and blocks recreating that exact filename. A renamed copy was also quarantined. The working launcher is the desktop `Prompt Studio App` shortcut pointing to `src-tauri\target\release\prompt-studio.exe`, with `portable.txt` beside it and a `data` junction to `dist-portable\Prompt Studio\data`. Add an Avast folder exception before restoring the portable copy.
- Last recorded gates: 164 Vitest tests, 241 passing Rust tests plus 2 ignored tests (one real-model, one 10k performance). Docs: `README.md`, `docs/user-guide.md`, `docs/troubleshooting.md`, `docs/release-notes.md`, `docs/drag-out.md`, `docs/ui-system.md`.

## Commands

- Dev: `npm run tauri dev`
- Gates: `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm test`, `npm run audit:tokens`, `npm run lint:rust`, `npm run test:rust`
- Rust format: `cargo fmt --manifest-path src-tauri/Cargo.toml`
- In this environment Rust is not on the inherited PATH; prepend `$env:USERPROFILE\.cargo\bin` for `tauri`/`cargo` commands.
- Close/stop the `prompt-studio` process before rebuilding. A running exe causes `Access is denied` during the rebuild.
- Debug sidecar check writes `%TEMP%\prompt-studio-sidecar-check.txt` at startup.

## Architecture

- `src/styles/tokens.css` owns every color, type, spacing, radius, motion, and control token; `src/styles/base.css` owns the page floor and shared primitives. Feature stylesheets keep layout only, and `npm run audit:tokens` fails on literals, gradients, tracked capitals, and layout-property animation.
- `src/App.tsx` is the rail shell: Compose, Media, Library, Jobs, Presets, Runtime, and Datasets are views; visited views stay mounted and hidden so compose inputs, filters, and editor state survive navigation.
- `src-tauri/src/lib.rs` registers every command in `generate_handler!`; add new commands there too.
- `src-tauri/src/commands/<domain>.rs` holds the DTOs and thin `#[tauri::command]` stubs. Shared types, `CommandError`, `Guidance`, and `Page<T>` live in `commands/mod.rs`.
- `src/lib/ipc/index.ts` is the single flat `commands` map plus domain-grouped `ipc`. `src/lib/ipc/types.ts` mirrors Rust DTOs (camelCase).
- Every command needs a fixture in `src/lib/ipc/mock.ts`; `src/lib/ipc/ipc.test.ts` fails if a command lacks one or routes arguments incorrectly.
- `paths.rs`: `PathGuard` rejects traversal/symlink/junction escapes; `AppPaths` resolves portable vs per-user storage.
- `settings.rs`: validated JSON settings plus a DPAPI-encrypted `token.bin` (user scope). Never write the token into `settings.json`.
- `workspace.rs`: debounced draft persistence (`workspace.json`) for the selected asset and unapplied edits.
- `diagnostics.rs`: redacted zip export; tokens and prompt bodies must never appear unless explicitly opted in.
- `src-tauri/binaries/` holds `ffmpeg`/`ffprobe` with the Tauri target-triple suffix; `FFMPEG-NOTICE.txt` documents licensing.
- CSP is enforced in `tauri.conf.json`; use `devCsp` for Vite HMR additions. Do not widen `capabilities/default.json` to `shell:` or `fs:` permissions — the frontend must not get them.
- `db/mod.rs` owns the SQLite connection (`rusqlite`, bundled), WAL, embedded migrations v1–v6, FTS5, pre-migration backups, and online backup/restore (`VACUUM INTO` + `rusqlite::backup`). DB lives at `<data>/prompt-studio.db`.
- `db/repository.rs` is the repository layer; feature code should call it rather than writing SQL in commands. Note that `rusqlite` has no `u64` support: repository rows use `i64` for durations/tokens and cast at the boundary.
- `builtin_presets.rs` embeds `src-tauri/presets/*.json` with `include_str!`, seeds them at startup, and stores a BLAKE3 content hash. Editing a preset file changes its hash and triggers a reseed on next launch.
- `library_backup` / `library_restore` are wired through the DB layer; restore writes a `.pre-restore.bak` snapshot first.
- `media/ffmpeg.rs` resolves the bundled `ffmpeg`/`ffprobe` by walking the exe directory and `CARGO_MANIFEST_DIR/binaries`. `media/import.rs` runs hash → dedupe → probe → thumbnail → proxy → DB insert, emits `assets://import-progress`, and exposes `ImportControl` for cancellation.
- `media/frames.rs` samples frames (first/middle/last, even, scene-change) into the cache and can build a contact sheet. `media/metadata.rs` reads A1111/ComfyUI PNG chunks and EXIF. `media/protocol.rs` serves `media://` with byte ranges and only from allowed roots (the cache root is canonicalized, so a junction-backed `data` folder still resolves).
- `MediaAccess` must be told about a source folder before `media://` will serve it; imports do this automatically and startup re-allows roots for assets already in the DB. Cache files are always allowed.
- The dialog and clipboard plugins are only used by the renderer (`src/lib/media/import.ts`); their capabilities are `dialog:default` and `clipboard-manager:allow-read-image`.
- Media tests generate fixtures with the bundled ffmpeg; the HEVC proxy test silently skips if `libx265` is unavailable.
- `media/edit.rs` owns geometry/resolution derivation, trim/fps validation, and the BLAKE3 edit cache key. `media/render.rs` builds the ffmpeg filter graph, writes atomically through a `.partial` file, resolves `edited/` collisions, supports an alternate output root, and streams progress/cancellation. `media/mask.rs` resizes/feathers/polarizes masks into grayscale PNGs.
- `commands/edits.rs` exposes `edits_preview`, `edits_save`, `edits_cancel`, and `edits_export_mask`; `EditControl` cancels the active render. Outputs go to `<source parent>/edited/` (or `<alt root>/edited/`), and cache hits are keyed by source hash + ops + target.
- The editor UI lives in `src/features/editor/MediaEditor.tsx`; edit undo/redo lives in `src/lib/media/editHistory.ts`. `Asset` includes `proxyPath` for video previews.
- `runtime/client.rs` is the LM Studio HTTP client (list/load/unload/chat/SSE, OpenAI-compatible inference). Model info includes optional `vision`, `reasoning`, `maxOutput`, and `quantization` parsed only from reported fields; absent means unknown. `runtime/manager.rs` owns keep-warm policy, explicit load/unload, describer/composer semaphores, TTL eviction, and busy guards. The manager reconciles with LM Studio's loaded instances before loading, and unload removes every instance of a model key. `runtime/roles.rs` ranks describer suggestions by known vision capability and composer suggestions by context. `runtime/lms_cli.rs` wraps `lms` for partial GPU offload and lifecycle fallback. Behind Tauri this is managed as `Arc<ModelManager>`.
- `runtime/budget.rs` trims low-priority prompt sections. `runtime/roles.rs` produces closest-model suggestions. New error categories: `modelMissing`, `contextOverflow`, `invalidResponse`.
- The runtime panel is `src/features/settings/RuntimePanel.tsx`; it calls `models_list`, `models_status`, `models_load`, `models_unload`, `models_unload_all`, `models_set_roles`, and `models_set_policy`.
- `preset.rs` owns the preset schema/validation/compilation/preflight; `commands/presets.rs` exposes CRUD, import/export, duplicate, and `presets_compile` for unsaved editor previews. Built-ins live in `src-tauri/presets/*.json` and are documented in `docs/presets.md`. The editor offers a text-first Guide tab; `src/features/presets/presetTemplate.ts` compiles name/kind/description/guide/limit into the same validated document, and the JSON tab stays authoritative for parameters, resolutions, references, and output schemas.
- `pipeline.rs` is the two-stage describe/compose service. It caches in SQLite, repairs/retries JSON, finalizes params/limits/negative/locked/checklist rules, records generations (including cancelled/invalid), supports variations/seeds, and batches all descriptions before compositions. `commands/pipeline.rs` owns `PipelineControl` for cancellation and streams tokens on `pipeline://token`.
- The prompt workspace is `src/features/compose/ComposePanel.tsx`: four modes (Describe, Compose, Refine, Batch), media/text-only inputs, guidance conflict display, locked phrases, language/variations/seed/cache controls, editable descriptions, result copy/regenerate/edit, and batch queue handoff. It is the default main-window surface. Compose and Describe render a media picker beside the form (asset grid, ordered multi-select, per-reference weights, `Add media`, `Edit media` via `onEditAsset`); `App.tsx` owns import/drop and bumps `mediaVersion` so the picker refreshes, and a drop while Compose is active stays in place instead of switching views. Describe runs for every selected asset missing a description; Compose waits for all of them. Each description has field checkboxes and only enabled fields are sent. When a result exists it renders before descriptions and the media picker. The panel also preflights model capabilities and owns the `Ctrl+Enter`, `Ctrl+Shift+Enter`, and `Esc` shortcuts while `active` (the Compose view is visible). Its header **Reset** clears the result, descriptions, brief, refine inputs, locked phrases, run guidance, media selection, and batch selection, leaving the preset, global guidance, library, and history untouched.
- `ComposeRequest` carries `assetId`/`editId`/`outputLanguage` plus an ordered `references: Vec<ComposeReference>` (`assetId`, `editId`, `weight`, `name`, `description`, all serde-defaulted). The primary `assetId`/`description` fields remain the first reference for backwards compatibility. `pipeline::normalize_references` defaults weights to 1.0, clamps to 0.0–2.0, warns above four references, feeds the cache key, appends a `References:` block to the prompt, and mirrors the list in `params.references`; multi-description input is trimmed through `runtime/budget.rs`. `pipeline_compose` returns `Generation[]` (one per variation) and `pipeline_batch` returns `Generation[][]`; batch items accept the same reference list.
- `job_queue.rs` is the durable queue: `jobs`/`job_items` rows, state-machine validation, content-hash dedupe, priority reorder, retry with backoff, cancellation flags, crash recovery, and one background runner. Its `TauriExecutor` runs `compose` items through `Pipeline`; `commands/jobs.rs` exposes list/get/status/enqueue/pause/resume/retry/cancel/cancel-waiting/reorder, resolves each item's ordered reference thumbnails/weights from its payload, and emits `jobs://progress` / `jobs://changed`. The panel is `src/features/jobs/JobPanel.tsx`.
- `db/repository.rs` jobs module owns all job SQL; `mark_interrupted` runs at startup before `JobQueue::start`. The queue uses `Mutex<Database>` only for short synchronous blocks, never across `.await`.
- `commands/library.rs` owns history queries, favorites, tags/collections, notes/ratings, versions/diffs, restore-to-workspace, trash/purge, export/import (JSON/JSONL/CSV/TXT), and re-run. `library_compare` returns 2–4 full details (with resolved reference thumbnails) and `library_backup`/`library_restore` use a `.zip` bundle (manifest, database, settings, token, workspace) while still accepting a legacy `.db`. `generations::query` builds filtered paginated SQL over FTS; `generations::version_tree` walks ancestors then descendants. Assets have `deleted_at` (migration 4) for trash. The UI is `src/features/library/LibraryPanel.tsx`.
- `dataset.rs` owns streaming dataset parsing (TXT/JSON/JSONL/CSV/TSV/image folders), encoding fallback, delimiter detection, auto-mapping, media-path safety, dedupe keys, and CSV/JSON/JSONL export. `commands/datasets.rs` adds preview/import/rows/run/export/delete/profiles; rows store a serialized `MappedRow` plus `media_asset_ids_json` (migration 6), runs import row media lazily and compose with ordered references, and `dataset_row_id` on `ComposeRequest` lets the job executor update row status/results. Media paths accept `|` separators and `@weight` suffixes. Import profiles live in `import_profiles` (migration 5). The UI is `src/features/datasets/DatasetPanel.tsx`.
- `drag_out.rs` materializes prompt/JSON/media/mask/bundle payloads under `<cache>/drag/<session>/`, expands the naming template, sanitizes Windows names, resolves collisions, copies with progress/cancellation, and cleans sessions older than 24 h. `commands/drag.rs` exposes `drag_prepare`, `drag_cancel`, atomic `drag_save`, and `drag_reveal`. Native drag uses `tauri-plugin-drag` `2.1.1` via `plugin:drag|start_drag` (capability `drag:default`); `src/lib/drag.ts` starts it, `docs/drag-out.md` records the spike, fallbacks, and the passed interactive Explorer drop.

## Conventions

- New IPC command checklist: Rust DTO + command, register it, mirror the TS type, add to `commands`/`ipc`, add fixture and sample args in `mock.ts`, keep the contract test green.
- Treat all renderer-supplied paths as untrusted; route them through `PathGuard` (or an explicit approved output root) before touching the filesystem.
- Settings and store updates validate first, then persist atomically, then commit in-memory state.
- Tests: Vitest + Testing Library for the renderer, in-module `#[cfg(test)]` for Rust, `wiremock` for LM Studio HTTP. The junction escape test skips silently when junctions are unavailable.
- Keep task checkboxes in `tasks.md` as `- [ ]` / `- [x]`; the OpenSpec CLI parses them literally.
- Behavior changes go through `/opsx-update` first; do not encode new requirements only in code.

## OpenSpec workflow

- Read `proposal.md` → `design.md` → the relevant `specs/<capability>/spec.md` → `tasks.md` before changing behavior; archived changes keep their history under `openspec/changes/archive/`.
- Verify with `openspec validate --specs --strict` and `openspec list` (no active changes should remain after archiving).
- `design.md` open questions (per-target limits, multi-image support, `lms` on PATH) are deferrable; keep the graceful fallbacks.
