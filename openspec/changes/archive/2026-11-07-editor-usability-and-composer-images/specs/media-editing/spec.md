# Spec Delta

## MODIFIED Requirements

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
