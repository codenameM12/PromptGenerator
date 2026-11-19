# Design

## Context

See `proposal.md` for motivation. The repository already contains ~55 Rust
backend modules (`src-tauri/src/`) that own all domain behaviour, plus a React 19
/ TypeScript renderer (`src/`) of 21 production files. The renderer reaches the
backend through 83 Tauri commands with 99 call sites. A prior slice ported
`editHistory.ts` into `src-tauri/src/ui/edit_history.rs` with a recorded corpus
and a green differential test; `PARITY-CONTRACT.md`, `PHASES.md`, and
`PORTING.md` hold the contract, sequencing, and reviewer notes for that work.

## Goals / Non-Goals

**Goals:**

- Preserve observable behaviour while replacing the renderer with a native UI.
- Keep both implementations alive until every ported unit is differentially green.
- Reuse the existing Rust backend without rewriting domain logic.
- Produce a single native binary as the only launcher at cut-over.

**Non-Goals:**

- Fixing bugs or improving architecture during the port (tracked in
  `PORTING.md` follow-ups, shipped afterwards).
- Porting abstractions that only existed for the source type system (TS DTO
  mirrors, IPC mock scaffolding, webview CSP).
- Pixel-perfect visual reproduction; layout and styling are acceptable
  differences under `PARITY-CONTRACT.md`.
- Changing the SQLite schema, capability levels, or data locations.

## Decisions

### D1 — Native toolkit: egui/eframe

Immediate-mode rendering keeps view-model state inspectable without a DOM, is
mature on Windows, and has built-in persistence that maps to the settings and
workspace stores. Alternative considered: iced — more declarative, but less
mature for dense tables and media grids. Reversible before Phase 3 because the
unit of parity is the view model, not the widget tree.

### D2 — Seam is a view-model boundary, not FFI

The Rust UI calls the existing backend modules directly in-process. The
observable contract is `(command name, JSON arguments) + view-model state`,
recorded from the source. Alternatives: keeping Tauri IPC behind the native UI
(rejected — it preserves an unnecessary boundary) or a language binding
(rejected — end state A forbids bindings).

### D3 — Corpus-driven differential parity

`src/port/characterize.test.ts` drives the *source* implementations and writes
`port/corpus/*.json`. Rust replay tests deserialize the corpus into typed
structs and compare structurally; each report includes `N inputs compared,
M differences`. Comparing typed structs rather than serialized strings avoids
false differences from float formatting, while the Rust duplicate check still
uses serialization to match source semantics.

### D4 — Side-by-side until cut-over

No source file is deleted until its Rust counterpart is green and the native
binary is the only launcher. Fixes discovered during the port are recorded in
`PORTING.md`, not applied. This prevents a difference being ambiguous between a
fix and a regression.

### D5 — Platform glue mapping

Dialogs → `rfd`; clipboard → native clipboard access; window state → eframe
persistence; shell sidecar → `std::process::Command` over existing ffmpeg
resolution; `media://` → in-process image loading under the existing
canonicalized `MediaAccess` roots; single instance → a native lock/guard;
background work → an owned tokio runtime. Native drag-out has no drop-in
equivalent and is the one open integration risk.

### D6 — Parallel implementation with disjoint ownership

Two subagents work concurrently on non-overlapping files:

- **Agent A — view-model ports.** Owns `src-tauri/src/ui/<module>.rs` (except
  `mod.rs`), `src/port/characterize_<module>.test.ts`, and
  `port/corpus/<module>.json`. Ports `errors`, `presetTemplate`,
  `workspaceDraft`, `status`; verifies DTO coverage for `types.ts` and routes
  `crop.ts` to the existing `media/edit.rs`.
- **Agent B — native shell.** Owns the new `native-ui/` crate (`Cargo.toml`,
  `src/main.rs`, `src/app.rs`, platform modules, view stubs). Does not touch
  `src-tauri/` or `src/`.

The main agent owns `src-tauri/src/ui/mod.rs`, `src-tauri/src/lib.rs`, the change
artifacts, and integration of the remaining panels into `native-ui/`. A separate
crate for the native binary avoids `Cargo.toml`/`Cargo.lock` contention with
Agent A's Rust test runs; at cut-over it remains the shipped binary and
`src-tauri` becomes a plain library.

### D7 — Spec lifecycle

The work is tracked as OpenSpec change `rust-native-ui-port` (proposal → specs →
design → tasks → apply → archive). Task checkboxes stay literal `- [ ]` / `- [x]`
so the CLI parses them.

## Risks / Trade-offs

- **Native drag-out has no equivalent** → document an intended divergence in
  `PORTING.md` and keep the prepared-payload logic in `drag_out.rs`; use a
  file-save fallback rather than claiming parity.
- **Pre-existing clippy failures block `lint:rust`** → leave untouched during
  the port; record in `PORTING.md`; fix in a separate change after parity.
- **Avast false positive on rebuilt executables** (`IDP.HELU.PSD11`) → keep using
  the established launcher during verification; add the folder exception before
  producing a fresh portable copy.
- **Float formatting differences** → compare typed structs in the differential
  harness while preserving serialization-based duplicate detection.
- **Corpus coverage gaps** → extend the corpus per module before claiming
  parity; every input class and error path in the contract must be present.
- **Two agents running cargo concurrently** → Agent B builds in its own target
  directory; Agent A uses `src-tauri`'s; no shared lock files.

## Migration Plan

1. Phase 1–2: inventory, seam, characterization corpus (in progress).
2. Phase 3: leaf-first ports with differential tests (first leaf done).
3. Phase 4: continuous differential harness over the full corpus.
4. Phase 5: native binary becomes the only launcher; `src/` and web tooling
   deleted; Tauri webview deps stripped; corpus retained as frozen record.
5. Rollback before cut-over: keep the Tauri app runnable; each leaf can be
   reverted independently. After cut-over: restore from the archived change.

## Open Questions

- Which native drag-out mechanism is acceptable if OS-level drag initiation is
  unavailable — documented fallback versus dropping the payload flow? (Does not
  change the parity contract; decided before Phase 5.)
- Whether to keep the recorded TypeScript corpus in the repository after the
  source is deleted, or move it to a release artifact. (Retention only.)
- Whether to strip the Tauri plugins from `src-tauri` immediately after cut-over
  or in a follow-up change. (Packaging detail, not behaviour.)
