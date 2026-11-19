# Prompt Studio user guide

## 0. Workspace

The app opens on **Compose**. The left rail switches between **Compose**,
**Media**, **Library**, **Jobs**, **Presets**, **Runtime**, and **Datasets**.
The current section is marked in the rail, and the top bar shows LM Studio
health. Sections stay loaded after the first visit, so moving away and back
keeps the selected asset, prompt inputs, filters, and editor state.

## 1. Connect LM Studio

1. Open LM Studio and go to the local server tab.
2. Start the server on `http://127.0.0.1:1234` (the default).
3. Make sure these models exist:
   - `lfm3-vl-2b` (describer)
   - your composer, default
     `gemma4-12b-qat-uncensored-hauhaucs-balanced`
4. In Prompt Studio open **Runtime**. Confirm the endpoint shows _online_, pick
   the describer and composer roles, and press **Load** for each if you want them
   resident before generating.

**Keep warm** controls whether the runtime unloads the other model when roles
switch: `Both`, `Describer`, `Composer`, or `None`. `None` uses less VRAM but
switches models on every stage change. **Unload all** frees everything; active
requests must finish first unless you force it.

The composer is serialized to one request at a time. The describer accepts 1–4
concurrent requests (configurable in **Runtime**), and extra work waits in the
job queue rather than failing.

Model cards show what LM Studio reports: context length, maximum output,
quantization, and **Vision** / **Text only** / **Vision unknown** badges.
**Suggest roles** ranks a vision-capable model first for the describer and the
largest loaded context for the composer; it only fills the selects, so choose
**Save roles** to apply them. When the describer is known to be text-only, or the
composer's reported context is smaller than the preset input budget, the
workspace warns before a run but still allows it. Reported context is used as the
budget when it is smaller than the preset's `maxInputTokens`; the generation
parameters record which value was used.

**Model instructions** edits the system prompt for each role. A describer override
replaces the built-in "describe this media" instructions; a composer override
replaces the selected preset's target instructions. The pipeline still appends its
required JSON shape, length limit, output language, guidance, and locked phrases,
so the response stays parseable. Leave a field empty and save to restore the
default. Overrides apply to every run — Describe, Compose, Refine, batch, and
queued jobs — and changing one invalidates cached results from the old
instructions.

## 2. Import media

Open **Media** in the rail.

- Drag files or folders from Explorer into the window.
- Or use **Add files**, **Add folder**, or **Paste image** in the Media toolbar.

Media is hashed with BLAKE3, deduplicated, probed with `ffprobe`, and given a
cached thumbnail plus a preview proxy when the codec is not web-friendly.
Videos show fps, resolution, duration, and audio state. Missing source files stay
visible with an offline marker; actions that need the source are disabled.

WebP, GIF, BMP, TIFF, and AVIF images import normally. Because the vision model
expects PNG or JPEG, the describer converts them once to a cached PNG before
sending; the original file is untouched.

Hover a media card to reveal **Remove**, or use the remove action on a Compose
tile. Removing moves the asset straight to the Library trash without a
confirmation popup: it leaves the Media view and
every picker, existing prompts keep their text but lose that reference, and a
restore from **Library → Trash** brings it back. The original file on disk is
never deleted or modified. Re-importing a removed file restores the same asset
instead of creating a second copy.

## 3. Edit media

Select a media card in **Media** to open the editor. A quick-action row above the
editor jumps to the tool you need: **Crop / trim edges** for images, **Crop /
aspect** and **Trim clip** for video, and **Mask regions** for both. The actions
only move the view; edits stay staged until you apply them.

The editor is non-destructive. Toolbar changes are staged until you apply them:

- crop (free or fixed aspect), rotate, flip, fit/fill/pad
- video trim (start/end milliseconds), loop, remove audio, constant frame rate
  (24/25/30/60)
- mask brush/eraser/rectangle, hardness, feather, invert, polarity
- target resolution from the preset table, dimension-multiple snapping, manual
  override
- undo, redo, reset

**Masking** works for images and video. The source is shown under the mask layer,
so you paint over the actual picture rather than a blank canvas. Paint the region
to keep or remove, invert the polarity when your target expects the opposite
convention, then **Export mask** to write a grayscale PNG at the target
resolution. For a clip the mask applies to the whole duration. Typical uses:
isolate a product from its background, protect a face while restyling the rest,
or give an edit target a precise region instead of a sentence.

**Crop** uses percentage edges (Left, Right, Top, Bottom) plus **1:1**, **16:9**,
and **9:16** presets. You can also drag a rectangle directly on the preview: the
live dashed box becomes the crop selection when released, and the sliders and
readout follow. Values are clamped to the source so an impossible crop cannot be
submitted. Pixel entry is not required.

**Trimming** applies to video. Set a start and end to drop a shaky intro or a
dead tail, optionally set a constant frame rate, loop a short clip, or remove
audio before composing. The original file is never changed; the edit is written to
`edited/` when applied and can be reverted with **Reset**.

Applying an edit writes a new file to `<source parent>\edited\`. The original is
never modified. Collisions get numeric suffixes; a configured alternate output
root is used when set. If Windows denies writing to the folder, the error names
it and explains the two ways out: choose another output folder, or allow the
app in Windows Security → Ransomware protection. Every result is re-probed and
material mismatches are warned about.

Masks are exported as grayscale PNGs at the target resolution and polarity.

## 4. Presets

Pick a preset for the selected target. The six built-ins are **generic image**,
**generic video**, **krea2**, **krea2 edit**, **qwen2 edit**, and **mmh3**.

Open **Presets** to:

- write a preset as a **text guide** — name, kind, target description, prompt
  guide, and an optional character limit — and let the app compile the document
- switch to **JSON** for parameters, resolutions, references, and exact schemas
- preview the compiled prompt live while you edit either view
- duplicate a built-in (built-ins cannot be edited or deleted in either view)
- create, import, export, or delete user presets
- see validation errors, unsupported negatives, truncation, and checklist
  warnings

The **Guide** fields become the target's instructions; saving bumps a user
preset's patch version and leaves advanced fields intact. Anchor to JSON when you
need parameters, reference slots, or a custom output schema.

See [presets.md](presets.md) for the schema and a validated example.

## 5. Generate prompts

The **Prompt workspace** is the default surface. Its mode selector has four modes:

- **Describe** — media to a verbose text description, no target prompt.
- **Compose** — brief and optional description to a prompt; text-only is supported.
- **Refine** — an existing prompt plus an instruction to a revised prompt.
- **Batch** — queue selected assets or a dataset through the job queue.

For media, Describe runs first so you can review the verbose prose description.
**Edit description** opens the text for correction, and the corrected text is
what Compose uses. Describe mode stops after the description. Compose mode
requires that review step before it enables the compose action. Text-only
Compose skips the describer.

**Media** sits in the panel beside the form. It lists every imported asset with
a thumbnail and supports multiple references:

- Click a tile to toggle it. The number badge and **Selected** label show its
  reference order; the order is the order sent to the model.
- Each selected tile has a **Weight** field below it (`0`–`2`, step `0.05`,
  default `1`). Weights are clamped and the app warns above four references.
- **Select all** and **Clear** manage the set. **Edit media** opens the first
  selected asset in the Media editor.
- Describe runs once per selected asset that has no description. Compose stays
  blocked until every selected asset has a reviewed description.

Each description is a detailed paragraph. Review it, correct any misread
detail with **Edit description**, and **Save description**; the composer
receives that text for the reference. Clearing the text is rejected so a
reference cannot silently lose its description. Long descriptions are trimmed
to the preset's input budget.

When a result exists, the **Result** section moves to the top of the output
column so the prompt, metadata, and copy actions are visible immediately. The
prompt lists every reference with its index, name, weight, and a short summary,
and the same list is stored in the generation parameters.

**Send media to composer** (Compose mode, only while media is selected) also
feeds the selected reference images to the composer itself, not just their
descriptions: images arrive as additional image parts after the prompt text.
Video references contribute one sampled frame and edited references use the
saved edit output. The switch is enabled only when the composer model reports
vision capability; it is disabled while the composer is text-only, and Reset
turns it off. Image runs use their own cache entries, so image and text-only
results never get mixed up.

When an asset has an applied crop or mask, the Compose tile previews that edit
with an **Edited** badge, and Describe and Compose send the edit identifier so
the prompt is built from the edited output. The tiles update as soon as you
press **Apply** in the Media editor.

Drop files on the workspace to import them without leaving it, or use **Add
media**.

**Reset** clears the current result, descriptions, brief, refine inputs, locked
phrases, per-run guidance, media selection, and batch selection so you can start
over. The chosen preset, global guidance, and the imported library stay, and
nothing is removed from history. The button is disabled when the workspace is
already empty.

The workspace also shows the effective guidance (global + preset + run) before
running and warns when levels appear to conflict. Locked phrases are preserved,
output language and 1–4 variations are selectable, and a seed plus cache-bypass
toggle are available. Results show prompt, negative prompt, parameters, warnings,
seed, model, and a `fresh`/`cached` badge. Use **Copy plain prompt**, **Copy
structured**, **Regenerate**, or **Edit prompt**; over-limit results flag the
excess and allow manual correction before copying.

Refine can start from a stored generation; the result is stored as its child.
Batch describes each selected asset before queuing one job with one item per
asset. The dataset tab queues `datasets.run` instead. Progress appears in the
workspace and in **Jobs**.

Outputs are validated, JSON is repaired once with feedback, and a failed repair
is stored as an `invalid` generation with the raw response intact.

- Variations 1–4 produce sibling generations with distinct seeds.
- Locked phrases are appended if the model omits them.
- Character limits slice the result and add a warning.
- Presets without negative-prompt support drop it with a warning.
- Output language is configurable; the prompt is still limited by preset length.

## 6. Job queue

Every long compose/dataset batch is a persisted job. Open **Jobs** to:

- see running, queued, waiting, failed, interrupted, and finished jobs
- pause/resume the queue (running items finish; no new items start)
- cancel a job or cancel everything still waiting
- retry failed/interrupted jobs; only unfinished items rerun
- reorder waiting jobs
- expand item errors and copy full details for a bug report
- see each item's ordered media references with a thumbnail and weight; missing
  assets stay listed with an unavailable marker

Compositions run one at a time because the composer is serialized. If the app
exits unexpectedly, the next launch marks running jobs as interrupted and keeps
their completed items. Resume with **Retry**.

Retryable backend/network failures retry automatically with backoff (three
attempts). Missing inputs, validation failures, and unsupported requests fail
immediately and require a manual retry after fixing the input.

### Worked example: prompts for 40 product photos

Goal: one target-ready prompt per photo, reviewed in the Library, exported as CSV.

1. **Import** the folder: Media → **Add folder**. All 40 images appear once
   (duplicates are skipped by hash).
2. **Pick a target** in Compose, for example `krea2` for stills. Open the Batch
   tab and choose **Assets**.
3. **Select the photos.** Use **Select all**, or click individual tiles. The
   selection is the batch input; per-tile weights still apply.
4. **Queue the batch.** The app describes each selected asset first, then composes
   one prompt per asset with one job item each. You do not need to describe
   manually first.
5. **Watch Jobs.** Progress shows completed, failed, and waiting counts. You can
   pause the queue, cancel waiting items, or leave it running. The composer is
   serialized, so 40 items run one at a time by design.
6. **Fix failures.** Expand the job to see the error per item. After correcting a
   missing file or a bad preset, press **Retry**; only unfinished items rerun.
7. **Review and export.** Results land in the Library. Filter by the batch's
   preset or date, then select all and **Export** as CSV or JSONL. Plain text
   writes one prompt per line.

For a spreadsheet instead of photos, use **Datasets**: import the CSV, map the
caption column to prompt and any media column to media path, then **Run batch**.
The row table shows imported or missing media, failures stay isolated per row, and
export adds the media asset ID, generation ID, and status alongside your original
columns.

## 7. Library

Open **Library** to browse every generation, including failed and cancelled ones.

- Search uses FTS over prompt, negative, guidance, and description text.
- Filter by preset, kind, status, tag, collection, favorite, or date.
- Sort by newest, oldest, rating, or prompt.
- Favorite generations and media independently.
- Add tags, collections, notes, and 1–5 ratings.
- **Versions** shows ancestors and descendants; **Diff** compares prompt,
  negative, parameters, guidance, seed, preset, models, and status.
- **Compare** opens two to four selected generations side by side, including
  unrelated presets. Each column shows the prompt with changed words highlighted,
  models, seed, reference thumbnails and weights, and **Copy prompt**. Compare is
  disabled outside two to four selections.
- Generation details list the ordered references with a thumbnail and weight;
  unavailable assets are marked missing.
- **Restore inputs** loads the asset, edit stack, preset, guidance, and parameters
  back into the workspace without changing history.
- Bulk actions: favorite, tag, assign a collection, re-run with another preset,
  export, trash.
- **Trash** hides items without deleting them. **Restore** brings them back.
  **Delete forever** requires confirmation and removes cached thumbnails, proxies,
  and frames but never source files.
- **Back up** writes a single `.zip` bundle containing the database, settings,
  the encrypted token, and the workspace draft. **Restore** accepts that bundle or
  a legacy `.db` backup and snapshots the current database first. Restart Prompt
  Studio after restoring to apply settings and the token. A token is DPAPI-bound to
  the Windows user and machine, so a bundle moved to another machine cannot decrypt
  it; the app treats it as absent and asks for the token again.

Exports support JSON, JSONL, CSV, and plain text. Plain text writes one prompt per
line and nothing else. Import accepts this app's JSON/JSONL/CSV export and
plain-text prompt lists.

**Re-run** queues one job per selected generation, links the new result to the
source as a child version, bypasses the cache, preserves the ordered references
and weights, and uses the stored description or prompt. The preset, seed, and
variations can change without losing the references.

## 8. Datasets

Open **Datasets** and choose **Import file** or **Import folder**.

Supported inputs:

- TXT: one prompt per line; blank/whitespace lines are skipped
- JSON: array of objects
- JSONL/NDJSON: one object per line
- CSV/TSV: header row required; delimiter detected automatically
- image folder: images paired with same-named `.txt` captions; images without a
  caption are listed as incomplete

The wizard previews the first rows and auto-maps likely fields to prompt,
negative, parameters, media path, tags, identifier, or ignore. Change any mapping
before importing. Save the mapping as a named profile to reuse it.

Validation reports row numbers and reasons. Valid rows import while invalid rows
are listed. UTF-8 is used when possible; invalid UTF-8 falls back to
Windows-1252 and the encoding is shown. Oversized files are refused before fully
loading; split them first. Media paths are resolved against the dataset root;
paths escaping that root are rejected. Formula-like cells are stored and exported
as text and never evaluated.

Duplicate handling:

- **Skip** — keep the first row only
- **Merge** — keep the first prompt and merge tags/parameters/missing fields
- **Keep** — import both with distinct row IDs

**Run batch** uses one job item per unfinished row and records queued, running,
succeeded, and failed counts. Succeeded rows are skipped on resume. Rows with a
media path import that media lazily into the asset library (hash deduplicated)
and compose with it as a reference; rows without media run as text-only compose.
A row may list several media paths separated by `|`, with an optional `@weight`
per path, for example `subject.png@0.8|style.png`. Weights default to 1.0 and
clamp to 0.0-2.0.

The row table shows each row's media state before and after a run:

- **None** — text-only row
- **Not imported** — a media path is present but not yet imported
- **1 reference** / **N references** — imported media with thumbnails
- **Missing** — the declared file is gone

A missing or unreadable row media marks only that row failed and records the
reason; other rows still run. Fix the file or mapping and run again to retry the
unfinished rows.

**Export** writes the original row fields plus media asset ID, generated prompt,
negative, parameters, status, error, and generation ID. Tick the columns you
want; the selection is remembered for that dataset. CSV, JSON, and JSONL are
supported.

## 9. Drag out and Save as

Select a generation, choose the payload, and drag the **Drag** handle into
Explorer:

- plain prompt (`.txt`)
- structured JSON (`.json`)
- original media
- edited media
- mask PNG
- bundle (media + prompt + JSON)

Enable **Sidecars** to add `.txt` and `.json` files with the same base name as a
media file. Names come from the naming template (`{preset}`, `{target}`,
`{asset}`, `{date}`, `{time}`, `{index}`, `{seed}`), with Windows-illegal
characters replaced and collisions suffixed. Dragging copies; sources are never
moved or renamed.

If native drag is unavailable, use **Save as** (native save dialog) or
**Reveal** (Explorer with the file selected). Preparation progress appears for
large clips; preparation can be cancelled.

Configure the naming template in settings. Prepared drag files live under the
cache and are removed after 24 hours.

## 10. Backup, diagnostics, and storage

- **Library → backup/restore** writes or restores a `.zip` bundle containing the
  database, settings, encrypted token, and workspace draft; a legacy `.db` backup
  is still accepted. Restore snapshots the current database first. See §7 for the
  portability caveat.
- **Diagnostics** exports a redacted zip. Tokens and prompt bodies are excluded
  unless you explicitly opt in.
- The top bar shows endpoint health, loaded models, and cache size; the rail
  foot shows portable/per-user mode and data size.
- Portable mode is enabled by `portable.txt` beside the executable or
  `PROMPT_STUDIO_PORTABLE=1`; data goes to `.\data`.

## 11. Keyboard and workspace behavior

The workspace autosaves the selected asset, edit stack, preset, guidance, and
parameters. Relaunching restores an unapplied edit draft. A corrupt draft is
backed up as `workspace.corrupt.json` and ignored.

## Keyboard

- **Ctrl+Enter** — run the active mode. In Compose with media it describes the
  selected media first, then the next press composes.
- **Ctrl+Shift+Enter** — describe the selected media.
- **Esc** — cancel the active describe or compose.

Shortcuts are ignored while a modal dialog is open, and while an input or select
has focus. `Ctrl+Enter` works from a textarea, so you can submit without leaving
the field.
