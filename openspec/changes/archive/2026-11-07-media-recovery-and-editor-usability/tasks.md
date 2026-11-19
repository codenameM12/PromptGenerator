# Tasks

## 1. Restore on re-import

- [x] 1.1 Restore a trashed asset whose hash matches an incoming import, clear its missing flag, and return `restored` in the import result; verify a Rust test trashes an asset and re-imports the same file, asserting one record, restored true, and an empty trash
- [x] 1.2 Mirror `restored` in the TypeScript result and mock fixture; verify `npm run typecheck` and the IPC contract test pass

## 2. Remove from the Compose picker

- [x] 2.1 Add a remove action to each Compose media tile with confirmation, trash, list refresh, and selection pruning; verify a Compose test asserts the trash call and that the selection drops
- [x] 2.2 Document both removal entry points and re-import restoration in `docs/user-guide.md`; verify the text matches the UI

## 3. Mask and crop usability

- [x] 3.1 Render the asset image under the mask layer, clear the canvas by default, and make the eraser cut through coverage; verify the editor test asserts the image is present and mask export still reads binary coverage
- [x] 3.2 Replace pixel crop inputs with percentage edge controls and aspect presets, clamping every change to valid bounds; verify tests cover edge changes, aspect presets, and that an invalid crop cannot be produced
- [x] 3.3 Document the mask overlay and percentage crop workflow in `docs/user-guide.md`; verify the labels match the UI

## 4. Verification

- [x] 4.1 Run `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run audit:tokens`, `npm test`, `npm run lint:rust`, and `npm run test:rust`; verify all pass
- [x] 4.2 Run `openspec validate media-recovery-and-editor-usability --strict`; verify it passes
- [x] 4.3 Rebuild after the user closes the app, then verify re-import, picker removal, mask overlay, and crop in the running app
