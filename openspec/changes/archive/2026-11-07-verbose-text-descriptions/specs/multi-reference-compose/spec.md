# Spec Delta

## REMOVED Requirements

### Requirement: Field-level description selection

**Reason**: Descriptions are verbose prose; the user edits the text instead of
toggling structured fields.

## ADDED Requirements

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

## MODIFIED Requirements

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
