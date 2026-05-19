use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::ui::theme::Theme;

pub const RAIDER_LOGO: [&str; 4] = [
    "                             ",
    "█▀▀█ █▀▀█ ▀██▀ █▀▀▄ █▀▀▀ █▀▀█",
    "█▀▀▄ █^^█ _██_ █__█ █^^^ █▀▀▄",
    "▀  ▀ ▀  ▀ ▀▀▀▀ ▀▀▀  ▀▀▀▀ ▀  ▀",
];

pub const RAIDER_LOGO_WIDTH: u16 = 29;

pub const RAIDER_LOGO_HEIGHT: u16 = 4;

fn shadow_color(background: Color, ink: Color) -> Color {
    blend(background, ink, 0.25)
}

fn blend(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let lerp = |x: u8, y: u8| -> u8 {
                let xf = x as f32;
                let yf = y as f32;
                (xf + (yf - xf) * t).round().clamp(0.0, 255.0) as u8
            };
            Color::Rgb(lerp(ar, br), lerp(ag, bg), lerp(ab, bb))
        }
        _ => b,
    }
}

pub fn render_logo(f: &mut Frame, area: Rect, theme: &Theme) {
    if area.width < RAIDER_LOGO_WIDTH || area.height < RAIDER_LOGO_HEIGHT {
        return;
    }

    let background = theme.background;
    let ink = theme.primary;
    let shadow = shadow_color(background, ink);

    let x_off = area.x + (area.width.saturating_sub(RAIDER_LOGO_WIDTH)) / 2;
    let y_off = area.y + (area.height.saturating_sub(RAIDER_LOGO_HEIGHT)) / 2;

    for (row_idx, row) in RAIDER_LOGO.iter().enumerate() {
        let line = render_row(row, ink, shadow, background);
        let target = Rect {
            x: x_off,
            y: y_off + row_idx as u16,
            width: RAIDER_LOGO_WIDTH,
            height: 1,
        };
        f.render_widget(Paragraph::new(line), target);
    }
}

pub fn render_row<'a>(row: &'a str, ink: Color, shadow: Color, background: Color) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut buf = String::new();
    let mut current: Option<(Color, Color)> = None;

    for ch in row.chars() {
        let (glyph, fg, bg) = classify(ch, ink, shadow, background);
        match current {
            Some((cf, cb)) if cf == fg && cb == bg => {
                buf.push(glyph);
            }
            _ => {
                if !buf.is_empty() {
                    let (cf, cb) = current.expect("buf non-empty implies style set");
                    spans.push(Span::styled(
                        std::mem::take(&mut buf),
                        Style::default().fg(cf).bg(cb),
                    ));
                }
                buf.push(glyph);
                current = Some((fg, bg));
            }
        }
    }
    if let Some((cf, cb)) = current {
        if !buf.is_empty() {
            spans.push(Span::styled(buf, Style::default().fg(cf).bg(cb)));
        }
    }
    Line::from(spans)
}

fn classify(ch: char, ink: Color, shadow: Color, background: Color) -> (char, Color, Color) {
    match ch {
        '_' => (' ', ink, shadow),
        '^' => ('▀', ink, shadow),
        '~' => ('▀', shadow, background),
        c => (c, ink, background),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_rows_are_all_expected_width() {
        for (i, row) in RAIDER_LOGO.iter().enumerate() {
            let cols = row.chars().count();
            assert_eq!(
                cols, RAIDER_LOGO_WIDTH as usize,
                "row {i} should be {} cells wide, got {cols}: {row:?}",
                RAIDER_LOGO_WIDTH,
            );
        }
        assert_eq!(RAIDER_LOGO.len(), RAIDER_LOGO_HEIGHT as usize);
    }

    #[test]
    fn classify_handles_each_marker_per_opencode_semantics() {
        let ink = Color::Rgb(0xff, 0x00, 0x00);
        let shadow = Color::Rgb(0x33, 0x00, 0x00);
        let bg = Color::Rgb(0x00, 0x00, 0x00);

        assert_eq!(classify('_', ink, shadow, bg), (' ', ink, shadow));
        assert_eq!(classify('^', ink, shadow, bg), ('▀', ink, shadow));
        assert_eq!(classify('~', ink, shadow, bg), ('▀', shadow, bg));
        assert_eq!(classify('█', ink, shadow, bg), ('█', ink, bg));
        assert_eq!(classify(' ', ink, shadow, bg), (' ', ink, bg));
    }

    #[test]
    fn blend_midpoint_is_arithmetic_mean() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(200, 100, 50);
        assert_eq!(blend(a, b, 0.5), Color::Rgb(100, 50, 25));
        assert_eq!(blend(a, b, 0.0), a);
        assert_eq!(blend(a, b, 1.0), b);
    }

    #[test]
    fn shadow_color_is_25pct_tint_of_ink() {
        let bg = Color::Rgb(0, 0, 0);
        let ink = Color::Rgb(200, 100, 50);
        assert_eq!(shadow_color(bg, ink), Color::Rgb(50, 25, 13));
    }

    #[test]
    fn render_row_yields_one_line_per_input() {
        let ink = Color::Rgb(255, 0, 0);
        let shadow = Color::Rgb(40, 0, 0);
        let bg = Color::Rgb(0, 0, 0);
        let row = "█^^█";
        let line = render_row(row, ink, shadow, bg);
        let rendered: String = line.spans.iter().flat_map(|s| s.content.chars()).collect();
        assert_eq!(rendered, "█▀▀█");
    }
}
