# Proposal

## Why

Users cannot change the instructions the two models receive: the describer text is fixed in code, and composer instructions require editing a preset. Masking and trimming exist but are buried in the editor with no entry point from the Media view. And there is no worked example of running a batch, which is the least obvious workflow.

## What Changes

- Add optional **system prompt overrides** for the describer and composer roles in the Runtime panel. An override replaces the built-in describer text or the selected preset's target instructions; required JSON, length, language, guidance, and locked-phrase rules are still appended. Clearing an override restores the default. Overrides apply to compose, describe, batch, and queued jobs, and take part in cache identity.
- Add **quick media tools** above the editor: Crop / trim edges (image), Crop / aspect and Trim clip (video), and Mask regions (both), which jump to the relevant editor section.
- Add a **worked batch guide** to the user guide with a concrete use case: generating prompts for a folder of product photos, monitoring the job, retrying failures, and exporting results.
- Fix the documentation drift found in the consistency audit.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `model-runtime`: role system prompt overrides, their precedence, persistence, and cache effect.
- `media-editing`: quick access to crop/trim/mask from the Media view.

## Impact

- Backend: `settings.rs` and `commands/settings.rs` (new optional fields), `pipeline.rs` (`DescribeInput`/`ComposeInput`/`BatchItemInput` overrides and cache keys), `commands/pipeline.rs`, `job_queue.rs`.
- Renderer: `RuntimePanel.tsx`, `App.tsx`, `MediaEditor.tsx`, IPC types and fixtures.
- Docs: `README.md`, `docs/user-guide.md`, `docs/release-notes.md`, `docs/troubleshooting.md`, `docs/presets.md`, `docs/drag-out.md`.
- No database migration; `settings.json` stays backward compatible because the new keys are optional.
