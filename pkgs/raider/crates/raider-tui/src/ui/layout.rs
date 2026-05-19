use ratatui::prelude::*;

use crate::app::App;

pub(crate) struct LayoutRects {
    pub messages: Rect,
    pub prompt: Rect,
    pub connector: Rect,
    pub sub_tray: Rect,
    pub tip: Rect,
    pub sidebar: Option<Rect>,
    pub modal: Rect,
    pub modal_active: bool,
    pub tip_visible: bool,
    pub tip_strip_height: u16,
}

pub(crate) fn compute_layout(
    app: &App,
    screen: Rect,
    prompt_box_height: u16,
    modal_height_request: u16,
) -> LayoutRects {
    let sidebar_min_main_width: u16 = 40;
    let sidebar_visible = app.sidebar.sidebar.visible
        && screen.width
            >= app
                .sidebar
                .sidebar
                .width
                .saturating_add(sidebar_min_main_width);
    let (main_outer, sidebar_area) = if sidebar_visible {
        let sidebar_x = screen.x + screen.width - app.sidebar.sidebar.width;
        let sidebar_rect = Rect::new(
            sidebar_x,
            screen.y,
            app.sidebar.sidebar.width,
            screen.height,
        );
        let main_full = Rect::new(
            screen.x,
            screen.y,
            screen.width.saturating_sub(app.sidebar.sidebar.width),
            screen.height,
        );
        (
            main_full.inner(Margin {
                vertical: 1,
                horizontal: 2,
            }),
            Some(sidebar_rect),
        )
    } else {
        (
            screen.inner(Margin {
                vertical: 1,
                horizontal: 2,
            }),
            None,
        )
    };
    let main = main_outer;

    let connector_height = 1u16;
    let sub_tray_height = 1u16;
    let home_hints_visible = app.sessions.sessions.current.is_none() && app.messages.is_empty();
    let tip_visible = home_hints_visible;
    let tip_strip_height: u16 = if tip_visible { 2 } else { 0 };

    let modal_active = modal_height_request > 0;
    let max_modal_height = (main.height.saturating_mul(2) / 3).max(8);
    let modal_height = modal_height_request.min(max_modal_height);

    let total_bottom = if modal_active {
        modal_height
    } else {
        prompt_box_height + connector_height + sub_tray_height + tip_strip_height
    };

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(total_bottom)])
        .spacing(1)
        .split(main);
    let messages_area = main_layout[0];
    let bottom_area = main_layout[1];

    let (prompt_area, connector_area, sub_tray_area, tip_area) = if modal_active {
        let empty = Rect {
            x: bottom_area.x,
            y: bottom_area.y,
            width: 0,
            height: 0,
        };
        (empty, empty, empty, empty)
    } else {
        let bottom_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(prompt_box_height),
                Constraint::Length(connector_height),
                Constraint::Length(sub_tray_height),
                Constraint::Length(tip_strip_height),
            ])
            .split(bottom_area);
        (
            bottom_split[0],
            bottom_split[1],
            bottom_split[2],
            bottom_split[3],
        )
    };

    LayoutRects {
        messages: messages_area,
        prompt: prompt_area,
        connector: connector_area,
        sub_tray: sub_tray_area,
        tip: tip_area,
        sidebar: sidebar_area,
        modal: bottom_area,
        modal_active,
        tip_visible,
        tip_strip_height,
    }
}
