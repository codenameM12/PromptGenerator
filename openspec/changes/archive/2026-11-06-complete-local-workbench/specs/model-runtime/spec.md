# Spec Delta

## MODIFIED Requirements

### Requirement: Model discovery

The system SHALL list the models available from the configured LM Studio endpoint, including identifier, model type, loaded state, maximum context length, and any reported capabilities such as vision input, reasoning output, and maximum output tokens where the endpoint reports them.

#### Scenario: Models are listed

- **WHEN** the endpoint is reachable
- **THEN** the model panel shows every available model with its loaded state
- **AND** the refresh action reflects changes made in LM Studio

#### Scenario: Endpoint unreachable

- **WHEN** the model list request fails
- **THEN** the panel shows a connection error with a retry action
- **AND** previously cached model metadata is marked stale

#### Scenario: Reported capabilities

- **WHEN** the endpoint reports that a model accepts image input and has a 32k context
- **THEN** the panel shows a vision badge and the reported context length
- **AND** the reported values are used by role suggestions and budgeting

#### Scenario: Capabilities not reported

- **WHEN** the endpoint omits a capability field
- **THEN** the model is treated as unknown for that capability
- **AND** the app does not infer the capability from the model name

### Requirement: Context budget enforcement

The system SHALL request a configured context length and SHALL check prompt size before sending, trimming by an explicit, reported policy rather than allowing silent truncation. When the selected model reports a maximum context length, the system SHALL use that value as the budget instead of the configured default.

#### Scenario: Oversized description

- **WHEN** assembled input exceeds the composer budget
- **THEN** the system trims the lowest-priority section
- **AND** the result records which section was trimmed and by how much

#### Scenario: Reported context is smaller

- **WHEN** the loaded model reports a context length below the preset's declared input budget
- **THEN** the smaller reported context is used
- **AND** trimming follows that budget

## ADDED Requirements

### Requirement: Capability-aware role suggestions

The system SHALL suggest describer and composer models from the available list,
SHALL prefer a vision-capable model for the describer when vision capability is
reported, and SHALL require explicit user confirmation before changing roles.

#### Scenario: Vision model suggested for describing

- **WHEN** the available models include one known to accept images
- **THEN** the describer suggestion names that model first
- **AND** applying it requires a user action

#### Scenario: No capability data

- **WHEN** no model reports capabilities
- **THEN** suggestions fall back to the existing name-based ranking
- **AND** the panel states that capability data is unavailable

### Requirement: Preflight capability warnings

Before a run that needs vision or a known budget, the system SHALL warn when the
describer is not vision-capable, when the composer's context is below the preset's
input budget, or when a configured role model is unavailable, and SHALL allow the
run to proceed after the warning.

#### Scenario: Describer cannot see images

- **WHEN** media is selected and the describer is known not to accept image input
- **THEN** the workspace warns before describing
- **AND** the warning names the model and the reason

#### Scenario: Model unavailable

- **WHEN** a configured role model is not in the available list
- **THEN** the warning offers the capability-ranked alternatives

### Requirement: Capability report

The diagnostics export SHALL include a snapshot of each model's reported
capabilities and context, marked unknown where data is absent, without including
prompt or description text.

#### Scenario: Diagnostics capability snapshot

- **WHEN** the user exports diagnostics
- **THEN** the archive lists model identifiers, vision, reasoning, and context values
- **AND** no prompt text is included
