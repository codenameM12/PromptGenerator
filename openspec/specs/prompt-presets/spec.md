# prompt-presets Specification

## Purpose

Defines the data-driven contract for each target generator so the same media and guidance can be compiled into the dialect, parameters, and limits that mmh3, Krea 2, Krea 2 Edit, Qwen Edit, or a user-added target actually expects.

## Requirements

### Requirement: Preset schema

Each preset SHALL declare an identity, a version, a kind of image, video, or edit, the target's system instructions, optional default guidance, an output schema, parameter definitions and defaults, length and token limits, supported aspect ratios and resolutions, required dimension multiples, frame-rate and duration constraints, negative-prompt support, and optional few-shot examples.

#### Scenario: Complete preset accepted

- **WHEN** a preset declares every required field with valid values
- **THEN** it is accepted and available in the target selector

#### Scenario: Missing required field

- **WHEN** a preset omits its output schema
- **THEN** it is rejected with an error naming the missing field
- **AND** it is not offered for generation

### Requirement: Built-in presets

The system SHALL ship versioned built-in presets for mmh3 (video), krea2 (image), krea2 edit (image edit), qwen2 edit (image edit), generic image, and generic video.

#### Scenario: Built-in selection

- **WHEN** the user opens the target selector
- **THEN** all built-in presets are listed with their kind
- **AND** each shows a short description of the output it produces

#### Scenario: Built-in immutability

- **WHEN** the user opens a built-in preset for editing
- **THEN** the system creates a user-owned copy
- **AND** the built-in remains unchanged

### Requirement: Preset authoring

The system SHALL allow users to create, duplicate, edit, rename, and delete user presets, and SHALL validate them before saving.

#### Scenario: Create a target

- **WHEN** the user duplicates generic video and changes its instructions, limits, and resolution table
- **THEN** the new preset is saved with its own identity and version
- **AND** it appears in the target selector

#### Scenario: Delete a preset in use

- **WHEN** the user deletes a user preset referenced by history
- **THEN** the deletion is allowed only after confirmation
- **AND** history records retain the preset identity and version snapshot

### Requirement: Preset import and export

The system SHALL import and export presets as JSON, SHALL reject files that fail schema validation or exceed a size limit, and SHALL NEVER execute content from an imported preset.

#### Scenario: Import a shared preset

- **WHEN** the user imports a valid preset file
- **THEN** it appears in the selector and is marked as user-owned

#### Scenario: Malformed import

- **WHEN** an imported file is not valid JSON or fails validation
- **THEN** the import is rejected with a specific reason
- **AND** no partial preset is saved

#### Scenario: Unsupported major version

- **WHEN** a preset declares an unsupported schema major version
- **THEN** it is rejected with an upgrade-required message

### Requirement: Compilation

The system SHALL compile a generation into the selected target's final form: the prompt text, optional negative prompt, and parameter values, and SHALL offer both a structured copy and a plain-prompt copy.

#### Scenario: Plain prompt copy

- **WHEN** the user chooses "copy plain prompt"
- **THEN** only the prompt text is placed on the clipboard

#### Scenario: Structured copy

- **WHEN** the user chooses "copy structured"
- **THEN** prompt, negative prompt, and parameters are copied in the preset's declared shape

### Requirement: Preflight constraints

Before or immediately after composition, the system SHALL check the result against the preset's constraints and SHALL distinguish blocking errors from advisory warnings.

#### Scenario: Character limit exceeded

- **WHEN** the prompt exceeds the soft character limit
- **THEN** a warning is shown with the actual count and the limit
- **AND** the user may shorten, accept, or cancel

#### Scenario: Illegal parameter

- **WHEN** a requested duration or resolution is outside the target's supported range
- **THEN** the run is blocked with a specific range error
- **AND** the nearest valid value is offered

#### Scenario: Unsupported negative prompt

- **WHEN** the preset does not support negative prompts and one is produced
- **THEN** the negative prompt is retained in history but excluded from the compiled output
- **AND** the user is informed

### Requirement: Duration, frame rate, and aspect constraints

Video presets SHALL declare supported durations, frame rates, aspect ratios, and resolutions, and the system SHALL expose only legal choices in the editor and parameter controls.

#### Scenario: Legal choices only

- **WHEN** a video preset supports 5 or 10 seconds only
- **THEN** the duration control offers 5 and 10
- **AND** an invalid value cannot be selected through the interface

#### Scenario: Imported value out of range

- **WHEN** an imported or restored generation carries an unsupported duration
- **THEN** it is corrected to the nearest legal value or flagged before running

### Requirement: Preset provenance

Every generation SHALL record the preset identity, version, and content hash used. Editing a preset SHALL NOT alter existing history or its compiled output.

#### Scenario: Preset changes after generation

- **WHEN** the user edits a preset after producing prompts with it
- **THEN** old generations still display and re-copy their original output
- **AND** the old preset version remains inspectable

### Requirement: Guidance fields

Each preset SHALL support a persistent human guidance field and optional target-specific guidance, which merge with per-run notes according to the precedence defined by the prompt pipeline.

#### Scenario: Persistent guidance

- **WHEN** the user saves guidance on a preset
- **THEN** every run with that preset includes it
- **AND** the user can override it per run without editing the preset

### Requirement: Preset update guidance

When a built-in preset changes version, the system SHALL notify the user and SHALL leave user copies and prior generations untouched.

#### Scenario: Built-in update available

- **WHEN** the application ships a newer built-in preset version
- **THEN** the selector marks it as updated
- **AND** the user may switch to the new version or keep the previous one

### Requirement: Text-first preset guide

The preset editor SHALL offer a guide form that accepts a name, a kind, a target description, a prompt guide written as plain text, and an optional prompt-character limit. Saving the guide SHALL compile a schema-valid preset document by supplying defaults for fields the guide does not cover, SHALL run the same validation as JSON authoring, and SHALL create or update a user preset. The raw JSON editor SHALL remain available and SHALL operate on the same stored document, and built-in presets SHALL stay read-only in both views.

#### Scenario: Create a preset from a text guide

- **WHEN** the user enters a name, chooses a kind, writes a prompt guide, and saves
- **THEN** a user preset is created with its own identity and version
- **AND** it appears in the target selector
- **AND** its compiled instructions contain the guide text

#### Scenario: Guide and JSON stay consistent

- **WHEN** the user saves a guide and then opens the JSON view
- **THEN** the JSON shows the compiled document that was validated and stored
- **AND** editing that JSON and saving updates the stored preset

#### Scenario: Incomplete guide is rejected

- **WHEN** the user saves a guide with an empty name or empty guide text
- **THEN** the save is rejected with a specific message
- **AND** no preset is created or overwritten

#### Scenario: Built-in preset is protected

- **WHEN** the user opens a built-in preset
- **THEN** the guide fields are read-only
- **AND** duplicating it produces an editable user preset
