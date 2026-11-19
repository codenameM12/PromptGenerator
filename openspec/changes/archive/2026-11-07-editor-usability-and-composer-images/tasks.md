# Tasks

## 1. Error text

- [x] 1.1 Add `src/lib/errors.ts` with `errorText()` unwrapping `CommandError` (`userMessage`, `message`, nested `data.message`); cover it with Vitest unit tests for the object, string, and error shapes
- [x] 1.2 Replace every `cause instanceof Error ? cause.message : String(cause)` catch site with `errorText(cause)`; verify no remaining occurrences by search and `[object Object]` can no longer reach the UI near the editor Apply button

## 2. Picker removal

- [x] 2.1 Remove the `window.confirm` from `removeAsset` in the Compose picker; verify removal still trashes the asset and updates the picker without any dialog

## 3. Crop drag-to-draw

- [x] 3.1 Implement drag rectangle selection on the editor preview with live overlay, clamped conversion through `cropFromEdges`, and sliders/readout staying in sync; add a Vitest test that dragging sets a valid clamped crop from both drag directions

## 4. Composer images option

- [x] 4.1 Backend: `ComposeRequest.include_images`, `ComposeInput.images`, command-layer image resolution (edit output → source → `still_frame` conversion, one sampled frame for video, cap 4), vision guard, and images-hash in the compose cache key; add a Rust test that image inclusion changes the cache key and that composing with images appends URL parts
- [x] 4.2 Frontend: types mirror `includeImages`, Compose/Refine run option checkbox enabled only when composer vision is true, Reset clears it; add fixture/args in `mock.ts` keeping `ipc.test.ts` green

## 5. Verification

- [x] 5.1 Run `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run audit:tokens`, `npm test`, `npm run lint:rust`, and `npm run test:rust`; verify all pass
- [x] 5.2 Run `openspec validate editor-usability-and-composer-images --strict` and check the wiki build; verify both pass
- [x] 5.3 Rebuild after the user closes the app and verify live: drag-draw crop sets the readout, mask/crop errors show real text, delete has no popup, and composing with images produces a prompt using a vision composer
