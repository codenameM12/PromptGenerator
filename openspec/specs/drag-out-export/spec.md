# drag-out-export Specification

## Purpose

Lets the user drag prompts, edited media, masks, and metadata straight out of the application into Windows Explorer, with predictable names and a Save-as fallback.

## Requirements

### Requirement: Native drag out

The system SHALL support dragging a generation, asset, edited media file, mask, or sidecar from the application window and dropping it into Windows Explorer or another file-aware application, producing a real file at the drop location.

#### Scenario: Drag a prompt to the desktop

- **WHEN** the user drags a generation's prompt card to the desktop
- **THEN** a text file containing the prompt is created on the desktop
- **AND** the file name follows the configured naming template

#### Scenario: Drag an edited clip to a folder

- **WHEN** the user drags an edited video to an Explorer folder
- **THEN** the video file is copied there
- **AND** it plays in the system player

#### Scenario: Drag a mask

- **WHEN** the user drags a mask thumbnail to Explorer
- **THEN** a PNG mask is written with the target's polarity

### Requirement: Payload selection

The system SHALL let the user choose what a drag produces: the prompt as plain text, the structured result as JSON, the original media, the edited media, the mask, or a bundle of these.

#### Scenario: Structured drag

- **WHEN** the payload is set to structured JSON
- **THEN** the dropped file contains prompt, negative prompt, and parameters
- **AND** the file has a `.json` extension

#### Scenario: Bundle drag

- **WHEN** the user drags a result with "prompt and media" enabled
- **THEN** both files are placed at the drop location

### Requirement: Naming template

The system SHALL build output names from a template supporting at minimum the preset name, target name, asset name, date, time, index, and seed. It SHALL remove characters illegal on Windows and SHALL resolve collisions without overwriting.

#### Scenario: Template expansion

- **WHEN** the template is `{preset}_{asset}_{date}` and the asset is `beach`
- **THEN** the file is named with the preset, `beach`, and the current date

#### Scenario: Illegal characters

- **WHEN** an expanded name contains `:` or `?`
- **THEN** those characters are replaced with a safe substitute
- **AND** the name remains unique

#### Scenario: Collision

- **WHEN** a file with the target name already exists
- **THEN** a numeric suffix is added
- **AND** the existing file is not overwritten

### Requirement: Sidecars

The system SHALL optionally write sidecar files next to a dragged media file: a plain-text prompt, a JSON metadata file, and a mask, according to user settings.

#### Scenario: Sidecar enabled

- **WHEN** the user drags media with sidecars enabled
- **THEN** the prompt `.txt` and metadata `.json` are written with matching base names

#### Scenario: Sidecar disabled

- **WHEN** sidecars are disabled
- **THEN** only the selected media file is written

### Requirement: Fallback actions

The system SHALL provide "Save as" and "Reveal in Explorer" actions for every draggable item, usable when native drag is unavailable.

#### Scenario: Save as

- **WHEN** the user chooses "Save as" for a prompt
- **THEN** a native save dialog opens with the template-expanded name
- **AND** the chosen path receives the file

#### Scenario: Reveal in Explorer

- **WHEN** the user chooses "Reveal in Explorer" for an edited file
- **THEN** Explorer opens with that file selected

### Requirement: Copy semantics

Dragging SHALL copy data and SHALL never move, rename, or delete the source file or the stored result. Temporary materializations SHALL be cleaned up.

#### Scenario: Source preserved

- **WHEN** any drag-out completes
- **THEN** the original source and the application's stored output remain unchanged

#### Scenario: Temporary cleanup

- **WHEN** the application restarts after a drag
- **THEN** temporary drag files older than the retention window are removed

### Requirement: Large file feedback

The system SHALL show progress or a busy state while preparing a large video for drag-out and SHALL allow cancellation.

#### Scenario: Large clip

- **WHEN** the user drags a multi-gigabyte clip
- **THEN** the UI shows preparation progress
- **AND** the drag begins only when the file is ready

### Requirement: Failure handling

If the destination cannot be written or the preparation fails, the system SHALL report the error, SHALL NOT leave a partial file at the destination, and SHALL offer the Save-as fallback.

#### Scenario: Unwritable destination

- **WHEN** the drop target rejects the write
- **THEN** an error identifies the destination
- **AND** no zero-byte or partial file remains

#### Scenario: Missing source

- **WHEN** the underlying source file has been deleted or unmounted
- **THEN** the drag is cancelled with a clear message
- **AND** cached results remain available
