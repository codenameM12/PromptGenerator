# Design

## Describer prompt

`DESCRIBE_SYSTEM` becomes:

```
You are a precise, verbose visual describer. Write detailed prose that a
prompt writer can turn into an image or video prompt. Cover the subject and
its expression, composition and framing, setting, lighting, color palette,
camera and lens character, and style. For video also describe camera motion,
subject motion, and how the scene changes over time. For edit inputs state the
intended change and what must be preserved. Never return JSON, Markdown, or
bullet lists: write 3-6 connected sentences. Say when something is not visible
instead of inventing it.
```

No `json_schema` in `ChatRequest`. `max_tokens` rises to 1200. A response
shorter than 40 characters triggers one retry with the previous text and a
short instruction; a second short response fails with `invalidResponse`.

## Contract and storage

`Description { id, mode, text, cached, model }` replaces the
`json`/`confidence` fields. Storage keeps the `descriptions.json` column
holding the JSON-encoded string (`serde_json::to_string(&text)`), so no
migration is needed and old rows still parse:

- legacy object → `flatten_legacy_description` renders `Subject: …`,
  `Composition: …`, etc. into prose;
- legacy string → used verbatim;
- cache read always yields `text`.

`confidence` is written as null for new rows and stays in the schema for old
data.

## Compose

`ComposeReference.description` and `ComposeRequest.description` become
`Option<String>`. The compose prompt renders `Visual description:\n{text}`.
`NormalizedReference.summary` is the first ~120 characters of the text.
`params.references[].description` carries the text, and generation
`description_json` stores the JSON string, so rerun and library export keep
working.

## UI

`DescriptionEntry { description, draft, editing }`. The description card shows
the model, cached flag, word count, and the prose in a readable block; **Edit
description** turns it into a textarea with **Save** / **Cancel**. Empty text
is rejected. Compose, batch, and images send `draft.trim()` (or the stored
text). Field checkboxes, JSON validation, confidence warnings, and
field-selection state disappear.

## Compatibility

Stored generations with structured description JSON are still readable:
library rerun flattens legacy JSON to text before composing. Job payloads with
structured `description` cannot deserialize as a string; the job executor
falls back to composing from `existingPrompt`/brief when the description does
not parse, rather than failing the item. (New payloads always carry text.)

## Risks

- Longer describer output increases token usage; the budget module already
  trims low-priority sections.
- Users lose per-field toggles by design; they can edit prose instead.
