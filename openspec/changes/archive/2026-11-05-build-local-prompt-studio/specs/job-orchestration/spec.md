# Spec Delta

## Purpose

Runs ingest, editing, description, and composition work as durable background jobs so long model and ffmpeg operations survive navigation, are observable, can be cancelled, and resume after an unexpected exit.

## ADDED Requirements

### Requirement: Persisted jobs

The system SHALL persist every unit of background work with its type, inputs, current stage, progress, timestamps, attempt count, and outcome.

#### Scenario: Job survives navigation

- **WHEN** the user starts a generation and switches to another workspace view
- **THEN** the job continues
- **AND** its progress remains visible in the job panel

#### Scenario: Job record after completion

- **WHEN** a job completes or fails
- **THEN** its final state, duration, and error where applicable are retained

### Requirement: State machine

Each job SHALL follow an explicit state sequence of queued, running, succeeded, failed, or cancelled, with an intermediate waiting state for resource or concurrency limits. Invalid transitions SHALL be rejected.

#### Scenario: Legal transition

- **WHEN** a queued job starts
- **THEN** its state changes from queued to running
- **AND** the start time is recorded

#### Scenario: Illegal transition

- **WHEN** a completed job is asked to transition to running without a new attempt
- **THEN** the transition is rejected

### Requirement: Concurrency and backpressure

The system SHALL enforce per-stage concurrency limits and a global limit, and SHALL queue excess work rather than dropping or failing it.

#### Scenario: Queue under load

- **WHEN** fifty compositions are enqueued while the composer limit is one
- **THEN** one runs and forty-nine wait in order
- **AND** the queue length is displayed

#### Scenario: Resource pause

- **WHEN** the runtime is unreachable or a required model cannot be loaded
- **THEN** affected jobs move to waiting
- **AND** unrelated job types continue where possible

### Requirement: Progress reporting

The system SHALL report per-item progress and an aggregate progress view for batch jobs, including counts of succeeded, failed, waiting, and cancelled items.

#### Scenario: Batch progress

- **WHEN** a twenty-item batch is halfway through composition
- **THEN** the panel shows ten completed and ten remaining
- **AND** the currently running item is named

### Requirement: Retry policy

The system SHALL retry retryable failures with backoff up to a configurable attempt limit, SHALL never automatically retry permanent failures, and SHALL allow manual retry of any failed item.

#### Scenario: Transient failure retried

- **WHEN** a model request fails with a temporary backend error
- **THEN** it is retried with increasing delay
- **AND** each attempt is recorded

#### Scenario: Permanent failure not retried

- **WHEN** a job fails because the input file no longer exists
- **THEN** it is marked failed without automatic retries
- **AND** the user can fix the input and retry manually

### Requirement: Cancellation

The system SHALL cancel a job promptly, terminate any child media process, abort in-flight model requests, and avoid marking partial output as complete.

#### Scenario: Cancel during encoding

- **WHEN** the user cancels a video export
- **THEN** the encoder process is terminated within 2 seconds
- **AND** no partial output file is presented as a result

#### Scenario: Cancel during inference

- **WHEN** the user cancels a composition
- **THEN** the request is aborted
- **AND** the job is marked cancelled rather than failed

### Requirement: Crash recovery

On startup the system SHALL detect jobs left running by an unclean exit, mark them interrupted, and offer automatic or manual resume. Resume SHALL continue from the first incomplete item and SHALL NOT duplicate completed outputs.

#### Scenario: Recovery after crash

- **WHEN** the application is closed unexpectedly during a batch
- **THEN** the next start lists the interrupted batch
- **AND** resuming processes only items without completed output

#### Scenario: No duplicate outputs

- **WHEN** an item already has a successful result and the batch is resumed
- **THEN** that item is skipped
- **AND** its existing result is preserved

### Requirement: Queue controls

The system SHALL allow pausing, resuming, cancelling, and reordering waiting jobs, and SHALL allow cancelling all waiting jobs at once.

#### Scenario: Pause and resume

- **WHEN** the user pauses the queue
- **THEN** running items finish or pause according to their stage
- **AND** no new items start until resumed

### Requirement: Responsiveness

Background work SHALL NOT block the user interface; the application SHALL remain interactive while media is encoded or models are queried.

#### Scenario: UI stays responsive

- **WHEN** a large video is being encoded
- **THEN** scrolling, navigation, and settings remain usable
- **AND** progress updates continue

### Requirement: Idempotent enqueue

The system SHALL detect an identical queued job for the same inputs and configuration and SHALL offer to reuse it instead of running duplicate work.

#### Scenario: Duplicate click

- **WHEN** the user starts the same generation twice in quick succession
- **THEN** the second request focuses the existing job
- **AND** no duplicate model call is made

### Requirement: Job details

The system SHALL expose, for each job, its inputs, selected preset, models used, stage timings, token usage where reported, attempts, and full error text.

#### Scenario: Diagnose a failure

- **WHEN** the user opens a failed job
- **THEN** the failed stage, error category, and technical message are shown
- **AND** the user can copy the details for a bug report
