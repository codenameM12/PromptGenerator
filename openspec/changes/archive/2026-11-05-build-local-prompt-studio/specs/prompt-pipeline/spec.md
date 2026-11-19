# Spec Delta

## Purpose

Turns media and human intent into a target-ready prompt through a two-stage local pipeline: a vision describer produces a structured description, and a composer writes the prompt, negative prompt, and parameters expected by the selected preset.

## ADDED Requirements

### Requirement: Prompt modes

The system SHALL provide four modes: Describe (media to structured description), Compose (brief and optional description to prompt), Refine (existing prompt plus instruction to revised prompt), and Batch (dataset to prompts). Media SHALL be optional, so text-only composition is supported.

#### Scenario: Text-only composition

- **WHEN** the user provides a written brief with no media and selects Compose
- **THEN** the composer produces a prompt without invoking the describer

#### Scenario: Describe only

- **WHEN** the user selects Describe
- **THEN** a structured description is produced and no target prompt is composed

#### Scenario: Refine an existing prompt

- **WHEN** the user supplies an existing prompt and an instruction
- **THEN** the revised prompt preserves the original subject unless the instruction explicitly changes it

### Requirement: Prompt workspace

The main window SHALL provide a prompt workspace with a mode selector for Describe, Compose, Refine, and Batch; media and preset selectors; a text-only option; brief and description inputs; global, preset, and per-run guidance; locked phrases; output language; variation count; a seed control; and a cache-bypass control. Compose SHALL be the default surface. The workspace SHALL show the effective guidance before a run and SHALL present the prompt, negative prompt, parameters, warnings, seed, cache state, and result actions together.

#### Scenario: First launch exposes Compose

- **WHEN** the application opens without a restored mode
- **THEN** the Compose workspace is visible in the main window
- **AND** no terminal or separate browser navigation is required

#### Scenario: Compose from media

- **WHEN** the user selects media, a preset, and Compose
- **THEN** the workspace describes the media and shows the structured description for review before composition
- **AND** the prompt, negative prompt, and parameters are shown together when composition completes

#### Scenario: Compose from text only

- **WHEN** the user enables text-only Compose, enters a brief, and runs
- **THEN** the describer is not invoked
- **AND** the composer result is shown with a copy action

#### Scenario: Mode entry points

- **WHEN** the user opens the mode selector
- **THEN** Describe, Compose, Refine, and Batch are available
- **AND** Describe stops after the structured description
- **AND** Refine requires an existing prompt and an instruction
- **AND** Batch hands selected or dataset items to the job queue

#### Scenario: Guidance conflict

- **WHEN** global, preset, or run guidance contains conflicting instructions
- **THEN** the effective guidance is displayed before generation
- **AND** the conflict is warned about without silently discarding a level

#### Scenario: Result actions

- **WHEN** a composition result is shown
- **THEN** the user can copy the plain prompt, copy the structured result, and regenerate
- **AND** the result indicates whether it came from cache
- **AND** an over-limit result offers shortening or manual editing before acceptance

### Requirement: Structured description contract

The describer SHALL return a structured description containing, at minimum, subject, composition, setting, lighting, color palette, camera and lens, style, visible text, and a confidence indicator. For video it SHALL additionally contain motion, temporal change, and segment summaries. For edit inputs it SHALL additionally contain the intended change and the elements to preserve.

#### Scenario: Image description

- **WHEN** an image is described
- **THEN** all required fields are present and schema-valid
- **AND** empty fields are explicitly marked unknown rather than invented

#### Scenario: Video description

- **WHEN** a clip is described
- **THEN** the description includes camera motion, subject motion, and temporal change across the sampled frames

#### Scenario: Low confidence

- **WHEN** the describer reports low confidence
- **THEN** the result is flagged in the UI
- **AND** the user is invited to edit the description before composition

### Requirement: Composition inputs

The composer SHALL assemble its input from the selected preset, the effective guidance, the structured description or brief, any existing prompt, and any locked elements, and SHALL respect the preset's token and length budget.

#### Scenario: Locked element survival

- **WHEN** the user locks the phrase "red leather jacket" and regenerates three times
- **THEN** every result contains that exact phrase unchanged

#### Scenario: Budget enforcement

- **WHEN** the assembled input exceeds the preset's input budget
- **THEN** the lowest-priority section is trimmed
- **AND** the trimming is recorded with the generation

### Requirement: Edit-target composition

For presets whose kind is edit, the composer SHALL produce an instruction-style prompt that references the source image, states the change, and states what must be preserved. It SHALL NOT produce a full re-description of the source.

#### Scenario: Edit prompt for krea2 edit

- **WHEN** the user describes a change to an image with the krea2 edit preset
- **THEN** the output instructs the target to apply that change
- **AND** the output lists the elements to preserve
- **AND** the output does not narrate the whole scene again

#### Scenario: Multiple references

- **WHEN** the preset supports reference images and the user attaches two
- **THEN** the prompt refers to them by position or role as the preset defines

### Requirement: Guidance precedence

The system SHALL merge global guidance, preset guidance, and per-run notes in that order of increasing priority, display the effective guidance, and warn when instructions conflict.

#### Scenario: Run note overrides preset

- **WHEN** preset guidance says "cinematic" and the run note says "flat lighting"
- **THEN** the effective guidance reflects the run note
- **AND** the conflict is shown before generation

#### Scenario: Empty guidance

- **WHEN** no guidance is provided at any level
- **THEN** composition proceeds using the preset only

### Requirement: Structured output contract

The composer SHALL return prompt text, an optional negative prompt, parameters required by the preset, optional notes, and warnings. The response SHALL be validated against the preset's output schema. On failure the system SHALL attempt repair, then one retry with validation feedback, and finally present the raw response marked invalid.

#### Scenario: Repairable JSON

- **WHEN** the composer returns JSON with a trailing comma
- **THEN** the system repairs and validates it without another model call

#### Scenario: Retry with feedback

- **WHEN** the response is missing a required field
- **THEN** one retry is made that names the missing field
- **AND** the retry count is recorded

#### Scenario: Persistent invalid output

- **WHEN** the retry still fails validation
- **THEN** the raw response is shown and marked invalid
- **AND** the user can copy it or regenerate

### Requirement: Variations and reproducibility

The system SHALL allow one to four candidate outputs per run with distinct sampling, SHALL record the seed used for each, and SHALL allow regenerating with a stored seed and identical inputs.

#### Scenario: Multiple candidates

- **WHEN** the user requests three candidates
- **THEN** three validated outputs are returned as siblings
- **AND** each records its seed and sampling settings

#### Scenario: Reproduce a result

- **WHEN** the user regenerates from a stored generation with the same seed and inputs
- **THEN** the request uses those values
- **AND** any backend-level non-determinism is noted in the result

### Requirement: Result caching

The system SHALL cache descriptions and compositions keyed by content hash plus model, preset version, guidance, and sampling parameters, and SHALL allow the user to bypass the cache.

#### Scenario: Cache hit

- **WHEN** an identical request is repeated
- **THEN** the cached result is returned without a model call
- **AND** the UI indicates that the result was cached

#### Scenario: Cache bypass

- **WHEN** the user chooses regenerate without cache
- **THEN** a fresh model call is made
- **AND** the cached entry is replaced

### Requirement: Stage batching

In Batch mode the system SHALL complete the description stage for all eligible items before starting composition, unless the keep-warm policy makes co-residency safe and the user disables batching.

#### Scenario: Twenty assets

- **WHEN** twenty assets are processed in Batch mode
- **THEN** all twenty descriptions complete before the first composition starts

### Requirement: Self-check

The system SHALL offer an optional self-check that evaluates a generated prompt against a preset-defined checklist and reports issues without silently rewriting the prompt.

#### Scenario: Checklist issue

- **WHEN** the self-check finds no camera or lens information for a video preset that requires it
- **THEN** the issue is listed with severity
- **AND** the prompt text is unchanged until the user applies a fix

### Requirement: Cancellation and partial results

The system SHALL allow cancelling a run, SHALL abort in-flight model requests, and SHALL preserve completed items and any partial description for the interrupted item.

#### Scenario: Cancel a batch

- **WHEN** the user cancels a running batch
- **THEN** no further model calls start
- **AND** completed generations remain in the library
- **AND** the interrupted item is marked cancelled

### Requirement: Output language and length

The system SHALL produce English prompts by default, allow another output language, and enforce the length limit declared by the selected preset.

#### Scenario: Non-English guidance

- **WHEN** the user writes guidance in another language and keeps English output
- **THEN** the prompt is produced in English
- **AND** the guidance is not translated in the history record

#### Scenario: Over length

- **WHEN** the composed prompt exceeds the preset's hard limit
- **THEN** the result is marked over-limit
- **AND** the user is offered a shortening pass or manual editing

### Requirement: Editable description

The user SHALL be able to edit the structured description or the brief after the describe stage and before composition, and recomposition SHALL use the edited text.

#### Scenario: Fix a misread detail

- **WHEN** the describer calls a dog a wolf and the user corrects it
- **THEN** the next composition reflects the correction
- **AND** the original and edited descriptions are both retained
