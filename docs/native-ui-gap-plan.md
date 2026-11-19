# Native UI gap audit and fix plan

Source of truth: the 13 capability specs in `openspec/specs/`, the user guide in
`docs/user-guide.md`, and `PARITY-CONTRACT.md`. This file records **what the port
built**, **what the specs still require**, and the **order the gaps get fixed**.

## 1. What the original product is for

Prompt Studio turns local media (or a text brief) into target-ready generation
prompts for a local LM Studio model, without a cloud service:

- **Describe** a photo or video frame with a vision model (verbose prose you can
  correct).
- **Compose** that description plus a target preset (krea2, mmh3, qwen2-edit…) into
  a prompt that fits the target's character limit, parameters, and resolutions.
- **Refine** an existing prompt from history.
- **Batch** many assets or a dataset row through the same pipeline.
- Everything is persisted (SQLite + FTS), versioned, exportable, and draggable
  out to Explorer.

Capabilities and their purpose:

| Spec | Purpose |
| --- | --- |
| `desktop-shell` | One-click launch, rail navigation, settings, portable storage, diagnostics, status |
| `interface-system` | One tokenized visual language, focus/disabled/error states, readable errors |
| `media-ingest` | Drag/drop/folder/paste import, dedupe, probe, thumbnails, proxies, frames, trash |
| `media-editing` | Non-destructive crop/rotate/fit/trim/fps/mask with atomic output under `edited/` |
| `prompt-presets` | Preset schema, built-ins, guide-vs-JSON authoring, compile, preflight |
| `prompt-pipeline` | Four modes, guidance precedence, JSON contract, variations, caching, cancel |
| `multi-reference-compose` | Ordered references with weights, describe-then-compose, result-first |
| `model-runtime` | LM Studio discovery, roles, load/unload, keep-warm, budget, preflight |
| `job-orchestration` | Durable queue, pause/resume, retry/backoff, progress, crash recovery |
| `generation-library` | History, FTS search, tags/collections/notes/ratings, versions, compare, backup |
| `dataset-import-export` | Streaming dataset import, mapping, dedupe, batch run, export |
| `drag-out-export` | Prepared payloads, naming template, sidecars, save-as/reveal |
| `native-desktop-ui` | The port contract itself: parity, in-process backend, state continuity |

## 2. What the port actually shipped

Verified by reading `native-ui/src/**` after cut-over:

| Area | State |
| --- | --- |
| 83 command names routed in-process | done (`src-tauri/src/native.rs`, `native-ui/src/backend_native.rs`) |
| Backend domain logic | reused verbatim; no reimplementation |
| Rail shell + 7 sections + state continuity | done |
| Theme tokens + visible text inputs + thumbnails | done |
| Differential corpus | frozen, 12 modules green |
| Compose | flow/actions complete, **UI renders only Brief, free-text Preset, Run guidance, picker, 3 buttons, output box** |
| Compose mount | **bug:** `ComposeAction::Mount` is never dispatched by `ui()`, so preset list, `presets_get` guidance, library sources, and batch lists are never loaded live |
| Media | grid + selection only — **no import buttons, no paste, no remove, no editor entry, no drag-drop intake** |
| Editor | `views/editor.rs` is replay-only, has **no `ui()`** and is unreachable |
| Library | search + favorites + 3 actions; preset is hardcoded `mmh3` for re-run; no filters/tags/versions/compare/export/backup |
| Jobs | list + pause/resume/cancel/retry; no items, progress, details, reorder-down |
| Presets | list + JSON editor + save/import/export; no guide tab, no duplicate/delete, no live preview |
| Runtime | roles + save; no keep-warm policy, load/unload, model cards, instruction overrides |
| Datasets | path textbox + preview/import/run; no file picker, mapping UI, row table, export, profiles |
| Drag out | save-as fallback only, no UI to select payload |
| App shell | no drag-drop import, no top-bar health, no status surface beyond one line |

## 2b. Live audit of the running binary (window-scoped)

Walked every rail section and all four Compose tabs in the running
`prompt-studio-native.exe`. Observed, not inferred:

| Screen | What is actually rendered | Missing versus spec/user guide |
| --- | --- | --- |
| Compose · Describe/Compose/Refine/Batch | Same layout in all four modes: Brief, free-text `Preset` field, Run guidance, one checkbox per asset, Import/Paste, Describe/Compose/Cancel, Output box | No mode-specific UI: no descriptions list, no weights, no text-only, no refine source/instruction, no batch queue, no advanced options, no result metadata/actions. Preset is a text field, so a typo selects nothing. |
| Media | Heading + thumbnail grid; one selectable name per asset | No Add files/Add folder/Paste, no Remove, no editor entry, no drag-drop intake, no metadata (fps/duration/missing marker) |
| Library | Search field, "Favorites only", Refresh, `0 generation(s)` | No filters (preset/kind/status/tag/collection/date/sort), no detail pane, no tags/notes/rating, no versions/diff/compare, no trash/restore/purge, no export/import/backup/restore |
| Jobs | Pause, Cancel waiting, Refresh, "Queue active" | No job list rendering with status/progress, no items, no details, no reorder down |
| Presets | Full-height JSON editor that clips the view; buttons unreachable (no scroll) | No preset list/selector, no Guide tab, no live preview, no Import/Export reachable, no built-in protection messaging |
| Runtime | Describer and Composer as **free-text fields**, Keep models warm checkbox, Refresh/Save roles/Load describer/Save policy/Unload all/Save instructions, "36 model(s) reported" | No model list or dropdowns, no endpoint health, no keep-warm policy choice (Both/Describer/Composer/None), no load composer/unload model, no instruction text areas (button exists, editor does not), no vision/context badges |
| Datasets | Source and Profile as free-text fields, Preview/Refresh/Import/Run batch/Export/Delete, `0 dataset(s) loaded`, no table | No file/folder picker, no mapping UI, no dedupe policy, no preview/row table, no media-state column, no export column selection, no profiles list |
| All views | Status bar retains a stale "Imported 1 item(s)" forever | Status is not transient and does not report endpoint health, cache size, or data size |

Additional confirmed defects: `ComposeAction::Mount` was never dispatched, so
the preset list, `presets_get` guidance, library sources and batch lists never
loaded live; Presets has no scroll area, so its own toolbar is unreachable;
Runtime has no way to pick a model although `models_list` returns 36.

## 3. Fix plan (ordered, spec-driven)

### Phase A — Compose workspace (blocking; `prompt-pipeline`, `multi-reference-compose`)

1. Dispatch `Mount` once per session so presets, assets, library sources, and
   preset guidance load through the backend.
2. Replace the free-text preset field with a `ComboBox` fed by `presets_list`.
3. Add per-reference weights (`0.0–2.0`, step 0.05), ordered numbering,
   Select all / Clear, and reference removal.
4. Add the description list: per-asset prose, `Edit description`, `Save
   description`, empty-text rejection, field checkboxes for what is sent.
5. Add guidance block: global + preset + run, effective guidance, conflict
   warnings.
6. Add locked phrases, output language, variations (1–4), seed, cache bypass,
   send-images (only when media is selected).
7. Result card first when a result exists: prompt (editable), negative, params,
   warnings, seed, model, `cached` badge, Copy plain, Copy structured,
   Regenerate, variations switcher, over-limit flag.
8. Refine mode: pick a stored generation, edit existing prompt, instruction.
9. Batch mode: select assets, Select all, queue a job, show job id/progress.
10. Disable Reset when the workspace is already empty; keep explicit busy text.

### Phase B — Media intake and editor (`media-ingest`, `media-editing`)

11. `Add files`, `Add folder`, `Paste image` in the Media toolbar; window-level
    drag-and-drop import.
12. Remove (trash) from the Media card and from the Compose tile, no confirmation.
13. Editor panel reachable from Media: percentage crop edges + 1:1/16:9/9:16
    presets, rotate/flip/fit, trim + fps + loop + remove audio, undo/redo/reset,
    preview (`edits_preview`), apply (`edits_save`), mask export entry.
14. Staged edits survive navigation; applying refreshes Compose tiles (`editId`).

### Phase C — Library and jobs (`generation-library`, `job-orchestration`)

15. Library filters: preset, kind, status, tag, collection, favorite, date, sort.
16. Detail pane: notes, rating, tags, collections, versions, diff, references.
17. Compare 2–4 generations; restore inputs; favorite/trash/restore/purge;
    export (JSON/JSONL/CSV/TXT), import, backup/restore.
18. Re-run using the selected preset, not a hardcoded `mmh3`.
19. Jobs: item list with status/error, progress counts, reorder down, cancel
    waiting per job, retry unfinished only, job details copy.

### Phase D — Presets, runtime, datasets, drag-out

20. Presets: Guide tab compiled by `presets_compile`, live preview, duplicate,
    delete with in-use warning, built-in protection.
21. Runtime: model cards (context/quantization/vision badges), keep-warm policy,
    load/unload/unload-all, role suggestions, instruction overrides.
22. Datasets: native file picker, mapping table, dedupe policy, row table with
    media state, run/resume, column-selecting export, profiles.
23. Drag-out: payload selector, sidecar toggle, naming template, Save as/Reveal.

### Phase E — Verification

24. `cargo test --manifest-path src-tauri/Cargo.toml --lib`
25. `cargo test --manifest-path native-ui/Cargo.toml`
26. `cargo build --manifest-path native-ui/Cargo.toml`, `cargo fmt -- --check`
27. Window-scoped visual pass per section; record results in `PORTING.md`.

## 4. Status

| Phase | Status |
| --- | --- |
| A | done — mount, preset combo, weights, descriptions, guidance/locks/language/variations/seed/cache, result card, refine, batch |
| B | done — Media toolbar (files/folder/paste/refresh), remove, clickable thumbnails, editor panel (crop %, aspect, fit, trim, fps, target, preview/apply), window drag-and-drop |
| C | partial — Library filters/search/sort/trash/actions/notes/export/import/backup/restore, Jobs list + details + reorder; versions/diff/compare/tags still missing |
| D | partial — Presets list/scroll/live preview/duplicate/delete/import/export, Runtime model pickers/load/unload/keep-warm/instructions/model cards, Datasets browse/mapping summary/dedupe/rows/export; Guides tab, profiles list, drag-out payload UI still missing |
| E | running — tests + build green after each step; visual pass after every fix |

## 5. Root causes found while fixing

| Symptom | Cause | Fix |
| --- | --- | --- |
| Compose showed `invalid type: map, expected a sequence` | `run_describe` parsed the single `Description` object as `Vec<Value>` | loop asset ids, accept object or list |
| Media/Compose always had no media | sinks serialized `Result<Vec<_>>` with `serde_json::to_value`, producing `{"Ok": ...}`; the views then saw an empty list | unwrap `Result` before serializing (compose + media sinks) |
| Compose preset could not be chosen | `ComposeAction::Mount` was never dispatched, so `presets_list`/`presets_get`/`library_query` never ran live | dispatch Mount once per session |
| Thumbnails blank on first paint | a texture created in the current frame is uploaded at end of frame; no repaint was requested | `request_repaint()` after drawing a thumbnail |
| Thumbnail click did nothing | only the caption label was clickable | clickable image response selects the asset |
| Presets toolbar unreachable | no scroll area and the editor consumed the viewport | scroll area + bounded editor height |
| Runtime could not pick a model | free-text fields despite `models_list` returning 36 models | model combos, load/unload, keep-warm policy, instruction editors |
| Dataset batch used a hardcoded preset/dedupe | literal `"mmh3"` / `"skip"` | preset combo + duplicate policy + file picker |

## 6. Still missing (next work)

- Library: versions/diff/compare, tags/collections/rating UI, page pagination, trash view polish.
- Presets: Guide tab compiled through `presets_compile`.
- Datasets: mapping editor with per-column role selection, saved profile list, column-selecting export.
- Editor: mask painting canvas, percentage crop drag on the preview, alternate output root setting.
- Drag-out: payload selector, sidecar toggle, naming template, Save as / Reveal buttons.
- Runtime: role suggestions ranked like `runtime/roles.rs` (current heuristics are simpler), per-model unload list.
- Jobs: live progress events (currently refresh on action), item thumbnails.
- Compose: per-description field checkboxes, send-images capability gating, batch dataset tab.

## 7. Wave 3 — descriptions, direct-manipulation editor, prompting guides

### Defects fixed

| Symptom | Cause | Fix |
| --- | --- | --- |
| Descriptions never appeared | the live sink returned `[{...}]` while the flow read a single object, so every description was stored as `""` | `describe_text` accepts object or list; an empty result now reports an error instead of silently storing blank text |
| Description editing reset each frame | `sync_from_vm` rebuilt the list every frame | skipped while an entry is being edited |
| Compose button required a manual Describe first | the button called compose directly | Compose describes first when media is selected and undescribed, like the keyboard shortcut |
| Window (and its right column) off-screen | eframe restored a viewport size captured at another DPI scale | fixed initial viewport `1150×540`; stored geometry is no longer replayed |
| Result action row clipped | unwrapped button row in a 50% column | horizontal wrapping |

### Direct-manipulation crop and mask (ComfyUI-style)

`views/editor.rs` gained a canvas above the numeric controls:

- **Crop**: drag a rectangle over the source image; it converts to percentage
  edges through the existing `crop_from_edges` math and pushes a `RectCrop`
  history entry. The sliders keep working and stay in sync.
- **Mask brush / eraser**: paint directly on the image. The strokes rasterise
  into a one-byte-per-pixel mask at canvas resolution (max 640 px wide), shown as
  a white overlay; brush size, feather and invert are live. `Export mask` sends
  the raster to `edits_export_mask` with the derived target size and polarity,
  then attaches the resulting path to the edit stack.
- Unit tests cover brush paint, eraser clear, bounds safety, crop percentage
  mapping, and the mask byte-count contract.

### Prompting guides (verified through the `context7` MCP)

Built-in presets updated from the published model guides:

- **mmh3 (MiniMax H3)** — `integrated_multimodal_description` written shot by
  shot (`[Shot 1]`, cuts as `[Shot N] At MM:SS.mmm`), camera motion in natural
  English with direction/amplitude/speed, `overall_soundscape` and
  `non_diegetic_music` fields, no negative prompt (source:
  `minimax-ai/minimax-h3` prompt-writing skill).
- **krea2** — Krea's documented formula `[Subject] + [Action] +
  [Location/context] + [Composition] + [Style]`, natural language, style early
  (source: krea.ai docs).
- **qwen2-edit (Qwen-Image-Edit)** — instruction-style edit that names the
  element, the change, and what must be preserved; the vendor notes results are
  unstable without an explicit rewrite, so the preset enforces one instruction
  and no negative prompt (source: `qwenlm/qwen-image`).

The Presets view now renders a read-only **Prompting guide** block (guidance,
sections, self-check, limits, example) from the active preset document.

## 8. Why the UI was slow, and what changed

| Cause | Effect | Fix |
| --- | --- | --- |
| wgpu adapter selector explicitly preferred `DeviceType::Cpu` (WARP) | every frame rasterised on the CPU, so the whole window dragged | DX12-only stays (that was the `nvoglv64.dll` crash), but the adapter is now discrete → integrated → software. `PROMPT_STUDIO_SOFTWARE_GPU=1` restores the old behaviour for driver triage |
| `ctx.request_repaint()` inside thumbnail paint | continuous 60 fps repaint even while idle | repaint only when a texture is actually created/uploaded |
| Thumbnails decoded inside `ui` | full-size photo decode (Lanczos3) froze the window for hundreds of ms per new asset | decoding moved to a worker thread with a result channel; `paint` returns immediately and fills in via `request_repaint_after(50ms)` |
| Mask overlay recounted every frame | O(pixels) scan per frame once a mask existed | cached `painted` counter updated during painting |
| State persisted every frame | JSON encode + storage write per frame | fingerprint-gated persistence |
| Free-running DX12 present | a whole core burned while idle | `SurfaceConfig::LOW_LATENCY` (vsync, latency 1) |
| Debug build had unoptimised `egui`/`image` | sluggish input even on a GPU | `[profile.dev] opt-level = 1`, deps at 3; `--release` recommended for normal use |
| Console subsystem binary | a console window flashed on every launch | `#![windows_subsystem = "windows"]` |

Measured after the changes: idle CPU ≈ 8 % of one core (was spinning at 100 %),
no decode stall when the Media/Compose grids first paint, and no console window
on launch.

## 9. Pending items

### Correctness / verification
1. Interactive drag verification of the editor canvas (crop rectangle, mask
   brush/eraser, export) in a real window — the maths and grid contract are
   unit-tested (212 tests), the pointer wiring is not yet visually confirmed.
2. Release-build smoke run (`cargo build --release` + window pass); the debug
   profile is now only lightly optimised.

### Blocking-free work in the UI thread
3. Prompt generation (`pipeline_describe`, `pipeline_compose`, batch) still runs
   synchronously on the UI thread, so the window freezes while LM Studio works
   and **Cancel cannot be clicked during a run**. Needs a worker + polling loop
   in the live shell (the differential flow stays synchronous).
4. Token streaming (`pipeline://token`) is discarded in the native shell; the
   output appears only when the request completes.

### Feature gaps (spec-backed)
5. Library: versions/diff/compare, tag + collection + rating edits, pagination,
   trash bucket polish, bulk favourite/tag/re-run.
6. Presets: Guide-tab authoring compiled through `presets_compile`, JSON
   validation messaging, in-use delete confirmation copy.
7. Datasets: per-column mapping editor, saved profile list, dedupe preview,
   column-selecting export, row-media thumbnails.
8. Editor: video frame preview (currently the poster thumbnail), crop-rectangle
   snapping to the aspect presets, alternate output-root setting, feather
   preview before export.
9. Drag-out: payload selector (prompt/JSON/media/mask/bundle), sidecar toggle,
   naming template, Save as / Reveal buttons.
10. Jobs: live progress events instead of refresh-on-action, item thumbnails,
    per-item retry.
11. Runtime: role suggestions matching `runtime/roles.rs` ranking, per-model
    unload list, capability preflight warnings surfaced in Compose,
    keep-warm badge per model card.
12. Compose: per-description field checkboxes, send-images capability gating,
    dataset batch tab, status text for cached vs fresh runs.

### Known limitations to keep documented
13. Single-instance foreground focus is best effort (loopback signal).
14. Window geometry is fixed at startup (stored geometry is written but not
    replayed) to avoid off-screen restores at other DPI scales.
15. Idle CPU is ≈8 % of a core due to periodic repaint; a fully event-driven
    pass would drop it to near zero.
