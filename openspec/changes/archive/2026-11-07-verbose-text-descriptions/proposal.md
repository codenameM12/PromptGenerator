# Proposal

## Why

The describer currently must return strict JSON. In practice that produces
terse, field-stuffed output ("subject: none", "style: none") and failures
outside the JSON shape, while the composer would do better with a rich,
verbose paragraph. The user wants a verbose text description instead of JSON,
applied consistently across the model prompt, IPC contract, storage, compose
prompt, UI editing, batch/job payloads, reruns, and docs.

## What Changes

- The describer prompt asks for a detailed multi-sentence prose description
  (subject, setting, light, palette, composition, camera, style, text-in-image;
  for video also motion and temporal change; for edit also the intended change
  and what to preserve) and never JSON. No JSON schema is sent; responses are
  accepted as text with a minimum-length retry.
- IPC and storage carry that text: `Description.text` replaces
  `json`/`confidence`; descriptions are stored as a JSON string in the existing
  column, and legacy structured rows are flattened to readable prose on read.
- Compose references and requests carry description text strings instead of
  JSON values. The composer prompt presents it as a "Visual description"
  section; reference summaries derive from the text.
- The Compose UI shows the verbose description, offers direct text editing, and
  drops the per-field checkboxes, JSON editor, confidence badge, and
  field-selection warnings.
- Batch items, queued jobs, library reruns, and generations store the text
  description.
- Docs and specs describe the verbose text contract.

## Capabilities

### Modified Capabilities

- `prompt-pipeline`: verbose text description contract, compose inputs, and
  editable description.
- `multi-reference-compose`: description text in the reference contract and
  the UI editing surface.

## Impact

- Backend: `pipeline.rs` (describe, messages, references, cache key, finalize),
  `commands/pipeline.rs` (DTOs), `commands/library.rs` (rerun), `job_queue.rs`,
  `commands/datasets.rs`, `db/repository.rs` (descriptions storage mapping).
- Renderer: `lib/ipc` types/fixtures, `ComposePanel.tsx`, tests.
- No schema migration: the existing `json` text column stores the description
  string; `confidence` stays null for new rows.
