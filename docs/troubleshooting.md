# Troubleshooting

## LM Studio is offline

**Symptoms:** status shows _offline_, runtime panel says no models, jobs wait or
fail with `network`.

1. Start the LM Studio local server on `http://127.0.0.1:1234`.
2. Check the endpoint in **Settings**; only local HTTP endpoints are accepted.
3. If a token is required, set it in settings. The token is DPAPI-encrypted in
   `token.bin` and never written to `settings.json`.
4. Press **Refresh** in the runtime panel.

The app never sends requests to the internet.

## Model not found

The error includes closest model ids. Rename the role to match LM Studio exactly,
or load the model in LM Studio and refresh. Both roles default to
`lfm3-vl-2b` and `gemma4-12b-qat-uncensored-hauhaucs-balanced`.

If a configured role model is missing from the available list, the compose
workspace warns before the run and **Suggest roles** offers the closest
capability-ranked alternatives. Capability badges come from LM Studio's reported
fields only; when a field is absent the model shows **Vision unknown** rather than
guessing from the name.

## Context overflow

The request exceeded the loaded context window. Lower the preset's
`maxInputTokens`/`maxOutputTokens`, use fewer sampled frames, or load the model
with a larger context. The error is marked non-retryable; retrying the same
request will fail again.

## Compose returns invalid JSON

The pipeline repairs trailing commas and retries once with validation feedback.
If it still fails, the generation is stored as `invalid` with the raw response
and warning. Inspect it in the library and retry manually after adjusting the
preset or guidance.

If the raw response is empty, the composer model is likely reasoning-capable and
spent the whole output budget on hidden reasoning. Current builds send
`reasoning_effort: "none"` on every chat request so the JSON lands in `content`.
Older builds should be updated, or reasoning can be turned off for the composer
model in LM Studio. Keep `maxOutputTokens` large enough for the JSON, not the
reasoning trace.

## Generation is slow or VRAM is full

- Use **Keep warm = None** and **Unload all** when switching workloads.
- The composer is serialized by design; queue length is not a bug.
- Lower context length or GPU offload in the runtime panel.
- Close other GPU applications before loading a 12B composer.

## Avast quarantines the portable exe

**Symptoms:** double-clicking `Prompt Studio.exe` stops the app and the file
disappears. Avast shows a chest entry for `Prompt Studio.exe` with
`IDP.HELU.PSD11`.

This is a behavior-shield false positive on a freshly built unsigned executable.
It also blocks recreating that exact filename. The portable **zip** carries a
working copy named `PromptStudio-new.exe`; the extracted portable folder currently
has no executable until an Avast exception exists. Use the desktop **Prompt Studio
App** shortcut meanwhile — it launches the release build directly.

To use the original name:

1. Add an Avast exception for the portable folder
   (`...\dist-portable\Prompt Studio`) in **Menu > Settings > General >
   Exceptions**.
2. Open **Protection > Quarantine**, select `Prompt Studio.exe`, and choose
   **Restore** (add the exception when prompted).
3. If the name is still blocked, rebuild and copy the exe again after the
   exception exists.

Submit the file to Avast as a false positive so future builds are not flagged.

On this machine Avast also quarantined a renamed copy in the portable folder. The
build-output executable
`src-tauri\target\release\prompt-studio.exe` is on Avast's exception list and
runs. A `portable.txt` beside it and a `data` junction to
`dist-portable\Prompt Studio\data` keep the same database. The desktop
**Prompt Studio App** shortcut launches that executable. Use it until the
portable folder has an Avast exception.

## Rebuild says `Access is denied`

A previous `prompt-studio.exe` is still running. Close the app or stop the
process, then rebuild. Incremental-cache warnings from Cargo are harmless; the
build still succeeds.

## Blank window in development

- Confirm Vite is serving `http://localhost:1420`.
- Install the WebView2 runtime.
- If `npm run tauri dev` fails to open a window after building, start
  `src-tauri\target\debug\prompt-studio.exe` directly while Vite runs.

## Sidecars or ffprobe missing

Both binaries must be present with the Tauri target triple suffix:

```
src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe
src-tauri/binaries/ffprobe-x86_64-pc-windows-msvc.exe
```

Debug builds write the check result to
`%TEMP%\prompt-studio-sidecar-check.txt`. Release builds resolve the binaries
next to the executable; reinstall or re-copy them if missing.

## Edited file is not in the expected folder

Edits go to `<source parent>\edited\` unless an alternate output root is set. A
collision gets ` (1)`, ` (2)`, and so on. Originals are never modified. If the
source is on a read-only or disconnected drive, the edit fails and no partial
file is left behind.

## Drag-out does nothing

Native drag uses `tauri-plugin-drag`. If the OS/WebView2 combination rejects it,
the UI reports the failure. Use **Save as** or **Reveal** instead. Prepared files
are under the cache `drag/` folder and are cleaned after 24 hours. A missing
source cancels preparation with a clear message; cached results remain available.

## Dataset import rejects rows

- Every row needs a mapped prompt.
- Media paths must stay inside the dataset root; `..` escapes are rejected.
- Invalid UTF-8 falls back to Windows-1252; the preview shows which encoding was
  used.
- Files larger than 256 MB are refused before loading. Split them.
- Blank TXT lines are skipped, not treated as empty prompts.
- CSV/TSV requires a header row.

## Dataset run seems stuck

Check **Jobs**. Compositions are serialized, so a 200-row batch runs one at a
time. Succeeded rows are skipped on resume. If the app exited mid-run, open the
job, press **Retry**, and only incomplete items run.

## Library search returns nothing

FTS tokens are matched as quoted whole words. Try fewer terms. Trash items are
hidden unless the Trash view is active. Check the status filter and active
collection/tag filters.

## Restore-to-workspace refuses

The generation has no source asset (for example, a text-only compose). Text-only
results can be copied from the library, but there is no media/edit stack to
restore.

## Database is from a newer build

The app refuses to open a database written by a newer binary and leaves it
untouched. Install the newer build or restore a backup made by this version.
Pre-migration backups are written automatically before upgrades.

## Portable data is not beside the executable

Portable mode requires `portable.txt` next to the executable, or
`PROMPT_STUDIO_PORTABLE=1`. If that folder is not writable, the app falls back to
per-user data and reports the reason in the status panel.

## Collect diagnostics

Use **Diagnostics** to export a redacted zip, then attach it to a bug report.
Tokens and prompt text are excluded by default. The zip contains app/OS versions,
settings with secrets removed, database summary, and recent errors.
