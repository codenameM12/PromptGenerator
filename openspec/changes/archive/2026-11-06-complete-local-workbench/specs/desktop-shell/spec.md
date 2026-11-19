# Spec Delta

## ADDED Requirements

### Requirement: Keyboard operation

The system SHALL provide keyboard shortcuts for the primary actions: compose or
run the active mode, describe selected media, and cancel the active operation.
Shortcuts SHALL NOT fire while a modal dialog or a text field that does not opt in
has focus, and the bindings SHALL be documented in the user guide.

#### Scenario: Compose shortcut

- **WHEN** the workspace is idle and the compose action is available
- **THEN** the compose shortcut starts the run

#### Scenario: Cancel shortcut

- **WHEN** an operation is running
- **THEN** the cancel shortcut requests cancellation
- **AND** the UI reports the cancelled state

#### Scenario: Shortcut ignored in dialogs

- **WHEN** a modal dialog is open
- **THEN** workspace shortcuts do not fire
