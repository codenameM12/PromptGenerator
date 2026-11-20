# Native UI style guide

Applies to `native-ui/` (egui). `docs/ui-system.md` describes the removed web
UI and is historical only. All colors, sizes and spacing come from
`native-ui/src/theme.rs`; never hard-code them in views.

## Tokens
- Colors: `theme.paper*` surfaces, `theme.ink`/`ink_2`/`muted`/`faint` text,
  `theme.accent` for primary/selection, `success`/`warning`/`danger` for state.
- Spacing: `space_3xs` .. `space_2xl`. Radii: `radius_xs` .. `radius_lg`.
- Text sizes: `text_xs` .. `text_xl`.

## Controls
- Buttons are large: padding (16, 9), minimum size 44x36. Do not shrink them.
- Text fields use `theme.text_edit_frame()`. Every editable text field gets the
  clear button: call `widgets::clear_overlay(ui, &response, &mut text, theme)`
  right after adding the `TextEdit` and treat a `true` return like a change.
  Read-only and Datasets fields are exempt.
- Lists: left-aligned selectable rows inside a `ScrollArea` with a stable
  `id_salt`; no walls of text, put actions above the content.
- Layout: panes via `allocate_ui_with_layout`; toolbars use `horizontal_wrapped`.

## Icons
Draw icons with painter shapes (see `widgets.rs`). The bundled font lacks many
symbols (for example the star), so glyph icons render as boxes.

## Destructive actions (trash style)
- The trash icon button (`widgets::trash_button`) is the one style for
  "move to trash" everywhere: outlined square, muted icon, red on hover,
  tooltip names the action.
- The small trash icon inside a text field means Clear: immediate, no undo.
- Moving to trash is reversible and needs no confirmation.
- Permanent deletion lives only in the Trash view, is red text, and asks to
  confirm with the exact item count.
