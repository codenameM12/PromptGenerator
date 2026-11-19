# Tasks

## 1. Backend Contract

- [x] 1.1 Add `ComposeReference` and `references` to `ComposeRequest` and `BatchItem` with serde defaults; keep `assetId`/`description` as the primary reference.
- [x] 1.2 Wire references through `pipeline_compose`, `pipeline_batch`, the job executor, dataset payloads, and library re-runs.
- [x] 1.3 Add `pipeline::normalize_references` with weight defaulting/clamping and soft warnings for out-of-range weights and more than four references.
- [x] 1.4 Include the ordered reference set in the cache key and store the normalized list in generation `params.references`.
- [x] 1.5 Append a deterministic `References:` block to the finalized prompt and budget multi-description input through `runtime/budget.rs`.

## 2. Renderer

- [x] 2.1 Replace single selection with ordered toggle selection, order badges, per-reference weight inputs, and Select all/Clear.
- [x] 2.2 Describe every selected asset missing a description and block Compose until all are reviewed.
- [x] 2.3 Add per-reference field checkboxes with Select all/None and send only enabled fields.
- [x] 2.4 Preserve field selections across JSON edits, auto-select new keys, and warn softly when `subject` or a checklist field is disabled.
- [x] 2.5 Render the result first when it exists, enlarge the compose action, compact re-describe, and let the right column fill the workspace.

## 3. Tests and Docs

- [x] 3.1 Add Vitest coverage for toggling/order, weight defaulting/clamping, describe-all compose references, field filtering, and result-first order.
- [x] 3.2 Add Rust coverage for clamping, cache keys, params, serde defaults, reference warnings, and budget trimming.
- [x] 3.3 Verify `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run audit:tokens`, `npm test`, `npm run lint:rust`, `npm run test:rust`, and `openspec validate multi-reference-compose --strict`.
- [x] 3.4 Update `docs/user-guide.md`, `docs/release-notes.md`, and `AGENTS.md`.
