# media-editing Specification

## Purpose

Prepares images and video for prompt generation with non-destructive crop, mask, trim, frame-rate, and resolution edits, without ever altering the original media, and writes results one folder below the source.

## Requirements

### Requirement: Non-destructive editing

The system SHALL represent edits as an ordered, reversible stack applied to a source asset, SHALL keep the original file unchanged, and SHALL support undo, redo, and reset.

#### Scenario: Reset an edit

- **WHEN** the user crops, rotates, and then resets the edit stack
- **THEN** the preview matches the untouched source
- **AND** the source file hash is unchanged

#### Scenario: Undo and redo

- **WHEN** the user applies a mask and then undoes and redoes it
- **THEN** the preview reflects each state in order

### Requirement: Crop and transform

The system SHALL support free crop, fixed aspect-ratio crop, automatic crop to a selected target aspect ratio, 90-degree rotation, horizontal and vertical flip, and fit, fill, or pad placement.

#### Scenario: Auto-crop to target aspect

- **WHEN** the selected preset requires 16:9 and the source is 4:3
- **THEN** a centered 16:9 crop is proposed
- **AND** the user can adjust the region before applying

#### Scenario: Pad instead of crop

- **WHEN** the user chooses pad
- **THEN** the full source is preserved and the remainder is filled with a chosen color or transparent

### Requirement: Video trim and conversion

The system SHALL support in and out trim points, audio removal, looping, conversion to a constant frame rate of 24, 25, 30, or 60, and selection of an output container and codec.

#### Scenario: Trim and set frame rate

- **WHEN** the user trims a clip from 2.0s to 7.0s and selects 24 fps
- **THEN** the result is approximately 5 seconds long with a constant 24 fps
- **AND** the result's actual duration and frame rate are displayed

#### Scenario: Remove audio

- **WHEN** the user enables audio removal
- **THEN** the result contains no audio stream

### Requirement: Masking

The system SHALL provide brush, eraser, and rectangle mask tools with adjustable size, hardness, feather, and invert. Masks SHALL be exportable at the target resolution with a polarity declared by the selected preset. For video, a mask SHALL apply to the entire clip.

#### Scenario: Paint and invert a mask

- **WHEN** the user paints a region and inverts the mask
- **THEN** the preview shows the inverted selection
- **AND** the exported mask uses the polarity required by the target preset

#### Scenario: Video mask covers all frames

- **WHEN** a mask is applied to a clip
- **THEN** every exported frame uses the same mask

#### Scenario: Mask resolution

- **WHEN** the target requires 1024 by 1024 and the source is 1920 by 1080
- **THEN** the exported mask is 1024 by 1024 and aligned with the exported media

### Requirement: Resolution derivation

The system SHALL display source and resulting width, height, frame rate, and duration, and SHALL derive a legal output resolution from the selected preset, snapping to that preset's required multiple and honoring the user's fit, fill, or pad choice.

#### Scenario: Snap to a legal resolution

- **WHEN** the source is 1000 by 563 and the preset requires multiples of 16
- **THEN** the proposed output is 1008 by 560
- **AND** the change from the source is shown

#### Scenario: Upscale warning

- **WHEN** the derived output is larger than the source
- **THEN** the system warns that the result will be upscaled
- **AND** the user may accept or choose a smaller legal resolution

#### Scenario: Manual override

- **WHEN** the user enters dimensions that violate the preset's multiple
- **THEN** the value is corrected to the nearest legal value or rejected with an explanation

### Requirement: Output placement

The system SHALL write edited media to an `edited` folder inside the source file's parent folder, using the source base name and an appropriate extension. It SHALL never overwrite an existing file and SHALL resolve collisions with a numeric suffix.

#### Scenario: Default placement

- **WHEN** the user saves an edit of `D:\clips\foo.mp4`
- **THEN** the result is written to `D:\clips\edited\foo.mp4`

#### Scenario: Collision

- **WHEN** `D:\clips\edited\foo.mp4` already exists
- **THEN** the new result is written as `D:\clips\edited\foo (2).mp4`
- **AND** the existing file is not modified

#### Scenario: Alternate root

- **WHEN** the source folder is not writable
- **THEN** the system offers a user-chosen output root
- **AND** remembers it for the session

### Requirement: Atomic output

The system SHALL write results to a temporary file and move them into place only after successful completion, so that a failed or cancelled operation leaves no partial output.

#### Scenario: Cancelled export

- **WHEN** the user cancels a video export midway
- **THEN** no output file remains in the edited folder
- **AND** any temporary file is removed

### Requirement: Processing feedback and errors

The system SHALL report progress for long operations, keep the interface responsive, and surface processing failures with a concise technical reason.

#### Scenario: Encoder failure

- **WHEN** an export fails
- **THEN** the UI shows the failed stage and a short error excerpt
- **AND** the user can retry with adjusted settings

### Requirement: Result verification

After writing, the system SHALL re-probe the result and display its actual dimensions, frame rate, duration, and audio presence. If they differ materially from the requested values, it SHALL warn.

#### Scenario: Frame-rate mismatch

- **WHEN** a requested 30 fps export probes as 29.97 fps
- **THEN** the result panel shows the actual value
- **AND** the difference is noted as acceptable or flagged according to preset tolerance

### Requirement: Edit reuse

The system SHALL key edits by source content, edit stack, and target specification so that repeated generations against the same edit reuse the existing output.

#### Scenario: Reuse an existing edit

- **WHEN** the same edit is requested again for the same source and target
- **THEN** the existing output is reused
- **AND** the pipeline does not re-encode

### Requirement: Quick media tools

The Media view SHALL expose quick actions for the selected asset that move focus to the relevant editor section: crop or edge trim for images, crop/aspect and trim for video, and mask regions for both. Masks SHALL remain available for images and video, and duration trimming SHALL remain available only for video.

#### Scenario: Image quick tools

- **WHEN** the user selects an image in the Media view
- **THEN** Crop / trim edges and Mask regions are offered
- **AND** a video trim action is not offered

#### Scenario: Video quick tools

- **WHEN** the user selects a video in the Media view
- **THEN** Crop / aspect, Trim clip, and Mask regions are offered

#### Scenario: Jump to the editor section

- **WHEN** the user chooses a quick action
- **THEN** the matching editor control section is brought into view
- **AND** the edit remains staged until applied

### Requirement: Mask canvas shows the source

The mask editor SHALL render the selected image or video thumbnail underneath the mask strokes so the user can see the region being masked. The mask layer SHALL be transparent where nothing is painted, and the eraser SHALL remove mask coverage rather than covering the image with an opaque colour.

#### Scenario: Paint a mask over a photo

- **WHEN** the user opens the Mask section for an image
- **THEN** the image is visible under the mask layer
- **AND** painted strokes are distinguishable from the image
- **AND** exported mask values still map to keep and remove regions

#### Scenario: Erase part of a mask

- **WHEN** the user erases over painted mask
- **THEN** the mask coverage is cleared at that point
- **AND** the source image becomes visible again there

### Requirement: Percentage crop controls

Crop SHALL be adjustable with edge controls expressed as percentages and with
aspect-ratio presets, without requiring pixel entry. The editor SHALL also
offer drag-to-draw crop selection on the preview image. Dragging a rectangle on
the source preview SHALL set the crop to that rectangle, clamped inside the
source, using the same percentage model, update history entry, and readout as
the sliders. An in-progress drag SHALL show a live overlay rectangle. The
system SHALL clamp the resulting rectangle to the source bounds and SHALL
prevent zero-size or out-of-range crops before an edit is applied.

#### Scenario: Adjust crop edges

- **WHEN** the user drags the left, right, top, or bottom percentage control
- **THEN** the preview rectangle updates
- **AND** the crop stays inside the source with a positive width and height

#### Scenario: Aspect preset

- **WHEN** the user chooses 1:1, 16:9, or 9:16
- **THEN** a centered crop with that aspect is applied
- **AND** the user can still adjust the edges afterwards

#### Scenario: No pixel entry required

- **WHEN** the user applies a crop
- **THEN** no pixel values needed to be typed
- **AND** an out-of-range crop cannot be submitted

#### Scenario: Draw a crop rectangle

- **WHEN** the user drags from the upper-left to the lower-right of the preview
- **THEN** the crop is set to the drawn rectangle clamped inside the source
- **AND** the edge sliders and readout reflect the new crop
- **AND** the full-resolution crop matches the on-screen selection proportionally

#### Scenario: Backwards drag direction

- **WHEN** the user drags from lower-right to upper-left
- **THEN** the resulting crop is still a valid positive-size rectangle
- **AND** no validation error is produced for that selection

### Requirement: Actionable permission failures

The system SHALL report write-failures for edited output and exported masks
with the blocked folder named and the way out explained: choosing another
output folder, or allowing the app in Windows ransomware protection when the
folder is protected by the system.

#### Scenario: Protected pictures folder

- **WHEN** applying an edit fails because the source folder denies writes
- **THEN** the message names the source folder
- **AND** the message points to the output-folder setting or the Windows
  ransomware-protection exception
- **AND** "Access is denied (os error 5)" never appears without context

### Requirement: Configurable edit output folder

The Runtime view SHALL expose an **Edit output** folder with Browse, Save, and
**Use source folder** actions, persisting an absolute alternate root for
applied edits and mask exports, so a Windows-protected source folder (such as
Pictures) cannot block editing with no remedy.

#### Scenario: Move edit output to a writable folder

- **WHEN** the user picks a writable folder and saves it
- **THEN** applying a crop or mask writes under `<folder>\edited\`
- **AND** the source file is unchanged
- **AND** **Use source folder** restores the beside-the-source default
