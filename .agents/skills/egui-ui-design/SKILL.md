---
name: egui-ui-design
description: Design and style the Prompt Studio native egui UI. Use when changing native-ui views, theme tokens, input visibility, layout, thumbnails, or visual parity in the Rust port.
---

# egui UI design for Prompt Studio

Use this skill for `native-ui/` work. Do not use the web/website design skill on
this existing native app.

## Sources

- Project design source: `native-ui/src/theme.rs`. Every colour, font size,
  spacing, radius, and motion value lives there. Do not introduce colour
  literals in view modules; `theme::tests::audit_no_ad_hoc_color_literals`
  enforces this.
- egui API reference: query Context7 library `/websites/rs_egui_egui`, version
  0.37.0, before relying on widget API details.

## Rules

1. Read `native-ui/src/theme.rs` and the target view before editing.
2. Add reusable visual primitives to `Theme`, not to individual views. Inputs
   use `Theme::text_edit_frame()`; widget state colours come from
   `Visuals::widgets` with non-zero `bg_stroke.width`.
3. Text inputs must be visible when empty. egui 0.37 `TextEdit` uses
   `Visuals::text_edit_bg_color`; keep it distinct from `panel_fill` and apply
   an explicit frame. Prefer `ui.add(TextEdit::... .frame(theme.text_edit_frame()))`
   over bare `ui.text_edit_*`.
4. Bound multiline editors with `ui.add_sized(...)` so long content cannot push
   controls off-screen. Long editor content should scroll inside its widget.
5. Load images once and cache `TextureHandle`s by asset id/path. Render with
   `ui.image((handle.id(), size))`. Reserve the same space when a thumbnail is
   missing so grids do not jump.
6. Keep view state in `ViewModel` or the view's flow struct; navigation must not
   clear user input. Only the active view is drawn.
7. Use existing theme spacing instead of ad-hoc pixel gaps where practical.
8. Verify with `cargo test --manifest-path native-ui/Cargo.toml` and
   `cargo build --manifest-path native-ui/Cargo.toml`. For visual changes,
   capture only the `prompt-studio-native.exe` window through the scoped
   `desktop` MCP; never capture the whole desktop.
