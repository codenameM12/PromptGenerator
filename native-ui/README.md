# prompt-studio-native

The standalone `eframe`/`egui` native UI that will replace the React renderer at
cut-over. It lives in its own crate so it cannot conflict with the Tauri app or
the in-progress view-model ports under `src-tauri/src/ui/`.

## Build and run

```powershell
# If cargo is not on PATH:
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

cargo build --manifest-path native-ui/Cargo.toml
cargo run --manifest-path native-ui/Cargo.toml
cargo test --manifest-path native-ui/Cargo.toml
```

A debug sidecar-free window opens on **Compose**. All seven rail sections are
present; only the active one is drawn, but every section's state lives in
`view_model::ViewModel`, so navigation never clears inputs, filters, or editor
state.

## Module map

| Module               | Responsibility                                                           |
| -------------------- | ------------------------------------------------------------------------ |
| `main.rs`            | Entry point, single-instance acquisition, eframe startup                 |
| `app.rs`             | `eframe::App` shell: theme install, shortcuts, persistence, panels       |
| `theme.rs`           | The single declared theme + the ad-hoc-colour audit test                 |
| `backend.rs`         | Typed `Backend` trait, source-shaped DTOs, `BackendError`, `StubBackend` |
| `view_model.rs`      | View state and state-preserving navigation                               |
| `views/`             | Rail and the seven section views                                         |
| `shortcuts.rs`       | Shortcut resolution with focus guards; OS reduced-motion detection       |
| `persistence.rs`     | eframe-storage geometry / last asset / draft round-trip                  |
| `single_instance.rs` | Loopback lock and focus-request channel                                  |
| `media_access.rs`    | In-process `MediaAccess`-equivalent allowed-roots guard                  |
| `media_tools.rs`     | Bundled ffmpeg/ffprobe resolver and `std::process::Command` execution    |
| `import.rs`          | Dialog/clipboard → backend → allowed-roots import flow                   |
| `platform/`          | rfd dialogs, clipboard image read, drag-out fallback                     |
| `errors.rs`          | Shared `CommandError`-style message unwrapping                           |

## Current scope and gaps

- The typed `Backend` seam is currently served by `StubBackend`. The real
  `prompt_studio_lib` path dependency compiles cleanly, but the command layer is
  bound to Tauri `State`/`AppHandle`, so a real adapter is still a follow-up.
- Native drag-out has no eframe equivalent: a prepared payload is written under
  the cache and saved to a user-chosen path via a native dialog. Record this as
  an intended divergence in `PORTING.md`.
- Single-instance focus is enforced by a loopback lock; focusing the existing
  window is a best-effort `ViewportCommand::Focus` that cannot be proven in a
  headless session.
