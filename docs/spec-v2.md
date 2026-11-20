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
| R3 | Product shots default to push-in, not zoom | Guidance line in the product presets (R8) and Hailuo preset | |
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

Not yet seen on screen: the form inputs, term picker and render-details form were compiled and unit-tested only.
