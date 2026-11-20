# Prompt presets

A preset is the contract between the app and one generation target. It declares
the instructions, output schema, legal parameters, limits, and validation
checklist used to compile a prompt. Presets are JSON documents with
`schema: 1`.

A complete, validated example lives in
[`example-preset.json`](example-preset.json).

## Fields

| Field                            | Meaning                                                               |
| -------------------------------- | --------------------------------------------------------------------- |
| `schema`                         | Must be `1`. Import rejects other major versions.                     |
| `id`                             | Kebab-case identifier; must be unique. Built-in ids are reserved.     |
| `name`, `version`, `description` | Human-facing metadata. `version` looks like `1.0.0`.                  |
| `kind`                           | `image`, `video`, or `edit`.                                          |
| `mode`                           | `compose` or `edit`. Edit presets must use `edit` mode.               |
| `systemPrompt`                   | Instructions for the composer model.                                  |
| `guidance`                       | Optional persistent guidance for every run.                           |
| `sections`                       | Description sections rendered into the prompt, in order.              |
| `outputSchema`                   | Optional JSON schema for the composer response.                       |
| `params`                         | Parameter definitions. Types: `enum`, `number`, `integer`, `boolean`. |
| `limits`                         | `promptChars`, `negativeChars`, `maxInputTokens`, `maxOutputTokens`.  |
| `resolutions`                    | Legal output sizes, e.g. `[[1024, 1024], [1344, 768]]`.               |
| `dimensionMultiple`              | 1, 2, 4, 8, 16, 32, or 64.                                            |
| `supportsNegative`               | When false, negative output is dropped with a warning.                |
| `checklist`                      | Terms the self-check warns about when absent from the prompt.         |
| `referenceSlots`                 | Number of reference images an edit target accepts (0–4).              |
| `instructionStyle`               | Marks edit presets that expect imperative instructions.               |

## Adding a target

Most presets are just a prompt guide. Open **Presets**, choose **New**, stay on
the **Guide** tab, and fill in:

- **Name** — becomes the preset name; the id is slugged from it.
- **Kind** — image, video, or image edit. The kind sets the mode, default
  sections, checklist, resolutions, negative support, and reference slots.
- **Target description** — one line shown in the selector.
- **Prompt guide** — the instructions the target expects, in plain text.
- **Prompt character limit** — optional; keeps the target's cap.

**Save** validates the compiled document through the same path as JSON
authoring. Editing an existing user preset keeps its id and bumps the patch
version when the guide changed. Advanced fields the guide does not expose —
parameters, token budgets, examples, output schema — are preserved, except that
changing **kind** resets the kind-specific fields (parameters, sections,
resolutions, negative support, reference slots) so an image preset cannot inherit
video settings.

Switch to **JSON** when you need exact control:

1. Copy `example-preset.json`.
2. Change `id`, `name`, `kind`, `version`, and the prompt/parameter details.
3. Open **Presets** in the left rail, choose **Import**, and select the file.
   Imported presets are always user-owned, so they cannot overwrite built-ins.
4. The editor previews the compiled prompt and reports validation errors.
5. Save, then choose the preset as the generation target on the Compose view.

Built-in presets are immutable. Use **Duplicate** to create an editable copy;
the copy gets a new id and keeps the original untouched.

## Compilation

Compilation merges guidance in increasing priority (global → preset → run),
renders the declared sections from the description, applies parameter defaults,
validates requested values, enforces character limits, and drops unsupported
negative prompts with a warning. Preflight then adds checklist warnings.

## Built-in target limits and sources

| Preset       | Verified limits                                                                                                                                                                                                                                                                                                                                                                                                 | Source                                                                                                                             |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `qwen2-edit` | Positive prompt up to 1300 tokens (longer input is auto-truncated); negative prompt up to 500 characters; 1–3 input images; total output pixels 512x512 to 2048x2048; sizes snap to a multiple of 16. The preset caps prompt characters at 4000 (about 1000 tokens) and keeps negatives disabled (the guidance forbids them; the model itself accepts one, so this is a house choice).                                                                                                                      | Alibaba Cloud Model Studio, Qwen Image Edit API reference: <https://help.aliyun.com/zh/model-studio/qwen-image-edit>               |
| `krea2`      | No published prompt character cap. Aspect ratios: 1:1, 4:3, 3:2, 16:9, 2.35:1, 4:5, 2:3, 9:16. Resolution is 1K only. Style references and moodboards are accepted; the preset allows four references.                                                                                                                                                                                                          | Krea 2 API overview: <https://www.krea.ai/docs/developers/krea-2/overview.md>                                                      |
| `krea2-edit` | Krea has no "Krea 2 Edit" product: Krea 2 is generation-only (style transfer, moodboards, sliders), and **Krea 3** is the current instruction-following edit model with native 2K output. The preset keeps conservative limits and remains a generic edit target; retarget it to Krea 3 before relying on vendor-exact parameters.                                                                              | Krea docs index: <https://www.krea.ai/docs/llms.txt> and Krea 3 overview: <https://www.krea.ai/docs/developers/krea-3/overview.md> |
| `mmh3`       | MiniMax-H3 accepts a prompt of at most 7000 characters, an integer duration of 4–15 s, and outputs 768P or 2K. The preset caps prompt characters at 7000, replaces the fixed 5/10 s list with an integer 4–15 s parameter, drops the undocumented frame-rate option, and disables negatives because H3's request schema has no negative field. Output sizes stay advisory because H3 chooses 768P or 2K itself. | MiniMax video generation guide: <https://platform.minimax.io/docs/guides/video-generation>                                         |
| `mmh3-official` | Full-reference (R2V) mode of MiniMax H3 in the official six-section output format: `subject_definitions`, `summary`, `retention_analysis`, `detailed_description`, `overall_soundscape`, `non_diegetic_music`, with `<Subject N>`/`<Picture N>` labels, task-type prefixes and retention markers. App references become `<Picture N>`; up to four reference slots. | MiniMaxAI/MiniMax-H3 `docs/VIDEO_PROMPT_WRITING_GUIDE_ref_en.md` (<https://huggingface.co/MiniMaxAI/MiniMax-H3>) |
| `mmh3-image` | MiniMax H3 Image text-to-image: one natural-English caption (scene or finished layout), exact quoted text, one aspect ratio from the ten supported ones, no negative prompt. | MiniMax-H3-Image model card (<https://huggingface.co/MiniMaxAI/MiniMax-H3-Image>); written in our own words, not a copy of MiniMax's templates (community licence). |
| `mmh3-image-edit` | MiniMax H3 Image edit: references become `<Picture N>`; the target image is described with kept/borrowed elements, preserved vs changed, and new content; aspect ratio or `adaptive`; up to four references. | Same model card and its documented prompt-expander behaviour. |
| `mmh3-official-image` | Image-to-video (I2VA) from the official H3 video prompt guide: fixed first line referencing `<Picture 1>` at 0.00 s, then the three core fields; one reference slot. | MiniMaxAI/MiniMax-H3 `docs/VIDEO_PROMPT_WRITING_GUIDE_base_en.md` |

When a vendor publishes a new cap, update the preset JSON, bump its `version`,
and record the source here; the content hash reseeds built-ins on the next launch.
