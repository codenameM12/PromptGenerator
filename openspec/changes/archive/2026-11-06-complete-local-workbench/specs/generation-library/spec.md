# Spec Delta

## ADDED Requirements

### Requirement: Multi-generation comparison

The system SHALL let the user compare two to four generations at once, including
unrelated generations, in aligned columns showing prompt, negative prompt,
parameters, seed, model, preset, references, and rating, with changed prompt
segments highlighted and a copy action per column.

#### Scenario: Compare unrelated generations

- **WHEN** the user selects generations from different presets and chooses compare
- **THEN** each selected generation appears in its own aligned column
- **AND** prompt differences are highlighted

#### Scenario: Comparison limits

- **WHEN** fewer than two or more than four generations are selected
- **THEN** compare is unavailable with a message explaining the allowed range

#### Scenario: Missing optional fields

- **WHEN** a generation has no negative prompt or no references
- **THEN** the column shows an explicit empty state rather than a blank cell

### Requirement: Reference-aware re-run

The system SHALL preserve each generation's ordered references and weights when
re-running from the library, while allowing the preset, seed, and variations to be
changed, and SHALL link the new generation to its source.

#### Scenario: Re-run keeps references

- **WHEN** the user re-runs a generation that used two weighted references
- **THEN** the new run uses the same references in the same order with the same weights
- **AND** the new generation records its parent

### Requirement: Complete backup bundle

The system SHALL produce a single backup bundle containing the database, settings,
the encrypted API token, and the workspace draft, and SHALL restore from that
bundle or from a legacy database backup while writing a pre-restore snapshot
first.

#### Scenario: Full backup and restore

- **WHEN** the user backs up and later restores the bundle
- **THEN** the database, settings, token, and workspace draft are restored
- **AND** the pre-restore database is kept

#### Scenario: Legacy database backup

- **WHEN** the user restores a database file produced by an earlier version
- **THEN** the restore succeeds using the existing database-only path

#### Scenario: Corrupt bundle rejected

- **WHEN** the bundle is missing its manifest or database entry
- **THEN** the restore is rejected with a reason
- **AND** the current data is unchanged
