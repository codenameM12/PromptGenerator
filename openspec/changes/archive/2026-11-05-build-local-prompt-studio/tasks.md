# Tasks

## 1. Toolchain and Scaffold

- [x] 1.1 Install the Rust stable toolchain and the MSVC build tools/Windows SDK required by Tauri, then verify `rustc --version`, `cargo --version`, and a trivial `cargo build` all succeed on this machine
- [x] 1.2 Scaffold the Tauri 2 + React + TypeScript + Vite project and verify `npm run tauri dev` opens the app window
- [x] 1.3 Configure strict TypeScript, ESLint, Prettier, `rustfmt`, and Clippy, and verify `npm run lint` and `cargo clippy -- -D warnings` pass on the scaffold
- [x] 1.4 Bundle recent static `ffmpeg`/`ffprobe` binaries as sidecar resources with their license notice, and verify the app can invoke `ffprobe -version` through the sidecar
- [x] 1.5 Add the test harness: Vitest for the renderer, `cargo test` for Rust, and a local mock LM Studio server, and verify one sample test in each layer passes
- [x] 1.6 Write the developer README covering prerequisites, install, dev run, test, and build commands, and verify the steps work from a clean checkout

## 2. App Shell, IPC, and Settings

- [x] 2.1 Implement single-instance launch, window geometry persistence, and the no-console Windows subsystem flag; verify a second launch focuses the first window and cold start shows no terminal
- [x] 2.2 Define the typed IPC command surface grouped by domain, with a renderer-side wrapper and mocked handlers; verify a contract test for each command's request and response types
- [x] 2.3 Implement the shared error type with categories, retryability, and user-facing messages, and verify serialization and mapping tests
- [x] 2.4 Add a restrictive CSP, capability allowlist, and a path canonicalization helper that rejects paths outside approved roots; verify unit tests cover traversal and symlink cases
- [x] 2.5 Implement the typed settings store with validation, defaults, and DPAPI-protected API token; verify invalid-value and restart-persistence tests
- [x] 2.6 Resolve data, cache, and portable paths, including the read-only-folder fallback; verify tests for portable and per-user modes
- [x] 2.7 Build the status surface polling endpoint health, loaded models, active jobs, and storage usage; verify it enters an error state within 5 seconds when the mock endpoint stops
- [x] 2.8 Implement workspace autosave and draft restore for the selected asset and editor state; verify an exit/reopen test restores an unapplied crop
- [x] 2.9 Implement diagnostics collection and export with secret and prompt redaction; verify a redaction test proves tokens and prompt text are absent by default

## 3. Database and Repositories

- [x] 3.1 Create the initial schema (assets, frames, edits, descriptions, presets, generations, tags, collections, jobs, datasets, settings) with embedded migrations; verify migration from an empty database and idempotent re-run
- [x] 3.2 Implement repositories for each aggregate with typed inputs and transactional writes; verify repository tests cover create, update, list, and delete
- [x] 3.3 Add the FTS5 index and triggers over prompt, negative, guidance, and description text; verify search returns expected rows and updates on edit
- [x] 3.4 Write a pre-migration backup and refuse to open a database newer than the binary; verify both behaviors in tests
- [x] 3.5 Implement library backup and restore; verify a round-trip test restores rows and presets
- [x] 3.6 Seed built-in preset records at startup from bundled resources without duplicating on subsequent runs; verify idempotency and content-hash storage

## 4. Media Ingest

- [x] 4.1 Implement drag-and-drop, native file picker, recursive folder picker, and clipboard image paste in the renderer-to-backend flow; verify each path creates an asset in an integration test
- [x] 4.2 Wrap `ffprobe` and normalize output into the asset metadata model, including rotation, display dimensions, frame rate, duration, codec, bitrate, audio, and color; verify fixture-based tests
- [x] 4.3 Implement BLAKE3 hashing and content deduplication, including renamed duplicates; verify duplicate import resolves to one asset
- [x] 4.4 Generate and cache thumbnails; verify the artifact exists, has bounded dimensions, and is reused on re-import
- [x] 4.5 Implement preview proxy generation for codecs the player cannot decode; verify a 10-bit or HEVC fixture produces a playable H.264 proxy and the source is untouched
- [x] 4.6 Implement frame sampling (first/middle/last, N-even, scene-change), deterministic timestamps, and contact-sheet assembly with the per-request image cap; verify determinism and timestamp tests
- [x] 4.7 Implement embedded metadata parsers for Automatic1111 infotext, ComfyUI workflow/prompt, PNG/WebP text chunks, and EXIF; verify fixture tests and that guidance is only offered, never overwritten
- [x] 4.8 Implement import progress, per-item failure isolation, and cancellation; verify a mixed batch imports valid files and reports invalid ones
- [x] 4.9 Serve media through a custom protocol with byte-range support; verify seeking works on a long video without loading it fully
- [x] 4.10 Track offline/missing assets and disable source-dependent actions while keeping thumbnails visible; verify with a moved file

## 5. Media Editing

- [x] 5.1 Implement the non-destructive edit stack with undo, redo, reset, and draft serialization; verify unit tests for ordering and reset
- [x] 5.2 Implement crop (free, fixed aspect, target aspect), rotate, flip, and fit/fill/pad with live preview; verify geometry tests for each mode
- [x] 5.3 Implement video trim, loop, audio stripping, constant-frame-rate conversion to 24/25/30/60, and container/codec selection; verify exported duration, frame rate, and audio presence with fixtures
- [x] 5.4 Implement the mask canvas (brush, eraser, rectangle, size, hardness, feather, invert) and grayscale PNG export at target resolution with preset polarity; verify pixel and polarity tests
- [x] 5.5 Implement resolution derivation with preset resolution tables, dimension-multiple snapping, upscale warnings, and manual override correction; verify a table-driven test including the 1000x563 to 1008x560 case
- [x] 5.6 Write edited media to `edited/` beside the source with atomic temp-then-rename, collision suffixes, and alternate root; verify placement, collision, and cancelled-export tests
- [x] 5.7 Re-probe every result and warn on material mismatches; verify the warning logic with a simulated mismatch
- [x] 5.8 Key edits by source hash, op stack, and target spec and reuse cached outputs; verify a repeated edit performs no re-encode
- [x] 5.9 Wire the editor UI with source-versus-result metadata, progress, cancel, and error display; verify the manual checklist for image and video edits

## 6. Model Runtime

- [x] 6.1 Implement the LM Studio client for list, load, unload, and chat with auth, timeouts, and SSE streaming; verify against the mock server including error responses
- [x] 6.2 Implement the `lms` CLI fallback for lifecycle and partial GPU offload plus capability detection; verify with a fake CLI on PATH and with it absent
- [x] 6.3 Implement configurable model roles, defaults, and missing-model errors with closest-match suggestions; verify role-change and missing-model tests
- [x] 6.4 Implement the serialized model manager and keep-warm policies (Both, Describer only, Composer only, None); verify policy matrix tests and no interleaved load/unload
- [x] 6.5 Enforce describer concurrency 1–4 and composer concurrency 1 with waiting rather than failure; verify peak in-flight counts
- [x] 6.6 Implement idle TTL with a busy guard and explicit unload-all with confirmation; verify that a model is never unloaded mid-request
- [x] 6.7 Implement the error taxonomy and retry classification; verify each category maps to the expected retryable flag
- [x] 6.8 Implement context budgeting and explicit, recorded trimming; verify overflow is prevented and the trimmed section is reported
- [x] 6.9 Build the runtime panel with model list, loaded state, load/unload, context length, TTL, offload, and unload-all; verify the manual checklist

## 7. Prompt Presets

- [x] 7.1 Define the preset schema in Rust and TypeScript and implement strict validation; verify valid and invalid fixture tests
- [x] 7.2 Implement compilation: guidance merge, section rendering, parameter mapping, negative handling, and limit enforcement; verify golden-output tests
- [x] 7.3 Implement preflight checks that separate blocking errors from warnings; verify character-limit, illegal-parameter, and unsupported-negative tests
- [x] 7.4 Author the six built-in presets (generic image, generic video, krea2, krea2 edit, qwen2 edit, mmh3) with version and content hash; verify snapshot tests of their compiled output
- [x] 7.5 Implement preset create, duplicate, edit, delete, and delete-in-use guard; verify CRUD tests and that built-ins stay immutable
- [x] 7.6 Implement preset import and export with size limits, schema validation, and major-version rejection; verify valid, malformed, oversized, and future-version tests
- [x] 7.7 Build the preset editor with live compiled preview and validation feedback; verify the manual checklist
- [x] 7.8 Document the preset schema and how to add a target, and verify the documented example validates and compiles

## 8. Prompt Pipeline

- [x] 8.1 Implement the describe stage prompt, JSON schema, parser, and validation for image, video, and edit inputs; verify mock-response tests for each shape
- [x] 8.2 Implement composition assembly with guidance precedence, locked elements, and budget handling; verify precedence and lock-survival tests
- [x] 8.3 Implement the edit-target branch that emits instructions plus a preserve list rather than a re-description; verify krea2 edit and qwen2 edit tests
- [x] 8.4 Implement streaming output and cancellation for both stages; verify partial tokens render and an aborted request leaves a cancelled record
- [x] 8.5 Implement output repair, one retry with validation feedback, and raw-response fallback marked invalid; verify all three paths
- [x] 8.6 Implement one-to-four variations with recorded seeds and seed-based regeneration; verify sibling creation and seed reuse
- [x] 8.7 Implement description and generation caches with bypass; verify a repeated request makes no model call and a bypass does
- [x] 8.8 Integrate stage batching so descriptions complete before compositions in batch runs; verify call-ordering in a test
- [x] 8.9 Implement the optional self-check pass with severity-annotated issues and no silent rewrite; verify a missing-camera issue test
- [x] 8.10 Implement text-only compose, describe-only, and refine modes; verify each mode's call graph
- [x] 8.11 Enforce output language and preset length limits; verify non-English guidance with English output and an over-limit result
- [x] 8.12 Implement the editable-description flow and recomposition from edits; verify the edit is used and both versions are retained

## 9. Job Orchestration

- [x] 9.1 Implement the job and job-item state machine with persistence and rejected illegal transitions; verify transition tests
- [x] 9.2 Implement the tokio worker pool with per-stage limits, a global limit, and backpressure; verify that fifty queued compositions run one at a time
- [x] 9.3 Implement per-item and aggregate progress with renderer events; verify event ordering and counts
- [x] 9.4 Implement retry with backoff, permanent-failure handling, and manual retry; verify attempt counts and no retry for missing input
- [x] 9.5 Implement cancellation that aborts HTTP and kills ffmpeg within the required window; verify no partial output is committed
- [x] 9.6 Implement crash recovery, interrupted-state marking, and idempotent resume; verify a simulated crash reprocesses only incomplete items
- [x] 9.7 Build the job panel with pause, resume, cancel, reorder, cancel-all-waiting, and error details; verify the manual checklist

## 10. Generation Library

- [x] 10.1 Persist every generation with full provenance, including failed ones; verify a failed record keeps its raw response and error
- [x] 10.2 Implement paginated history with FTS search, filters, and sorting; verify a 10,000-row fixture renders the first page within the performance budget
- [x] 10.3 Implement favorites for generations and assets with independent filters; verify persistence across restart
- [x] 10.4 Implement tags, collections, notes, and ratings; verify filter and rename behavior
- [x] 10.5 Implement version lineage, a version tree, and side-by-side field diffs; verify parent/child creation and diff output
- [x] 10.6 Implement restore-to-workspace from a stored generation; verify asset, edit, preset, and guidance are restored without mutating history
- [x] 10.7 Implement trash, restore, permanent delete, and bulk delete with confirmation; verify soft-delete visibility and cache cleanup
- [x] 10.8 Implement library export and re-import in JSON, JSONL, CSV, and plain text; verify round-trip fidelity and one-prompt-per-line text
- [x] 10.9 Build the library UI with search, filters, favorites, versions, and bulk actions; verify the manual checklist

## 11. Dataset Import and Export

- [x] 11.1 Implement parsers for TXT, JSON, JSONL, CSV, TSV, and image-folder caption sidecars with delimiter detection; verify fixture tests
- [x] 11.2 Implement field mapping with preview, auto-detection, and reusable profiles; verify mapping and profile-reuse tests
- [x] 11.3 Implement validation, encoding fallback, partial import, and per-row error reporting; verify a 97-of-100 success case
- [x] 11.4 Implement deduplication by normalized text or identifier with skip, merge, and keep options; verify each option
- [x] 11.5 Implement batch generation over dataset rows with enqueue, per-row status, and resume; verify succeeded rows are not reprocessed
- [x] 11.6 Implement export with column selection and verify a CSV round trip through the same profile
- [x] 11.7 Stream large files with bounded memory and report progress; verify a synthetic 100,000-row import stays within the memory budget
- [x] 11.8 Implement import safety: no code execution, no formula evaluation, and path-traversal rejection outside approved roots; verify each case
- [x] 11.9 Build the dataset wizard (file, mapping, validation, run, export); verify the manual checklist

## 12. Drag-Out and Filesystem Integration

- [x] 12.1 Spike native drag-out with the Tauri drag plugin and confirm a file dropped into Explorer arrives intact; record the result and fall back to Save-as only if native drag is unavailable
- [x] 12.2 Implement drag payload materialization, naming-template expansion, Windows-illegal character sanitization, and collision suffixes; verify name and collision tests
- [x] 12.3 Implement payload types (plain prompt, structured JSON, original, edited, mask, bundle) and optional sidecars with matching base names; verify each payload test
- [x] 12.4 Implement Save-as and Reveal in Explorer fallbacks; verify both on a real file
- [x] 12.5 Implement preparation progress, cancellation, and temporary-file cleanup on restart; verify no stale temp files remain
- [x] 12.6 Implement destination failure handling with no partial files and a clear error; verify with an unwritable target
- [x] 12.7 Build the drag affordance and payload selector in the UI; verify the manual checklist for prompt, media, mask, and bundle drags

## 13. Packaging and Integration Verification

- [x] 13.1 Produce portable and NSIS installer builds, and verify double-click launch on a clean Windows profile without a terminal
- [x] 13.2 Measure cold and warm start, idle memory, and first paint; verify the startup budget and record the numbers in the README
- [x] 13.3 Run an end-to-end offline scenario with real LM Studio models: import, describe, compose, edit, drag out, restart, resume; verify every step against the specs
- [x] 13.4 Verify Unicode, long-path, removable-drive, and network-share behavior end to end
- [x] 13.5 Write user documentation (quick start, model setup, presets, edited-folder layout, troubleshooting, diagnostics) and verify each documented step against the built app
- [x] 13.6 Run the full spec checklist, confirm no untracked regressions, and record the release notes for v1

## 14. Prompt Workspace UI

- [x] 14.1 Build the main-window prompt workspace with the four-mode selector (Describe, Compose, Refine, Batch), Compose as the default, media/preset selectors, and a text-only toggle
- [x] 14.2 Implement Describe mode: render the structured description with confidence, allow field edits, and recompose from the corrected description
- [x] 14.3 Implement Compose controls: effective guidance display with conflict warnings, locked phrases, output language, variations, seed, and cache bypass
- [x] 14.4 Implement result actions: copy plain prompt, copy structured result, regenerate, cache badge, and over-limit shortening or manual editing
- [x] 14.5 Implement Refine mode: existing prompt plus instruction, with the result linked to its parent generation
- [x] 14.6 Implement Batch mode: hand selected assets or dataset items to the job queue and show per-item progress
- [x] 14.7 Verify the prompt workspace manual checklist for all four modes and the media and text-only paths
