# Spec Delta

## ADDED Requirements

### Requirement: Workspace reset

The Compose workspace SHALL provide a single reset action that clears the current result, generated descriptions, brief, refine inputs, locked phrases, per-run guidance, and selected media. A reset SHALL leave the selected preset, the imported asset library, and all stored history unchanged, and SHALL be unavailable when the workspace is already empty.

#### Scenario: Reset after composing

- **WHEN** the user has a composed result, descriptions, a brief, locked phrases, and selected media and chooses reset
- **THEN** the result, descriptions, brief, locked phrases, refine inputs, and per-run guidance are cleared
- **AND** the media selection is empty
- **AND** the selected preset remains selected

#### Scenario: Reset preserves history and library

- **WHEN** the user resets a workspace that produced generations
- **THEN** the generations remain in the Library
- **AND** imported assets remain available in Media

#### Scenario: Reset is unavailable when empty

- **WHEN** the workspace has no result, description, brief, or selection
- **THEN** the reset action is disabled
