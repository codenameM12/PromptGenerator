# Proposal

## Why

Prompt Studio composes from one media asset, but image and video generation models accept several reference images, often with per-reference weights. Users currently have to run separate compose passes and lose the relationship between references, which makes multi-reference prompts impossible to express in one request.

## What Changes

- The Compose and Describe media picker becomes an ordered, toggleable multi-selection. Every selected asset shows its reference number and a weight field (`0.0-2.0`, default `1.0`).
- Describe runs for every selected asset that has no description. Compose waits until every selected asset has a reviewed description.
- Each description exposes field-level checkboxes. Only enabled fields are sent to the composer, per reference, and selections survive JSON edits.
- `ComposeRequest` gains an optional ordered `references` list (`assetId`, `editId`, `weight`, `name`, `description`) while the existing `assetId`/`description` fields keep working for old callers and stored jobs.
- Weights are defaulted and clamped to `0.0-2.0`; out-of-range values and more than four references produce soft warnings, not failures.
- The composed prompt appends a deterministic `References:` block with index, name, weight, and a short summary. The same normalized list is stored in generation `params.references`.
- The cache key includes the full ordered reference set and per-reference descriptions so different references never collide.
- Multi-reference descriptions are summarized and budgeted through `runtime/budget.rs` before the request is sent.
- When a result exists it renders first in the workspace, above descriptions and the media picker, so the generated prompt is immediately visible.

## Capabilities

### New Capabilities

- `multi-reference-compose`: ordered multi-asset selection, per-reference weights, field-level description filtering, reference-aware compose requests, prompt surfacing, and result-first layout.

### Modified Capabilities

<!-- No existing capability is renamed or removed. -->

## Impact

- Renderer: `src/features/compose/ComposePanel.tsx`, `ComposePanel.css`, `ComposePanel.test.tsx`.
- IPC types and fixtures: `src/lib/ipc/types.ts`, `src/lib/ipc/mock.ts`.
- Rust: `commands/pipeline.rs`, `pipeline.rs`, `job_queue.rs`, plus payload constructors in `commands/datasets.rs` and `commands/library.rs`.
- No database migration: normalized references live in the existing generation `params` JSON.
- Docs: `docs/user-guide.md`, `docs/release-notes.md`, `AGENTS.md`.
