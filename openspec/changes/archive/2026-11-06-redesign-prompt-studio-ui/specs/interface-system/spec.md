# Spec Delta

## Purpose

Defines the shared visual language, app shell, interaction states, accessibility floor, and responsive behavior for the Prompt Studio renderer so every workflow feels like one product.

## ADDED Requirements

### Requirement: Tokenized visual system

Every color, font family, type size, spacing step, radius, elevation, and motion duration used by the renderer SHALL come from a token declared in one theme block. Feature stylesheets SHALL NOT introduce literal color values or `font-family` declarations outside that block.

#### Scenario: Token audit

- **WHEN** the renderer stylesheets are searched for literal colors or font stacks outside the token block
- **THEN** no undeclared values are found
- **AND** changing a token updates every surface that uses it

#### Scenario: One visual language

- **WHEN** the user moves between Compose, Media, Library, Jobs, Presets, Runtime, and Datasets
- **THEN** surfaces, controls, spacing, and status treatments come from the same token set
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

The renderer SHALL respect `prefers-reduced-motion`, SHALL animate only transform and opacity, and SHALL NOT use `transition: all` or animate width, height, top, or left. All interactive elements SHALL be reachable by keyboard with visible focus.

#### Scenario: Reduced motion

- **WHEN** the operating system requests reduced motion
- **THEN** reveals, progress transitions, and decorative motion are disabled or reduced
- **AND** content remains visible

#### Scenario: Motion audit

- **WHEN** the stylesheets are reviewed
- **THEN** no `transition: all` or layout-property animation is present
- **AND** every animation has a reduced-motion fallback

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

The page SHALL use a capped, centered container. At wider viewports the gutters SHALL grow while the content measure and grid widths remain stable. The renderer SHALL avoid horizontal overflow at 1100, 1280, 1440, and 1920 pixels.

#### Scenario: Wide screen

- **WHEN** the window is 1920 pixels wide
- **THEN** the content remains capped and centered
- **AND** the extra space becomes gutter, not stretched controls

#### Scenario: Narrow desktop

- **WHEN** the window is 1100 pixels wide
- **THEN** dense two-column areas stack or reflow
- **AND** no horizontal scrollbar appears

### Requirement: Visual verification

The change SHALL be verified with a token audit, stylesheet lint, the existing renderer tests, keyboard and reduced-motion checks, and viewport screenshots at the supported widths.

#### Scenario: Release review

- **WHEN** implementation is complete
- **THEN** the token audit and stylesheet lint pass
- **AND** renderer tests pass
- **AND** the four viewport screenshots show no overflow or unreadable controls
- **AND** the keyboard and reduced-motion checks are recorded
