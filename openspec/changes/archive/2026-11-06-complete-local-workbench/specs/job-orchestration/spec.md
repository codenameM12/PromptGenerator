# Spec Delta

## ADDED Requirements

### Requirement: Reference visibility in job details

The system SHALL show each job item's ordered media references, including a
thumbnail, name, and weight, and SHALL update the display as items run.

#### Scenario: Compose job with references

- **WHEN** a queued compose item uses two weighted references
- **THEN** the job detail lists both references in order with their weights
- **AND** a thumbnail is shown for each available asset

#### Scenario: Missing asset

- **WHEN** a referenced asset is no longer available
- **THEN** the job detail still shows the reference with an unavailable marker
