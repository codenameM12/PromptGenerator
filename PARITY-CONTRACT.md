# Parity Contract — Port Prompt Studio to a Rust-native UI

Status: **active** (Phase 1 complete; Phase 2 first corpus recorded; Phase 3 first
leaf ported and differentially green). This contract is the
definition of done for the port. Per the `port-to-rust` skill, no Rust UI code was
written before this file existed, and behaviour changes are **not** allowed inside
the port.

## 1. End state

**A — Replacement.** The deliverable is a standalone native Rust application.
The React 19 / TypeScript / Vite renderer is deleted at cut-over. No bindings
ship, at any point. The seam during the port is a **view-model boundary**, not a
language FFI: both implementations are driven by the same recorded interaction
corpus, and the Rust one calls the existing backend logic in-process.

- Source: `src/` (41 `.ts/.tsx` files — 21 production, 20 test/fixture).
- Target: `src-tauri/src/ui/` (Rust view-model modules) + a native binary
  (egui/eframe shell) that renders them.
- Reused verbatim, **not ported**: `src-tauri/src/{commands,db,media,runtime,
pipeline,preset,dataset,job_queue,drag_out,diagnostics,paths,settings,
workspace,builtin_presets}.rs` (55 Rust modules). This is already the source of
  truth for all domain behaviour. The native UI calls it directly instead of
  over Tauri IPC.

Toolkit decision: **egui / eframe 0.37**. Rationale: immediate-mode rendering
makes view-model state directly testable without a DOM, it is mature on Windows,
and its persistence/serialization story maps cleanly onto the settings/workspace
stores. This is an early, reversible decision — changing it before Phase 3 does
not invalidate the contract, because the unit of parity is the view model, not
the widget tree.

## 2. Seam (Phase 1 inventory)

| Seam element                                    | Count | Where                                                                                              |
| ----------------------------------------------- | ----: | -------------------------------------------------------------------------------------------------- |
| Backend commands exposed over IPC               |    83 | `src/lib/ipc/index.ts` (`commands` map), mirrored by `generate_handler!` in `src-tauri/src/lib.rs` |
| Production IPC call sites in the renderer       |    99 | 21 production `.ts/.tsx` files (excluding `src/lib/ipc/index.ts` and tests)                        |
| Pure client-side logic modules (real port work) |     4 | `editHistory.ts`, `crop.ts`, `errors.ts`, `presetTemplate.ts`                                      |
| State/hook modules                              |     4 | `useWorkspaceDraft.ts`, `useStatus.ts`, `lib/media/import.ts`, `lib/drag.ts`                       |
| UI panels + shell                               |    13 | `App.tsx`, `main.tsx`, 7 feature panels, `MediaEditor.tsx`, `presetTemplate` editor bits           |
| Contract/test scaffolding                       |    20 | `*.test.ts(x)`, `src/lib/ipc/mock.ts`, `src/test/setup.ts`                                         |

The **seam** is the set of 83 command invocations. In the source, a user action
becomes an `ipc.<domain>.<method>(args)` call plus a state transition. In the
target, the same user action must become the same domain call with the same
arguments (as Rust DTOs) plus the same view-model state transition.

## 3. Unit of parity

A **scenario**: an ordered list of user events applied to one view, with a fixed
set of mocked backend responses, observed as:

1. the ordered list of backend calls `(command name, JSON arguments)`,
2. the final view-model state (serialized to JSON),
3. the user-visible status/error text.

One recorded scenario ≈ one existing Vitest test case. The corpus is
`port/corpus/*.json`, generated from the **source implementation** by
`src/port/characterize.test.ts` (not hand-written from intended behaviour).

## 4. Acceptable differences

- Widget choice, pixel layout, colours, spacing, fonts, scroll/animation timing.
- Internal names, module layout, trait/type shape in the Rust code.
- Log wording and stdout/stderr formatting.
- Ordering that is not observable through the view model (e.g. hash-map order).
- Threading/task scheduling, as long as resulting state and call order match.
- The absence of a webview/CSP boundary (native UI has no `default-src 'self'`).

## 5. Unacceptable differences (a caller can branch on these)

- A missing, extra, renamed, or reordered backend call.
- A different JSON argument for a command (including defaults, weights, field
  selection, and reference ordering).
- Different view-model values, list ordering, selection, enabled/disabled state,
  or history cursor/branch behaviour.
- Different validation rules or user-visible error/status text.
- Different path-safety decisions (`PathGuard` rejections).
- A behaviour "fixed" during the port. Fixes go in `PORTING.md` follow-ups and
  ship **after** parity is proven.

## 6. How parity is measured

- Phase 2 records each scenario's observed output from the TS source into the
  corpus. The existing Vitest suite is the primary characterization source;
  quirks asserted there (including bugs) are kept.
- Phase 3+ runs the Rust view model over the same corpus in
  `src-tauri/src/ui/*` tests and compares JSON.
- The harness is run in CI from Phase 3 onward, not once at the end.
- Reports state **the denominator**: `N inputs compared, M differences`. Zero
  differences over nine inputs is not parity.
- Verification commands: `npm test` (characterization), then
  `cargo test --manifest-path src-tauri/Cargo.toml` (differential), then
  `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` and
  `cargo fmt --check` on port-created files only.

## 7. Deliberately not ported

| Source item                                  | Reason                                                                                        | Where behaviour lives now                                                |
| -------------------------------------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `lib/ipc/types.ts`                           | Rust DTOs are the source of truth; the TS mirror is a contract artifact                       | `src-tauri/src/commands/mod.rs` + domain DTOs                            |
| `lib/ipc/mock.ts`, `ipc.test.ts`             | Test scaffolding for the seam; replaced by the Rust differential harness after cut-over       | kept until Phase 5                                                       |
| `lib/media/crop.ts` geometry                 | Duplicates `media/edit.rs`; porting the abstraction would create two geometry implementations | `media/edit.rs`, reached via `edits_preview`                             |
| Tauri single-instance / window-state plugins | Source-language integration, not behaviour to preserve literally                              | replaced by eframe persistence + a native single-instance guard (see §8) |
| `media://` webview protocol                  | No webview in the target; images are loaded in-process from canonicalized allowed roots       | `media/protocol.rs` path rules reused                                    |

Each entry is a judgment call recorded here, not made silently mid-file.

## 8. Target-side integration that must be reimplemented (not "ported")

These have no source-code counterpart; they are platform glue the webview used to
provide. They are tracked here so they are not mistaken for port regressions.

| Tauri facility                                | Native replacement                                        | Status                                                      |
| --------------------------------------------- | --------------------------------------------------------- | ----------------------------------------------------------- |
| window creation / `tauri-plugin-window-state` | eframe + `egui` persistence                               | planned                                                     |
| `tauri-plugin-dialog`                         | `rfd`                                                     | planned                                                     |
| `tauri-plugin-clipboard-manager`              | egui clipboard / `arboard`                                | planned                                                     |
| `tauri-plugin-shell` (ffmpeg sidecar)         | `std::process::Command` over `media/ffmpeg.rs` resolution | planned                                                     |
| `tauri-plugin-single-instance`                | native lock/guard                                         | planned — open risk                                         |
| `tauri-plugin-drag` (native drag-out)         | platform drag-out; **no drop-in equivalent**              | open risk — intended divergence to document if unattainable |
| `media://` protocol                           | in-process image load under `MediaAccess` allowed roots   | planned                                                     |
| `tauri::async_runtime`                        | owned tokio runtime                                       | planned                                                     |

## 9. Definition of done

- [ ] Every one of the 21 production source files is either ported to a Rust
      view-model/UI module or explicitly listed in §7 with a behaviour owner.
- [ ] Differential harness green over the recorded corpus, with counts reported.
- [ ] All 99 IPC call sites have a Rust counterpart in the view model.
- [ ] `src/` and the Vite renderer are deleted; the app runs as a native binary.
- [ ] Tauri webview dependencies removed from the shipped binary; the backend
      modules compile as a plain Rust library.
- [ ] All gates green: `npm test`, `npm run typecheck`, `npm run lint`,
      `npm run audit:tokens`, `npm run lint:rust`, `npm run test:rust`.
- [ ] `PORTING.md` updated with final verification, including what could not run.
