# Spec Delta

## ADDED Requirements

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
