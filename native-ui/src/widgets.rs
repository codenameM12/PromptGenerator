//! Shared widgets. See `docs/native-ui-style-guide.md` for when to use each.
//!
//! Icons are painted with vector shapes: the bundled egui font lacks many
//! symbol glyphs (for example the star), so glyph icons render as boxes.

use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::theme::Theme;

/// Side length of the square icon buttons.
pub const ICON_BUTTON: f32 = 24.0;

/// Paint a trash can inside `rect`.
pub fn paint_trash(ui: &Ui, rect: Rect, color: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(1.4, color);
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.5; // half size of the icon box
    // Lid and handle.
    let lid_y = c.y - s * 0.55;
    painter.line_segment([pos2(c.x - s * 0.7, lid_y), pos2(c.x + s * 0.7, lid_y)], stroke);
    painter.line_segment(
        [pos2(c.x - s * 0.25, lid_y), pos2(c.x - s * 0.25, lid_y - s * 0.25)],
        stroke,
    );
    painter.line_segment(
        [pos2(c.x + s * 0.25, lid_y), pos2(c.x + s * 0.25, lid_y - s * 0.25)],
        stroke,
    );
    painter.line_segment(
        [pos2(c.x - s * 0.25, lid_y - s * 0.25), pos2(c.x + s * 0.25, lid_y - s * 0.25)],
        stroke,
    );
    // Bin body, slightly tapered.
    let top = lid_y + s * 0.15;
    let bottom = c.y + s * 0.8;
    let body = [
        pos2(c.x - s * 0.55, top),
        pos2(c.x - s * 0.45, bottom),
        pos2(c.x + s * 0.45, bottom),
        pos2(c.x + s * 0.55, top),
    ];
    painter.line_segment([body[0], body[1]], stroke);
    painter.line_segment([body[1], body[2]], stroke);
    painter.line_segment([body[2], body[3]], stroke);
    // Slats.
    for dx in [-0.2_f32, 0.2] {
        painter.line_segment(
            [pos2(c.x + s * dx, top + s * 0.3), pos2(c.x + s * dx, bottom - s * 0.25)],
            Stroke::new(1.0, color),
        );
    }
}

fn icon_button_response(ui: &mut Ui, rect: Rect, id_salt: &str, tip: &str) -> Response {
    let id = ui.id().with(("trash-icon", id_salt));
    ui.interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tip)
}

/// Standard destructive icon button (move to trash). Same look everywhere.
pub fn trash_button(ui: &mut Ui, theme: &Theme, id_salt: &str, tip: &str) -> Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(ICON_BUTTON + 6.0), Sense::hover());
    let response = icon_button_response(ui, rect, id_salt, tip);
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, theme.radius_sm, theme.danger_surface);
    }
    ui.painter().rect_stroke(
        rect,
        theme.radius_sm,
        Stroke::new(1.0, if hovered { theme.danger } else { theme.rule_strong }),
        egui::StrokeKind::Inside,
    );
    paint_trash(
        ui,
        rect.shrink(6.0),
        if hovered { theme.danger } else { theme.muted },
    );
    response
}

/// Fixed-height box for long text: content scrolls inside instead of
/// stretching the page. `id_salt` must be unique per box on screen.
pub fn scroll_box<R>(
    ui: &mut Ui,
    theme: &Theme,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    height: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    theme
        .output_frame()
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt(id_salt)
                .max_height(height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    add(ui)
                })
                .inner
        })
        .inner
}

// ---- Prompt formatting -------------------------------------------------------

/// Section keys used by the MiniMax H3 prompt formats.
const SECTION_KEYS: &[&str] = &[
    "subject_definitions",
    "summary",
    "retention_analysis",
    "detailed_description",
    "integrated_multimodal_description",
    "overall_soundscape",
    "non_diegetic_music",
];

#[derive(Debug, PartialEq)]
enum Block {
    Header(String),
    Paragraph(String),
}

/// Splits a prompt into section headers and paragraphs. Blank-line runs
/// collapse, so spacing between blocks is uniform whatever the model emitted.
fn parse_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let flush = |paragraph: &mut Vec<String>, blocks: &mut Vec<Block>| {
        if !paragraph.is_empty() {
            blocks.push(Block::Paragraph(paragraph.join(" ")));
            paragraph.clear();
        }
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut paragraph, &mut blocks);
            continue;
        }
        let plain = line.trim_start_matches(['#', '*', ' ']);
        if let Some((key, rest)) = plain.split_once(':') {
            let key = key.trim().trim_matches('*');
            let looks_like_key = !key.is_empty()
                && key.len() <= 40
                && key
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch == '_' || ch.is_ascii_digit())
                && (SECTION_KEYS.contains(&key) || key.contains('_'));
            if looks_like_key {
                flush(&mut paragraph, &mut blocks);
                blocks.push(Block::Header(key.replace('_', " ")));
                let rest = rest.trim().trim_matches('*').trim();
                if !rest.is_empty() {
                    paragraph.push(rest.to_string());
                }
                continue;
            }
        }
        paragraph.push(line.to_string());
    }
    flush(&mut paragraph, &mut blocks);
    blocks
}

fn push_span(job: &mut egui::text::LayoutJob, text: &str, color: Color32, background: Color32) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        0.0,
        egui::TextFormat {
            color,
            background,
            font_id: egui::FontId::proportional(14.5),
            line_height: Some(22.0),
            ..Default::default()
        },
    );
}

/// Colours the inline markers: `[Shot N]` chips, `<Subject N>` / `<Picture N>`
/// tags, dialogue tags and `(S1)` speaker ids.
fn paragraph_job(text: &str, theme: &Theme) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let chars: Vec<char> = text.chars().collect();
    let mut plain = String::new();
    let mut index = 0;
    let none = Color32::TRANSPARENT;
    while index < chars.len() {
        let rest: String = chars[index..].iter().take(40).collect();
        let closer = match chars[index] {
            '[' if rest.starts_with("[Shot") || rest.starts_with("[shot") => Some(']'),
            '<' => Some('>'),
            '(' if rest.starts_with("(S") => Some(')'),
            _ => None,
        };
        if let Some(close) = closer
            && let Some(offset) = chars[index..].iter().take(60).position(|ch| *ch == close)
        {
            let token: String = chars[index..=index + offset].iter().collect();
            let inner = token.trim_matches(['<', '>', '[', ']', '(', ')', '/']);
            let styled = match chars[index] {
                '[' => Some((theme.info, theme.info_surface)),
                '<' if inner.starts_with("Subject")
                    || inner.starts_with("Picture")
                    || inner.starts_with("image") =>
                {
                    Some((theme.accent, none))
                }
                '<' if matches!(inner, "d" | "scenetrans" | "cutoff") => {
                    Some((theme.success, none))
                }
                '(' if inner.len() <= 8
                    && inner
                        .chars()
                        .skip(1)
                        .all(|ch| ch.is_ascii_digit() || ch == ',' || ch == 'S') =>
                {
                    Some((theme.success, none))
                }
                _ => None,
            };
            if let Some((color, background)) = styled {
                push_span(&mut job, &std::mem::take(&mut plain), theme.ink, none);
                push_span(&mut job, &token, color, background);
                index += offset + 1;
                continue;
            }
        }
        plain.push(chars[index]);
        index += 1;
    }
    push_span(&mut job, &plain, theme.ink, none);
    job
}

/// Readable prompt view: coloured section headers, highlighted markers and
/// even spacing (one gap between blocks, one line height everywhere).
pub fn formatted_prompt(ui: &mut Ui, theme: &Theme, text: &str) {
    let blocks = parse_blocks(text);
    for (position, block) in blocks.iter().enumerate() {
        match block {
            Block::Header(name) => {
                if position > 0 {
                    ui.add_space(theme.space_sm);
                }
                ui.label(
                    egui::RichText::new(name.to_uppercase())
                        .size(12.5)
                        .strong()
                        .color(theme.accent),
                );
                ui.add_space(2.0);
            }
            Block::Paragraph(body) => {
                ui.add(egui::Label::new(paragraph_job(body, theme)).wrap());
                ui.add_space(theme.space_xs);
            }
        }
    }
}

/// Small clear button drawn in the top-right corner of a text field. Call it
/// right after adding the `TextEdit`. Returns true when the text was cleared.
/// Hidden while the field is empty.
pub fn clear_overlay(ui: &mut Ui, field: &Response, text: &mut String, theme: &Theme) -> bool {
    if text.is_empty() {
        return false;
    }
    let size = vec2(ICON_BUTTON - 4.0, ICON_BUTTON - 4.0);
    let origin: Pos2 = pos2(field.rect.right() - size.x - 4.0, field.rect.top() + 4.0);
    let rect = Rect::from_min_size(origin, size);
    let id_salt = format!("{:?}", field.id);
    let response = icon_button_response(ui, rect, &id_salt, "Clear");
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, theme.radius_xs, theme.danger_surface);
    }
    paint_trash(
        ui,
        rect.shrink(3.0),
        if hovered { theme.danger } else { theme.faint },
    );
    if response.clicked() {
        text.clear();
        field.request_focus();
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_blocks_splits_sections_and_collapses_blank_lines() {
        let text = "intro line\n\n\n[Shot 1] a\nb\n\noverall_soundscape: rain.\n\nnon_diegetic_music: N/A";
        let blocks = parse_blocks(text);
        assert_eq!(
            blocks,
            vec![
                Block::Paragraph("intro line".into()),
                Block::Paragraph("[Shot 1] a b".into()),
                Block::Header("overall soundscape".into()),
                Block::Paragraph("rain.".into()),
                Block::Header("non diegetic music".into()),
                Block::Paragraph("N/A".into()),
            ]
        );
    }

    #[test]
    fn ordinary_colons_are_not_headers() {
        assert_eq!(
            parse_blocks("Note: keep it short"),
            vec![Block::Paragraph("Note: keep it short".into())]
        );
    }

    #[test]
    fn markers_become_separate_spans() {
        let theme = Theme::dark(true);
        let job = paragraph_job("[Shot 1] <Picture 1> says (S1) hi", &theme);
        assert!(job.sections.len() >= 4);
    }

    #[test]
    fn clear_overlay_is_hidden_for_empty_text() {
        let ctx = egui::Context::default();
        let theme = Theme::dark(true);
        let mut cleared = true;
        let mut text = String::new();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let field = ui.add(egui::TextEdit::singleline(&mut text));
            cleared = clear_overlay(ui, &field, &mut text, &theme);
        });
        output.textures_delta.clear();
        assert!(!cleared);
    }

    #[test]
    fn clear_overlay_keeps_text_without_a_click() {
        let ctx = egui::Context::default();
        let theme = Theme::dark(true);
        let mut text = String::from("hello");
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let field = ui.add(egui::TextEdit::singleline(&mut text));
            assert!(!clear_overlay(ui, &field, &mut text, &theme));
        });
        output.textures_delta.clear();
        assert_eq!(text, "hello");
    }
}
