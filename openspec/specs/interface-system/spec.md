# interface-system Specification

## Purpose
Defines the shared visual language, app shell, interaction states, accessibility floor, and responsive behavior for the Prompt Studio renderer so every workflow feels like one product.

## Requirements

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

### Requirement: App shell and navigation

The main window SHALL provide a persistent navigation surface for Compose, Media, Library, Jobs, Presets, Runtime, and Datasets. Compose SHALL remain the default view. Navigating between views SHALL NOT discard the selected asset, compose inputs, or unsaved editor state.

#### Scenario: First launch

- **WHEN** the application opens without a restored view
- **THEN** the Compose workspace is the active view
- **AND** the navigation shows every section and the current section

#### Scenario: Return to a view

- **WHEN** the user opens another section and returns to Compose
- **THEN** the selected media, preset, brief, guidance, and any result remain in place

#### Scenario: Dense workspace remains scannable

- **WHEN** a view contains nested panels and long status text
- **THEN** the primary action, current content, and status are visually distinct
- **AND** the view adds no marketing prose or invented metrics

### Requirement: Consistent component states

Every button, input, select, textarea, tab, and toggle SHALL provide default, hover, focus-visible, active, disabled, busy, invalid, and error states as applicable. Each view SHALL have at most one primary action style.

#### Scenario: Keyboard focus

- **WHEN** the user moves through controls with the keyboard
- **THEN** the focused control shows an immediate `:focus-visible` indicator
- **AND** the indicator is not removed by the pointer or transition timing

#### Scenario: Busy and disabled controls

- **WHEN** an action is running or unavailable
- **THEN** the control is visibly disabled or busy
- **AND** repeated activation is prevented

#### Scenario: Field error

- **WHEN** a field fails validation
- **THEN** the error is associated with that field
- **AND** the message remains readable without relying on color alone

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

### Requirement: Readable app-screen presentation

Labels, captions, table heads, tags, and navigation SHALL use sentence case in the body or label face without tracked capitals. The renderer SHALL avoid decorative status dots, middle-dot-joined metadata, purple-to-blue gradients, cards inside cards, and emoji used as icons. Long headings and grid content SHALL wrap without overflowing their tracks.

#### Scenario: Status is textual

- **WHEN** a runtime, job, or item status is displayed
- **THEN** the status is expressed in text and shape or color treatment
- **AND** it does not depend on a bare decorative dot

#### Scenario: Long content

- **WHEN** a prompt, filename, or error message is longer than its container
- **THEN** it wraps or truncates with an accessible full value
- **AND** it does not force horizontal page overflow

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

### Requirement: Picker removal without confirmation

Removing media from the Compose picker SHALL move the asset to the Library
trash immediately, without a confirmation popup. Trash remains the only
recovery step, and the original source file is never deleted.

#### Scenario: Remove from the picker

- **WHEN** the user presses Remove on a picked asset
- **THEN** the asset leaves the selection and moves to the Library trash
- **AND** no blocking dialog appears
