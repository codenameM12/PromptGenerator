# Interface system

Prompt Studio uses one visual system across the workbench. Every colour, type
face, size, spacing step, radius, border, motion duration, and control size is a
token in `src/styles/tokens.css`. Feature stylesheets hold layout only. The
shared primitives and the page floor live in `src/styles/base.css`.

The stylesheet opens with the Hallmark stamp and the pre-emit critique:

```
/* Hallmark · shape: app screen · genre: modern-minimal · theme: workbench (by request)
 * nav: section rail with a slim status bar · signature: one amber signal on the active section and the primary action · states: default, hover, focus-visible, active, disabled, busy, invalid, error
 * axes: P4 H5 E5 S4 R5 V4 · contrast: pass · v2.0.0 */
```

## Shape

The shell is a persistent left rail of seven sections (Compose, Media, Library,
Jobs, Presets, Runtime, Datasets), a slim top bar for runtime health and storage,
and one capped workspace. Compose is the default view. Visited views stay mounted
and hidden, so selected media, compose inputs, filters, and editor state survive
navigation.

The content column is capped at 1280 px. At wider windows the gutters grow; at
1100 px dense two-column areas stack. `html` and `body` use `overflow-x: clip`.

## Tokens

### Surfaces and colour

| Token                     | Use                                     |
| ------------------------- | --------------------------------------- |
| `--color-paper`           | page and media canvas                   |
| `--color-paper-2`         | wells, cards, raised view surfaces      |
| `--color-paper-3`         | controls, chips, compact strips         |
| `--color-paper-4`         | hovered controls, active rail and tabs  |
| `--color-rule`            | default hairline                        |
| `--color-rule-strong`     | hovered and dashed boundaries           |
| `--color-ink`             | primary text and headings               |
| `--color-ink-2`           | secondary text and form values          |
| `--color-muted`           | labels, metadata, captions              |
| `--color-faint`           | placeholders and empty canvas text      |
| `--color-accent`          | primary action, active marker, progress |
| `--color-accent-strong`   | primary hover                           |
| `--color-accent-ink`      | text on the accent                      |
| `--color-focus`           | focus-visible ring                      |
| `--color-selection`       | text selection and crop overlay         |
| `--color-success`         | succeeded states                        |
| `--color-warning`         | warnings and interrupted states         |
| `--color-warning-surface` | warning callout background              |
| `--color-danger`          | errors and destructive actions          |
| `--color-danger-surface`  | error callout background                |
| `--color-info`            | informational callouts                  |
| `--color-info-surface`    | informational callout background        |

### Type

| Token             | Use                                          |
| ----------------- | -------------------------------------------- |
| `--font-sans`     | every label, heading, control, and body line |
| `--font-mono`     | prompts, JSON, parameters, code-like values  |
| `--text-xs`       | metadata, badges, table heads                |
| `--text-sm`       | dense body and labels                        |
| `--text-md`       | controls and default form text               |
| `--text-base`     | page body                                    |
| `--text-lg`       | view head and top-bar title                  |
| `--text-xl`       | panel title                                  |
| `--leading-tight` | headings                                     |
| `--leading-body`  | running text and prompt bodies               |
| `--weight-normal` | running text                                 |
| `--weight-medium` | selected and emphasis text                   |
| `--weight-strong` | headings and primary actions                 |

### Space, shape, and motion

| Token                  | Value                          |
| ---------------------- | ------------------------------ |
| `--space-3xs`          | 2 px                           |
| `--space-2xs`          | 4 px                           |
| `--space-xs`           | 8 px                           |
| `--space-sm`           | 12 px                          |
| `--space-md`           | 16 px                          |
| `--space-lg`           | 24 px                          |
| `--space-xl`           | 32 px                          |
| `--space-2xl`          | 48 px                          |
| `--radius-xs`          | 4 px                           |
| `--radius-sm`          | 6 px                           |
| `--radius-md`          | 8 px                           |
| `--radius-lg`          | 12 px                          |
| `--radius-pill`        | 999 px                         |
| `--border-width`       | 1 px                           |
| `--border`             | 1 px solid rule                |
| `--dur-micro`          | 120 ms                         |
| `--dur-short`          | 180 ms                         |
| `--ease-out`           | cubic-bezier(0.22, 1, 0.36, 1) |
| `--control-sm`         | 28 px                          |
| `--control-md`         | 32 px                          |
| `--control-lg`         | 38 px                          |
| `--rail-width`         | 208 px                         |
| `--rail-width-compact` | 176 px                         |
| `--container-max`      | 1280 px                        |

## Primitives

`base.css` owns the shared vocabulary. Feature stylesheets position these classes
and must not redefine their colour, type, radius, or state treatments.

| Primitive       | Class or element                                    | Notes                                        |
| --------------- | --------------------------------------------------- | -------------------------------------------- |
| Button          | `button`, `.btn`                                    | default secondary                            |
| Primary action  | `.primary`, `.btn--primary`                         | one per view; amber fill                     |
| Quiet action    | `.btn--ghost`                                       | rail and inline actions                      |
| Destructive     | `.danger`, `.btn--danger`                           | outline, never louder than the primary       |
| Field           | `input`, `select`, `textarea`                       | constant border, 32 px minimum height        |
| Check and radio | `input[type=checkbox]`, `input[type=radio]`         | accent colour                                |
| Panel           | `.panel`                                            | only for independently interactive content   |
| Badge           | `.badge` plus `--success/--warning/--danger/--info` | status text and shape                        |
| Progress        | `.progress`, `.progress__bar`                       | fill uses `transform: scaleX`                |
| Empty state     | `.empty-state`                                      | dashed boundary, one next action             |
| Error callout   | `.alert`                                            | danger surface, always with text             |
| Info callout    | `.notice`, `.notice--warning`                       | info or warning surface                      |
| Layout row      | `.row`, `.row--inputs`                              | wrapping sibling controls                    |
| Hidden label    | `.visually-hidden`                                  | landmark or table heading for assistive tech |

## States

Every interactive control provides the states that apply to it:

- **Default**: `--color-paper-3` on the page, ink text.
- **Hover**: one step lighter plus a stronger rule; no movement except a 1 px
  press on the active state.
- **Focus-visible**: a 2 px `--color-focus` outline with 2 px offset, shown
  immediately and never transitioned away.
- **Active**: 1 px `translateY` press, no layout change.
- **Disabled**: native `disabled`, `opacity: 0.55`, `cursor: not-allowed`.
- **Busy**: disabled plus a label that names the running stage.
- **Invalid**: `aria-invalid="true"` paints the danger border and surface.
- **Error**: an `.alert` with `role="alert"`, associated to the field with
  `aria-describedby` where the field owns the failure.

Labels are sentence case, untracked. Status is text plus a badge shape; no
decorative status dot. Metadata is separated into labelled spans, never joined
with middle dots.

## Motion

Only `transform` and `opacity` animate. Hover and press use `--dur-micro` and
`--ease-out`; progress fill uses `--dur-short`. There is no `transition: all`,
no width, height, top, or left animation. `prefers-reduced-motion: reduce`
collapses every transition to 0.01 ms and keeps all content visible.

## Adding a value

1. Add the token to the single `:root` block in `src/styles/tokens.css`, in the
   matching group.
2. Consume it with `var(--token)` from `base.css` or a feature stylesheet.
3. Never add a literal colour, gradient, or `font-family` outside
   `tokens.css`.
4. Run `npm run audit:tokens`; it fails on literals, gradients, tracked
   capitals, `transition: all`, layout-property animation, and unused tokens.
5. Update this file when a token or primitive is added or removed.

## Verification

- `npm run audit:tokens` reports a clean stylesheet scan.
- Renderer tests cover navigation persistence, Compose modes, guidance, refine,
  batch, media, editor, library, and jobs.
- Viewport review uses 1100, 1280, 1440, and 1920 px widths.
- Keyboard focus, field-error association, disabled/busy states, and reduced
  motion are checked across all seven views. Results are recorded below.

### Accessibility checks

Recorded on 2026-11-06:

- Focus-visible: shared 2 px ring with offset on buttons, fields, tabs, rail
  items, clickable library cards, summary, and canvas.
- Field errors: the Compose error summary uses `role="alert"` and the preset
  field and brief field reference it with `aria-describedby`; editor and panel
  errors also use `role="alert"`.
- Disabled and busy: every async action disables its control and names the
  running stage in the label.
- Reduced motion: a single media query collapses transitions and keeps content
  visible; no animation or reveal is required to read a screen.

### Viewport review

The 28 screenshots in [`docs/ui-screens`](ui-screens) cover Compose, Media,
Library, Jobs, Presets, Runtime, and Datasets at 1100×700, 1280×800, 1440×900,
and 1920×1080. They were captured against the Vite preview, so the LM Studio
offline banner and IPC error alerts are expected; the layout states below them
are the reviewed surface.

Measured at every combination:

- `document.documentElement.scrollWidth` equals `window.innerWidth`, so there is
  no horizontal page scroll.
- `.workspace.scrollWidth` equals `.workspace.clientWidth`, so the workspace has
  no horizontal overflow.
- No button, input, select, textarea, or role-button element extends past the
  workspace edge. The only wide element is the `.visually-hidden` heading, which
  is clipped by design.
- At 1920 the content column stays at 1280 px and the gutters grow; at 1100
  Compose, Library, Presets, Editor, and Dataset two-column areas stack.

### Stylesheet lint

`node <hallmark>/scripts/sloplint.mjs src/App.css --scope app --genre
modern-minimal` follows the local imports and reports: **0 FAIL, 0 REFLEX,
0 WARN**. The run also checks the imported token and base stylesheets.
