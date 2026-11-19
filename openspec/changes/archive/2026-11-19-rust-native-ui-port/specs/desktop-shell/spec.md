# Spec Delta

## MODIFIED Requirements

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

### Requirement: Isolated renderer

The native UI SHALL call backend capabilities directly in-process; renderer-supplied paths and external input SHALL be validated by the same path guards and typed errors that governed the previous bridge, and no capability SHALL be reachable without that validation.

#### Scenario: Renderer cannot read arbitrary paths

- **WHEN** a UI-supplied path attempts a filesystem read outside an approved root
- **THEN** the path guard denies the operation and reports a typed error
- **AND** no file outside the approved root is read

#### Scenario: Unknown bridge call rejected

- **WHEN** the UI requests an operation that is not part of the backend command surface
- **THEN** the call fails with a typed error and no side effect occurs
