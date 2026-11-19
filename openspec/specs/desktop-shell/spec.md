# desktop-shell Specification

## Purpose

Provides the double-clickable Windows desktop shell that hosts the web UI, isolates privileged operations, persists settings, keeps all data local, and exposes health and diagnostics.

## Requirements

### Requirement: One-click launch

The application SHALL start from a double-click on a packaged executable or Start-menu shortcut without requiring a terminal, a separately started web server, a browser, or manual browser navigation.

#### Scenario: Cold start

- **WHEN** the user double-clicks the installed or portable executable
- **THEN** the native application window opens and the workspace is interactive within 3 seconds on the reference machine
- **AND** no console window is shown

#### Scenario: First-ever launch

- **WHEN** the application is launched for the first time on a machine
- **THEN** startup may take longer while security scanning initializes
- **AND** the application still opens without a terminal
- **AND** the Compose workspace is shown

#### Scenario: Second launch focuses the existing window

- **WHEN** the application is already running and the user launches it again
- **THEN** the existing window is brought to the foreground
- **AND** no second instance or second database connection is created

### Requirement: Workspace navigation

The main window SHALL expose direct controls for Compose, Media, Library, Jobs, Presets, Runtime, and Datasets. Compose SHALL be the default surface. Opening or closing a panel SHALL NOT discard the selected asset, compose inputs, or unsaved editor state.

#### Scenario: Open a side panel

- **WHEN** the user opens Library, Jobs, Presets, Runtime, or Datasets
- **THEN** the panel appears inside the main window
- **AND** closing it returns to the current workspace with prior inputs intact

#### Scenario: Return to Compose

- **WHEN** the user navigates away from Compose and back
- **THEN** the selected media, preset, brief, guidance, and any result remain in place

### Requirement: Isolated renderer

The native UI SHALL call backend capabilities directly in-process; renderer-supplied paths and external input SHALL be validated by the same path guards and typed errors that governed the previous bridge, and no capability SHALL be reachable without that validation.

#### Scenario: Renderer cannot read arbitrary paths

- **WHEN** a UI-supplied path attempts a filesystem read outside an approved root
- **THEN** the path guard denies the operation and reports a typed error
- **AND** no file outside the approved root is read

#### Scenario: Unknown bridge call rejected

- **WHEN** the UI requests an operation that is not part of the backend command surface
- **THEN** the call fails with a typed error and no side effect occurs

### Requirement: Settings persistence and validation

The system SHALL persist settings for the LM Studio endpoint, API token, model roles, storage locations, concurrency, keep-warm policy, theme, and editor defaults, and SHALL validate values before applying them.

#### Scenario: Invalid endpoint rejected

- **WHEN** the user saves a malformed LM Studio URL
- **THEN** the save is rejected with a field-level error
- **AND** the previous value remains active

#### Scenario: Settings survive restart

- **WHEN** the user changes the theme and description concurrency, then restarts the application
- **THEN** both values are restored

### Requirement: Local-first data and privacy

User media, prompts, and model traffic SHALL remain on the machine. The application SHALL make no outbound network request other than to the configured local model endpoint, and SHALL collect no telemetry.

#### Scenario: Offline operation

- **WHEN** the machine has no internet access and a local LM Studio endpoint is running
- **THEN** all core workflows remain available
- **AND** no request is sent to a non-loopback address

### Requirement: Storage locations

The system SHALL store its database, cache, and settings in a documented per-user location and SHALL offer a portable mode that keeps them beside the executable.

#### Scenario: Portable mode

- **WHEN** the portable build is launched from a writable folder
- **THEN** a `data` directory is created beside the executable and used for the database, cache, and settings

#### Scenario: Read-only location rejected

- **WHEN** portable mode is selected in a folder that is not writable
- **THEN** the application reports the problem and falls back to the per-user location

### Requirement: Diagnostics export

The system SHALL provide a diagnostics view and a one-click export containing the application version, OS, redacted configuration, model status, storage usage, and recent logs. Prompt and description text SHALL be redacted by default.

#### Scenario: Export diagnostics

- **WHEN** the user exports diagnostics
- **THEN** a single archive is written to a user-chosen location
- **AND** API tokens and prompt or description text are redacted unless the user explicitly opts in

### Requirement: Status surface

The UI SHALL continuously show model endpoint health, which models are loaded, the number of active jobs, and cache and storage usage.

#### Scenario: Endpoint becomes unreachable

- **WHEN** LM Studio stops responding
- **THEN** the status surface changes to an error state within 5 seconds
- **AND** it offers retry and start-server actions

### Requirement: Workspace continuity

The system SHALL restore window geometry, the last selected workspace asset, and any unapplied editor draft after a normal restart.

#### Scenario: Restore an unfinished edit

- **WHEN** the user exits with an in-progress crop and reopens the application
- **THEN** the crop edit is restored as an unapplied draft
- **AND** the original file is unchanged

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
