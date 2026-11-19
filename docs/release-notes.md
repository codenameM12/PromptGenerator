# Release notes — Prompt Studio v1

## Scope

Prompt Studio is a local-first Windows workbench for turning media into prompts
for image/video generation targets. v1 includes:

- media ingest with hashing, dedupe, probing, thumbnails, proxies, frames, and
  embedded metadata parsing
- non-destructive image/video editing with masks, trim, fps, resolution, and the
  `edited/` output layout
- LM Studio runtime management with roles, keep-warm policies, TTL, unload, and
  local-only HTTP
- six built-in presets plus import/export/duplicate/edit
- two-stage describe/compose pipeline with caching, retries, variations, seeds,
  output-language enforcement, and self-check warnings
- durable job queue with pause/resume/cancel/retry/reorder/recovery
- generation library with search, filters, favorites, tags, collections, notes,
  ratings, versions/diffs, restore, trash, backup/restore, and exports
- dataset import wizard, mapping profiles, dedupe, batch runs, and exports
- native drag-out plus Save-as/Reveal fallbacks and sidecars

## Compose, media, and preset ergonomics

- **Reset** in the Compose header clears the result, descriptions, brief, refine
  inputs, locked phrases, per-run guidance, media selection, and batch selection.
  The preset, global guidance, imported library, and stored history are untouched.
- Each Media card exposes **Remove**, which moves the asset to the Library trash,
  drops it from every picker, and leaves the original file on disk. Restore from
  Library → Trash brings it back.
- Presets can be authored as a **text guide** (name, kind, target description,
  prompt guide, optional character limit). The app compiles and validates the
  stored JSON document, so hand-writing JSON is optional; the **JSON** tab stays
  available for parameters, resolutions, references, and output schemas.
- **Model instructions** in the Runtime panel add optional system prompt
  overrides for the describer and composer. They apply to describe, compose,
  batch, and jobs; the pipeline still adds its JSON, length, and language rules.
- **Quick media tools** above the editor jump straight to crop/trim edges, video
  trim, and mask regions, for images and video.
- **Docs wiki**: `npm run docs:wiki` builds a styled offline site in `docs/wiki/`
  with a sidebar, per-page contents, wiki-style backlinks, previous/next, search,
  dark mode, and print styles.
- Re-importing a removed asset restores it instead of leaving it hidden; Compose
  media tiles also expose **Remove**.
- The mask canvas shows the source image underneath, and crop uses percentage
  edge controls with aspect presets instead of pixel entry. Crop can now also be
  drawn as a rectangle directly on the preview, IPC failures render their real
  message instead of `[object Object]`, and picker removal asks nothing because
  the asset still goes to the Library trash.
- WebP and other non-PNG/JPEG stills are converted once to a cached PNG for the
  describer, so they no longer fail at the describe step.
- Compose gained a **Send media to composer** option: reference images join the
  composer request as image parts when the composer reports vision, with image
  runs cached separately from text-only runs.
- Describe, Compose, and the Send-media-to-composer images now follow the
  latest applied edit: picker tiles show the cropped/masked output with an
  **Edited** badge, and edit-write permission failures name the folder plus the
  output-folder and ransomware-protection remedies.
- Descriptions are now verbose prose instead of JSON. The describer is prompted
  for three to six detailed sentences, stores text, flattens legacy structured
  descriptions to readable prose, and the composer receives a **Visual
  description** section. The UI edits the paragraph directly.

## Automated verification

Run from the repository root:

```powershell
npm run typecheck
npm run lint
npm run format:check
npm test
npm run lint:rust
npm run test:rust
```

Last recorded result: **161 Vitest tests** and **238 Rust tests** (236 passed,
two ignored: one needs a live LM Studio and one is the 10k performance
measurement), with `real_models_describe_and_compose` and
`measures_first_page_for_ten_thousand_generations` as the ignored tests. All 12
capability specs pass `openspec validate --specs --strict`, and no active changes
remain.

## Multi-reference compose

- Compose and Describe now accept an ordered multi-selection of images and clips.
  Each selected tile shows its reference number and a weight field (`0`–`2`,
  default `1`). Describe runs per asset and Compose waits for every description.
- Descriptions are verbose prose now. Each description card shows the paragraph
  with a word count and a plain-text editor; the composer receives the text
  directly. Field checkboxes, JSON editing, and confidence are gone.
- `ComposeRequest.references` carries `assetId`, `editId`, `weight`, `name`, and
  text `description` with a serde default, so older payloads and stored jobs still
  work. Weights are clamped, more than four references warn, and the cache key
  includes the ordered reference set.
- The generated prompt appends a `References:` block and generation
  `params.references` mirrors it for Library/JSON consumers.
- When a result exists it renders first in the output column; the compose button
  is full width and the media grid fills the remaining height.

## Runtime fixes

- Dataset rows with media now import it lazily (hash-deduplicated) and compose
  with it as a reference; multi-media rows accept `a.png@0.8|b.png`. Row failures
  are isolated, unfinished rows are retried, and exports include the media asset
  id, generation id, and status.
- Runtime capability detection reads LM Studio's reported vision, reasoning,
  context, maximum output, and quantization. The Runtime panel shows badges and a
  **Suggest roles** action; Compose warns when the describer is text-only or the
  composer context is below the preset budget, and budgets with the smaller
  reported context.
- The Library compares two to four generations side by side with a word-level
  prompt diff, reference thumbnails, and per-column copy. Re-run preserves the
  ordered references and weights.
- Backup is now one `.zip` bundle with the database, settings, token, and
  workspace draft; restore accepts the bundle or a legacy `.db`.
- Job details list ordered references with thumbnails and weights, and the app
  shell provides compose, describe, and cancel shortcuts.
- The composer model reports `reasoning.default = "on"`. A reasoning model can
  spend its whole output budget in `reasoning_content` and return an empty
  `content`, which the pipeline correctly reports as invalid JSON. The LM Studio
  client now sends `reasoning_effort: "none"` on every chat request, so structured
  describe and compose calls produce content instead of a reasoning trace. This is
  covered by `runtime::client::tests::disables_reasoning_for_structured_chat`.
  The actual composer model was also tested directly with `reasoning_effort:
"none"` and a JSON schema; it returned valid JSON with zero reasoning tokens.
- Generated seeds are masked to the non-negative `i64` range. LM Studio rejects
  seeds below `-1`, which previously made both compose attempts return an empty
  response and store `invalid` generations. Covered by
  `pipeline::tests::seeds_are_non_negative_for_lm_studio`.
- The runtime manager now reconciles with LM Studio's loaded instances before
  loading, so a model already loaded by a previous app run or by hand is reused
  instead of loaded a second time. Unload now removes every instance of a model
  key, not just the first. Covered by
  `runtime::manager::tests::reuses_a_model_already_loaded_in_lm_studio` and
  `runtime::client::tests::finds_every_loaded_instance_for_a_key`.
- `media://` now canonicalizes the cache directory before comparing paths, so
  thumbnails and proxies load when the portable `data` folder is reached through
  a junction. Previously the canonical file path was compared against the
  un-resolved cache path and rejected as unapproved. Covered by
  `media::protocol::tests::cache_files_are_allowed_through_a_junction`.
- The Compose form is narrowed to a 480 px input column, short fields are grouped into two- and three-column rows, textareas are shorter, and the primary action
  stays visible while scrolling. The media dropdown is gone: an asset grid now
  fills the space beside the form, with click-to-select, drag-and-drop import
  that keeps you in the workspace, **Add media**, and **Edit media** next to the
  selection.

The build was refreshed and the portable folder and zip were updated without
touching the existing `data` folder.

## UI redesign verification

The `redesign-prompt-studio-ui` change replaces the top-bar toggles and the
per-panel palettes with the rail shell and the token-first interface system.

- `npm run audit:tokens`: clean, 10 stylesheets, 59 tokens.
- Hallmark `sloplint src/App.css --scope app --genre modern-minimal`: 0 FAIL,
  0 REFLEX, 0 WARN. The run follows the local `tokens.css` and `base.css`
  imports.
- `npm run format:check`, `npm run typecheck`, `npm run lint`, `npm test`,
  `npm run lint:rust`, and `npm run test:rust` all pass.
- 28 screenshots in `docs/ui-screens/` cover all seven views at 1100×700,
  1280×800, 1440×900, and 1920×1080. No horizontal page or workspace overflow,
  and no interactive control extends past the workspace edge.
- Keyboard focus, field-error association, disabled/busy states, and reduced
  motion are recorded in `docs/ui-system.md`. The screenshots were taken against
  the Vite preview, so the LM Studio offline banner and IPC error alerts are
  expected in those images.

The real-model test is opt-in:

```powershell
cargo test real_models_describe_and_compose -- --ignored --nocapture
```

It loads `lfm3-vl-2b` and the composer, describes a generated image, and composes
with `mmh3`. Running it surfaced two real LM Studio compatibility issues that are
fixed in v1:

- chat/load requests no longer send an unsupported `ttl` field
- model tracking uses the model key, and unload uses `instance_id`

## Packaging verification

- NSIS installer produced at
  `src-tauri/target/release/bundle/nsis/Prompt Studio_0.1.0_x64-setup.exe`
  (58.26 MiB).
- Portable folder and zip produced by `npm run package:portable`.
- The portable executable was launched twice; the second (warm) launch reached a
  window in 0.09 s with a 29.1 MB idle working set. The first cold launch took
  27.61 s in this environment, including first-run WebView2 setup and Defender
  scanning of the bundled sidecars.

## Manual checklist

These are the checks that still need a human at the target machine. They are
tracked here, not in an archived change.

### Install and launch

- [ ] Install the NSIS bundle on a clean Windows profile.
- [ ] Launch from the Start menu; no terminal window appears.
- [ ] Launch the portable zip after extracting to a writable folder.
- [ ] Confirm data lives under `.\data` in portable mode.
- [ ] Confirm a second launch focuses the first window.

### Offline end-to-end

- [ ] Start LM Studio with `lfm3-vl-2b` and the composer; disconnect the network.
- [ ] Import an image and a video.
- [ ] Describe, compose, and verify prompt/negative/params.
- [ ] Apply a crop or trim and confirm `edited/` output.
- [x] Drag a prompt, media, mask, and bundle into Explorer; verify bytes/playback.
- [ ] Restart and confirm the draft, library, and finished jobs persist.
- [ ] Interrupt a batch, restart, retry, and confirm succeeded items are skipped.

### Prompt workspace

- [ ] Open the app: Compose is the default mode in the Prompt workspace.
- [ ] Text-only Compose skips the describer.
- [ ] Media Compose runs Describe first, shows the verbose text, allows an edit,
      and composes from the correction.
- [ ] Describe mode stops after the description.
- [ ] Refine from a stored generation links the new result as a child.
- [ ] Effective guidance is visible; conflicting guidance warns before running.
- [ ] Copy plain prompt and copy structured produce the expected clipboard text.
- [ ] Regenerate bypasses the cache and the result shows fresh vs cached.
- [ ] Over-limit output warns and permits manual prompt editing.
- [ ] Batch from selected assets and from a dataset queues one job and shows
      progress in the workspace and Jobs.

### Filesystem edge cases

- [ ] Unicode/CJK/emoji filenames and source folders.
- [ ] Long paths (>260 characters) with the Windows long-path policy enabled and
      disabled; record the result.
- [ ] Removable drive: import, edit, disconnect, and confirm the offline state.
- [ ] Network share: import, edit, drag out, and confirm latency/permissions.
- [ ] Read-only source folder: edit fails with no partial output.
- [ ] Unwritable destination: Save-as fails with a clear error and no partial file.

### UI

- [ ] Runtime: load/unload both roles, keep-warm matrix, unload-all busy guard.
- [ ] Presets: duplicate, edit, import, export, delete-in-use guard.
- [ ] Editor: undo/redo/reset, mask polarity, fps/trim, resolution snapping.
- [ ] Jobs: pause/resume, cancel waiting, reorder, error details, copy details.
- [ ] Library: search, every filter, favorites, tags, collections, notes, rating,
      versions, diff, restore, trash, purge, export/import round trip.
- [ ] Datasets: each import format, mapping profile reuse, dedupe modes, 97/100
      partial import, run/resume, column-selected export.
- [ ] Drag payload selector for prompt/JSON/original/edited/mask/bundle and
      sidecars on/off.

## Measurements

- 10,000-generation library on this Windows x64 machine, debug in-memory build:
  seeding took 900 ms and the first 25-row page took 1 ms (target: under 300 ms,
  spec `generation-library`). Reproduce with
  `cargo test --manifest-path src-tauri/Cargo.toml --lib measures_first_page_for_ten_thousand -- --ignored --nocapture`.

## Known limitations

- Native drag-out uses `tauri-plugin-drag` 2.1.1 and passed interactive Explorer
  drop testing on the target machine; Save-as and Reveal remain as fallbacks.
- Long-path, removable-drive, and network-share behavior depends on the Windows
  configuration and was not exercised in the build environment.
- Cold start time is dominated by WebView2 first-run and antivirus scanning; it
  should be re-measured on the release target.
- The storyboard/sequence schema reservation is v2 only; no storyboard UI ships
  in v1.
