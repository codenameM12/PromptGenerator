# Spec Delta

## ADDED Requirements

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
