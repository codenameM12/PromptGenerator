# Design

## Context

The app already persists every composition in the Library, exposes `library_trash` for generations and assets, and stores presets as validated JSON documents. The three gaps are UI-level: no reset, no removal entry point, and a JSON-only authoring surface. None of them needs a schema change.

## Decisions

### 1. Reset lives in the Compose panel and is renderer-only

`ComposePanel` already owns every piece of in-progress state: `result`, `descriptions`, `brief`, `refine*`, `locked`, `runGuidance`, and `selection`. A reset function clears exactly those and leaves `presetId`, `globalGuidance`, and the asset list alone.

- No IPC call and no database write: the composed result is already stored in the Library, so reset only discards in-progress input.
- The button is disabled when there is nothing to clear (no result, no descriptions, no brief text, no locked phrases, no run guidance, no selection) and while a run is active.
- Reset does not clear `globalGuidance`, because that is a saved preference rather than per-run input.
- No confirmation: nothing durable is destroyed.

### 2. Removal reuses the library trash and hardens asset listing

The Media view calls the existing `library_trash` command with `assetIds` and an empty `generationIds`. That sets `assets.deleted_at`, which the Library trash already lists and restores.

One backend correction is required: `repository::assets::list` currently ignores `deleted_at`, so trashed assets still appear in Media and in pickers. It will exclude deleted rows, matching `list_favorites`. `list_deleted` keeps serving the trash view.

- The confirmation copy states that generations keep their text, the reference becomes unavailable, and the original file is untouched.
- `assets_purge` remains the only path that deletes cached thumbnails/proxies; source files are still never deleted.
- After removal the renderer refreshes the asset list. `ComposePanel` prunes any selection whose asset id is no longer present, so a removed asset cannot be composed with.
- If the removed asset was `selectedId` in the Media view, the selection moves to the next asset or empty.

### 3. The preset guide compiles to the existing schema in the renderer

The editor keeps one source of truth: the JSON document currently in `text`. A new `presetTemplate.ts` module provides:

- `defaultTemplate` (moved from `PresetPanel`) as the base for new presets.
- `guideFromDocument(document)` to prefill guide fields from a stored preset: name, kind, description, `systemPrompt` + `guidance` as the guide text, and `limits.promptChars`.
- `documentFromGuide(guide, base)` to produce a valid document: slugged id from the name, kind-specific defaults for sections, checklist, resolutions, reference slots, and negative support, while preserving `base` fields the guide does not expose (params, limits token budgets, examples).

Guide fields: name, kind, target description, prompt guide (multiline), prompt-character limit (optional number).

- Saving in guide mode compiles the document, then calls the existing `presets_save` command, so the same backend validation still applies.
- New presets get version `1.0.0`; saving changes to an existing user preset keeps its id and bumps the patch version so the content hash changes and history stays attributed.
- Built-in presets keep the guide fields read-only; **Duplicate** is the path to an editable copy.
- Switching to the JSON tab shows the compiled document; JSON edits remain authoritative when saving from that tab.

### 4. Rejected alternatives

- **A separate "clear prompt" IPC command**: unnecessary; no durable state is involved.
- **Hard-deleting assets from Media**: violates the library's trash model and would strand history.
- **A new preset format for text-only guides**: would break import/export, provenance hashing, and the compiled-output contract. Compiling text into the existing schema keeps one stored format.

## Risks

- The guide cannot express parameters, references, or resolutions. That is intentional; the JSON tab remains the escape hatch, and the guide states so.
- Narrowing `assets::list` changes counts anywhere that expected trashed rows. Tests that seed and then trash assets must use `list_deleted` instead.

## Migration

None. Existing preset documents remain valid and unchanged.
