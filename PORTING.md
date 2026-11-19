# Porting notes — reviewer companion

Companion to `PARITY-CONTRACT.md`. This is not a second copy of the contract: it
records item/test correspondence, intended divergences, quirks kept intact,
follow-ups, and verification as the port actually happens.

## Current status

- End state: **A — Replacement** (native Rust binary, no bindings).
- Phases 1–5 complete. Cut-over done: React/Vite/Node removed, native binary is
  the only launcher, all 83 command names are wired in-process.
- Backend adapter: 83/83 commands real; 0 `not_implemented` in the dispatcher.
- Differential totals: `editHistory` 25/0, `errors` 25/0, `presetTemplate` 29/0,
  `workspaceDraft` 17/0, `status` 17/0, `crop` 4/0 (partial), `compose` 52/0,
  `media` 21/0, `library` 32/0, `jobs_datasets` 24/0, `presets_runtime` 24/0.
- Frozen corpus retained at `port/corpus/*.json`; source generators deleted at
  cut-over. A pre-cut-over backup of the web frontend is in `%TEMP%`.

## Item correspondence

| Source item                                                                      | Rust item                                              | Phase | Status                                                                                      |
| -------------------------------------------------------------------------------- | ------------------------------------------------------ | ----- | ------------------------------------------------------------------------------------------- |
| `src/lib/media/editHistory.ts`                                                   | `src-tauri/src/ui/edit_history.rs`                     | 3     | done — differential green (25/0)                                                            |
| `src/lib/errors.ts`                                                              | `src-tauri/src/ui/errors.rs`                           | 3     | done — differential green (25/0)                                                            |
| `src/lib/media/crop.ts`                                                          | existing `media/edit.rs` via `edits_preview`           | 3     | partial — 4/0; edges math has no Rust counterpart; odd-centre rounding gap documented       |
| `src/lib/ipc/types.ts`                                                           | existing `src-tauri/src/commands/*` DTOs               | 3     | verified: 83/83 command names; 3 field-level gap reports, 12 types without same-name struct |
| `src/features/presets/presetTemplate.ts`                                         | `src-tauri/src/ui/preset_template.rs`                  | 3     | done — differential green (29/0); output validates through `preset.rs`                      |
| `src/lib/useWorkspaceDraft.ts`                                                   | `src-tauri/src/ui/workspace_draft.rs`                  | 3     | done — differential green (17/0)                                                            |
| `src/lib/useStatus.ts`                                                           | `src-tauri/src/ui/status.rs`                           | 3     | done — differential green (17/0)                                                            |
| `src/lib/media/import.ts`                                                        | `native-ui/src/import.rs` + platform dialogs/clipboard | 3     | done (wiring); interactive dialog/clipboard unverified headlessly                           |
| `src/lib/drag.ts`                                                                | `native-ui/src/platform/drag_out.rs`                   | 3     | done (file-save fallback); OS drag initiation divergent                                     |
| `src/features/compose/ComposePanel.tsx`                                          | `native-ui/src/views/compose.rs`                       | 5     | done — differential green (52/0); real adapter still needed for non-trait commands          |
| `src/features/editor/MediaEditor.tsx`                                            | `native-ui/src/views/editor.rs`                        | 5     | done — differential green (21/0); history reuses `edit_history.rs`                          |
| `src/App.tsx` media grid/import                                                  | `native-ui/src/views/media.rs`                         | 5     | done — import/trash differential green; grid selection retained in ViewModel                |
| `src/features/library/LibraryPanel.tsx`                                          | `native-ui/src/views/library.rs`                       | 5     | partial — differential green (32/0); live backend adapter pending                           |
| `src/features/jobs/JobPanel.tsx`, `src/features/datasets/DatasetPanel.tsx`       | `native-ui/src/views/jobs.rs`, `datasets.rs`           | 5     | partial — differential green (24/0); live adapter pending                                   |
| `src/features/presets/PresetPanel.tsx`, `src/features/settings/RuntimePanel.tsx` | `native-ui/src/views/presets.rs`, `runtime.rs`         | 5     | partial — differential green (24/0); live adapter pending                                   |
| `src/App.tsx`, `src/main.tsx`                                                    | `native-ui/src/app.rs`, `main.rs`                      | 3     | done — eframe shell with rail navigation and state-preserving views                         |

## Test correspondence

| Source test                                     | Characterization                                                                                 | Differential                                                                         | Status                                              |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------ | --------------------------------------------------- |
| `editHistory.test.ts` (4 cases)                 | `src/port/characterize.test.ts` → `port/corpus/edit_history.json`                                | `ui::edit_history` tests                                                             | green: 25 inputs, 0 differences                     |
| `errors.test.ts`                                | `src/port/characterize_errors.test.ts` → `port/corpus/errors.json` (25)                          | `ui::errors::tests::matches_the_recorded_typescript_behaviour`                       | green: 25 inputs, 0 differences                     |
| `presetTemplate.test.ts`                        | `src/port/characterize_preset_template.test.ts` → `port/corpus/preset_template.json` (29)        | `ui::preset_template::tests::matches_the_recorded_typescript_behaviour`              | green: 29 inputs, 0 differences                     |
| `useWorkspaceDraft.test.ts`                     | `src/port/characterize_workspace_draft.test.ts` → `port/corpus/workspace_draft.json` (17 events) | `ui::workspace_draft::tests::matches_the_recorded_typescript_behaviour`              | green: 17 inputs, 0 differences                     |
| `useStatus.ts` (no source unit test)            | `src/port/characterize_status.test.ts` → `port/corpus/status.json` (17 events)                   | `ui::status::tests::matches_the_recorded_typescript_behaviour`                       | green: 17 inputs, 0 differences                     |
| `crop.test.ts`                                  | cases embedded in `src-tauri/src/ui/crop_parity.rs` (no corpus file)                             | `ui::crop_parity::tests::crop_test_cases_are_accepted_by_the_existing_rust_geometry` | green: 4 inputs, 0 differences (partial parity)     |
| `types.ts` + `ipc/index.ts`                     | `src/port/dto_coverage.test.ts` (no corpus file)                                                 | n/a                                                                                  | 83/83 commands, 0 command gaps; field gaps reported |
| `ComposePanel.test.tsx`                         | `src/port/characterize_compose.test.ts` → `port/corpus/compose.json` (9 scenarios / 52 steps)    | `compose_parity::compose_panel_matches_recorded_typescript_behaviour`                | green: 52 inputs, 0 differences                     |
| `MediaEditor.test.tsx` + import.ts              | `src/port/characterize_media.test.ts` → `port/corpus/media.json` (6 scenarios / 21 steps)        | `media_parity::media_and_editor_match_recorded_typescript_behaviour`                 | green: 21 inputs, 0 differences                     |
| `LibraryPanel.test.tsx`                         | `src/port/characterize_library.test.ts` → `port/corpus/library.json`                             | `tests/library_parity.rs`                                                            | green: 32 inputs, 0 differences                     |
| `JobPanel.test.tsx`, `DatasetPanel.test.tsx`    | `src/port/characterize_jobs_datasets.test.ts` → `port/corpus/jobs_datasets.json`                 | `tests/jobs_datasets_parity.rs`                                                      | green: 24 inputs, 0 differences                     |
| `PresetPanel.test.tsx`, `RuntimePanel.test.tsx` | `src/port/characterize_presets_runtime.test.ts` → `port/corpus/presets_runtime.json`             | `tests/presets_runtime_parity.rs`                                                    | green: 24 inputs, 0 differences                     |
| `src/lib/ipc/index.ts` call sites               | `src/port/callsite_coverage.test.ts`                                                             | n/a                                                                                  | 99 call sites, 99 covered, 0 uncovered              |

## Intended divergences

From Agent B (native shell, tasks 3.x / 4.x):

| Source facility                                          | Native replacement                                                                                    | Rationale                                                                                                                                                                |
| -------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `tauri-plugin-drag` (native drag-out)                    | `platform::drag_out` file-save fallback via `rfd`                                                     | eframe/egui has no OS drag-initiation API. A prepared payload under `<cache>/drag/<session>/` is written to a user-chosen path; "start a native drag" is not reproduced. |
| `tauri-plugin-single-instance`                           | `single_instance` loopback lock (`127.0.0.1:49173`, one-byte focus signal) + `ViewportCommand::Focus` | Prevents a second window/database connection deterministically; foregrounding the existing window is best-effort and cannot be verified in a headless session.           |
| `media://` protocol                                      | in-process `AllowedRoots` guard + direct file read                                                    | No webview. Path rules mirror `MediaAccess`: canonicalized cache root plus source roots approved on import.                                                              |
| `tauri-plugin-shell` ffmpeg sidecar                      | `std::process::Command` over a local mirror of `media/ffmpeg.rs` resolution                           | The separate `native-ui/` crate cannot call `src-tauri` modules directly; resolution candidates and ffprobe/thumbnail commands are mirrored and integration-tested.      |
| `tauri-plugin-dialog` / `tauri-plugin-clipboard-manager` | `rfd` dialogs + `arboard` clipboard image read                                                        | Equivalent native integration; requires an interactive desktop session to exercise end-to-end.                                                                           |
| CSS token system (`src/styles/tokens.css`)               | single Rust `theme.rs` + `audit_no_ad_hoc_color_literals`                                             | One declared theme; colours approximated from oklch to sRGB. Layout/styling are acceptable differences under the parity contract.                                        |

From Agent A (view-model ports, tasks 1.x / 2.x):

| Source facility                             | Native replacement              | Rationale                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------------------------------------------- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cropToAspect` ↔ `media/edit.rs::auto_crop` | keep the existing Rust geometry | The 4 `crop.test.ts` cases are accepted unchanged (4/0). On odd centre offsets the implementations differ: TS `Math.round` vs Rust integer division (`1001×800 @ 1` → TS `x=101`, Rust `x=100`; also `801×600`). Rust additionally clamps `width/height` to ≥1. Fixing this would change `media/edit.rs`, which is out of scope for the port; accept as an intended divergence and revisit as a follow-up. |
| `edgesFromCrop` / `cropFromEdges`           | no Rust counterpart             | UI percentage↔pixel math whose output is passed unchanged as `EditOps.crop` to `edits_preview`; not a domain behaviour to port.                                                                                                                                                                                                                                                                            |

Additional gaps recorded from Agent A's report:

- **DTO coverage:** `83/83` command names have a Rust handler, `0` command gaps.
  `106/118` TS interfaces matched a Rust struct/enum; `3` field-level gap reports
  and `12` interfaces without a same-named Rust counterpart: `ImportProgress,
PresetLimits, PresetExample, BatchStarted, EnqueueSpec, EnqueueResult,
ReorderRequest, JobProgressEvent, LibraryImportRequest, DatasetMapping,
RowIssue, MappedRow`. Several are event payloads or exist under different
  names (e.g. `PresetLimits` → `preset::Limits`). Two reported field gaps are
  false positives (`ImportRequest` has two Rust structs sharing the snake name;
  `GenerationDetail` uses `#[serde(flatten)] record`).

Additional gaps recorded from Agent B's report (not yet resolved):

- **Backend adapter is partial.** `src-tauri/src/native.rs` + `native-ui/src/backend_native.rs`
  now call the real repository/media services for `assets_list`, `assets_import`,
  `assets_import_clipboard` and cancel; `app.rs` prefers `NativeBackend` with a
  stub fallback. The `Backend` trait still exposes only 7 methods, so the live
  shell cannot execute the other recorded commands: `library_trash`,
  `jobs_enqueue`/`jobs_get`, `presets_get`, `library_query`, `models_status`,
  `edits_preview`/`save`/`export_mask`/`cancel`, and `datasets.run` return
  "not implemented" in `backend_sink`, and `NativeBackend` itself returns
  "not implemented" for describe/compose/drag-out. Panel parity is proven via
  `RecordingBackend` replay, not live UI→backend calls. **Cut-over is blocked
  until this adapter is completed.**
- Only the active section is drawn; all state is retained in `ViewModel` rather
  than literally rendering hidden widgets (acceptable under the parity contract).
- `rfd`/`arboard` interaction and OS foreground focus need an interactive desktop
  session to verify; they are wired and unit-tested but not exercised end-to-end.

Additional gaps from the panel-port wave:

- **Panel flows for Library/Jobs/Datasets/Presets/Runtime are replayed against a
  `RecordingBackend` test double and differentially green (80 inputs, 0
  differences), but the live views are not connected to backend operations**
  because the `Backend` trait exposes only 7 methods. A domain adapter is
  required before these panels are fully ported.
- **Uncharacterised flows (implemented but not differential-tested):** clipboard
  plain/structured copy (no IPC, `copied` state), Esc-cancel while busy (needs a
  pending promise; only the not-busy no-op guard is recorded), batch dataset tab
  (`datasets.run`), media drag-drop (App-level), and asset-grid selection
  (retained in `ViewModel`, covered by the navigation test). Parity is **not**
  claimed for these.
- `editor.rs` mask export mirrors the test canvas mock (480×~360, one sampled
  pixel); a real canvas payload will differ in `mask`/`maskWidth`/`maskHeight`,
  though the request shape is preserved.
- Compose quirks kept: selection order is reference order; `promptChars`/`seed`/
  `weight` serialize whole JS numbers as integers (1, not 1.0) — the Rust view
  model normalizes typed numbers to match; `resetWorkspace` issues no IPC and
  leaves preset/global guidance/history untouched; `clear` failures are
  swallowed. Editor quirks: reset is an undoable push; undo/redo each trigger a
  fresh `edits_preview`; trim end defaults to `asset.durationMs` when no trim
  exists.

## Quirks kept intact

- `editHistory.push` compares states with `JSON.stringify`, so key insertion
  order affects duplicate detection. The Rust port must compare serialized DTOs
  in the same field order (serde derives preserve declaration order), not via a
  semantic equality that would also treat reordered keys as equal.
- `editHistory.push` keeps at most 100 entries via `slice(-100)`; an undo beyond
  100 pushes is impossible. Keep the cap.
- `reset` is implemented as `push(DEFAULT_EDIT_OPS)`, so it is undoable.
- `errorText`: an empty Error message wins over every fallback; whitespace-only
  strings are JSON-serialized (`"   "` → `"\"   \""`); `{}`/`null` map to
  "Unknown error"; only one level of nested `data` is read.
- `presetTemplate`: destructuring defaults in `nextVersion` apply only to absent
  components (a present non-numeric part becomes 0); `slugify` caps at 48 chars
  and re-trims trailing dashes; `promptChars <= 0` falls back to the base limit;
  changing kind resets `params` to `{}`.
- `useWorkspaceDraft`: a load error leaves the old draft; save is optimistic and
  only a successful save clears the error; `clear` has no catch, so a rejected
  clear leaves state unchanged.
- `useStatus`: a failed poll preserves the previous snapshot; non-Error causes
  use JS `String()` (`"[object Object]"`).

## Follow-ups (do **not** ship until parity is proven)

- **Pre-existing, not caused by the port:** `cargo clippy ... -D warnings`
  fails on three findings in untouched files — `job_queue.rs:1229`
  (`await_holding_lock`), `runtime/lms_cli.rs:130` and `:134`
  (`cloned_ref_to_slice_refs`). They are left alone here because a port that also
  fixes things cannot be validated. With those two lints allowed, the ported code
  produces no new warnings. Fix in a separate change after parity is proven.
- The characterization generator uses `src/port/node-shims.d.ts` (and Agent A's
  `port-fs-shims.d.ts`) rather than a new `@types/node` dependency, to keep the
  source project's dependency surface unchanged. Temporary scaffolding deleted
  with `src/` at cut-over.
- **Real backend adapter (cut-over blocker):** extend the 7-method `Backend`
  trait and complete `NativeBackend` so the live views execute the full command
  surface (library/jobs/datasets/presets/runtime/edits/models/datasets.run and
  describe/compose/drag-out). Section 5 differential parity is proven via
  `RecordingBackend`, but cut-over must not proceed until the shipped binary can
  actually perform those operations.
- **DTO gaps:** TS `ComposeRequest` lacks `dataset_row_id`, which Rust
  `commands::pipeline::ComposeRequest` has; mirror it in the TS source or accept
  it as a source-side gap at cut-over. Review the 12 interfaces without a
  same-named Rust struct and the two false-positive field reports before
  treating the DTO surface as complete.
- **Crop rounding:** the odd-centre `Math.round` vs integer-division divergence
  is accepted as an intended divergence (documented above); revisit in a
  separate change if it surfaces in practice.
- **Residual Tauri type layer:** `src-tauri` still depends on the `tauri` crate
  because the legacy `#[tauri::command]` wrappers remain (they are unused by the
  native binary). The webview *binary* and `tauri-build` script were removed and
  `cargo build --lib` is green. Removing the wrappers and the crate dependency is
  a separate mechanical refactor; do not attempt it during the port.
- **Uncharacterised flows** from the panel wave remain as listed above (editor
  view route, library compare/versions/collections/archive, jobs_get, presets
  preview, single-model unload). They are implemented in replay but not all have
  a dedicated egui control.

## Verification

Run in this order and record actual output here:

1. `npm test` — source characterization and existing Vitest suite.
2. `npm run typecheck`, `npm run lint`, `npm run audit:tokens`.
3. `cargo test --manifest-path src-tauri/Cargo.toml` — differential tests.
4. `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`.
5. `cargo fmt --check` on port-created files only.

Latest run (after both agents; re-verified by the main agent, not just reported):

| Gate                                                                                 | Result                                                                 |
| ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------- |
| `npm test`                                                                           | 27 files, 178 tests passed (was 164)                                   |
| `npm run typecheck`                                                                  | clean                                                                  |
| `npm run lint`                                                                       | clean                                                                  |
| `npm run format:check`                                                               | clean                                                                  |
| `npm run audit:tokens`                                                               | clean (10 stylesheets, 59 tokens)                                      |
| `cargo test --manifest-path src-tauri/Cargo.toml`                                    | 271 passed, 0 failed, 2 ignored (was 241 + 2)                          |
| differential: `ui::edit_history`                                                     | 25 inputs compared, 0 differences                                      |
| differential: `ui::errors`                                                           | 25 inputs compared, 0 differences                                      |
| differential: `ui::preset_template`                                                  | 29 inputs compared, 0 differences                                      |
| differential: `ui::workspace_draft`                                                  | 17 inputs compared, 0 differences                                      |
| differential: `ui::status`                                                           | 17 inputs compared, 0 differences                                      |
| differential: `ui::crop_parity`                                                      | 4 inputs compared, 0 differences (partial parity)                      |
| `cargo clippy --all-targets -- -D warnings`                                          | fails: 3 pre-existing findings (see Follow-ups); none from ported code |
| `cargo clippy ... -A clippy::await_holding_lock -A clippy::cloned_ref_to_slice_refs` | clean                                                                  |
| `rustfmt --check` on port-created files                                              | clean                                                                  |

Panel-port wave (re-verified by the main agent with the full suite):

| Gate                                              | Result                                                                  |
| ------------------------------------------------- | ----------------------------------------------------------------------- |
| `cargo test --manifest-path native-ui/Cargo.toml` | 6 binaries green: 37 + 35 + 24 + 24 + 35 + 24 = **179 tests, 0 failed** (final) |
| differential: `compose_parity`                    | 52 inputs compared, 0 differences                                       |
| differential: `media_parity`                      | 21 inputs compared, 0 differences                                       |
| differential: `library_parity`                    | 32 inputs compared, 0 differences                                       |
| differential: `jobs_datasets_parity`              | 24 inputs compared, 0 differences                                       |
| differential: `presets_runtime_parity`            | 24 inputs compared, 0 differences                                       |
| callsite coverage                                 | 99 call sites, 99 covered, 0 uncovered                                  |

Security review of both panel waves: clean — no `unsafe`, process spawning,
network clients, secrets, or credential writes in `native-ui/src/views/` or
`native-ui/tests/`. `app.rs` now routes all seven sections to the ported views.

Security review of Agent A's output: clean. No `unsafe`, process spawning,
network clients, secrets, or credential writes in `src-tauri/src/ui/`; the only
URL is a loopback LM Studio test fixture (`127.0.0.1:1234`). No edits outside its
declared ownership.

Agent B (native shell) verification, reported after tasks 3.x / 4.x:

| Gate                                                          | Result                                      |
| ------------------------------------------------------------- | ------------------------------------------- |
| `cargo build --manifest-path native-ui/Cargo.toml`            | exit 0; `prompt-studio-native.exe` produced |
| `cargo test --manifest-path native-ui/Cargo.toml`             | 37 passed, 0 failed                         |
| `rustfmt --check --edition 2024` on all 21 native `.rs` files | clean                                       |
| theme audit (`audit_no_ad_hoc_color_literals`)                | passes                                      |

Security review of Agent B's output: clean. Only `unsafe` is a confined Windows
`SystemParametersInfoW` call for reduced-motion detection; the single-instance
listener binds loopback only (`127.0.0.1:49173`) with a one-byte focus signal; no
secrets, hardcoded credentials, or outbound endpoints. No edits outside
`native-ui/**` and the additive `README.md` section.

What could not run cleanly: the unmodified `lint:rust` gate, for the pre-existing
reasons above. Interactive dialog/clipboard flows and OS foreground focus remain
unverified headlessly (documented above).

## Post-cut-over runtime bug fixes (2026-11-19)

Window-scoped UI testing of the shipped native binary found and fixed these
implementation defects. They are fixes against the archived parity contract,
not new requirements, so no new OpenSpec change was opened.

- `NativeState::open` now seeds built-in presets; before this, Presets showed
  `Preset not found` and Compose could reference a missing default preset.
- `library_query` and `settings_update` now read their Tauri named arguments
  (`query`, `patch`). Before this, Library showed
  `Invalid command arguments: invalid type: null, expected struct GenerationQuery`
  and Runtime's Save instructions failed.
- Native queue pause state is owned by `NativeState` and reported by
  `jobs_status`; Pause/Resume now changes the visible queue state.
- Presets selects the first available preset instead of a hardcoded `mmh3`,
  populates the JSON editor from the loaded document, saves from the edited
  JSON, and uses a temp exchange path that matches Import/Export.
- Datasets no longer calls `datasets_rows` with fabricated `ds-1` when empty;
  actions on an empty dataset list report `No dataset loaded`.
- Compose import refreshes `vm.assets`; the stale "Imported N item(s)" message
  after a successful import is gone.
- Text inputs are visible when empty: egui 0.37 `text_edit_bg_color` is set from
  the theme, widget strokes have non-zero width, and every input uses
  `Theme::text_edit_frame()`. Multiline editors are size-bounded.
- Imported media thumbnails are decoded once into cached `TextureHandle`s and
  rendered in both the Media grid and Compose reference picker.
- eframe/wgpu is restricted to DX12 with a software-adapter preference. The
  previous GL-capable default loaded `nvoglv64.dll` and crashed with
  `0xc0000005` on NVIDIA driver shutdown.

UI design work followed the new project skill at
`.agents/skills/egui-ui-design/SKILL.md`, sourced from `native-ui/src/theme.rs`
and egui 0.37 API docs.

Verification after fixes: backend library tests 272 passed / 0 failed / 2
ignored; native tests 185 passed / 0 failed. Visual checks were scoped to the
`prompt-studio-native.exe` window through the `desktop` MCP. Stale protected
app processes were left untouched; only windows with a live main window were
closed with `CloseMainWindow()` before rebuilds.

Additional post-cut-over fixes (same verification pass):

- `NativeBackend::run_describe` no longer parses the single `Description`
  object returned by `pipeline_describe` as `Vec<Value>`; that mismatch produced
  the visible `invalid type: map, expected a sequence` error in Compose. It now
  loops all selected asset ids and accepts object or list result shapes.
- Compose always renders a framed Output area; it shows `No output yet` before
  the first run instead of an invisible result label. Compose errors use the
  danger token.

Verification: native tests 186 passed / 0 failed after these fixes; scoped
window capture confirms the Output box is visible and the serde error is gone
on a fresh launch.

## Post-cut-over UI completion pass (2026-11-19, after live audit)

A window-scoped audit of the shipped binary showed the views were far thinner
than the specs; the gap list and fixes are in `docs/native-ui-gap-plan.md`.
Defects fixed in this pass:

- `NativeBackend::run_describe` no longer parses the single `Description` object
  as `Vec<Value>` (this produced `invalid type: map, expected a sequence`).
- The Compose and Media call sinks no longer serialize `Result` values directly.
  `serde_json::to_value(backend.list_assets())` produced `{"Ok": [...]}`, so both
  views saw an empty asset list even though the repository returned rows.
- `ComposeAction::Mount` is dispatched once per session, so presets, preset
  guidance, library sources, and the batch list load from the live backend.
- Thumbnails request a repaint after texture creation (the upload happens at the
  end of the frame) and are clickable in the Media grid.
- Presets gained a scroll area, bounded editor, preset list, live compiled
  preview, duplicate/delete/import/export with native dialogs, and built-in
  protection messaging.
- Runtime gained model dropdowns, load/unload per role, keep-warm policy
  selection, system-prompt editors, endpoint health, and model cards.
- Datasets gained native file/folder pickers, duplicate policy, preset-driven
  batch runs, row table, and export to a chosen path.
- Library gained preset/status/tag/sort filters, trash view, notes, export,
  import, backup/restore, restore-from-trash, purge, and preset-aware re-run.
- Jobs gained a status list with progress counts, details with per-item errors
  and references, and reorder down.
- Media gained Add files/Add folder/Paste/Refresh, remove, metadata lines, and an
  editor panel (percentage crop, aspect presets, rotate/flip/fit, trim, fps,
  target resolution, preview, apply) wired through the recorded `EditorAction`
  set.
- Window-level drag-and-drop import with a status-bar hint.

Verification: native tests 204 passed / 0 failed (6 binaries); `cargo build`
green; `cargo fmt --check` clean; scoped window captures confirm thumbnails in
Compose and Media, the editor panel, and the asset count in the status bar.

## Post-cut-over UI completion pass 3 (descriptions, editor canvas, guides)

- Descriptions: the live describe sink returns a list while the flow read a
  single object, so every description was stored empty. `describe_text` accepts
  both shapes and an empty response is reported instead of stored.
- Compose's button now describes first when media is selected but undescribed,
  matching the keyboard shortcut.
- Editor canvas: drag-to-crop and a mask brush/eraser paint directly on the
  source image; export sends the raster to `edits_export_mask`. Covered by unit
  tests (212 native tests pass).
- Window geometry is fixed at startup instead of replaying a stored size, which
  had pushed the right-hand column off-screen at this DPI scale.
- Built-in presets `mmh3`, `krea2`, and `qwen2-edit` now carry the published
  prompting guides (verified with the `context7` MCP) and the Presets view shows
  a read-only Prompting guide block.

Verification: native 212 passed / 0 failed; backend 272 passed / 2 ignored;
`cargo build` green; `cargo fmt --check` clean. Interactive drag verification of
the new canvas is still pending; its math is unit-tested.

## Post-cut-over performance pass (2026-11-19)

The UI felt slow because of implementation choices, not the toolkit:

- The wgpu adapter selector preferred `DeviceType::Cpu` (WARP), so every frame
  was rasterised on the CPU. DX12-only remains (the GL backend loaded
  `nvoglv64.dll` and crashed), but the adapter order is now discrete →
  integrated → software, with `PROMPT_STUDIO_SOFTWARE_GPU=1` as an escape hatch.
- Thumbnail decoding moved to a worker thread; previously a single full-size
  photo decode blocked the frame for hundreds of milliseconds.
- Removed the per-frame `request_repaint()` from thumbnail painting and the
  per-frame mask scan in the editor canvas.
- Persistence is fingerprint-gated instead of encoding/writing every frame.
- `SurfaceConfig::LOW_LATENCY` (vsync + latency 1) stops the free-running DX12
  present that burned a full core while idle.
- `[profile.dev] opt-level = 1` with dependencies at 3; release build advised.
- `#![windows_subsystem = "windows"]` keeps a console window from flashing on
  launch.

Measured: idle CPU ≈ 8 % of one core (previously ~100 %), and the grids paint
without a decode stall.

Verification: native 212 passed / 0 failed; backend 272 passed / 2 ignored;
`cargo build` green; `cargo fmt --check` clean.

Pending work is listed in `docs/native-ui-gap-plan.md` §9 — chiefly: async
pipeline execution so Cancel works mid-run, interactive verification of the
canvas pointer wiring, and the remaining spec-backed panels (Library
versions/compare, Presets guide authoring, Datasets mapping, drag-out payload UI,
Jobs live progress, Runtime role ranking).
