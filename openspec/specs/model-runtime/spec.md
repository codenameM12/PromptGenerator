# model-runtime Specification

## Purpose

Connects to the locally served LM Studio runtime, exposes model discovery and explicit load/unload control, and applies a VRAM-aware policy so the small describer model and the large composer model do not thrash or exhaust memory.

## Requirements

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

### Requirement: Configurable model roles

The system SHALL assign one configurable model to the describer role and one to the composer role, defaulting to `lfm3-vl-2b` and `gemma4-12b-qat-uncensored-hauhaucs-balanced` respectively.

#### Scenario: Missing role model

- **WHEN** a configured role model is not present on the endpoint
- **THEN** starting a job fails before any inference with an actionable error naming the missing model
- **AND** the error offers the closest matching available identifiers and a load action

#### Scenario: Role reassignment

- **WHEN** the user selects a different available model for the composer role
- **THEN** the next generation uses it
- **AND** existing history records retain the model that produced them

### Requirement: Explicit model lifecycle

The system SHALL load and unload models through the runtime API, SHALL report load duration, and SHALL NOT rely on implicit just-in-time loading for pipeline runs.

#### Scenario: Load the composer with a context length

- **WHEN** the user loads the composer model with a requested context length
- **THEN** the model reports loaded with the effective context length
- **AND** the load duration is displayed

#### Scenario: Unload all

- **WHEN** the user activates "unload all"
- **THEN** every model loaded by the application is unloaded
- **AND** if a job is running the user is warned and must confirm

### Requirement: VRAM-aware keep-warm policy

The system SHALL offer keep-warm policies of Both, Describer only, Composer only, and None, and SHALL batch pipeline stages so models required for the same stage run contiguously.

#### Scenario: No models kept warm

- **WHEN** a batch of assets is processed with the None policy
- **THEN** all descriptions are produced before any composition begins
- **AND** the describer is unloaded before the composer is loaded

#### Scenario: Both must fit

- **WHEN** the Both policy is selected
- **THEN** both role models are explicitly loaded and remain resident across the batch
- **AND** the application does not unload either between stages

#### Scenario: Co-residency is not possible

- **WHEN** the configured policy cannot be satisfied because the runtime refuses the second load
- **THEN** the system reports a policy conflict
- **AND** falls back to serialized stages without losing job state

### Requirement: Partial GPU offload

The system SHALL allow a GPU offload ratio between 0 and 1 per role model where the runtime supports it, and SHALL report when the capability is unavailable.

#### Scenario: Apply partial offload

- **WHEN** the user sets the composer offload ratio to 0.5
- **THEN** the next load uses that ratio
- **AND** the effective load configuration is displayed

#### Scenario: Offload unsupported

- **WHEN** partial offload cannot be applied
- **THEN** the control reports that the runtime does not expose it
- **AND** the model loads with runtime defaults

### Requirement: Health monitoring and recovery

The system SHALL poll endpoint health, surface outages, retry transient failures with backoff, and support starting the local server where the runtime exposes a start capability.

#### Scenario: Server stops mid-job

- **WHEN** the endpoint becomes unreachable during a running job
- **THEN** in-flight work fails with a retryable connectivity error
- **AND** the queue pauses instead of marking items permanently failed

#### Scenario: Recovery

- **WHEN** health returns
- **THEN** paused jobs resume from the first incomplete item
- **AND** completed items are not reprocessed

### Requirement: Concurrency control

The system SHALL allow one to four concurrent describer requests and SHALL serialize composer requests. Requests exceeding a limit SHALL wait in the queue rather than fail.

#### Scenario: Concurrency cap enforced

- **WHEN** ten assets are described with a limit of four
- **THEN** no more than four describer requests are in flight at any moment

#### Scenario: Composer serialization

- **WHEN** multiple compositions are queued
- **THEN** at most one composer request is in flight
- **AND** the remaining items wait in order

### Requirement: Idle TTL

The system SHALL allow an idle time-to-live for loaded models and SHALL NOT unload a model while a request against it is in flight.

#### Scenario: Idle unload

- **WHEN** a model has been idle longer than the configured TTL
- **THEN** it is unloaded
- **AND** the status surface reflects the change

#### Scenario: Busy model is retained

- **WHEN** the TTL expires while a request is in flight
- **THEN** the model remains loaded until the request completes

### Requirement: Error taxonomy

The system SHALL classify runtime failures as unreachable, model missing, context overflow, invalid response, timeout, cancelled, or backend error, and SHALL mark each as retryable or permanent.

#### Scenario: Context overflow

- **WHEN** a request exceeds the model context length
- **THEN** it fails with a context-overflow error that reports estimated and allowed token counts
- **AND** the error is marked non-retryable without a shorter input or larger context

#### Scenario: Transient backend error

- **WHEN** the runtime returns a temporary server error
- **THEN** the request is retried with backoff up to the configured attempt limit

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
