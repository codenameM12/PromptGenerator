# Proposal

## Why

Three everyday actions are missing or too heavy: the Compose workspace has no way to clear the prompt and start fresh, imported media cannot be removed from the Media view, and authoring a preset requires hand-writing the app's full JSON document even though a preset is usually just a text guide. Users hit all three within minutes of normal use.

## What Changes

- Add a **Reset** action to the Compose workspace that clears the current result, descriptions, brief, refine inputs, locked phrases, per-run guidance, and media selection, while keeping the chosen preset and the imported library. The action is disabled when there is nothing to clear and never touches stored history.
- Add **Remove** to each asset in the Media view. Removal moves the asset to the existing library trash (restorable), drops it from Compose selections and media pickers, and never deletes the original source file. A confirmation explains the trash behavior when the asset is referenced by history.
- Add a **text-first preset guide**: name, kind, target description, prompt guide, and optional length limits. Saving compiles a valid preset document with sensible defaults, so users never write JSON by hand. The raw JSON editor remains available for advanced edits, and validation still runs before saving.
- Keep the JSON document as the stored and portable contract; this change is about authoring, not about changing the format.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `prompt-pipeline`: the Prompt workspace requirement gains an explicit reset action and its clearing scope.
- `media-ingest`: adds asset removal from the workspace, with trash, picker cleanup, and source-file safety.
- `prompt-presets`: Preset authoring gains a text-first guide that compiles to the existing schema, with JSON retained for advanced use.

## Impact

- Renderer: `src/features/compose/ComposePanel.tsx`, `src/App.tsx` (Media view), `src/features/presets/PresetPanel.tsx`, plus their stylesheets.
- IPC: reset is renderer-only; media removal reuses `library_assets_trash`; preset guide uses existing save/validate commands, with a shared preset-template helper.
- Tests: Compose reset behavior, Media removal and picker cleanup, preset guide compilation and validation, plus IPC fixtures where a command is newly called.
- Docs: `docs/user-guide.md`, `docs/presets.md`.
- No schema migration, no new dependency, and no change to stored preset JSON.
