# Tasks

## 1. Backend description contract

- [x]  Rewrite the describer system prompt for verbose prose, remove the JSON schema, raise the token budget, accept plain text, and retry once on an unusably short response; update Rust tests and add one asserting a text description round-trips
- [x]  Change `Description` to `{ id, mode, text, cached, model }`; store the text as a JSON string in the existing column; flatten legacy structured rows to prose on read and cache hit; update repository tests
- [x]  Change compose description inputs (`ComposeRequest`, `ComposeReference`, `ComposeInput`, `NormalizedReference`, params, generation storage) from JSON to text, update the composer prompt section and reference summary, and keep rerun/library/library export working with legacy values

## 2. Batch, jobs, and datasets

- [x]  Update job executor and dataset batch paths plus rerun request building to text descriptions with a safe fallback for legacy structured payloads

## 3. Renderer

- [x]  Mirror the text contract in `lib/ipc` types and fixtures; update `ComposePanel` description cards to display and edit prose (remove field checkboxes, JSON editor, confidence, field warnings), send edited text in compose/batch, and add tests for the edit/save/empty behavior

## 4. Docs and verification

- [x]  Update `docs/user-guide.md`, `docs/release-notes.md`, `README.md`/AGENTS if needed, rebuild the wiki, and run all gates (`typecheck`, `lint`, `format:check`, `audit:tokens`, Vitest, Rust tests/clippy)
- [x]  Run `openspec validate verbose-text-descriptions --strict`; verify it passes
- [x]  Rebuild after the user closes the app and verify live: describe returns prose, the card shows/edits it, and compose sends the text
