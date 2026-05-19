use ratatui::prelude::*;

use crate::ui::theme::Theme;

pub(crate) fn draw_scrollbar(
    f: &mut Frame,
    body_area: Rect,
    total_lines: usize,
    offset: usize,
    max_offset: usize,
    theme: &Theme,
    panel_bg: Color,
) {
    if body_area.width == 0 || body_area.height == 0 {
        return;
    }
    let scrollbar_x = body_area.x + body_area.width - 1;
    let track_height = body_area.height as usize;
    let thumb_height = (track_height * track_height / total_lines.max(1)).max(1);
    let thumb_top = if max_offset == 0 {
        0
    } else {
        offset * track_height.saturating_sub(thumb_height) / max_offset
    };
    let buf = f.buffer_mut();
    let thumb_style = Style::default().fg(theme.text_muted).bg(panel_bg);
    let track_style = Style::default().bg(panel_bg);
    for row in 0..track_height {
        let y = body_area.y + row as u16;
        let in_thumb = row >= thumb_top && row < thumb_top + thumb_height;
        let cell = &mut buf[(scrollbar_x, y)];
        if in_thumb {
            cell.set_symbol("█");
            cell.set_style(thumb_style);
        } else {
            cell.set_symbol(" ");
            cell.set_style(track_style);
        }
    }
}
