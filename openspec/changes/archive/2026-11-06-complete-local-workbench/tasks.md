# Tasks

## 1. Dataset media data model

- [x] 1.1 Add migration 6 with a nullable `dataset_rows.media_asset_ids_json` column and raise the latest schema version; verify a migration test upgrades a v5 database and old rows read as null
- [x] 1.2 Parse a row's media field into ordered segments supporting `a.png|b.png` and per-segment `@weight`; verify unit tests cover order, default weight 1.0, clamping to 0.0–2.0, and malformed segments
- [x] 1.3 Persist resolved asset ids for a row and read them back with `COALESCE`; verify repository tests round-trip single and multiple ids
- [x] 1.4 Extend the dataset row view with media asset ids and a found/missing/imported status; verify the DTO test covers text-only, single, and multi-media rows

## 2. Dataset media composition

- [x] 2.1 Import resolved row media through the existing hash/dedupe/probe/thumbnail path and store the resulting ids; verify a test imports the same file from two rows once and reuses the asset on rerun
- [x] 2.2 Resolve and validate row media against the dataset root before a run, rejecting traversal and marking missing files; verify tests cover found, missing, and `../` paths
- [x] 2.3 Build dataset compose payloads with the row's references and the row prompt as brief, leaving text-only rows unchanged; verify the existing dataset run test is extended to assert references
- [x] 2.4 Isolate row failures so a bad row records its reason while valid rows queue and succeed; verify a mixed-row test avoids aborting the job
- [x] 2.5 Skip rows that already succeeded and requeue only unfinished rows on a second run; verify the run test queues exactly the unfinished rows
- [x] 2.6 Add `mediaAssetId`, `generationId`, and `status` to dataset exports after the original columns; verify CSV and JSONL round-trip tests preserve original fields and provenance
- [x] 2.7 Render row media thumbnails and status in the dataset preview with an import/run summary; verify the DatasetPanel test covers found, missing, and multi-media rows
- [x] 2.8 Update `docs/user-guide.md` for dataset media and multi-media rows; verify the described fields and syntax match the shipped UI

## 3. Runtime capabilities

- [x] 3.1 Extend the model DTO with optional vision, reasoning, maximum output, and quantization, mirror it in the TypeScript types and mock fixture; verify `cargo test` covers reported, absent, and malformed fields
- [x] 3.2 Parse LM Studio's reported capability fields without inferring from names; verify `wiremock` fixtures cover a vision model, a text-only model, and a payload missing the fields
- [x] 3.3 Show capability badges and reported context in the Runtime panel; verify the RuntimePanel test asserts vision and unknown states
- [x] 3.4 Rank describer suggestions with known vision capability first and fall back to the existing name matcher; verify Rust ranking tests and a UI test that applying a suggestion is an explicit action
- [x] 3.5 Warn before a run when the describer is known non-vision, the composer context is below the preset budget, or a role model is unavailable; verify Compose tests assert each warning and that the run still proceeds
- [x] 3.6 Budget with `min(reported context, configured context)` and record which value was used when trimming; verify pipeline tests cover the smaller-reported case
- [x] 3.7 Include a model capability snapshot in diagnostics without prompt text; verify the diagnostics test asserts vision/context fields and redaction
- [x] 3.8 Update `docs/user-guide.md` and `docs/troubleshooting.md` for capability badges, warnings, and unavailable models; verify the documented states match the UI

## 4. Library comparison and re-run

- [x] 4.1 Add a read-only compare command returning full details for two to four generation ids, rejecting other counts; verify Rust tests cover unrelated generations and both boundaries
- [x] 4.2 Render aligned compare columns with prompt diff, metadata, references, and per-column copy; verify the LibraryPanel test covers two and four columns and the limit message
- [x] 4.3 Show reference thumbnails and weights in library generation details; verify the detail test covers present, empty, and missing assets
- [x] 4.4 Lock reference-aware re-run with tests that preserve ordered references/weights and parent linkage while changing preset and seed; verify `cargo test` passes and fix any gaps found
- [x] 4.5 Update `docs/user-guide.md` for compare limits, rerun behavior, and reference display; verify the documented limits match the UI

## 5. Complete backup bundle

- [x] 5.1 Write a backup bundle containing a manifest, database, settings, encrypted token, and workspace draft using atomic output; verify a Rust test lists the entries and confirms no prompt text in the manifest
- [x] 5.2 Detect zip versus legacy database input on restore, keep the pre-restore snapshot for both, and reject bundles missing required entries; verify tests cover bundle, legacy database, and corrupt bundle
- [x] 5.3 Treat an undecryptable token as absent and ask the user to re-enter it; verify the restore test places a foreign token file and reports no token
- [x] 5.4 Wire the bundle through the IPC command and Library UI, keeping legacy restores selectable; verify the Vitest fixture and UI test cover both formats
- [x] 5.5 Update `docs/user-guide.md` and `docs/release-notes.md` for the bundle format, portability caveat, and legacy restore; verify the caveats match the implementation

## 6. Job reference visibility

- [x] 6.1 Enrich job item details with ordered references, thumbnails, weights, and a missing marker; verify a Rust test covers an available and a missing asset
- [x] 6.2 Render the references in the Job panel; verify the JobPanel test asserts order, weight labels, and the unavailable state
- [x] 6.3 Update `docs/user-guide.md` for job reference details; verify the description matches the panel

## 7. Keyboard operation

- [x] 7.1 Add app-level compose, describe, and cancel shortcuts with dialog and input guards; verify Vitest covers each shortcut and that dialogs and text inputs suppress them
- [x] 7.2 Document the bindings in `docs/user-guide.md`; verify every listed binding matches the implementation

## 8. Verification and hardening

- [x] 8.1 Run the Explorer drag-out checklist and record results in `docs/drag-out.md`; verify the Save-as fallback is documented if native drag fails
- [x] 8.2 Add a dev-only seeding script that builds 10,000 generations in a temporary database and measure first-page load; verify the measurement is recorded with machine details and indexes are added only if the 300 ms target misses
- [x] 8.3 Confirm mmh3, krea2, krea2-edit, and qwen2-edit limits from current vendor documentation, update the built-in presets, and record sources in `docs/presets.md`; verify `npm test` and preset validation pass after reseeding
- [x] 8.4 Re-verify the NSIS build and portable package with the Avast exception procedure; verify the installer launches and update `docs/troubleshooting.md` or `AGENTS.md` if the procedure changed
- [x] 8.5 Remove the resolved dataset-media limitation from `docs/release-notes.md` and note the capability, compare, backup, and shortcut changes; verify no stale limitation remains
- [x] 8.6 Run `openspec validate complete-local-workbench --strict`; verify it passes with every delta in sync
- [x] 8.7 Run `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run audit:tokens`, `npm test`, `npm run lint:rust`, and `npm run test:rust`; verify all pass
- [x] 8.8 Rebuild the NSIS installer and portable zip, launch the desktop shortcut, and verify dataset media, capability warnings, compare, backup, and shortcuts in the running app
