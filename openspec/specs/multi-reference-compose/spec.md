# multi-reference-compose Specification

## Purpose
Defines ordered multi-reference composition: selecting several assets, weighting them, describing each one, choosing which description fields reach the composer, and surfacing reference-aware output without a database migration.

## Requirements

### Requirement: Ordered multi-reference selection

The Compose and Describe media picker SHALL allow more than one selected asset. Selection order SHALL define reference order and SHALL be shown as a numbered badge plus an accessible selected label on each selected tile. Clicking a tile SHALL toggle only that tile. The picker SHALL offer Select all and Clear.

#### Scenario: Select two references

- **WHEN** the user selects two assets in order
- **THEN** the first tile shows reference 1 and the second shows reference 2
- **AND** the selected count reflects two references

#### Scenario: Toggle one tile off

- **WHEN** the user clicks an already selected tile
- **THEN** only that asset leaves the selection
- **AND** the remaining references keep their order

### Requirement: Per-reference weights

Every selected tile SHALL show a weight input below it with `min=0`, `max=2`, `step=0.05`, and a default of `1`. The accessible label SHALL include the asset name. The backend SHALL default missing weights to `1.0`, clamp finite values to `0.0-2.0`, and emit a soft warning for out-of-range values or more than four references. Compose SHALL NOT fail because of a weight or reference count.

#### Scenario: Clamp an out-of-range weight

- **WHEN** a request supplies weight `7.5` or `-1`
- **THEN** the normalized weight is `2.0` or `0.0`
- **AND** the generation carries a `reference_weight_clamped` warning

#### Scenario: More than four references

- **WHEN** a request supplies five references
- **THEN** all five are sent with weights
- **AND** the generation carries a `too_many_references` warning

### Requirement: Describe every selected asset

Describe SHALL run for each selected asset that has no description. The panel
SHALL keep one description entry per selected asset, allow each entry's JSON to
be edited, and block Compose until every selected asset has a reviewed
description. When an asset has an applied crop or mask, the describe request
uses that asset's latest applied edit output instead of the raw source, so the
drawn selection is what reaches the describer.

#### Scenario: Describe a fresh multi-selection

- **WHEN** two assets are selected and neither has a description
- **THEN** one describe request runs per asset
- **AND** Compose stays blocked until both descriptions exist

#### Scenario: Describe uses the edited version

- **WHEN** the user describes a selected asset that has an applied crop or mask
- **THEN** the describe request carries the asset's latest edit
- **AND** the description reflects the edited composition

### Requirement: Reference-aware compose contract

`ComposeRequest` SHALL accept an optional ordered `references` list with
`assetId`, `editId`, `weight`, `name`, and text `description`. The existing
`assetId`/`editId`/`description` fields SHALL keep working as the primary
(first) reference so older payloads and stored jobs still deserialize. The
cache key SHALL include the full ordered reference set. Generation
`params.references` SHALL carry index, name, weight, asset/edit ids, the text
description, and a short summary. The finalized prompt SHALL include a
`References:` section listing each reference's index, name, weight, and
summary.

#### Scenario: Old payload

- **WHEN** a compose payload omits `references`
- **THEN** it deserializes with an empty reference list
- **AND** composition proceeds from `assetId` and `description`

#### Scenario: Different weights

- **WHEN** two composes share assets but use different weights
- **THEN** they use different cache keys and both reach the model

#### Scenario: Text description in the prompt

- **WHEN** a reference carries a verbose description
- **THEN** the composer prompt includes that text as a visual description
- **AND** the reference summary is derived from the text

### Requirement: Result-first workspace

When a result exists, the Result section (prompt, metadata, copy actions) SHALL render before the structured descriptions and the media picker in the output column. With no result the media picker SHALL render first. The compose action SHALL be the full-width sticky primary action; re-describe SHALL be a compact secondary action. The right column SHALL stretch to the available height and the media grid SHALL flex and scroll instead of using a fixed viewport cap.

#### Scenario: Compose succeeds

- **WHEN** a generation returns
- **THEN** the prompt is visible at the top of the output column without scrolling

#### Scenario: No result yet

- **WHEN** the workspace has not produced a result
- **THEN** the media picker is the first element in the output column

### Requirement: References resolve to the applied edit

Compose SHALL attach the selected reference's latest applied edit identifier
(`editId`) instead of the source asset when the asset has one, so prompts are
generated from the cropped and masked output. Media tiles in the picker SHALL
preview the applied edit output with an "edited" badge and SHALL refresh after
an apply from the Media view. Assets without an applied edit keep the
established source behavior.

#### Scenario: Picker previews the applied edit

- **WHEN** a selected asset has an applied edit
- **THEN** the Compose tile shows the edited output with an "edited" badge
- **AND** the tile refreshes right after the user applies an edit

#### Scenario: Compose contract carries the edit

- **WHEN** the user composes with a reference whose asset has an applied edit
- **THEN** the request reference carries that edit identifier
- **AND** the generation parameters still record the ordered reference

### Requirement: Editable verbose description

Each selected asset SHALL show its verbose text description with the model and
cached state, and SHALL offer an edit mode with a plain-text editor, Save, and
Cancel. Compose SHALL send the edited text for that reference. Empty text SHALL
be rejected with a clear message. Descriptions SHALL NOT be validated or
displayed as JSON.

#### Scenario: Edit the prose

- **WHEN** the user edits a description and saves
- **THEN** the compose request sends the edited text for that reference
- **AND** cancelling restores the previous text

#### Scenario: Empty description rejected

- **WHEN** the user clears a description and tries to save
- **THEN** the save is rejected with a clear message
- **AND** the previous text stays in place
