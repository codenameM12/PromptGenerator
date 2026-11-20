# Prompt Studio V2 requirements spec

Status: agreed 2026-11-20 after a design interview. Source: the "Prompt Studio
Feature Requirements" doc (review-board rounds 1-2) plus the decisions below.
Branch: `V2`. Build state is tracked in the last table.

## Scope line
Prompt Studio writes prompts (LM Studio); it does not render. Render-side facts
are recorded by hand, never produced by the app. No ComfyUI or MiniMax runners.

## Requirements

| ID | Requirement | Decision | Notes |
|---|---|---|---|
| R1 | Hailuo bracket camera syntax with a menu of named combinations | Separate `hailuo` preset; H3 presets unchanged | Blocked on checking the syntax against MiniMax API docs (the source is a vendor blog) |
| R2 | One camera move per clip, static default, pinned lighting | Lives only in the Hailuo preset | Conflicts with H3 multi-shot cuts, so not applied to `mmh3*` |
| R3 | Product shots default to push-in, not zoom | Push-in rule in `mmh3`, `mmh3-official`, `mmh3-official-image`, `hailuo` and `product-macro` | |
| R4 | Style reference with strength; brand moodboard | Strength = existing reference weight (0-2). Moodboard deferred | |
| R5 | Product photo templates (product, audience, setting) | Built-in presets with a labelled-input form | Needs the brief-fields schema addition |
| R6 | Multi-shot character consistency | Not needed; closed | Existing `mmh3-official` R2V preset already covers labelled references |
| R7 | Negative prompt only where the target supports it | Already built: per-preset `supportsNegative` | Facts: H3 Image does not support it (official README); H3 video unconfirmed; Qwen edit and Krea stay hidden by choice |
| R8 | Product macro/material detail template (detail, finish, lens, focus, lighting cue, optional slow reveal) | Built-in preset with form, same mechanism as R5 | |
| R9 | Seed lock for variant sets; store model, sampler and seed per result | Seed and model already stored. Sampler/engine/steps/CFG/notes are typed by the user in Library detail | New table `generation_render_meta` (migration 8) |
| T1 | Term list from DareDev256 `patterns.json` | Pick-list in Compose with category tabs, inserts into the brief. Terms only, no prompt texts. Credits screen carries the MIT notice | Wizard (f002) stays declined |

Declined: f001, f006, f007, f009, f011, f019, f020, f021, f002 (wizard).

## Brief fields (R5, R8)
A preset may declare `briefFields: [{ key, label, placeholder }]`. When the
selected preset has them, Compose shows one labelled input per field instead of
the free brief. The composed brief is `Label: value` lines in field order;
empty fields are omitted. Presets without `briefFields` behave as before.

## Render metadata (R9)
`generation_render_meta(generation_id PK, engine, sampler, steps, cfg, notes)`,
all optional text/number. Commands `library_render_meta_get` and
`library_render_meta_set`. Edited in the Library detail panel.

## Term list (T1)
Bundled `terms.json` (category -> terms) derived from `patterns.json` via the
upstream `scripts/extract-patterns.ts` path, plus `NOTICE-terms.txt` with the MIT
licence and copyright (2026 DareDev256). The picker is a Compose panel.

## Build order and state

| Step | Item | State |
|---|---|---|
| 1 | R7 doc facts | Done in this spec |
| 2 | Brief-fields schema + R5/R8 presets (`product-photo`, `product-macro`) | Built, tests pass |
| 3 | R9 render metadata (migration 8, Library detail form) | Built, tests pass |
| 4 | T1 term pick-list (`native-ui/assets/terms.json`, credits in `NOTICE-terms.txt`) | Built, tests pass |
| 5 | R1/R2 Hailuo preset (`hailuo`) | Built. Verified against the MiniMax API reference: 15 commands, simultaneous in one bracket (max 3 recommended), sequential in separate brackets; supported on Hailuo-2.3, Hailuo-02, T2V-01-Director. H3 is not covered by that page. Named combinations are in the Terms picker under "Hailuo camera" and are sent verbatim as Style terms |

Seed lock (R9): a value in the Compose Seed field fixes the base seed, so a variant set reproduces; blank picks a random one. Seed and model are stored per result.

Verified on screen by the user (2026-11-20): term chips, product forms, render details. Not yet exercised against a live model: the form inputs, term picker and render-details form were compiled and unit-tested only.

## Characters tab (named sets of reference assets)

Agreed 2026-11-21 after a design interview. The tab is labelled "Characters" but
holds any named subject (a person, a product). **The composer prompt never uses
the word "character"**: only the user's own names and descriptions are sent.

### Characters tab
- Create, rename, delete (confirmation; images go with it).
- Fields: name, overall description.
- Images are imported as copies into the app workspace
  (`<data>/characters/<id>/`), not linked to the source file. Each image has a
  display name, a role label (front, profile, expression, ...), an optional
  description, and a "default" flag.
- A per-character link opens Library filtered to generations that used it.

### Using one in Compose
- "Add" picker at the top of the media panel.
- Adding loads the default images as reference thumbnails (weights editable) and
  registers them as normal Media items (deduplicated by file hash), so temporary
  crop/mask edits work and never touch the stored copy.
- No defaults ticked: one random image is picked once when added; the user can
  swap or shuffle it; Regenerate keeps it.
- Several can be added. If loaded images exceed the preset's reference slots,
  Compose refuses to run until the user unticks enough.
- Removing one takes its images out of the reference list only; they stay in
  Media with their edits.

### Descriptions (all media, not only these)
- A description is optional and is never generated automatically. Describe is a
  per-image button plus "Describe missing". Describe mode is unchanged.
- Each image has its own "send description" toggle; a small indicator on the
  Compose thumbnail shows whether a description exists.
- Each added set has an overall "send description" toggle. Compose shows the
  overall and per-image descriptions.
- A temporary edit keeps an authored description and shows an "edited" marker.
- Wording to the composer is neutral: `<imageN>` tags, the user's names, role
  labels and descriptions under a "Reference notes" heading.

### Records
- `characters`, `character_images` tables (migration 9);
  `generation_characters(generation_id, character_id)` links results so Library
  can filter by set.

### Build state (Characters)

| Part | State |
|---|---|
| Tables, import copies, defaults/random pick, link rows (migration 9) | Built, unit-tested |
| Characters tab (create/rename/delete, import, roles, descriptions, defaults, Library link) | Built, compiled and parity-tested; not yet seen on screen |
| Compose: add set, shuffle, remove, send toggles, indicator, Describe buttons, slot blocking, neutral reference notes | Built, unit-tested |
| Library: filter by set | Built |
| Compose no longer auto-describes or demands descriptions | Built (the parity corpus passes) |

### Drops and folders (Characters, Media)
- Characters: "Import images…" and "Import folder…"; folders are scanned recursively for image files (max 500).
- Dropping files or folders on a name in the Characters list (or on the open panel) adds them to that one. Elsewhere on that tab nothing is imported and the footer says where to drop.
- Media: drops import as before, now with a visible hint, and the grid reloads after a drop.

### Runtime: GPU
- Keep warm stays a setting and defaults to None.
- Compose has a "Free GPU" button (force-unloads every model LM Studio holds, including ones loaded outside the app). It is disabled while a run is active.
