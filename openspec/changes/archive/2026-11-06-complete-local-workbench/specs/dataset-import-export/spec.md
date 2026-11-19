# Spec Delta

## ADDED Requirements

### Requirement: Row media resolution

The system SHALL resolve each dataset row's declared media path against the
dataset root before a run, SHALL reject paths that escape the root, and SHALL
mark unresolved files as missing without aborting the import.

#### Scenario: Row media resolves

- **WHEN** a row declares `image: frame.png` and the file exists under the dataset root
- **THEN** the preview shows the resolved path and a found status
- **AND** the path is stored with the row

#### Scenario: Missing row media

- **WHEN** a row declares a media path that does not exist
- **THEN** the row is marked missing and remains visible
- **AND** other rows continue to import

#### Scenario: Traversal rejected

- **WHEN** a row declares `../../outside.png`
- **THEN** the row is rejected with a reason
- **AND** no file outside the dataset root is read

### Requirement: Row media import

Before composing a row that has media, the system SHALL import that media into
the asset library using the existing hash, dedupe, probe, and thumbnail path,
SHALL persist the resulting asset identifier with the row, and SHALL reuse an
existing asset instead of importing the same file twice.

#### Scenario: One import across rows

- **WHEN** two rows reference the same image file
- **THEN** the file is imported once
- **AND** both rows reference the same asset identifier

#### Scenario: Resume after a partial run

- **WHEN** a row already has an imported media asset and is run again
- **THEN** the system reuses the stored asset without re-importing

### Requirement: Media-aware batch composition

The system SHALL compose each dataset row that has media with that media as the
row's reference and the row prompt as its brief, and SHALL keep rows without
media on the text-only path.

#### Scenario: Row with media

- **WHEN** a row has a prompt and a resolved media asset
- **THEN** the composed generation records the asset in its references
- **AND** the generation lists the row as its source

#### Scenario: Text-only row unchanged

- **WHEN** a row has a prompt but no media
- **THEN** the row composes text-only as before

### Requirement: Per-row failure isolation and resume

The system SHALL continue a dataset run after a row fails, SHALL record the
failure reason on that row, and SHALL skip rows that already succeeded when the
dataset is run again.

#### Scenario: Mixed results

- **WHEN** one row references unsupported media and the others are valid
- **THEN** the valid rows succeed
- **AND** the failed row records its reason
- **AND** the job completes rather than aborting

#### Scenario: Retry skips successes

- **WHEN** the user runs a dataset again after a partial failure
- **THEN** only unfinished rows are queued

### Requirement: Multi-media rows

The system SHALL accept several media paths in one row, separated by `|`, with an
optional `@weight` suffix per path, SHALL preserve their order as reference order,
and SHALL clamp weights to the supported range with a warning when out of range.

#### Scenario: Multiple references with weights

- **WHEN** a row declares `subject.png@0.8|style.png`
- **THEN** the generation references `subject.png` at weight 0.8 and `style.png` at weight 1.0
- **AND** the order is preserved

### Requirement: Media identifiers in preview and export

The system SHALL show each row's media status in the dataset preview and SHALL
include the media asset identifier, generation identifier, and row status in
dataset exports.

#### Scenario: Export includes provenance

- **WHEN** the user exports a dataset after a run
- **THEN** each row includes its media asset identifier and generation identifier where present
- **AND** the original fields are preserved
