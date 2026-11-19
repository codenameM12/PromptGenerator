# Proposal

## Why

Every Prompt Studio workflow now exists, but the renderer grew panel by panel: colors, fonts, spacing, and control states are hardcoded in each feature stylesheet, there is no shared navigation model, and several interaction states are missing. The result is a capable tool that feels assembled rather than designed, and it is harder to scan under load. A coherent visual system is now the highest-leverage improvement.

## What Changes

- Introduce a token-first interface system: one declared palette, type scale, spacing, radius, elevation, motion, and control-state set. No literal color or `font-family` outside the token block.
- Replace the ad-hoc top bar and stacked panels with a persistent app shell: a section rail or equivalent navigation for Compose, Media, Library, Jobs, Presets, Runtime, and Datasets; Compose remains the default surface.
- Restyle every existing surface without changing workflow behavior: Compose workspace, media grid and editor, Library, Jobs, Presets, Runtime, and Datasets.
- Establish one component vocabulary for buttons, fields, selects, tabs, panels, badges, progress, empty states, and error text. One primary action per view.
- Apply the Hallmark Floor: capped centered container, `overflow-x: clip`, focus-visible on every control, reduced-motion fallbacks, transform/opacity-only motion, `minmax(0, 1fr)` media tracks, untracked sentence-case labels, no decorative status dots, no middle-dot meta strings, no purple-to-blue gradients, no card-inside-card, no emoji icons.
- Add responsive rules for 1100, 1280, 1440, and 1920 widths, with gutters growing while the measure and grids stay fixed.
- Improve information hierarchy in dense screens so status, primary action, and current content are legible at a glance without adding marketing copy.
- Add visual and interaction verification: token audit, focus/reduced-motion checks, viewport screenshots, and the existing renderer test suite.

## Capabilities

### New Capabilities

- `interface-system`: the shared visual system, app shell and navigation, component states, accessibility floor, responsive behavior, and visual verification for the Prompt Studio renderer.

### Modified Capabilities

<!-- None: this change restyles the renderer without changing prompt, media, runtime, library, dataset, or job behavior. -->

## Impact

- Renderer only: `src/App.tsx`, `src/App.css`, `src/features/**` JSX class names, and feature stylesheets.
- New shared styles: a token stylesheet and a base/primitive stylesheet imported by the app.
- No Rust, IPC, database, preset, pipeline, or job behavior changes.
- Existing renderer tests remain the regression baseline; add visual/accessibility checks and a viewport checklist.
- No new runtime dependencies are required; system fonts are acceptable. Optional asset/font choices stay inside the change's design.
