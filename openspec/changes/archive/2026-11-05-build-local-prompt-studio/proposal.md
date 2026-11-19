# Proposal

## Why

Local generation workflows now have strong open-weight targets (mmh3, Krea 2, Qwen-Image-Edit), but writing prompts for each of them is still manual, inconsistent, and disconnected from the source media. Existing prompt tools are cloud APIs, browser extensions, or Gradio forms; none combine a local vision model that reads images and video, a local LLM that composes target-specific prompts, VRAM-aware model switching, media prep, and native Windows file drag in/out.

This change builds a single-machine desktop workbench that turns a local image or clip plus human guidance into a prompt written in the dialect the selected target actually expects.

## What Changes

- New Windows desktop app that launches by double-click, renders a web UI, and talks only to localhost LM Studio.
- Two-stage pipeline: LFM3-VL-2B produces a structured media description; Gemma 4 12B composes/refines the final prompt. Both model roles are configurable.
- Data-driven target presets with built-ins for `mmh3`, `krea2`, `krea2 edit`, `qwen2 edit`, and generic image/video; presets are user-editable and importable.
- Media workbench: drag-in from Explorer, native file/folder picker, clipboard paste, probing, thumbnails and preview proxies, metadata and embedded-generation-parameter extraction.
- Non-destructive media editing: crop, static mask, trim, FPS change, automatic target-resolution selection. Edited media is written one folder down from the source; originals are untouched.
- Prompt modes: Describe, Compose, Refine, and Batch. Structured output (prompt, negative, parameters) plus copy-as-plain-prompt.
- Explicit runtime management: load/unload, TTL, partial GPU offload, keep-warm policy, stage-batched scheduling, and up to four concurrent LFM descriptions.
- Persistent, resumable job queue with progress and cancellation.
- Generation history with favorites, tags, full-text search, and regeneration lineage.
- Prompt-list and dataset import with field mapping, validation, dedupe, batch generation, and export.
- Drag-out to Windows Explorer with naming templates, optional `.txt`/`.json` sidecars, and a Save-as fallback.
- Local-first storage, no telemetry, and exportable diagnostics.

**Non-goals:** generating images or video; cloud inference; storyboard/multi-shot UI (v2 toggle; the data model reserves room); real-time collaboration; auto-updating; macOS/Linux packaging; downloading models outside LM Studio.

## Capabilities

### New Capabilities
- `desktop-shell`: app lifecycle, double-click launch, window, settings, diagnostics, IPC boundary, local storage locations.
- `model-runtime`: LM Studio connectivity, model discovery, explicit load/unload, TTL/offload controls, health, and VRAM-aware scheduling policy.
- `media-ingest`: drag/drop, file and folder selection, clipboard paste, probing, thumbnails, preview proxies, metadata and embedded-parameter extraction, deduplication.
- `media-editing`: non-destructive crop, static mask, trim, FPS conversion, resolution derivation, output placement, and undo/redo.
- `prompt-pipeline`: describe/compose/refine/batch modes, prompt assembly, structured output contract, validation, repair, retries, caching, and cancellation.
- `prompt-presets`: preset schema, built-in target presets, editing, import/export, compilation, and preflight constraints.
- `job-orchestration`: persistent queue, stage transitions, concurrency limits, progress, retry, resume, and cancellation.
- `generation-library`: history, favorites, tags, collections, search, filters, versions/lineage, notes, and ratings.
- `dataset-import-export`: prompt list and dataset import, field mapping, validation, dedupe, batch runs, and result export.
- `drag-out-export`: native drag-out to Explorer, file materialization, naming templates, sidecars, and fallback saving.

### Modified Capabilities
<!-- None: this is a greenfield project with no existing specs. -->

## Impact

- New codebase: Tauri 2 (Rust) backend, React + TypeScript renderer, SQLite storage, bundled `ffmpeg`/`ffprobe` sidecars.
- External dependency: LM Studio v1 REST API on localhost (with OpenAI-compatible fallback), `lms` CLI for partial GPU offload.
- Platform: Windows 10/11 with WebView2; portable and installed builds.
- No existing APIs, data, or users are modified.
