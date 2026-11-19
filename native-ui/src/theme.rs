//! The single declared theme for the native UI.
//!
//! Every colour, type, spacing, radius and motion value used by a view lives
//! here. Views read these values (or the egui styles derived from them) and must
//! not introduce ad-hoc literals. `tests::audit_no_ad_hoc_color_literals` scans
//! the rest of `src/` to enforce that rule.

use egui::{Color32, Context, FontId, TextStyle, Visuals};

/// Approximate sRGB rendering of the source CSS token palette (warm graphite /
/// single amber signal). Exact hue is an acceptable port difference.
const HEX: ThemeHex = ThemeHex {
    paper: "#1e1b18",
    paper_2: "#26221e",
    paper_3: "#302b26",
    paper_4: "#3a342e",
    rule: "#454039",
    rule_strong: "#5a544b",
    ink: "#f2efe9",
    ink_2: "#d6d1c7",
    muted: "#b0a99c",
    faint: "#968f81",
    accent: "#e3a23c",
    accent_strong: "#f0b655",
    accent_ink: "#1a140c",
    focus: "#f3bf5a",
    selection: "#6b4f2b",
    success: "#7fbf8f",
    warning: "#e0b050",
    warning_surface: "#382e1f",
    danger: "#d9663f",
    danger_surface: "#38231d",
    info: "#6ea9c7",
    info_surface: "#1f3038",
};

struct ThemeHex {
    paper: &'static str,
    paper_2: &'static str,
    paper_3: &'static str,
    paper_4: &'static str,
    rule: &'static str,
    rule_strong: &'static str,
    ink: &'static str,
    ink_2: &'static str,
    muted: &'static str,
    faint: &'static str,
    accent: &'static str,
    accent_strong: &'static str,
    accent_ink: &'static str,
    focus: &'static str,
    selection: &'static str,
    success: &'static str,
    warning: &'static str,
    warning_surface: &'static str,
    danger: &'static str,
    danger_surface: &'static str,
    info: &'static str,
    info_surface: &'static str,
}

fn hex(value: &str) -> Color32 {
    // The theme is the sole owner of literals; this parser keeps them in one place.
    Color32::from_hex(value).expect("theme colours are valid #rrggbb strings")
}

/// All design values, owned in one place.
pub struct Theme {
    pub paper: Color32,
    pub paper_2: Color32,
    pub paper_3: Color32,
    pub paper_4: Color32,
    pub rule: Color32,
    pub rule_strong: Color32,
    pub ink: Color32,
    pub ink_2: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_strong: Color32,
    pub accent_ink: Color32,
    pub focus: Color32,
    pub selection: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub warning_surface: Color32,
    pub danger: Color32,
    pub danger_surface: Color32,
    pub info: Color32,
    pub info_surface: Color32,

    // Type scale (px, derived from the source rem scale at a 16px root).
    pub text_xs: f32,
    pub text_sm: f32,
    pub text_md: f32,
    pub text_base: f32,
    pub text_lg: f32,
    pub text_xl: f32,

    // Spacing scale (px).
    pub space_3xs: f32,
    pub space_2xs: f32,
    pub space_xs: f32,
    pub space_sm: f32,
    pub space_md: f32,
    pub space_lg: f32,
    pub space_xl: f32,
    pub space_2xl: f32,

    // Shape (px).
    pub radius_xs: f32,
    pub radius_sm: f32,
    pub radius_md: f32,
    pub radius_lg: f32,
    pub border_width: f32,
    pub rail_width: f32,

    // Motion (milliseconds). Zero when the OS asks for reduced motion.
    pub dur_micro_ms: u64,
    pub dur_short_ms: u64,
}

impl Theme {
    /// The one declared theme. `reduced_motion` zeroes every duration.
    pub fn dark(reduced_motion: bool) -> Self {
        let (dur_micro_ms, dur_short_ms) = if reduced_motion { (0, 0) } else { (120, 180) };
        Self {
            paper: hex(HEX.paper),
            paper_2: hex(HEX.paper_2),
            paper_3: hex(HEX.paper_3),
            paper_4: hex(HEX.paper_4),
            rule: hex(HEX.rule),
            rule_strong: hex(HEX.rule_strong),
            ink: hex(HEX.ink),
            ink_2: hex(HEX.ink_2),
            muted: hex(HEX.muted),
            faint: hex(HEX.faint),
            accent: hex(HEX.accent),
            accent_strong: hex(HEX.accent_strong),
            accent_ink: hex(HEX.accent_ink),
            focus: hex(HEX.focus),
            selection: hex(HEX.selection),
            success: hex(HEX.success),
            warning: hex(HEX.warning),
            warning_surface: hex(HEX.warning_surface),
            danger: hex(HEX.danger),
            danger_surface: hex(HEX.danger_surface),
            info: hex(HEX.info),
            info_surface: hex(HEX.info_surface),
            text_xs: 11.0,
            text_sm: 12.0,
            text_md: 13.0,
            text_base: 14.0,
            text_lg: 16.0,
            text_xl: 20.0,
            space_3xs: 2.0,
            space_2xs: 4.0,
            space_xs: 8.0,
            space_sm: 12.0,
            space_md: 16.0,
            space_lg: 24.0,
            space_xl: 32.0,
            space_2xl: 48.0,
            radius_xs: 4.0,
            radius_sm: 6.0,
            radius_md: 8.0,
            radius_lg: 12.0,
            border_width: 1.0,
            rail_width: 208.0,
            dur_micro_ms,
            dur_short_ms,
        }
    }

    /// Shared frame for every text input. egui's default `TextEdit` frame has
    /// no stroke, so inputs need this to stay visible against the panel.
    pub fn text_edit_frame(&self) -> egui::Frame {
        egui::Frame::new()
            .fill(self.paper_3)
            .stroke(egui::Stroke {
                width: self.border_width,
                color: self.rule,
            })
            .corner_radius(self.radius_sm)
            .inner_margin(egui::Margin::symmetric(
                self.space_xs as i8,
                self.space_2xs as i8,
            ))
    }

    /// Frame for read-only generated output. Keeps the result area visible
    /// even before the first compose run.
    pub fn output_frame(&self) -> egui::Frame {
        egui::Frame::new()
            .fill(self.paper_2)
            .stroke(egui::Stroke {
                width: self.border_width,
                color: self.rule_strong,
            })
            .corner_radius(self.radius_md)
            .inner_margin(egui::Margin::symmetric(
                self.space_sm as i8,
                self.space_xs as i8,
            ))
    }

    /// Applies the theme to egui's global visuals and text styles so every view
    /// inherits it without restating values.
    pub fn install(&self, ctx: &Context) {
        let mut visuals = Visuals::dark();
        visuals.panel_fill = self.paper;
        visuals.window_fill = self.paper_2;
        // egui 0.37 reads `text_edit_bg_color` first; keep both it and the
        // fallback distinct from the panel ground so empty inputs read as inputs.
        visuals.extreme_bg_color = self.paper_3;
        visuals.text_edit_bg_color = Some(self.paper_3);
        visuals.faint_bg_color = self.paper_3;
        visuals.code_bg_color = self.paper_3;
        visuals.hyperlink_color = self.accent_strong;
        visuals.selection.bg_fill = self.selection;
        visuals.selection.stroke.color = self.accent;
        visuals.widgets.noninteractive.bg_fill = self.paper_2;
        visuals.widgets.noninteractive.bg_stroke.color = self.rule;
        visuals.widgets.noninteractive.bg_stroke.width = self.border_width;
        visuals.widgets.noninteractive.fg_stroke.color = self.muted;
        visuals.widgets.inactive.bg_fill = self.paper_3;
        visuals.widgets.inactive.bg_stroke.color = self.rule;
        visuals.widgets.inactive.bg_stroke.width = self.border_width;
        visuals.widgets.inactive.fg_stroke.color = self.ink_2;
        visuals.widgets.hovered.bg_fill = self.paper_4;
        visuals.widgets.hovered.bg_stroke.color = self.rule_strong;
        visuals.widgets.hovered.bg_stroke.width = self.border_width;
        visuals.widgets.hovered.fg_stroke.color = self.ink;
        visuals.widgets.active.bg_fill = self.paper_4;
        visuals.widgets.active.bg_stroke.color = self.accent;
        visuals.widgets.active.bg_stroke.width = self.border_width;
        visuals.widgets.active.fg_stroke.color = self.ink;
        visuals.widgets.open.bg_fill = self.paper_3;
        visuals.widgets.open.bg_stroke.color = self.rule_strong;
        visuals.widgets.open.bg_stroke.width = self.border_width;
        ctx.set_visuals(visuals);

        let animation_time = if self.dur_micro_ms == 0 {
            0.0
        } else {
            self.dur_micro_ms as f32 / 1000.0
        };
        ctx.all_styles_mut(|style| {
            style.visuals.selection.bg_fill = self.selection;
            for (text_style, size) in [
                (TextStyle::Small, self.text_xs),
                (TextStyle::Body, self.text_base),
                (TextStyle::Button, self.text_base),
                (TextStyle::Heading, self.text_xl),
                (TextStyle::Monospace, self.text_md),
            ] {
                style
                    .text_styles
                    .insert(text_style, FontId::proportional(size));
            }
            style.animation_time = animation_time;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Theme;
    use egui::Context;

    #[test]
    fn text_inputs_have_a_visible_ground_and_border() {
        let ctx = Context::default();
        Theme::dark(false).install(&ctx);
        let style = ctx.style_of(egui::Theme::Dark);
        let visuals = &style.visuals;
        assert_ne!(
            visuals.text_edit_bg_color(),
            visuals.panel_fill,
            "text edit ground must differ from panel ground"
        );
        assert!(visuals.widgets.inactive.bg_stroke.width > 0.0);
        assert!(visuals.widgets.hovered.bg_stroke.width > 0.0);
        assert!(visuals.widgets.active.bg_stroke.width > 0.0);
    }

    #[test]
    fn declares_every_token_group() {
        let theme = Theme::dark(false);
        assert!(theme.dur_micro_ms > 0 && theme.dur_short_ms > 0);
        assert_eq!(theme.rail_width, 208.0);
        assert_eq!(theme.space_xs, 8.0);
        assert_eq!(theme.radius_md, 8.0);
        assert_eq!(theme.text_base, 14.0);
        // Signal colours are distinct from the paper ground.
        assert_ne!(theme.accent, theme.paper);
        assert_ne!(theme.ink, theme.paper_4);
    }

    #[test]
    fn reduced_motion_zeroes_durations() {
        let theme = Theme::dark(true);
        assert_eq!(theme.dur_micro_ms, 0);
        assert_eq!(theme.dur_short_ms, 0);
    }

    /// Scans every Rust source except this theme module for ad-hoc colour
    /// literals. Hex colours, `from_rgb*` calls and `Color32::from_hex` calls
    /// are only allowed inside the single declared theme.
    #[test]
    fn audit_no_ad_hoc_color_literals() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders: Vec<String> = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let entry = entry.expect("dir entry");
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|v| v.to_str()) != Some("rs") {
                    continue;
                }
                if path.file_name().and_then(|v| v.to_str()) == Some("theme.rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).expect("read source");
                for (line_no, line) in source.lines().enumerate() {
                    let stripped = line.split("//").next().unwrap_or("");
                    if has_color_literal(stripped) {
                        offenders.push(format!(
                            "{}:{}: {}",
                            path.display(),
                            line_no + 1,
                            stripped.trim()
                        ));
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "ad-hoc colour literals outside theme.rs:\n{}",
            offenders.join("\n")
        );
    }

    fn has_color_literal(line: &str) -> bool {
        let bytes = line.as_bytes();
        // #rrggbb / #rgb hex literals.
        for start in 0..bytes.len() {
            if bytes[start] != b'#' {
                continue;
            }
            let rest = &line[start + 1..];
            let len = rest.bytes().take_while(|b| b.is_ascii_hexdigit()).count();
            if len == 3 || len == 6 || len == 8 {
                return true;
            }
        }
        line.contains("Color32::from_rgb")
            || line.contains("Color32::from_hex")
            || line.contains("Rgba::from_rgb")
    }
}
