# Design

## Role system prompt overrides

Settings gain two optional, trimmed strings: `describerSystemPrompt` and `composerSystemPrompt`. Missing keys keep old `settings.json` files valid.

- `DescribeInput.systemPrompt` replaces `DESCRIBE_SYSTEM`; the user message, schema demand, mode, and frames are unchanged.
- `ComposeInput.systemPrompt` replaces `spec.systemPrompt` as the first line of the system message. The pipeline still appends JSON shape, length, edit mode, guidance, locked phrases, and reference rules.
- `BatchItemInput.systemPrompt` carries the same value through `pipeline_batch`.
- `commands/pipeline.rs` and `job_queue.rs` read the setting at execution time, so queued work uses the current override.
- Both content keys include the override, so changing it invalidates cached descriptions and generations.
- Empty or whitespace clears the override. Over 8000 characters is rejected.
- The UI lives in Runtime → Model instructions, with save and clear-fields actions and a note about scope.

## Quick media tools

The Media view already renders `MediaEditor` for the selected asset. Editor sections get stable ids (`editor-geometry`, `editor-trim`, `editor-mask`), and a quick-action row scrolls the matching section into view. Actions only navigate; they do not apply edits, so the staged-edit contract is unchanged. Video-only actions are hidden for images.

## Batch guide

Documentation only. It uses one concrete case (a folder of product photos), names the two batch entry points (selected assets and datasets), and walks queue, describe-first behaviour, monitoring, retry, and export. No behaviour changes.

## Rejected alternatives

- **Global prompt in the preset**: would not cover the describer and would not apply to assets without a preset.
- **Free-form override of the whole message**: would let users remove the JSON contract and break parsing; the pipeline keeps its required suffix.
- **Auto-applying mask/trim from the Media view**: surprising because edits are staged; navigation is explicit.
