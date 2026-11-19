# Spec Delta

## Purpose

Keeps every description and prompt with its full provenance, and makes the library searchable, organizable, restorable, versionable, and exportable so good prompts are not lost.

## ADDED Requirements

### Requirement: Generation records

The system SHALL persist each generation with its identifier, timestamps, source assets and edit references, preset identity and version, models used, guidance, structured description, final prompt, negative prompt, parameters, seeds, token counts, latency, status, and error where applicable.

#### Scenario: Failed generation recorded

- **WHEN** composition fails validation
- **THEN** the record is saved with failed status, the raw response, and the error
- **AND** it remains discoverable in history

#### Scenario: Provenance completeness

- **WHEN** the user opens a stored generation
- **THEN** the exact preset version, models, guidance, and seeds are visible

### Requirement: History browsing

The system SHALL provide a paginated history view with free-text search and filters for target, model, kind, date range, tags, favorites, and status, with selectable sorting.

#### Scenario: Search by content

- **WHEN** the user searches for a phrase that appears only in the description
- **THEN** matching generations are returned
- **AND** the matched field is highlighted

#### Scenario: Filter by target and date

- **WHEN** the user filters to krea2 edit generations from the last seven days
- **THEN** only those records are listed

#### Scenario: Large library

- **WHEN** the library contains ten thousand generations
- **THEN** the first page renders within 300 milliseconds on the reference machine

### Requirement: Favorites

The system SHALL allow marking both generations and media assets as favorites, and SHALL provide a favorites filter for each.

#### Scenario: Favorite a generation

- **WHEN** the user marks a generation as a favorite
- **THEN** it appears under the favorites filter
- **AND** the state persists across restarts

#### Scenario: Favorite an asset

- **WHEN** the user marks a source asset as a favorite
- **THEN** it can be filtered independently from generation favorites

### Requirement: Organization

The system SHALL support tags, collections, free-text notes, and a rating for generations.

#### Scenario: Tag and collect

- **WHEN** the user adds the tag "neon" and assigns a generation to the collection "city spots"
- **THEN** both are searchable and filterable

#### Scenario: Rename a collection

- **WHEN** the user renames a collection
- **THEN** its members are unchanged
- **AND** the new name appears immediately in filters

### Requirement: Version lineage

The system SHALL link regenerations and refinements to their parent generation, SHALL present the resulting version tree, and SHALL provide a side-by-side difference between any two versions.

#### Scenario: Regenerate creates a child

- **WHEN** the user regenerates a stored generation
- **THEN** the new result is stored as a child
- **AND** the original remains unchanged

#### Scenario: Compare versions

- **WHEN** the user selects two versions
- **THEN** the differences in prompt, negative prompt, and parameters are shown

### Requirement: Restore to workspace

The system SHALL restore a stored generation's inputs, including media, edit stack, preset, guidance, and parameters, into the workspace for further work.

#### Scenario: Restore inputs

- **WHEN** the user chooses "restore inputs" on a generation
- **THEN** the workspace loads the same asset, edit, preset, and guidance
- **AND** the existing generation is not modified

### Requirement: Deletion and recovery

The system SHALL move deleted generations and assets to a trash state, SHALL allow restore from trash, and SHALL require confirmation for permanent deletion and bulk deletion.

#### Scenario: Soft delete

- **WHEN** the user deletes a generation
- **THEN** it is removed from normal views but retained in trash
- **AND** it can be restored

#### Scenario: Permanent delete

- **WHEN** the user permanently deletes items
- **THEN** a confirmation names the number of items
- **AND** linked cached media is cleaned up without deleting source files

### Requirement: Library export and import

The system SHALL export selected or all generations as JSON, JSONL, CSV, or plain text, and SHALL re-import its own export without loss of core fields.

#### Scenario: Round trip

- **WHEN** the user exports a set to JSON and imports it into an empty library
- **THEN** prompts, negatives, parameters, tags, and provenance are restored

#### Scenario: Plain text export

- **WHEN** the user exports plain text for a selection
- **THEN** one prompt is written per line with no metadata

### Requirement: Schema migrations and backup

The system SHALL apply forward-only schema migrations on startup and SHALL provide backup and restore of the library and presets.

#### Scenario: Upgrade

- **WHEN** the application starts with an older database
- **THEN** migrations run before the UI becomes interactive
- **AND** a pre-migration backup is written

#### Scenario: Restore backup

- **WHEN** the user restores a backup
- **THEN** the current library is backed up first
- **AND** the restored library opens without errors

### Requirement: Bulk actions

The system SHALL allow selecting multiple generations to favorite, tag, export, delete, or re-run with a different preset.

#### Scenario: Re-run with another target

- **WHEN** the user selects several generations and chooses "re-run with mmh3"
- **THEN** one job per item is queued
- **AND** each new result links to its source generation
