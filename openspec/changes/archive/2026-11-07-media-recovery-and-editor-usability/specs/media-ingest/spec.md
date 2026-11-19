# Spec Delta

## ADDED Requirements

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
