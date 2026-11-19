# Spec Delta

## MODIFIED Requirements

### Requirement: Tokenized visual system

Every color, font family, type size, spacing step, radius, elevation, and motion duration used by the native renderer SHALL come from a single declared theme definition. Feature views SHALL NOT introduce private palettes or ad-hoc visual constants.

#### Scenario: Token audit

- **WHEN** the native view code is searched for literal colors or font stacks outside the theme definition
- **THEN** no undeclared values are found
- **AND** changing a theme value updates every surface that uses it

#### Scenario: One visual language

- **WHEN** the user moves between Compose, Media, Library, Jobs, Presets, Runtime, and Datasets
- **THEN** surfaces, controls, spacing, and status treatments come from the same theme definition
- **AND** no panel introduces a private palette

### Requirement: Accessibility and motion floor

The native renderer SHALL respect the operating system's reduced-motion setting, SHALL keep every interactive element reachable by keyboard with a visible focus indicator, and SHALL NOT animate layout properties.

#### Scenario: Reduced motion

- **WHEN** the operating system requests reduced motion
- **THEN** reveals, progress transitions, and decorative motion are disabled or reduced
- **AND** content remains visible

#### Scenario: Motion audit

- **WHEN** the native view code is reviewed
- **THEN** no layout-property animation is present
- **AND** every interactive control has a visible keyboard focus indicator

### Requirement: Capped responsive layout

The native window SHALL keep content within a capped, centered measure as the window widens, with extra space becoming gutters rather than stretched controls. Dense areas SHALL reflow at narrow desktop widths without horizontal scrolling.

#### Scenario: Wide screen

- **WHEN** the window is 1920 pixels wide
- **THEN** the content remains capped and centered
- **AND** the extra space becomes gutter, not stretched controls

#### Scenario: Narrow desktop

- **WHEN** the window is 1100 pixels wide
- **THEN** dense two-column areas stack or reflow
- **AND** no horizontal scrollbar appears

### Requirement: Visual verification

The change SHALL be verified with a theme audit, the Rust lint and test gates, the differential parity harness, keyboard and reduced-motion checks, and window captures at the supported widths.

#### Scenario: Release review

- **WHEN** implementation is complete
- **THEN** the theme audit and Rust gates pass
- **AND** the differential parity harness passes
- **AND** captures at the supported widths show no overflow or unreadable controls
- **AND** the keyboard and reduced-motion checks are recorded

### Requirement: Readable IPC error text

The native renderer SHALL present backend failures through a single shared error-text helper that unwraps the structured `CommandError` (preferring `userMessage`, then `message`, then a nested message), so validation and media errors show their real text instead of a debug type representation.

#### Scenario: Edit validation failure

- **WHEN** a crop or mask creation fails with a validation error
- **THEN** the message shown near the controls names the failure, such as "Crop 10+10 at 0,0 exceeds the source 8x8"
- **AND** no debug type representation appears anywhere in the UI
