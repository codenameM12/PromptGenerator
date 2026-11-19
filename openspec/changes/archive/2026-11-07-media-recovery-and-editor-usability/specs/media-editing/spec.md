# Spec Delta

## ADDED Requirements

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

Crop SHALL be adjustable with edge controls expressed as percentages and with aspect-ratio presets, without requiring pixel entry. The system SHALL clamp the resulting rectangle to the source bounds and SHALL prevent zero-size or out-of-range crops before an edit is applied.

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
