# media-ingest Specification

## Purpose

Brings local images and videos into the workspace from Windows Explorer, the clipboard, or a folder, and derives the metadata, thumbnails, playable previews, embedded generation parameters, and sampled frames the prompt pipeline needs.

## Requirements

### Requirement: Intake methods

The system SHALL accept media through drag and drop from Windows Explorer, a native file picker, a folder picker with recursion, and clipboard paste.

#### Scenario: Mixed drag and drop

- **WHEN** the user drops three images and one video onto the workspace
- **THEN** all four are imported
- **AND** each appears once with its kind, dimensions, and duration where applicable

#### Scenario: Paste an image

- **WHEN** the user presses the paste shortcut while an image is on the clipboard
- **THEN** the image is imported as a new asset with a generated name

#### Scenario: Folder import

- **WHEN** the user selects a folder containing nested media
- **THEN** supported files at every depth are imported
- **AND** progress and a final count are shown

### Requirement: Format support and graceful failure

The system SHALL support common image and video formats and SHALL report unsupported, corrupt, or unreadable files per item without aborting the rest of the import.

#### Scenario: Unsupported file in a batch

- **WHEN** a batch contains a text file among images
- **THEN** the text file is rejected with a reason
- **AND** every supported item is still imported

#### Scenario: Corrupt video

- **WHEN** a video cannot be probed
- **THEN** it is marked failed with the probe error
- **AND** the user can remove it from the workspace

### Requirement: Probing and metadata

The system SHALL record and display width, height, display rotation applied, frame rate, duration, codec, bitrate, audio presence, and color information. For rotated video, displayed dimensions SHALL reflect the rotation.

#### Scenario: Rotated phone video

- **WHEN** a video carries a 90-degree rotation flag
- **THEN** the workspace shows the display dimensions, not the stored dimensions
- **AND** generated frames and proxies are oriented correctly

#### Scenario: Missing metadata

- **WHEN** the container does not report a value such as frame rate
- **THEN** the field is shown as unknown
- **AND** the asset remains usable

### Requirement: Thumbnails and preview proxies

The system SHALL generate cached thumbnails for every asset and SHALL generate a playable proxy for video whose codec the built-in player cannot decode.

#### Scenario: Unsupported preview codec

- **WHEN** a video uses a codec the embedded player cannot decode
- **THEN** a lower-resolution preview proxy is generated
- **AND** the source file remains unchanged

#### Scenario: Proxy cache cleanup

- **WHEN** the user clears the media cache
- **THEN** proxies and thumbnails are removed
- **AND** source media and saved edits are untouched

### Requirement: Embedded generation metadata

The system SHALL detect and extract embedded generation parameters, including Automatic1111-style infotext, ComfyUI workflow or prompt data, and PNG or WebP text chunks, and SHALL offer them as optional guidance rather than applying them silently.

#### Scenario: Image with Automatic1111 metadata

- **WHEN** an imported PNG contains generation parameters
- **THEN** the parameters are shown and can be copied into guidance or a preset
- **AND** the user's current guidance is not overwritten without confirmation

#### Scenario: No embedded metadata

- **WHEN** no recognized metadata is present
- **THEN** the asset is imported normally without an error

### Requirement: Frame sampling

The system SHALL sample frames from video using a configurable strategy: first/middle/last, N evenly spaced, or scene-change based. Frame count SHALL be configurable from 1 to 16, and sampling SHALL be deterministic for the same input and configuration.

#### Scenario: Even sampling

- **WHEN** a 30-second clip is sampled with eight evenly spaced frames
- **THEN** eight frames are produced at predictable timestamps
- **AND** the timestamps are recorded with the asset

#### Scenario: Scene-change sampling

- **WHEN** scene-change sampling is selected
- **THEN** frames are chosen at detected scene boundaries
- **AND** the number of detected scenes is reported

#### Scenario: Deterministic repeat

- **WHEN** the same asset is sampled twice with the same configuration
- **THEN** the same frames and timestamps are produced

### Requirement: Contact sheet and frame budget

The system SHALL support sending sampled frames either as separately labeled images or as a single contact sheet, and SHALL respect a maximum image count per describer request.

#### Scenario: Frame cap applied

- **WHEN** the describer request budget allows nine images and twelve frames were sampled
- **THEN** the system uses a contact sheet or selects nine frames
- **AND** the chosen representation is recorded with the description

### Requirement: Content deduplication

The system SHALL identify assets by content hash so that the same file imported twice, including under a different name or path, resolves to one asset.

#### Scenario: Renamed duplicate

- **WHEN** the user imports a file that is byte-identical to an existing asset
- **THEN** the existing asset is selected instead of a duplicate being created
- **AND** the user is informed that it was already imported

### Requirement: Import progress and cancellation

The system SHALL report per-item and aggregate progress for imports and SHALL allow cancellation, keeping items already completed.

#### Scenario: Cancel an import

- **WHEN** the user cancels a folder import midway
- **THEN** no new items are started
- **AND** already imported assets remain available

### Requirement: Source files are read-only

The system SHALL never modify, move, or delete source media during import, probing, thumbnail generation, or frame extraction.

#### Scenario: Source integrity

- **WHEN** any ingest operation completes
- **THEN** the source file's content hash and timestamp are unchanged

### Requirement: Path robustness

The system SHALL handle long paths, Unicode names, removable drives, and network locations, and SHALL report a clear error when a path becomes unavailable.

#### Scenario: Disconnected network drive

- **WHEN** an imported asset's source location is no longer reachable
- **THEN** the asset is marked offline
- **AND** cached thumbnails remain viewable while actions requiring the source are disabled

### Requirement: Remove an asset from the workspace

The Media view SHALL let the user remove an imported asset. Removal SHALL move the asset to the library trash, remove it from Compose selections and media pickers, and SHALL NOT delete, move, or modify the original source file. An asset referenced by stored history SHALL require confirmation that names the impact, and a removed asset SHALL be restorable from the Library.

#### Scenario: Remove an unused asset

- **WHEN** the user removes an asset that no generation references
- **THEN** it disappears from the Media view and from media pickers
- **AND** it is listed in the Library trash
- **AND** its original source file still exists at the original path

#### Scenario: Remove a referenced asset

- **WHEN** the user removes an asset that stored generations reference
- **THEN** the confirmation states that history keeps those generations
- **AND** after confirmation the generations remain readable with the asset marked unavailable

#### Scenario: Cancel a removal

- **WHEN** the user dismisses the confirmation
- **THEN** no asset is trashed and the selection is unchanged

#### Scenario: Restore a removed asset

- **WHEN** the user restores the asset from the Library trash
- **THEN** it reappears in Media and is selectable in Compose

### Requirement: Re-import restores a removed asset

When imported content matches an asset in the trash, the system SHALL restore that asset instead of leaving it hidden or creating a duplicate. The import result SHALL report the asset as restored and the asset SHALL appear in the normal listing and pickers again.

#### Scenario: Re-add a removed image

- **WHEN** the user removes an image and later imports the same file again
- **THEN** the existing asset is restored
- **AND** it appears in Media and in the Compose picker
- **AND** no second asset record is created

#### Scenario: Duplicate while active

- **WHEN** the user imports a file that matches an asset that is not trashed
- **THEN** the existing asset is returned as a duplicate
- **AND** nothing is restored or duplicated

### Requirement: Remove from the Compose picker

The Compose media picker SHALL offer a remove action for each asset, using the same trash semantics and confirmation as the Media view. Removing from the picker SHALL drop the asset from the current selection and SHALL NOT delete the source file.

#### Scenario: Remove from Compose

- **WHEN** the user removes an asset from the Compose media picker
- **THEN** the asset moves to the Library trash
- **AND** it disappears from the picker and from every selection
- **AND** the original file remains on disk
