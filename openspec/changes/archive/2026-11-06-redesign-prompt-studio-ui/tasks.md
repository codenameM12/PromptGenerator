# Tasks

## 1. Visual Foundations

- [x] 1.1 Create `src/styles/tokens.css` with the Hallmark stamp, critique and waiver lines, a warm-graphite palette, type scale, spacing, radii, borders, shadows, motion and control-size tokens; verify the app still builds and every token is consumed.
- [x] 1.2 Create `src/styles/base.css` with the page reset, capped container, `overflow-x: clip`, shared button/field/panel/badge/progress primitives, immediate `:focus-visible`, and reduced-motion fallbacks; verify stylesheet lint and the renderer tests pass.
- [x] 1.3 Add a token audit check that finds literal colors, font stacks, `transition: all`, and layout-property animations outside the token block; verify it reports zero findings.
- [x] 1.4 Document the system in `docs/ui-system.md` (tokens, primitives, states, motion, when to add a value); verify every documented token exists in `tokens.css`.

## 2. App Shell and Navigation

- [x] 2.1 Replace the top-bar toggles with a persistent left section rail and slim top bar for Compose, Media, Library, Jobs, Presets, Runtime, and Datasets; verify Compose is the default view.
- [x] 2.2 Render each feature panel as a workspace view rather than a card above Media, preserving selected asset, compose inputs and editor state across navigation; verify with a navigation test.
- [x] 2.3 Add responsive rules for the rail, top bar and workspace at 1100, 1280, 1440 and 1920; verify no horizontal overflow and correct stacking.
- [x] 2.4 Update `README.md` and `docs/user-guide.md` navigation descriptions to match the new shell; verify the documented sections exist.

## 3. Compose Workspace

- [x] 3.1 Restyle Compose with the shared primitives: mode tabs, media/preset selectors, guidance, locked phrases, language/variations/seed/cache controls, description editor and result cards; verify `ComposePanel.test.tsx` passes.
- [x] 3.2 Tune Compose density and hierarchy at 1100 and 1440 (controls/output stacking, prompt measure, result actions); verify both widths visually and confirm no control is clipped.

## 4. Media and Editor

- [x] 4.1 Restyle the media grid, import toolbar, empty state and asset metadata using shared primitives; verify the media tests and screen-reader labels still pass.
- [x] 4.2 Restyle the editor toolbar, canvas, mask controls, metadata and progress/error states; verify `MediaEditor.test.tsx` passes and the editor remains usable at 1100.
- [x] 4.3 Update the media/editor section of `docs/user-guide.md` for the new controls and verify each documented control is present.

## 5. Library and Jobs

- [x] 5.1 Restyle Library: filters, result list, detail pane, versions/diffs, trash and bulk actions; verify the existing App and Library tests pass.
- [x] 5.2 Restyle Jobs: queue summary, job cards, progress, item errors and controls; verify pause/resume/cancel/retry labels and error details remain accessible.
- [x] 5.3 Add/adjust renderer tests for the new Library and Jobs landmarks and headings; verify the full Vitest suite passes.

## 6. Presets, Runtime and Datasets

- [x] 6.1 Restyle Presets and Runtime panels with shared primitives, including validation warnings, model rows, role/policy controls and busy states; verify their interactions and tests pass.
- [x] 6.2 Restyle Datasets: import wizard, mapping table, preview, row status, run controls and export column selection; verify the dataset tests pass.
- [x] 6.3 Update `docs/presets.md` and `docs/user-guide.md` where the redesigned controls change the described UI; verify each documented step is reachable.

## 7. Accessibility and Visual Review

- [x] 7.1 Run the Floor audit: no literal colors/fonts outside tokens, no `transition: all`, no width/height/top/left animation, no tracked-caps labels, no middle-dot meta, no decorative status dots, no purple-to-blue gradient, no card-inside-card, no emoji icons; fix every finding.
- [x] 7.2 Verify keyboard focus, form error association, disabled/busy states and `prefers-reduced-motion` across all seven views; record the checks in `docs/ui-system.md`.
- [x] 7.3 Capture screenshots at 1100, 1280, 1440 and 1920 for Compose, Media, Library, Jobs, Presets, Runtime and Datasets; fix overflow, clipping and unreadable contrast.
- [x] 7.4 Run Hallmark `sloplint` with app scope if the script is available, otherwise the token audit and stylesheet lint; fix every `FAIL` and record the outcome in `docs/ui-system.md`.
- [x] 7.5 Run the full gates (`format:check`, `typecheck`, `lint`, `test`, `lint:rust`, `test:rust`) and update `docs/release-notes.md` with the UI verification result.
