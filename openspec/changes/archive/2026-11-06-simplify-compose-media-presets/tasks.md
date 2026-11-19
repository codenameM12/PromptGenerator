# Tasks

## 1. Compose reset

- [x] 1.1 Add a reset action to the Compose workspace that clears the result, descriptions, brief, refine inputs, locked phrases, per-run guidance, and media selection, is disabled while a run is active, and does nothing durable; verify a Vitest test asserts each cleared field and that the preset stays selected
- [x] 1.2 Verify reset does not touch stored history or the imported library; verify a Vitest test asserts the library list and generation calls are unchanged and the reset control is disabled on an empty workspace
- [x] 1.3 Document the reset scope in `docs/user-guide.md`; verify the described fields match the implementation

## 2. Media removal

- [x] 2.1 Exclude trashed assets from the normal asset listing and cover it with a repository test that trashes an asset and asserts it leaves `list` while remaining in `list_deleted`
- [x] 2.2 Add a Remove action to each Media asset with confirmation text covering history and source-file safety, refresh the list after removal, and fall back to the next asset when the removed one was selected; verify a Vitest test asserts the trash call, the confirmation, and the selection fallback
- [x] 2.3 Prune Compose selections whose assets are no longer present so a trashed asset cannot be composed with; verify a Vitest test removes an asset from the list and asserts the selection is dropped
- [x] 2.4 Document removal, trash restore, and source-file safety in `docs/user-guide.md`; verify the documented behaviour matches the UI

## 3. Text-first preset guide

- [x] 3.1 Extract the preset template into a shared module with helpers that map a stored document to guide fields and compile guide fields back into a valid document, preserving advanced fields; verify unit tests cover slugging, kind defaults, preserved params, and round-trip mapping
- [x] 3.2 Add a Guide tab to the preset editor with name, kind, target description, prompt guide, and optional character limit, read-only for built-ins, saving through the existing validation command; verify a Vitest test creates a preset from text and asserts the compiled document
- [x] 3.3 Keep the JSON tab authoritative and consistent with the guide, and require Duplicate for built-ins in both tabs; verify a Vitest test switches tabs, saves JSON, and asserts the stored document
- [x] 3.4 Document text-first authoring and when to use JSON in `docs/presets.md` and `docs/user-guide.md`; verify the documented fields match the shipped form

## 4. Verification

- [x] 4.1 Run `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run audit:tokens`, `npm test`, `npm run lint:rust`, and `npm run test:rust`; verify all pass
- [x] 4.2 Run `openspec validate simplify-compose-media-presets --strict`; verify it passes
- [x] 4.3 Rebuild and verify reset, removal, and preset guide in the running app; verify the observed behaviour matches the specs
