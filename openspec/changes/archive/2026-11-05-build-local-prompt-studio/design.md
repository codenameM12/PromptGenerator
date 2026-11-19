# Design

## Context

This is a greenfield project: the repository contains only OpenSpec scaffolding, so there is no existing code, schema, or API to preserve.

Constraints that shape the design:

- Windows 10/11 is the only target for v1; the app must launch by double-click and feel native.
- All inference is served by a local LM Studio instance. Current LM Studio exposes a v1 REST API with explicit model load/unload, idle TTL, and auto-evict, plus OpenAI-compatible chat endpoints and structured output.
- The describer is a small vision model (`lfm3-vl-2b`) and the composer is a 12B model; both must fit in consumer VRAM, so residency must be controlled rather than left to just-in-time loading.
- Media processing needs reliable demux, decode, encode, and frame extraction, which means an ffmpeg/ffprobe sidecar.
- Users must be able to drag files out to Explorer, which requires a native shell, not a browser tab.

## Goals / Non-Goals

**Goals:**

- Sub-2-second warm start and low idle memory relative to a Chromium-bundled shell.
- One Rust process owning filesystem, database, ffmpeg, model HTTP, and job execution; a thin React renderer with no ambient privileges.
- Deterministic, content-addressed caching so retries and preset comparisons do not repeat expensive calls.
- A persisted queue that survives crashes and enforces concurrency and VRAM policy in one place.
- Preset validation and compilation in a single implementation, shared by UI preview and final output.
- Native-feeling media ingestion and drag-out, including files WebView2 cannot decode.

**Non-Goals:**

- Cross-platform packaging, auto-update, and code signing in v1.
- Generating images or video, or integrating with ComfyUI/A1111 execution APIs.
- Per-frame video mask keyframing; v1 uses a single mask per clip.
- Multi-user, sync, or cloud storage.
- Storyboard/project hierarchy in the UI; the schema stores an optional `project_id`/`shot_index` but no workflow is built for it until v2.

## Decisions

### D1: Tauri 2 with a Rust backend, not Electron or a browser-served app

Electron would be faster to prototype but ships Chromium, costs 150+ MB, and starts in seconds. A local web server plus browser cannot produce native drag-out and adds port/lifecycle management. Tauri 2 uses the system WebView2, starts in well under a second, and keeps a single native process for the heavy work.

Trade-off: WebView2's codec coverage is narrower than Electron's bundled Chromium, and Tauri's drag-out plugin needs an early spike. Mitigation: always transcode an H.264/AAC preview proxy for anything the player cannot decode, and validate drag-out in the first implementation milestone with a Save-as fallback already specified.

### D2: Single Rust process, typed IPC, no local HTTP server

The Tauri core hosts all services. The renderer calls explicit commands through the invoke bridge; Rust emits progress events on channels. There is no localhost HTTP API, no CORS surface, and no port conflicts. The renderer runs with a restrictive CSP, no `eval`, and no direct filesystem access.

Commands are grouped by domain (`assets`, `edits`, `models`, `presets`, `pipeline`, `jobs`, `library`, `datasets`, `settings`, `drag`) and every path argument is canonicalized and checked against an allowlist of user-approved roots.

### D3: SQLite with FTS5, WAL, embedded migrations

`rusqlite` with the bundled SQLite feature avoids a system dependency. WAL gives concurrent readers while the worker writes. Migrations are embedded in the binary and run before the window becomes interactive; a pre-migration backup file is written first.

Alternatives: JSON files (simpler, but no FTS, no transactional batch updates, poor at 10k+ rows) and a separate database server (unnecessary).

### D4: Content-addressed media and caches on disk

Every source file is hashed with BLAKE3 on import. Cache keys derive from hashes, never from paths:

- Asset: `hash(source bytes)`.
- Edit: `hash(source hash + normalized op stack + target resolution + encoder settings)`.
- Description: `hash(edit hash + describer model + describer prompt version + sampling)`.
- Generation: `hash(description hash + effective guidance + preset hash + composer model + sampling + seed)`.

Derived artifacts (thumbnails, proxies, frames, masks, edited media, previews) live under the app cache directory in two-level hash folders; edited media is the exception and is written to the user's `edited` folder per spec, with the cache holding only the key mapping.

### D5: Inference through LM Studio's OpenAI-compatible chat plus v1 lifecycle

- Lifecycle: `GET /api/v1/models` (normalizing both the `models[].loaded_instances` shape and the OpenAI `data[]` fallback), `POST /api/v1/models/load`, `POST /api/v1/models/unload`. Load passes context length, flash attention, and KV-cache placement. Unload takes an `instance_id`, not a model key; the manager tracks model keys and resolves the first loaded instance. Idle TTL is enforced locally by the `ModelManager` and is not sent in chat or load bodies, because current LM Studio rejects an unrecognized `ttl` field.
- Inference: `POST /v1/chat/completions` with SSE streaming, `image_url` data-URL parts for vision, and `response_format: json_schema` for structured output where the runtime accepts it.
- Fallback chain when v1 lifecycle is unavailable: `lms load`/`lms unload` CLI, then JIT with auto-evict disabled and explicit unload between stages.
- Partial GPU offload: `lms load --gpu <ratio>` only, because the REST load body does not expose the ratio. If the CLI is unavailable, the control is disabled with an explanation.

Alternative considered: LM Studio's stateful `/api/v1/chat`. Rejected because the pipeline does not need server-held conversation state, and the OpenAI-compatible endpoint has broader vision and structured-output support.

### D6: Two-phase pipeline with a stage-batched scheduler

The pipeline is an explicit state machine per item: `describe → compose → validate → persist`. The scheduler groups by stage, so a batch describes every item before composing any, unless the user selects keep-warm "Both" and disables batching.

```
asset ──hash──▶ edit ──▶ frames/contact sheet ──▶ describer ──▶ description JSON
                                                                    │
guidance + preset + locked elements ────────────────────────────────┼──▶ composer
                                                                    │
                                                              validate / repair
                                                                    │
                                                     prompt + negative + params
```

Semaphores enforce describer concurrency 1–4 and composer concurrency 1. Model residency is owned by a single `ModelManager` that serializes load/unload operations and consults the keep-warm policy before each stage.

### D7: Preset compilation in a shared Rust core crate

Presets are JSON documents validated by `serde` into a typed model. The same crate performs compilation (merge guidance, render sections, map parameters, apply limits, run preflight) and is exposed to the renderer through `presets.preview` and `presets.compile` commands. The UI never reimplements rules, so preview and final output cannot drift.

Built-in presets are bundled as read-only resources with a schema major version and a content hash. User copies live in the data directory. Every generation stores the preset hash, id, and version, making old outputs reconstructable even after edits.

### D8: Deterministic structured output with repair

Composition requests the preset's JSON schema through `response_format`. On a schema violation the pipeline runs a local repair pass (strip fences, fix trailing commas, coerce types), then one model retry whose prompt names the validation errors, then persists the raw text with `invalid` status. This is bounded and recorded, never silent.

### D9: Queue and cancellation

A single ordered tokio runner executes jobs from a SQLite-backed queue; the stage concurrency limits (describer 1–4, composer 1) live in `ModelManager` semaphores, so the runner cannot start a second composition while one is active. Each job carries a cancellation flag; cancellation aborts the HTTP future or kills the ffmpeg child process, then transitions the job without marking partial output complete. On startup, jobs in `running` are marked `interrupted` and offered resume; completed `job_items` are skipped by state so resume processes only incomplete items.

### D10: Media pipeline details

- Probe: `ffprobe -print_format json -show_streams -show_format`, then normalize rotation and display dimensions.
- Thumbnails: first decodable frame, max 512 px.
- Preview proxy: H.264/AAC MP4, max 1280 px, generated only when the source codec or pixel format is not playable.
- Frames: `-vf fps=...` or explicit `-ss` seeks; scene detection via `select='gt(scene,0.3)'`; contact sheet assembled with the `tile` filter.
- Mask: painted in the renderer on a canvas at source resolution, exported as 8-bit grayscale PNG, then composed with ffmpeg `alphamerge`/`maskedmerge` or used as a sidecar for targets that accept masks.
- Writes: ffmpeg to `*.partial.<ext>` in the destination directory, then atomic rename; probe the result before committing the DB row.

Video preview is served through a custom `media://` protocol that supports byte ranges so `<video>` seeking works without loading whole files.

### D11: Drag-out

The renderer starts a drag with a payload descriptor; Rust materializes any non-file content (prompt text, JSON, mask) into a temporary cache file, expands the naming template, sanitizes Windows-illegal characters, resolves collisions, then calls `tauri-plugin-drag` `2.1.1`'s `start_drag` in copy mode with one or more absolute paths. Preparation is a job-like task with progress and cancellation. Source files are never moved or modified. "Save as" and "Reveal in Explorer" are first-class fallbacks using the dialog and opener plugins.

### D12: Secrets and diagnostics

The LM Studio token is stored with Windows DPAPI, not plaintext. Logs use `tracing` with a rotating file appender. The diagnostics export zips version, OS, redacted settings, model status, and the last N log lines; prompt and description fields are replaced with length markers unless the user opts in.

### D13: Prompt workspace as the default surface

The main window hosts the prompt workspace directly rather than behind a separate route. A mode selector exposes Describe, Compose, Refine, and Batch. Compose is the default; media selection is optional, and text-only Compose skips the describer. The workspace owns the brief/description, guidance, locked phrases, language, variations, seed, and cache-bypass controls; it shows the effective guidance before running and renders results with copy-plain, copy-structured, regenerate, cache state, warnings, and over-limit actions. Side panels (Media, Library, Jobs, Presets, Runtime, Datasets) open in-window and must not discard workspace inputs.

Trade-off: a single dense workspace is discoverable but risks crowding. Controls are grouped by stage, and advanced inputs stay collapsed until needed.

## Data Model

Core tables (all timestamps UTC, all IDs UUIDv7):

| Table | Purpose |
|---|---|
| `assets` | hash, kind, source path, probe fields, thumbnail/proxy paths, missing flag, trash timestamp |
| `asset_frames` | asset, index, timestamp, cached frame path |
| `edits` | asset, op stack JSON, target spec, output path, cache hash |
| `descriptions` | asset/edit, model, prompt version, sampling, JSON, confidence |
| `presets` | owner, name, kind, version, document JSON, content hash |
| `generations` | parent, asset, edit, description, preset id/version/hash, prompt, negative, params, guidance, seed, models, status, error, tokens, latency, raw, rating, notes, trash timestamp, optional project/shot |
| `tags`, `generation_tags`, `asset_tags` | organization |
| `collections`, `collection_items` | organization |
| `generations_fts` | FTS5 over prompt, negative, guidance, description text |
| `jobs`, `job_items` | queue, priority, dedupe key, note, stage, progress, attempts, idempotency key |
| `datasets`, `dataset_rows` | import mapping, export columns, row data, status, linked generation |
| `import_profiles` | named reusable dataset mapping |
| `settings` | key/value |
| `schema_migrations` | applied migrations |

## Module Layout

```
src-tauri/
  src/
    commands/        thin IPC handlers and DTOs
    runtime/         LM Studio client, model manager, lms CLI, budget, role suggestions
    media/           probe, ffmpeg, import, frames, metadata, protocol, edit, render, mask
    db/              connection, migrations, repositories
    pipeline.rs      two-stage describe/compose service
    preset.rs        preset schema, validation, compile, preflight
    job_queue.rs     queue runner, transitions, retry, recovery
    dataset.rs       dataset parsing, mapping, dedupe, export
    drag_out.rs      drag materialization, naming, sidecars, save-as
    builtin_presets.rs
    paths.rs         data/cache/portable resolution, canonicalization
    diagnostics.rs   redaction and export
  presets/           built-in preset JSON
  binaries/          ffmpeg.exe, ffprobe.exe
src/
  features/compose   prompt workspace, modes, result actions
  features/editor    media editor
  features/library   history, favorites, filters, versions
  features/presets   editor, import/export, preview
  features/jobs      queue panel, progress, errors
  features/datasets  import wizard, mapping, batch runs
  features/settings  runtime and app settings
  lib/ipc            typed command wrappers and event subscriptions
```

## Risks / Trade-offs

- **LM Studio may serialize requests per model** → measure actual parallelism in the first milestone; if serialized, send multi-image contact sheets instead of four concurrent requests and keep the concurrency setting for future runtimes.
- **WebView2 cannot decode some source codecs** → always proxy before preview; never rely on the player for correctness.
- **Tauri drag-out behavior differs across Explorer versions** → spike drag-out first; the Save-as fallback satisfies the spec if native drag fails.
- **A 2B vision model will misread scenes** → editable descriptions, confidence flag, configurable describer model, and a manual-description path.
- **VRAM thrash between 2B and 12B** → explicit load/unload, stage batching, keep-warm policy, and a policy-conflict fallback.
- **Structured output can still fail** → bounded repair + one retry + raw fallback, all recorded.
- **ffmpeg binaries are GPL** → personal use is fine; document licensing in the README and keep the binary as a replaceable resource.
- **Unsigned executable triggers SmartScreen** → document it; signing is a distribution-time decision, not a code change.
- **Large libraries and long paths** → pagination, FTS, canonicalized `\\?\` paths, and bounded-memory streaming for imports.
- **SQLite write contention** → single writer task, WAL, short transactions; media bytes never stored in the DB.

## Migration Plan

Greenfield, so there is no data migration. The app writes a fresh schema, seeds built-in presets resource-side, and records their hashes. Database migrations are forward-only and applied at startup with a pre-migration backup. Rollback is "install the previous version and restore the backup"; the app refuses to open a database whose schema is newer than the binary and offers the backup path instead.

## Open Questions

- Exact prompt character limits and negative-prompt support for mmh3, krea2, krea2 edit, and qwen2 edit need to be confirmed against each target's documentation while populating the built-in presets; the preset schema already makes these data, so no code decision depends on the answer.
- Whether LM Studio accepts more than one image per chat request for the chosen VL model will be measured in the pipeline spike; both the separate-frame and contact-sheet paths are specified.
- Whether `lms` is guaranteed to be on PATH on the user's machine will be probed at startup; the offload control degrades gracefully if it is not.
