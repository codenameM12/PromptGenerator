# Spec Delta

## ADDED Requirements

### Requirement: Composer image input option

Compose/Refine requests SHALL carry an "include images" option that, when
enabled, sends each selected reference image to the composer model as an image
part after the prompt sections. Video references SHALL contribute one sampled
frame image. Edited references SHALL use the saved edit output.

The option SHALL only be usable when the composer model reports vision
capability; otherwise it is disabled with an explanatory label. Backend
execution SHALL reject image inclusion when the composer is known to reject
images and SHALL include the image payload hash in the cache key. Batch runs
and queued jobs SHALL remain text-only.

#### Scenario: Send images to the composer

- **WHEN** the option is enabled and the composer reports vision
- **THEN** the composer request includes the reference images as data URLs
- **AND** the produced prompt still matches the preset schema

#### Scenario: Composer without vision

- **WHEN** the composer model does not report vision capability
- **THEN** the option is disabled in the UI
- **AND** a backend request for images fails with a clear message instead of
  silently sending images to the model

#### Scenario: Cache correctness

- **WHEN** the same brief is composed with and without images
- **THEN** the cache keys differ
- **AND** a cached text-only result is never returned for an image request
