# Design

## Context

See `proposal.md` — Why. The renderer is a React 19 + Vite app inside Tauri 2, with feature stylesheets for Compose, Media, Editor, Library, Jobs, Presets, Runtime, and Datasets. `src/App.css` declares only a few root variables; feature CSS uses literal hex colors, borders, radii, and font stacks. Navigation is a top bar of toggle buttons, and panels render as stacked cards above the media grid.

Constraints:

- Windows WebView2, desktop-first, minimum window 1100×700; default 1440×900.
- No new runtime dependency for styling; system fonts are available.
- All workflow behavior and IPC contracts stay unchanged.
- The app is a tool screen, not a landing page: no hero, marketing copy, invented metrics, testimonials, or stock imagery.
- Hallmark Floor applies: tokenized colors/fonts, focus-visible, reduced motion, capped container, no `transition: all`, no layout-property animation, no purple-to-blue gradient, no card-inside-card, no decorative dots, no tracked-caps labels.

## Goals / Non-Goals

**Goals:**

- One visual system across every surface, with a distinct utilitarian-workbench identity.
- A persistent app shell that makes the seven sections obvious and keeps Compose the default.
- Dense screens that scan quickly: clear primary action, current content, and status.
- Accessible controls and motion, verified at the supported desktop widths.

**Non-Goals:**

- New product features, backend changes, or IPC changes.
- A marketing page, custom illustrations, stock photography, or a logo redesign.
- A light theme in this change. Tokens are structured for one, but only the dark workbench theme ships.
- Mobile layouts below the 1100 px minimum window.

## Decisions

### D1: Token-first stylesheets, one palette

Add `src/styles/tokens.css` (theme values and Hallmark stamp) and `src/styles/base.css` (reset, page floor, shared primitives), imported before feature CSS. Tokens cover color, type, spacing, radius, border, shadow, motion, and control sizes. Feature stylesheets consume tokens and keep only layout rules.

Palette: warm graphite workbench rather than blue-black. Surfaces step from a near-black page through raised panels; text is warm off-white; one amber signal color carries primary actions and current selection; muted green/red/amber carry success/danger/warning. This avoids the purple-to-blue default and gives the tool a recognizable identity. Alternatives considered: cool slate + cyan (too close to every developer tool) and pure monochrome (status colors lose meaning).

### D2: System type, mono only for data

Use a Windows-first sans stack through `--font-sans`; use `--font-mono` only for prompt text, JSON, parameters, and code-like values. Labels, nav, captions, and badges use the sans face at a smaller step, sentence case, untracked. No external font download or new dependency. Alternatives considered: bundling a variable font (larger app, licensing review) and all-mono (harder to read dense prose).

### D3: Persistent rail, one workspace

Replace the current top-bar toggles with a left section rail (Compose, Media, Library, Jobs, Presets, Runtime, Datasets), a slim top bar for runtime status and window-level actions, and one scrollable workspace. Compose is the default view. Feature panels become views rather than cards stacked above Media. At narrow widths the rail compresses before the workspace stacks; below 1100 the app is outside the supported range.

The rail is text-led, not an icon-only strip, so each section is readable without tooltips. Selection is a filled surface plus a left marker, not a decorative dot.

### D4: Shared primitives over per-panel silhouettes

Define one component vocabulary in `base.css`: button variants (primary, secondary, ghost, danger), fields, selects, textareas, tabs, panels, badges, progress bars, empty states, and alert/warning/error text. Feature CSS positions these primitives; it does not redefine their color, radius, type, or states. One primary action per view.

### D5: Motion and state floor

Hover/press feedback uses color and transform only, 120–180 ms. Reveals use opacity/translate. No `transition: all`, no width/height/top/left animation. Progress bars use `transform: scaleX` or snap; they never animate width. Every animated rule has a `prefers-reduced-motion: reduce` fallback. `:focus-visible` uses a 2 px signal ring with an offset and appears immediately. Disabled, busy, invalid, and error states are explicit tokens and classes.

### D6: Readability rules

Body text is 13–16 px depending on density; all interactive text is at least 12 px. Labels are sentence case, not tracked capitals. Long prompts and errors wrap with `overflow-wrap: anywhere`; media grids use `minmax(0, 1fr)`. No middle-dot metadata strings: use separate lines or labeled pairs. No emoji as icons. Status is text plus a badge shape; the old bare status dot is removed.

### D7: Container and responsive rules

The workspace uses a capped content column inside the rail: at 1440 the content is capped near 1280 px, at 1920 the gutters grow, and at 1100 the Compose two-column area becomes one column. `html` and `body` use `overflow-x: clip`. Tables and batch lists scroll inside their own containers, never the page.

### D8: Verification without new tooling

Use the existing Vitest suite for behavior regression, a token audit script or grep for literal colors/fonts outside `tokens.css`, Hallmark's `sloplint` when available in the environment, and manual screenshots at 1100, 1280, 1440, and 1920. Keyboard and reduced-motion checks are recorded in the change's verification notes. No browser automation dependency is added.

## Risks / Trade-offs

- **CSS refactor regresses dense screens** → migrate one surface at a time; keep the renderer tests green; check each viewport after each surface.
- **A left rail reduces content width** → cap the rail, compress it first, and keep the Compose workspace's two columns until 1100.
- **Warm palette reduces contrast on muted text** → set explicit contrast targets and verify body text at 4.5:1 and large text/controls at 3:1.
- **Per-panel CSS still leaks literals** → token audit in the task list; fix before visual review.
- **Motion rules drift** → one shared motion token set and a reduced-motion check.
- **The settings already expose a theme value but only dark ships** → document dark-only for this change and keep tokens structured for a later light theme.

## Migration Plan

No data migration. The change is renderer-only and rolls back with the renderer bundle. Apply tokens/base first, then shell, then surfaces; keep `npm run typecheck`, `npm run lint`, `npm test`, and the existing Rust tests green throughout.

## Open Questions

- Should a light theme follow in a separate change? It is deliberately out of scope here and does not affect the requirements.
