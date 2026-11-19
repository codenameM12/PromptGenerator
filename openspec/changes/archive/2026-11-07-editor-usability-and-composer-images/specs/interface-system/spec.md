# Spec Delta

## ADDED Requirements

### Requirement: Readable IPC error text

The renderer SHALL render IPC failures through a single shared error-text
helper that unwraps the structured `CommandError` (preferring `userMessage`,
then `message`, then a nested message), so validation and media errors show
their real text instead of `[object Object]`.

#### Scenario: Edit validation failure

- **WHEN** a crop or mask creation fails with a validation error
- **THEN** the message shown near the controls names the failure, such as
  "Crop 10+10 at 0,0 exceeds the source 8x8"
- **AND** `[object Object]` never appears anywhere in the UI

### Requirement: Picker removal without confirmation

Removing media from the Compose picker SHALL move the asset to the Library
trash immediately, without a confirmation popup. Trash remains the only
recovery step, and the original source file is never deleted.

#### Scenario: Remove from the picker

- **WHEN** the user presses Remove on a picked asset
- **THEN** the asset leaves the selection and moves to the Library trash
- **AND** no blocking dialog appears
