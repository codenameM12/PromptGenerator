# Spec Delta

## REMOVED Requirements

### Requirement: Structured description contract

**Reason**: The describer now returns verbose prose text; JSON fields,
confidence, and per-field selection are gone.

## ADDED Requirements

### Requirement: Verbose text description contract

The describer SHALL return a verbose prose description of the supplied image,
video frames, or edit target, covering subject, composition and framing,
setting, lighting, color palette, camera and lens character, style, and any
visible text. Video descriptions SHALL additionally describe camera motion,
subject motion, and temporal change. Edit descriptions SHALL state the intended
change and what must be preserved. The response SHALL NOT be JSON, Markdown, or
a bullet list, and the system SHALL accept it as plain text.

#### Scenario: Image description

- **WHEN** an image is described
- **THEN** the description is connected, detailed prose rather than a JSON object
- **AND** elements that are not visible are described as unknown instead of invented

#### Scenario: Video description

- **WHEN** a clip is described
- **THEN** the description covers camera motion, subject motion, and temporal change across the sampled frames

#### Scenario: Short response retried

- **WHEN** the describer returns a response shorter than a usable description
- **THEN** one retry is made with an instruction to be more thorough
- **AND** a repeated short response fails with a clear invalid-response error

## MODIFIED Requirements

### Requirement: Composition inputs

The composer SHALL assemble its input from the selected preset, the effective
guidance, the verbose text description or brief, any existing prompt, and any
locked elements, and SHALL respect the preset's token and length budget.

#### Scenario: Locked element survival

- **WHEN** the user locks the phrase "red leather jacket" and regenerates three times
- **THEN** every result contains that exact phrase unchanged

#### Scenario: Budget enforcement

- **WHEN** the assembled input exceeds the preset's input budget
- **THEN** the lowest-priority section is trimmed
- **AND** the trimming is recorded with the generation

### Requirement: Editable description

The user SHALL be able to edit the verbose text description or the brief after
the describe stage and before composition, and recomposition SHALL use the
edited text.

#### Scenario: Fix a misread detail

- **WHEN** the describer calls a dog a wolf and the user corrects the text
- **THEN** the next composition reflects the correction
- **AND** the original and edited descriptions are both retained
