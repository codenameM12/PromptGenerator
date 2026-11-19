# Phases — Prompt Studio renderer → Rust native UI

The five phases run **in order**. Do not start a later phase early. Depth per
phase is below; the governing rules and acceptance criteria live in
`PARITY-CONTRACT.md`.

## Phase 1 — Inventory and seam ✅ complete

Ends when the boundary the Rust lives behind is named and the call sites crossing
it are counted.

- [x] Enumerate production source files (21) and test files (20).
- [x] Count the command surface: 83 commands in `src/lib/ipc/index.ts`, mirrored
      by `generate_handler!` in `src-tauri/src/lib.rs`.
- [x] Count renderer call sites across that seam: 99.
- [x] Name the seam: the **view-model boundary** — a user event becomes a domain
      call (name + JSON args) plus a view-model state transition.
- [x] Decide end state: **A — Replacement**, egui/eframe toolkit.

## Phase 2 — Characterize 🔄 in progress

Ends when existing behaviour is captured as executable tests **against the
source implementation, bugs included**. A test asserting intended behaviour is a
bug report disguised as a test.

- [x] Confirm the existing characterization sources: `src/lib/media/editHistory.test.ts`
      (4 cases), `crop.test.ts`, `errors.test.ts`, `presetTemplate.test.ts`, the
      seven panel tests, `App.test.tsx`.
- [x] Add `src/port/characterize.test.ts`: drive source modules over a fixed
      scenario corpus and write `port/corpus/*.json`.
- [x] Generate the first corpus (`edit_history.json`, 8 cases / 25 observations)
      by **running** the source — no hand-authored expected values.
- [ ] Extend the corpus to every input class the contract names, including error
      paths (the ones a port silently narrows).
- [ ] Wire corpus generation into CI before the differential test runs.

## Phase 3 — Port leaf-first 🔄 in progress

Ends when the lowest-dependency module is in Rust, called through the seam, with
**both implementations still present**.

Order (lowest dependency first):

1. `lib/media/editHistory.ts` → `src-tauri/src/ui/edit_history.rs`
2. `lib/errors.ts` → view-model error classification (reuse `CommandError`)
3. `lib/ipc/types.ts` → verify Rust DTO coverage, no new code
4. `lib/media/crop.ts` → route to existing `media/edit.rs` (see contract §7)
5. `features/presets/presetTemplate.ts` → compare against `preset.rs` compilation
6. `lib/useWorkspaceDraft.ts`, `lib/useStatus.ts` → view-model state
7. `lib/media/import.ts`, `lib/drag.ts` → native dialog/drag integration
8. Seven feature panels + `MediaEditor.tsx` → egui views (one at a time)
9. `App.tsx`, `main.tsx` → eframe shell with rail/navigation

For each step: add the Rust module, add its differential test against the corpus,
and keep the TS file reachable. Bag fixes for `PORTING.md`; do not fix while
porting.

- [x] Step 1: `edit_history.rs` ported, both implementations present, 4 mirroring
      unit tests green.

## Phase 4 — Run both 🔄 partial (first module green)

Ends when every input reaching the seam goes through both implementations and the
outputs are compared.

- [ ] Differential harness runs from CI on every change, not once at the end.
- [x] Report `N inputs compared, M differences` per module (denominator always):
      `editHistory: 25 inputs compared, 0 differences` locally; CI wiring pending.
- [ ] Cover error paths and edge cases named in the corpus contract.
- [ ] A difference found six modules later must be attributable to the module
      that introduced it — hence the per-module harness from Phase 3.

## Phase 5 — Cut over and delete ⏳ not started

Ends when traffic goes to Rust only and the source implementation is **deleted**,
not left as a dead fallback.

- [ ] Native binary is the only launcher; React/Vite removed from the build.
- [ ] Delete `src/`, `index.html`, `vite.config.ts`, `vitest.config.ts`,
      `tsconfig*.json`, `eslint.config.js`, Node dependencies.
- [ ] Strip Tauri webview dependencies; backend compiles as a plain library.
- [ ] Remove the characterization generator (source no longer exists) but keep
      the frozen corpus as a regression record.
- [ ] Delete any temporary binding/scaffold (end state A: none should exist).
- [ ] Re-run every gate; update `PORTING.md` with final verification.
