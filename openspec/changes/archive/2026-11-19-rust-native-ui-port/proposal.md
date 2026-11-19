# Proposal: Rust-native UI port

## Why

Prompt Studio's domain logic is already implemented in ~55 Rust modules; the
remaining React 19 / TypeScript renderer is mostly UI plus a thin IPC client.
Maintaining two languages, a webview, a CSP boundary, and an 83-command IPC
surface to reach logic that lives in-process is unnecessary overhead. The user
has chosen a full **replacement** (port-to-rust end state A): one native Rust
binary, with behaviour preserved and proven differentially before the source is
deleted.

## What Changes

- Replace the React/Vite renderer with an **egui/eframe native UI** (rail shell
  plus seven panels and the media editor).
- Port the four client-side logic modules and the state hooks into a Rust
  **view-model layer** under `src-tauri/src/ui/`, driven by a recorded behaviour
  corpus from the TypeScript source. Both implementations run side by side until
  cut-over; no improvements are folded into the port.
- Replace the 83 Tauri IPC commands with **direct in-process calls** to the
  existing backend modules through a typed view-model boundary.
- Reimplement webview platform glue natively: dialogs (`rfd`), clipboard,
  window persistence, single-instance guard, and in-process media loading under
  the existing `MediaAccess` roots. Native drag-out has no drop-in equivalent
  and is treated as a documented intended divergence if unattainable.
- **BREAKING**: remove the webview, React, Vite, the Node toolchain, and the CSS
  token system. The app ships as a native Windows binary.
- Delete the TypeScript source at cut-over (Phase 5) rather than leaving it as a
  dead fallback.

## Capabilities

### New Capabilities

- `native-desktop-ui`: the native Rust shell, view-model parity contract,
  in-process backend access, native platform integration, and cut-over rules.

### Modified Capabilities

- `desktop-shell`: the requirement that the UI be hosted in a web UI behind a
  typed allowlisted bridge is replaced by a native window calling Rust logic
  in-process; the allowlist/CSP contract no longer applies to the renderer.
- `interface-system`: the CSS-token, viewport-screenshot, and web-focus
  requirements are replaced by a native theme/persistence and keyboard contract.

## Impact

- Code: `src/` (41 files) retired; `src-tauri/src/ui/` added; native binary added.
- Dependencies: `eframe`/`egui`, `rfd`, clipboard, single-instance; Tauri
  webview plugins removed from the shipped binary at cut-over.
- Tests: existing Vitest suite becomes the characterization corpus source;
  Rust differential tests become the ongoing parity gate.
- Tooling: `openspec` change drives tasks; `npm` gates remain until cut-over,
  then Rust gates only.
- Docs: `PARITY-CONTRACT.md`, `PHASES.md`, `PORTING.md` are the reviewer-facing
  companions; `AGENTS.md` and user docs updated at cut-over.
