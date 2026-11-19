# Tasks

## 1. Backend

- [x] 1.1 Repository: `latest_for_asset` / `latest_by_assets` returning the newest edit row per asset; Rust test with two saved edits returns the newest only
- [x] 1.2 New `edits_latest` command returning `EditLatest { assetId, editId, outputPath, width, height }` per requested asset id; register the command with the fixture and sample args in `mock.ts` so `ipc.test.ts` stays green
- [x] 1.3 Permission-denied write failures in render and mask export produce an error naming the folder plus the output-folder setting / ransomware-protection guidance; Rust test asserts the message contains the folder and the hint
- [x] 1.4 Composer image resolution prefers the reference edit when set, otherwise the asset's latest applied edit; Rust test asserts the image identity resolves to the edit output
- [x] 1.5 Runtime view exposes the Edit output folder (Browse, Save, Use source folder) and persists `outputRoot`; verify the setting round-trips through `settings_get`/`settings_update`

## 2. Renderer

- [x] 2.1 Compose panel: fetch `edits_latest` for all assets, send the latest `editId` in describe and compose references, tile previews use the edited output with an "edited" badge; tests cover the edit-aware reference payload and badge rendering

## 3. Verification

- [x] 3.1 Run `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run audit:tokens`, `npm test`, `npm run lint:rust`, and `npm run test:rust`; verify all pass
- [x] 3.2 Run `openspec validate compose-uses-applied-edits --strict`, rebuild the wiki, and verify both pass
- [x] 3.3 Rebuild after the user closes the app and verify live: applying a crop updates the Compose tile and badge, describe/compose send `editId`, and a protected-folder failure shows the actionable message
