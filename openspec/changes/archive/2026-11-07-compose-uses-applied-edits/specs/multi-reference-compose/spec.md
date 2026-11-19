# Spec Delta

## MODIFIED Requirements

### Requirement: Describe every selected asset

Describe SHALL run for each selected asset that has no description. The panel
SHALL keep one description entry per selected asset, allow each entry's JSON to
be edited, and block Compose until every selected asset has a reviewed
description. When an asset has an applied crop or mask, the describe request
uses that asset's latest applied edit output instead of the raw source, so the
drawn selection is what reaches the describer.

#### Scenario: Describe a fresh multi-selection

- **WHEN** two assets are selected and neither has a description
- **THEN** one describe request runs per asset
- **AND** Compose stays blocked until both descriptions exist

#### Scenario: Describe uses the edited version

- **WHEN** the user describes a selected asset that has an applied crop or mask
- **THEN** the describe request carries the asset's latest edit
- **AND** the description reflects the edited composition

## ADDED Requirements

### Requirement: References resolve to the applied edit

Compose SHALL attach the selected reference's latest applied edit identifier
(`editId`) instead of the source asset when the asset has one, so prompts are
generated from the cropped and masked output. Media tiles in the picker SHALL
preview the applied edit output with an "edited" badge and SHALL refresh after
an apply from the Media view. Assets without an applied edit keep the
established source behavior.

#### Scenario: Picker previews the applied edit

- **WHEN** a selected asset has an applied edit
- **THEN** the Compose tile shows the edited output with an "edited" badge
- **AND** the tile refreshes right after the user applies an edit

#### Scenario: Compose contract carries the edit

- **WHEN** the user composes with a reference whose asset has an applied edit
- **THEN** the request reference carries that edit identifier
- **AND** the generation parameters still record the ordered reference
