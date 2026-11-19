# Prompt Studio

A local-first Windows desktop workbench that turns images and video into prompts
for image and video generation targets. A small vision model describes the media;
a larger local model writes the prompt for the selected target. Nothing leaves
the machine.

> Status: the renderer has been ported to a **native Rust UI**. The React/Vite
> frontend has been removed; `native-ui` is the only launcher. The port was run
> as OpenSpec change `rust-native-ui-port` and its parity evidence is recorded
> in `PORTING.md`, `PARITY-CONTRACT.md` and `PHASES.md`.

## What it does

- Two-stage local pipeline: `lfm3-vl-2b` describes, a configurable composer
  (default `gemma4-12b-qat-uncensored-hauhaucs-balanced`) writes the prompt.
- Target presets for `mmh3`, `krea2`, `krea2 edit`, `qwen2 edit`, plus generic
  image/video. Presets are editable and importable.
- Import and non-destructively edit images and video (crop, mask, trim, fps,
  resolution), generate prompts, and export results with the native drag-out
  fallback.
- Durable job queue with pause/resume, retry, cancel, reorder and crash recovery.
- Prompt library with FTS search, filters, favorites, tags, collections, notes,
  ratings, versions/diffs, trash, backup/restore, and export.
- Dataset import, mapping profiles, dedupe, batch runs and result export.
- Talks only to a local LM Studio instance.

## Prerequisites

- Windows 10/11. No WebView2, Node.js, or browser is required.
- Rust stable with the MSVC toolchain, plus Visual Studio Build Tools with the
  C++ workload and a Windows SDK.
- LM Studio for inference.

Check the toolchain:

```powershell
rustc --version
cargo --version
```

If the commands are missing right after installing Rust, restart the shell so
`%USERPROFILE%\.cargo\bin` is on `PATH`.

## Quick start

1. Clone with Git LFS installed (`git lfs install` once per machine). The bundled
   `ffmpeg`/`ffprobe` sidecars in `src-tauri/binaries/` are stored as LFS objects;
   without LFS they arrive as small text pointer files and media import fails.
2. Start LM Studio's local server on `http://127.0.0.1:1234`.
3. Build and run the native app:

```powershell
cargo build --manifest-path native-ui/Cargo.toml
cargo run --manifest-path native-ui/Cargo.toml
```

4. Import an image or video from the **Media** view, edit if needed, choose a
   preset in **Compose**, and run Describe or Compose. Results are saved to the
   Library and batch runs appear in **Jobs**.

The app stores its database, cache, settings and workspace in the per-user
application data folder, or beside the executable when a `portable.txt` marker
is present. If the portable location is not writable it falls back to the
per-user location.

## Workspace

The main window has a persistent rail with seven sections: **Compose** (default),
**Media**, **Library**, **Jobs**, **Presets**, **Runtime**, and **Datasets**.
Navigation preserves the selected asset, compose inputs, filters and editor
state. The top bar shows the current section and LM Studio health.

Keyboard shortcuts: `Ctrl+Enter` runs Compose, `Ctrl+Shift+Enter` runs Describe,
and `Esc` cancels the active operation. They are suppressed while a modal or a
text field has focus.

## Commands

| Command | Purpose |
| --- | --- |
| `cargo build --manifest-path native-ui/Cargo.toml` | Build the native binary |
| `cargo run --manifest-path native-ui/Cargo.toml` | Run the native app |
| `cargo test --manifest-path native-ui/Cargo.toml` | Native UI + differential tests |
| `cargo build --manifest-path src-tauri/Cargo.toml --lib` | Build the backend library |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Backend tests |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | Rust lint |
| `cargo fmt --all -- --check` | Rust formatting |

Before shipping, run both test suites and the lint gate. `PORTING.md` records
any gate that cannot run cleanly and why.

## Media sidecars

`ffmpeg` and `ffprobe` are bundled and resolved from the executable directory:

```
src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe
src-tauri/binaries/ffprobe-x86_64-pc-windows-msvc.exe
```

See `src-tauri/binaries/FFMPEG-NOTICE.txt` for version and licensing. The build
is GPLv3; redistribution obligations apply.

## Project layout

```
native-ui/           Native egui/eframe application (the launcher)
src-tauri/           Rust backend library, repository, media and pipeline
src-tauri/binaries/  Bundled ffmpeg/ffprobe sidecars
src-tauri/presets/   Built-in target presets
port/corpus/         Frozen behaviour corpus from the port (regression record)
openspec/            Spec-driven change artifacts
docs/                User and operator documentation
```

## Testing and parity

- Backend tests use `cargo test --manifest-path src-tauri/Cargo.toml`.
- Native UI and differential tests use `cargo test --manifest-path native-ui/Cargo.toml`.
- The frozen corpus in `port/corpus/` records behaviour captured from the
  original TypeScript renderer. Rust view-model modules replay it and report
  `N inputs compared, M differences`.
- LM Studio HTTP behaviour is exercised with `wiremock`; the normal suite needs
  no local model server.
- `pipeline::tests::real_models_describe_and_compose` is ignored and needs a live
  LM Studio; the 10k library performance test is also ignored.

## Documentation

- [User guide](docs/user-guide.md)
- [UI system](docs/ui-system.md) — the original CSS token contract, retained as
  historical design context; the native theme is in `native-ui/src/theme.rs`.
- [Preset schema](docs/presets.md)
- [Drag-out spike](docs/drag-out.md) — includes the native fallback rationale.
- [Troubleshooting](docs/troubleshooting.md)
- [Release notes](docs/release-notes.md)
- Port evidence: `PARITY-CONTRACT.md`, `PHASES.md`, `PORTING.md`.
- [Rust port report](docs/rust-port-report.md) — timeline, volume, token/cost estimates.
