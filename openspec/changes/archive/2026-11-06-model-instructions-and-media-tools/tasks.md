# Tasks

## 1. Role system prompt overrides

- [x] 1.1 Add optional describer and composer system prompt overrides to settings with trimming, clearing, an 8000-character cap, and backward-compatible defaults; verify Rust tests cover round-trip, reload, clear, and the cap
- [x] 1.2 Pass overrides through describe, compose, batch, and queued job execution and include them in cache keys; verify Rust tests assert the override reaches the system message and changes cache identity
- [x] 1.3 Add a Model instructions section to the Runtime panel with save and clear actions; verify the RuntimePanel test asserts the settings patch
- [x] 1.4 Document override scope, precedence, and clearing in `docs/user-guide.md`; verify the description matches the panel

## 2. Quick media tools

- [x] 2.1 Add quick Crop / trim edges, Crop / aspect, Trim clip, and Mask regions actions that jump to the matching editor section, hiding video-only actions for images; verify the App test covers image and video availability
- [x] 2.2 Document image masking, video trimming, and the quick actions in `docs/user-guide.md`; verify the labels match the UI

## 3. Batch guide and doc fixes

- [x] 3.1 Add a worked batch guide with a concrete use case covering asset and dataset batches, describe-first behaviour, monitoring, retry, and export to `docs/user-guide.md`; verify the steps match the Jobs and Datasets panels
- [x] 3.2 Fix the documentation drift from the consistency audit (stale OpenSpec paths, bundle and installer sizes, portable-folder claim, backup wording, reset scope, preset kind behaviour, mojibake, duplicated drag verification); verify `npm run format:check` and a re-read against the artifacts
- [x] 3.3 Generate a styled offline HTML wiki from the Markdown guides with a persistent sidebar, per-page table of contents, wiki-style backlinks, previous/next navigation, page filter, responsive layout, dark mode, and print styles; verify every internal link resolves and `npm run docs:wiki` rebuilds all pages

## 4. Verification

- [x] 4.1 Run `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run audit:tokens`, `npm test`, `npm run lint:rust`, and `npm run test:rust`; verify all pass
- [x] 4.2 Run `openspec validate model-instructions-and-media-tools --strict`; verify it passes
- [x] 4.3 Rebuild only after the user closes the running app, then verify overrides, quick tools, and masking/trimming in the running app
