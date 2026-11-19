# Spec Delta

## Purpose

Defines the native desktop shell that replaces the webview renderer, the behaviour-parity contract that governs the port, and the cut-over rules that retire the source implementation once parity is proven.

## ADDED Requirements

### Requirement: Native single-binary shell

The application SHALL start from a double-click on a packaged executable or Start-menu shortcut without a terminal, a separately started web server, a browser, or a webview runtime. A second launch SHALL focus the existing window and SHALL NOT open a second database connection.

#### Scenario: Cold start without a web runtime

- **WHEN** the user double-clicks the installed or portable executable
- **THEN** a native window opens and the Compose workspace is interactive
- **AND** no console window, browser, or webview process is required

#### Scenario: Second launch focuses the existing window

- **WHEN** the application is already running and the user launches it again
- **THEN** the existing window is brought to the foreground
- **AND** no second instance or second database connection is created

### Requirement: Behaviour parity before cut-over

Until the source renderer is deleted, every ported unit SHALL be exercised against a recorded corpus captured from the source implementation, and the count of compared inputs and differences SHALL be reported. A unit SHALL NOT be considered ported while differences remain.

#### Scenario: Differential check on a ported unit

- **WHEN** a ported view-model unit is verified against its recorded corpus
- **THEN** the harness reports the number of inputs compared and the number of differences
- **AND** the unit is blocked from cut-over when any difference remains

#### Scenario: No improvement folded into the port

- **WHEN** a discrepancy is found between source and ported behaviour
- **THEN** the port is changed to match the source, including source quirks
- **AND** any desired fix is recorded as a separate follow-up, not applied during the port

### Requirement: In-process backend access

The native UI SHALL reach the existing backend capabilities by direct calls within the same process, with the same observable results as the previous command surface. Renderer-supplied paths and external input SHALL be validated by the same path and input guards before any filesystem or process operation.

#### Scenario: Operation result matches the source command

- **WHEN** a UI action is translated into a backend call
- **THEN** the call arguments and the resulting view state match the recorded source behaviour
- **AND** no IPC serialization boundary is observable to the user

#### Scenario: Untrusted path rejected

- **WHEN** a UI-supplied path tries to escape an approved root
- **THEN** the operation is rejected by the existing path guard
- **AND** the user-visible error matches the source command's error text

### Requirement: Native view state continuity

Navigating between views SHALL preserve the selected asset, compose inputs, filters, and unapplied editor state, and restoring a window after restart SHALL restore geometry, the last workspace asset, and any unapplied draft.

#### Scenario: Navigate away and back

- **WHEN** the user opens another section and returns to Compose
- **THEN** the selected media, preset, brief, guidance, and any result remain in place

#### Scenario: Restore after restart

- **WHEN** the application is reopened after a normal exit with an in-progress edit
- **THEN** the window geometry and unapplied editor draft are restored
- **AND** the original file is unchanged

### Requirement: Native platform integration

The application SHALL provide native equivalents for file and folder selection, clipboard image read, window persistence, single-instance behaviour, media display, and drag-out of prepared payloads. Where an equivalent cannot preserve observable behaviour, the divergence SHALL be documented before it ships.

#### Scenario: Select a source file

- **WHEN** the user requests import and picks a file in the native dialog
- **THEN** the import proceeds with the same dedupe, probe, and thumbnail behaviour as the source
- **AND** no in-app browser prompt appears

#### Scenario: Documented divergence

- **WHEN** a native platform capability cannot reproduce a source behaviour
- **THEN** the difference is recorded as an intended divergence with a rationale
- **AND** no user-visible promise contradicts the documented behaviour

### Requirement: Readable backend error text

The native UI SHALL present backend failures using the structured error's user-facing message, falling back through its message fields, so validation and media errors show their real text rather than a type name or debug representation.

#### Scenario: Validation failure

- **WHEN** a crop or mask operation fails with a validation error
- **THEN** the message shown near the controls names the failure
- **AND** no debug type representation appears anywhere in the UI

### Requirement: Source deletion at cut-over

When parity is proven for every ported unit, the source renderer and its build tooling SHALL be deleted rather than left as an unreachable fallback, and the application SHALL run solely from the native binary.

#### Scenario: Cut-over

- **WHEN** all units are differentially green and the native binary is the only launcher
- **THEN** the renderer source, web build files, and Node dependencies are removed
- **AND** the differential corpus is retained as a frozen regression record

#### Scenario: No dead fallback

- **WHEN** the port is complete
- **THEN** no source-language implementation of a ported unit remains in the repository
