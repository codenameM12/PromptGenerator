# dataset-import-export Specification

## Purpose

Imports existing prompt lists, caption files, and media datasets with flexible field mapping, runs them through the prompt pipeline in bulk, and exports the results with the original rows intact.

## Requirements

### Requirement: Import formats

The system SHALL import plain text with one prompt per line, JSON arrays, JSON Lines, CSV and TSV, and image folders with caption or metadata sidecars.

#### Scenario: Text list

- **WHEN** the user imports a text file with one prompt per line
- **THEN** each non-empty line becomes a row
- **AND** blank lines and whitespace-only lines are skipped

#### Scenario: CSV with headers

- **WHEN** the user imports a CSV with a header row
- **THEN** headers are offered as mappable fields
- **AND** the delimiter is detected automatically

#### Scenario: Image folder with captions

- **WHEN** the user imports a folder containing images and same-named `.txt` captions
- **THEN** each image is paired with its caption
- **AND** images without captions are listed as incomplete

#### Scenario: JSON Lines with media paths

- **WHEN** the user imports JSONL rows that include relative media paths
- **THEN** paths are resolved against the dataset file location
- **AND** missing media is flagged per row

### Requirement: Field mapping

The system SHALL provide a mapping step with a preview of the first rows, auto-detect likely source fields, and map them to prompt, negative prompt, parameters, media path, tags, identifier, and ignored. The mapping SHALL be saved as a reusable import profile.

#### Scenario: Auto-detect and adjust

- **WHEN** the user imports a CSV whose "caption" column contains prompts
- **THEN** "caption" is preselected as the prompt source
- **AND** the user can change the mapping before importing

#### Scenario: Reuse a profile

- **WHEN** the user imports a second file with the same columns
- **THEN** the previously saved mapping is offered

#### Scenario: Unmapped required field

- **WHEN** no source field is mapped to prompt and the preset requires one
- **THEN** import is blocked until a mapping is chosen

### Requirement: Validation and partial import

The system SHALL validate encoding, required fields, row structure, and media references, SHALL report per-row errors, and SHALL allow importing valid rows while listing rejected ones.

#### Scenario: Some invalid rows

- **WHEN** three of one hundred rows lack a prompt
- **THEN** ninety-seven rows are imported
- **AND** the three failures are listed with row numbers and reasons

#### Scenario: Unsupported encoding

- **WHEN** a file cannot be decoded as UTF-8
- **THEN** the system attempts a documented fallback encoding
- **AND** reports the encoding used or fails with a clear message

#### Scenario: Oversized file

- **WHEN** an import exceeds the configured size limit
- **THEN** the system refuses it before loading it fully
- **AND** suggests splitting the file

### Requirement: Deduplication

The system SHALL detect duplicate rows by normalized prompt text or an explicit identifier and SHALL let the user skip, merge, or keep duplicates.

#### Scenario: Skip duplicates

- **WHEN** two rows normalize to the same prompt and the user chooses skip
- **THEN** only the first is imported
- **AND** the duplicate count is reported

#### Scenario: Keep duplicates

- **WHEN** the user chooses keep
- **THEN** both rows are imported with distinct row identifiers

### Requirement: Batch generation

The system SHALL run selected dataset rows through the prompt pipeline with a chosen preset and guidance, and SHALL track per-row status and results.

#### Scenario: Dataset run

- **WHEN** the user starts a batch over two hundred rows
- **THEN** one job item per row is created
- **AND** the dataset view shows queued, running, succeeded, and failed counts

#### Scenario: Resume

- **WHEN** a dataset run is paused or interrupted
- **THEN** it can resume without reprocessing succeeded rows

#### Scenario: Rows without media

- **WHEN** a row has no media but has prompt text
- **THEN** it runs in text-only compose mode
- **AND** the result is marked as not media-derived

### Requirement: Result export

The system SHALL export dataset results preserving the original row fields and adding the generated prompt, negative prompt, parameters, status, and error, in CSV, JSON, or JSONL.

#### Scenario: Column selection

- **WHEN** the user configures the export columns
- **THEN** only those columns are written
- **AND** the selection is remembered for the dataset

#### Scenario: Round trip

- **WHEN** an exported CSV is re-imported with the same profile
- **THEN** original and generated fields map back to their previous roles

### Requirement: Streaming and responsiveness

The system SHALL stream large imports and exports without loading the entire file into memory, and SHALL keep the interface responsive.

#### Scenario: Hundred-thousand-row file

- **WHEN** a very large file is imported
- **THEN** memory use stays bounded
- **AND** progress is reported during parsing

### Requirement: Import safety

The system SHALL treat imported content as data only, SHALL never execute code or formulas from it, SHALL resolve media paths only within the selected dataset root or explicitly approved locations, and SHALL reject path traversal outside those roots.

#### Scenario: Traversal attempt

- **WHEN** a row references a media path outside the dataset root
- **THEN** the row is rejected or requires explicit approval
- **AND** the reason is recorded

#### Scenario: Formula-like cell

- **WHEN** a CSV cell begins with a formula marker
- **THEN** it is stored and exported as text without evaluation

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
