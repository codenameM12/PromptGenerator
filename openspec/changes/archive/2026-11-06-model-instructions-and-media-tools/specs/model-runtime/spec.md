# Spec Delta

## ADDED Requirements

### Requirement: Role system prompt overrides

The system SHALL allow an optional, persisted system prompt override for the describer and for the composer. An override SHALL replace the built-in describer instructions or the selected preset's target instructions for every run, including describe, compose, batch, and queued job execution. The pipeline SHALL still append its required output-format, length, language, guidance, and locked-phrase rules. Clearing an override SHALL restore the default, and changing an override SHALL change composition cache identity.

#### Scenario: Custom describer instructions

- **WHEN** the user saves a describer system prompt override and describes media
- **THEN** the describer request uses the override as its system message
- **AND** the structured-description schema is still requested

#### Scenario: Custom composer instructions

- **WHEN** the user saves a composer system prompt override and composes with a preset
- **THEN** the composer request starts from the override instead of the preset's `systemPrompt`
- **AND** the JSON shape, prompt-length, and output-language rules are still added

#### Scenario: Overrides apply to jobs and batches

- **WHEN** a compose runs from the job queue or a batch
- **THEN** the same saved overrides are used

#### Scenario: Clearing an override

- **WHEN** the user clears a system prompt field and saves
- **THEN** that role returns to its built-in or preset default
- **AND** new runs no longer include the cleared text

#### Scenario: Cache freshness

- **WHEN** the user changes a composer or describer override
- **THEN** a new run does not reuse a cached result produced with the previous instructions
