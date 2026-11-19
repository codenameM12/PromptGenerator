# Design

## Context

The compose pipeline already receives one `assetId` plus a structured `description`, caches on a content key, persists generations, and finalizes params. The renderer already lists assets in the picker. The change extends those paths without a schema migration.

## Goals / Non-Goals

**Goals:**

- One ordered selection model shared by Describe and Compose.
- Per-reference weights with backend normalization and soft warnings.
- Per-reference description field selection.
- Backwards-compatible IPC: old payloads and stored jobs still deserialize.
- Result-first workspace layout so generated prompts are visible without scrolling.

**Non-Goals:**

- Per-target reference limits beyond the soft four-reference warning.
- Multi-image describe in one model call; descriptions remain per asset.
- A database migration or a new reference table.
- Changing Batch's existing multi-select queue behavior.

## Decisions

### D1: References ride alongside the primary fields

`ComposeRequest` gains `references: Vec<ComposeReference>` with `#[serde(default)]`. `assetId`, `editId`, and `description` remain and mean the primary/first reference, so old callers and persisted job payloads continue to work. The pipeline falls back to the first reference when the primary fields are absent.

### D2: Normalize once, use everywhere

`pipeline::normalize_references` defaults missing weights to `1.0`, clamps finite values to `0.0-2.0`, resets non-finite values, and returns soft warnings. The same normalized list feeds the cache key, the user message, the final `References:` block, and `params.references`, so the four surfaces cannot disagree.

### D3: Deterministic reference block in the prompt

The model is told about references in the user message, but the final prompt appends a deterministic block:

```
References:
1. fox.png (weight 1.00): a red fox
2. wolf.png (weight 0.50): a grey wolf
```

The block is budgeted against the preset prompt limit: the model text is trimmed first so the reference binding survives truncation.

### D4: Field selection lives in the renderer

Each description entry keeps `fields: Record<string, boolean>`. Compose sends `filterDescription(json, fields)` per reference and as the primary description. JSON edits merge selections: unchanged keys keep their state and new keys default to selected. Deselecting `subject` or a preset checklist field only shows a warning.

### D5: Result-first layout

The output column renders `Result` when a result exists, then descriptions, then the media picker. Without a result the media picker stays first. The right column stretches to the available height and the media grid flexes into it with its own scroll, replacing the fixed `40vh` cap.

### D6: Budget multi-description input

`compose_messages` builds budget sections for the brief/existing prompt (priority 0), the primary description (priority 1), and the references summary (priority 2), then calls `runtime::budget::fit_sections` against `max_input_tokens` minus the system prompt estimate. Reference summaries are capped at 120 characters before trimming.

## Risks / Trade-offs

- The appended reference block consumes prompt characters; it is included in the preset limit and trimmed deterministically.
- Old generations do not have `params.references`; consumers must treat the key as optional.
- More than four references are allowed but warned; some models will ignore extras.
