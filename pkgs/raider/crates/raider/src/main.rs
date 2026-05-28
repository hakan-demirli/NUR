use std::{
    collections::HashMap,
    error::Error,
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

use clap::Parser;
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event as XEvent, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    Terminal,
};

use raider_host::{default_lua_plugin_paths, HostHandle, OpencodeBackend, Runtime, RuntimeConfig};
use raider_opencode::{types::session::Session, Client, ClientConfig, SessionId};
use raider_tui::action::{Action, Lifecycle, Toast, ToastVariant, UserAction, ViewAction};
use raider_tui::app::App;
use raider_tui::event::Event as AppEvent;
use raider_tui::logging::{self, LoggingConfig};
use raider_tui::ui::theme::{Mode as ThemeMode, ThemeRegistry};
use raider_tui::ui::theme_detect::{detect, OsEnv};
use raider_tui::ui::ui;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(long, env = "OPENCODE_SERVER", default_value = "http://127.0.0.1:4096")]
    server: String,

    #[arg(long)]
    directory: Option<PathBuf>,

    #[arg(short = 's', long, help = "session id to continue")]
    session: Option<String>,

    #[arg(short = 'c', long = "continue", help = "continue the last session")]
    continue_session: bool,

    #[arg(
        long,
        help = "fork the session when continuing (use with --continue or --session)"
    )]
    fork: bool,

    #[arg(long, env = "OPENCODE_TOKEN")]
    token: Option<String>,

    #[arg(long = "plugin", value_name = "PATH")]
    plugins: Vec<PathBuf>,

    #[arg(long)]
    no_plugins: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    if cli.fork && !cli.continue_session && cli.session.is_none() {
        eprintln!("raider: --fork requires --continue or --session");
        std::process::exit(2);
    }

    let _log_guard = match logging::init(LoggingConfig::default()) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("raider: logging disabled: {e}");
            None
        }
    };

    let detection_registry = ThemeRegistry::with_user_themes();
    let mut detected = detect(&OsEnv, &detection_registry);
    if detected.mode.is_none() {
        detected.mode = Some(detect_mode_via_osc());
    }
    tracing::info!(
        theme = ?detected.theme,
        mode = ?detected.mode,
        "theme detection complete",
    );

    let directory = match cli.directory.clone() {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    let directory_str = directory.to_string_lossy().to_string();

    let host = match build_host(&cli, &directory_str).await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("raider: failed to connect to {}: {e}", cli.server);
            return Err(e);
        }
    };

    install_panic_hook();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::with_user_themes_and_detection(detected.theme.as_deref(), detected.mode);

    let cwd_display = current_cwd_display();
    let initial_branch = current_branch();
    tracing::debug!(
        cwd = %cwd_display,
        branch = ?initial_branch,
        "workspace footer seeded",
    );
    app.set_workspace_cwd(Some(cwd_display));
    app.set_vcs_branch(initial_branch);
    let label = build_label();
    app.prompt.set_build_label(Some(label.clone()));
    app.sidebar.set_footer(label);

    let res = run_loop(&mut terminal, &mut app, host).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        tracing::error!(error = %e, "event loop terminated with error");
        eprintln!("raider: {e:?}");
    }
    Ok(())
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste,
            Show,
        );
        previous(info);
    }));
}

async fn build_host(cli: &Cli, directory: &str) -> Result<HostHandle, Box<dyn Error>> {
    let config = ClientConfig::new(&cli.server, directory)?.with_token(cli.token.clone());
    let client = Client::connect(config)?;
    let initial_session = resolve_initial_session(&client, cli).await?;
    let backend = Arc::new(OpencodeBackend::new(client));
    let lua_plugin_paths = if cli.plugins.is_empty() {
        default_lua_plugin_paths()
    } else {
        cli.plugins.clone()
    };
    let runtime_config = RuntimeConfig {
        initial_session,
        workspace_directory: Some(directory.to_string()),
        lua_plugin_paths,
        disable_plugins: cli.no_plugins,
        ..Default::default()
    };
    Ok(Runtime::spawn(backend, runtime_config))
}

async fn resolve_initial_session(
    client: &Client,
    cli: &Cli,
) -> Result<Option<SessionId>, Box<dyn Error>> {
    if let Some(session) = &cli.session {
        let session_id = SessionId::new(session.clone());
        let current = client.session_get(&session_id).await?;
        if cli.fork {
            let forked = client.session_fork(&session_id, None).await?;
            return Ok(Some(forked.id));
        }
        return Ok(Some(current.id));
    }

    if !cli.continue_session {
        return Ok(None);
    }

    let sessions = client.sessions_list().await?;
    let Some(base_session) = select_continue_session(&sessions) else {
        return Ok(None);
    };

    if cli.fork {
        let forked = client.session_fork(&base_session.id, None).await?;
        return Ok(Some(forked.id));
    }

    Ok(Some(base_session.id.clone()))
}

fn select_continue_session(sessions: &[Session]) -> Option<&Session> {
    sessions.iter().find(|session| session.parent_id.is_none())
}

fn is_gutter_glyph(c: char) -> bool {
    c == ' ' || matches!(c, '┃' | '│' | '┊' | '╎' | '┆' | '┇' | '┋')
}

fn gutter_run_len(line: &str) -> usize {
    line.chars().take_while(|c| is_gutter_glyph(*c)).count()
}

fn strip_common_gutter(lines: Vec<String>) -> Vec<String> {
    let mut has_content = Vec::with_capacity(lines.len());
    let mut min_run: Option<usize> = None;
    for line in &lines {
        let total = line.chars().count();
        let run = gutter_run_len(line);
        let content = run < total;
        has_content.push(content);
        if content {
            min_run = Some(min_run.map_or(run, |m| m.min(run)));
        }
    }
    let strip = min_run.unwrap_or(0);
    lines
        .into_iter()
        .zip(has_content)
        .map(|(line, content)| {
            if !content {
                String::new()
            } else {
                line.chars().skip(strip).collect()
            }
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct SelPoint {
    line: usize,
    col: u16,
}

#[derive(Default)]
struct MouseSelection {
    anchor: Option<SelPoint>,
    focus: Option<SelPoint>,
    dragging: bool,
    origin_in_messages: bool,
    last_mouse: Option<(u16, u16)>,
    line_cache: HashMap<usize, Vec<String>>,
}

fn point_in_rect(col: u16, row: u16, rect: Rect) -> bool {
    col >= rect.x
        && col < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

fn screen_to_content(col: u16, row: u16, mrect: Rect, offset: usize) -> SelPoint {
    let width = mrect.width.max(1);
    let last_col = width.saturating_sub(1);
    let max_x = mrect.x.saturating_add(last_col);
    let cx = col.clamp(mrect.x, max_x);
    let max_y = mrect.y.saturating_add(mrect.height.saturating_sub(1));
    let cy = row.clamp(mrect.y, max_y);
    let rel_col = cx.saturating_sub(mrect.x).min(last_col);
    SelPoint {
        line: offset + cy.saturating_sub(mrect.y) as usize,
        col: rel_col,
    }
}

impl MouseSelection {
    fn start(&mut self, column: u16, row: u16, mrect: Rect, offset: usize) {
        let point = screen_to_content(column, row, mrect, offset);
        self.anchor = Some(point);
        self.focus = Some(point);
        self.dragging = false;
        self.origin_in_messages = true;
        self.last_mouse = Some((column, row));
        self.line_cache.clear();
    }

    fn drag_to(&mut self, column: u16, row: u16, mrect: Rect, offset: usize) {
        if !self.origin_in_messages {
            return;
        }
        let Some(anchor) = self.anchor else {
            return;
        };
        let point = screen_to_content(column, row, mrect, offset);
        if point != anchor {
            self.dragging = true;
        }
        self.focus = Some(point);
        self.last_mouse = Some((column, row));
    }

    fn extend_to_line(&mut self, line: usize, col: u16) {
        if !self.origin_in_messages {
            return;
        }
        if let Some(anchor) = self.anchor {
            let point = SelPoint { line, col };
            if point != anchor {
                self.dragging = true;
            }
            self.focus = Some(point);
        }
    }

    fn reset(&mut self) {
        self.anchor = None;
        self.focus = None;
        self.dragging = false;
        self.origin_in_messages = false;
        self.last_mouse = None;
        self.line_cache.clear();
    }

    fn ordered(&self) -> Option<(SelPoint, SelPoint)> {
        let a = self.anchor?;
        let f = self.focus?;
        if (f.line, f.col) < (a.line, a.col) {
            Some((f, a))
        } else {
            Some((a, f))
        }
    }

    fn capture(&mut self, snapshot: &ScreenSnapshot, mrect: Rect, offset: usize) {
        if !self.origin_in_messages {
            return;
        }
        let width = mrect.width as usize;
        for ry in 0..mrect.height {
            let line = offset + ry as usize;
            let cells = snapshot.row_cells(mrect.x, mrect.y + ry, width);
            self.line_cache.insert(line, cells);
        }
    }

    fn selected_text(&self, width: u16) -> Option<String> {
        if !self.dragging {
            return None;
        }
        let (start, end) = self.ordered()?;
        if start == end {
            return None;
        }
        let last_col = width.saturating_sub(1) as usize;
        let mut lines = Vec::new();
        for line in start.line..=end.line {
            let left = if line == start.line {
                start.col as usize
            } else {
                0
            };
            let right = if line == end.line {
                end.col as usize
            } else {
                last_col
            };
            let mut s = String::new();
            if left <= right {
                if let Some(cells) = self.line_cache.get(&line) {
                    if !cells.is_empty() {
                        let r = right.min(cells.len().saturating_sub(1));
                        if left <= r {
                            for cell in &cells[left..=r] {
                                s.push_str(cell);
                            }
                        }
                    }
                }
            }
            while s.ends_with(' ') {
                s.pop();
            }
            lines.push(s);
        }
        let lines = strip_common_gutter(lines);
        let text = lines.join("\n");
        if text.trim().is_empty() {
            None
        } else {
            Some(text)
        }
    }

    fn paint(&self, buffer: &mut Buffer, mrect: Rect, offset: usize) {
        if !self.dragging || mrect.width == 0 || mrect.height == 0 {
            return;
        }
        let Some((start, end)) = self.ordered() else {
            return;
        };
        let width = mrect.width;
        let last_col = width.saturating_sub(1);
        let viewport = mrect.height as usize;
        let style = Style::default().add_modifier(Modifier::REVERSED);
        for line in start.line..=end.line {
            if line < offset || line >= offset + viewport {
                continue;
            }
            let y = mrect.y + (line - offset) as u16;
            let left_col = if line == start.line { start.col } else { 0 };
            let right_col = if line == end.line { end.col } else { last_col };
            if left_col > right_col {
                continue;
            }
            let x_start = mrect.x + left_col;
            let x_end = mrect.x + right_col.min(last_col);
            for x in x_start..=x_end {
                if let Some(cell) = buffer.cell_mut((x, y)) {
                    cell.set_style(style);
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ScreenSnapshot {
    area: Rect,
    rows: Vec<Vec<String>>,
}

impl ScreenSnapshot {
    fn from_buffer(buffer: &Buffer) -> Self {
        let area = buffer.area;
        let mut rows = Vec::with_capacity(area.height as usize);
        for y in area.y..area.y.saturating_add(area.height) {
            let mut row = Vec::with_capacity(area.width as usize);
            for x in area.x..area.x.saturating_add(area.width) {
                let symbol = buffer
                    .cell((x, y))
                    .map(|cell| cell.symbol().to_string())
                    .unwrap_or_else(|| " ".to_string());
                row.push(symbol);
            }
            rows.push(row);
        }
        Self { area, rows }
    }

    fn row_cells(&self, x_start: u16, y: u16, width: usize) -> Vec<String> {
        let mut out = vec![String::from(" "); width];
        let row_idx = y.saturating_sub(self.area.y) as usize;
        if let Some(row) = self.rows.get(row_idx) {
            let base = x_start.saturating_sub(self.area.x) as usize;
            for (i, slot) in out.iter_mut().enumerate() {
                if let Some(cell) = row.get(base + i) {
                    *slot = cell.clone();
                }
            }
        }
        out
    }
}

async fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut host: HostHandle,
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    let mut selection = MouseSelection::default();
    let mut last_autoscroll = Instant::now();
    const SCROLL_LINES_PER_TICK: i32 = 3;
    const AUTO_SCROLL_INTERVAL: Duration = Duration::from_millis(25);
    const AUTO_SCROLL_STEP: isize = 1;

    let mut dirty = true;

    loop {
        if dirty {
            let mut mrect: Option<Rect> = None;
            let frame = terminal.draw(|f| {
                ui(f, app);
                let r = app.messages.last_messages_rect;
                mrect = r;
                if let Some(r) = r {
                    selection.paint(f.buffer_mut(), r, app.scroll.list_state.offset());
                }
            })?;
            if let Some(r) = mrect {
                let snapshot = ScreenSnapshot::from_buffer(frame.buffer);
                selection.capture(&snapshot, r, app.scroll.list_state.offset());
            }
            dirty = false;
        }

        let autoscroll_dir: isize = if selection.dragging && selection.origin_in_messages {
            match (app.messages.last_messages_rect, selection.last_mouse) {
                (Some(mrect), Some((_, row))) => {
                    let top = mrect.y;
                    let bottom = mrect.y.saturating_add(mrect.height.saturating_sub(1));
                    if row <= top {
                        -1
                    } else if row >= bottom {
                        1
                    } else {
                        0
                    }
                }
                _ => 0,
            }
        } else {
            0
        };
        if autoscroll_dir != 0 && last_autoscroll.elapsed() >= AUTO_SCROLL_INTERVAL {
            last_autoscroll = Instant::now();
            if let Some(mrect) = app.messages.last_messages_rect {
                let before = app.scroll.list_state.offset();
                app.scroll
                    .scroll_messages(autoscroll_dir * AUTO_SCROLL_STEP);
                let after = app.scroll.list_state.offset();
                if after != before {
                    let viewport = mrect.height as usize;
                    let total = app.scroll.total_visual_lines;
                    let edge_line = if autoscroll_dir < 0 {
                        after
                    } else {
                        (after + viewport.saturating_sub(1)).min(total.saturating_sub(1))
                    };
                    let col = selection
                        .last_mouse
                        .map(|(c, r)| screen_to_content(c, r, mrect, after).col)
                        .unwrap_or(0);
                    selection.extend_to_line(edge_line, col);
                    dirty = true;
                }
            }
        }

        let demand = app.animation_demand();
        let mut tick = demand.idle_poll_or_animation();
        if autoscroll_dir != 0 {
            tick = tick.min(AUTO_SCROLL_INTERVAL);
        }
        let timeout = tick
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            match event::read()? {
                XEvent::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        app.dispatch(Action::User(UserAction::Key {
                            code: key.code,
                            mods: key.modifiers,
                        }));
                        dirty = true;
                    }
                }
                // BUG5: forward resize so the App can invalidate
                XEvent::Resize(cols, rows) => {
                    selection.reset();
                    app.dispatch(Action::Lifecycle(Lifecycle::Resize { cols, rows }));
                    dirty = true;
                }
                // BUG6: forward scroll-wheel events. Mouse capture
                XEvent::Mouse(mev) => {
                    use crossterm::event::{MouseButton, MouseEventKind};
                    let in_sidebar = app
                        .sidebar
                        .last_sidebar_rect
                        .map(|r| {
                            mev.column >= r.x
                                && mev.column < r.x.saturating_add(r.width)
                                && mev.row >= r.y
                                && mev.row < r.y.saturating_add(r.height)
                        })
                        .unwrap_or(false);
                    match mev.kind {
                        MouseEventKind::ScrollUp => {
                            if in_sidebar {
                                app.dispatch(Action::View(ViewAction::ScrollSidebar(
                                    -SCROLL_LINES_PER_TICK,
                                )));
                            } else {
                                app.dispatch(Action::User(UserAction::MouseScroll {
                                    lines: SCROLL_LINES_PER_TICK,
                                }));
                            }
                            dirty = true;
                        }
                        MouseEventKind::ScrollDown => {
                            if in_sidebar {
                                app.dispatch(Action::View(ViewAction::ScrollSidebar(
                                    SCROLL_LINES_PER_TICK,
                                )));
                            } else {
                                app.dispatch(Action::User(UserAction::MouseScroll {
                                    lines: -SCROLL_LINES_PER_TICK,
                                }));
                            }
                            dirty = true;
                        }
                        MouseEventKind::Down(MouseButton::Right) => {
                            let hit = app
                                .messages
                                .user_message_rects
                                .iter()
                                .rev()
                                .find(|(_, r)| {
                                    mev.column >= r.x
                                        && mev.column < r.x.saturating_add(r.width)
                                        && mev.row >= r.y
                                        && mev.row < r.y.saturating_add(r.height)
                                })
                                .map(|(id, _)| id.clone());
                            if let Some(id) = hit {
                                app.dispatch(Action::View(ViewAction::OpenMessageActions(id)));
                                dirty = true;
                            }
                        }
                        MouseEventKind::Down(MouseButton::Left) => {
                            match app.messages.last_messages_rect {
                                Some(mrect) if point_in_rect(mev.column, mev.row, mrect) => {
                                    let offset = app.scroll.list_state.offset();
                                    selection.start(mev.column, mev.row, mrect, offset);
                                }
                                _ => selection.reset(),
                            }
                        }
                        MouseEventKind::Drag(MouseButton::Left) => {
                            if selection.origin_in_messages {
                                if let Some(mrect) = app.messages.last_messages_rect {
                                    let offset = app.scroll.list_state.offset();
                                    selection.drag_to(mev.column, mev.row, mrect, offset);
                                    dirty = true;
                                }
                            }
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            if selection.origin_in_messages {
                                if let Some(mrect) = app.messages.last_messages_rect {
                                    let offset = app.scroll.list_state.offset();
                                    selection.drag_to(mev.column, mev.row, mrect, offset);
                                }
                            }
                            let copied_selection = if selection.dragging {
                                let width = app
                                    .messages
                                    .last_messages_rect
                                    .map(|r| r.width)
                                    .unwrap_or(0);
                                if let Some(text) = selection.selected_text(width) {
                                    let _ = handle_clipboard_copy(&text);
                                }
                                dirty = true;
                                true
                            } else {
                                false
                            };
                            selection.reset();

                            if !copied_selection {
                                let sidebar_hit = app
                                    .sidebar
                                    .sidebar_header_rects
                                    .iter()
                                    .find(|(_, r)| {
                                        mev.column >= r.x
                                            && mev.column < r.x.saturating_add(r.width)
                                            && mev.row >= r.y
                                            && mev.row < r.y.saturating_add(r.height)
                                    })
                                    .map(|(slot, _)| *slot);
                                if let Some(slot) = sidebar_hit {
                                    app.dispatch(Action::View(ViewAction::ToggleSidebarSection(
                                        slot,
                                    )));
                                    dirty = true;
                                } else {
                                    let hit = app
                                        .messages
                                        .tool_block_rects
                                        .iter()
                                        .rev()
                                        .find(|(_, r)| {
                                            mev.column >= r.x
                                                && mev.column < r.x.saturating_add(r.width)
                                                && mev.row >= r.y
                                                && mev.row < r.y.saturating_add(r.height)
                                        })
                                        .map(|(id, _)| id.clone());
                                    if let Some(id) = hit {
                                        app.dispatch(Action::View(
                                            ViewAction::ToggleToolExpanded { id },
                                        ));
                                        dirty = true;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                XEvent::Paste(text) => {
                    app.dispatch(Action::User(UserAction::PasteText(text)));
                    dirty = true;
                }
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick {
            last_tick = Instant::now();
            if demand.any() {
                app.dispatch(Action::Lifecycle(Lifecycle::Tick));
                dirty = true;
            }
        }

        {
            let mut rx = host.actions.lock().await;
            while let Ok(action) = rx.try_recv() {
                app.dispatch(action);
                dirty = true;
            }
        }

        for ev in app.take_events() {
            tracing::info!(event = ?ev, "app event");
            match &ev {
                AppEvent::Export {
                    suggested_filename,
                    markdown,
                } => handle_export(app, suggested_filename, markdown),
                AppEvent::CopyToClipboard {
                    text,
                    success_message,
                    error_message,
                } => match handle_clipboard_copy(text) {
                    Ok(()) => app.dispatch(Action::View(ViewAction::ShowToast(Toast::new(
                        success_message.clone(),
                        ToastVariant::Success,
                    )))),
                    Err(e) => {
                        tracing::warn!(error = %e, "clipboard copy failed");
                        app.dispatch(Action::View(ViewAction::ShowToast(Toast::new(
                            error_message.clone(),
                            ToastVariant::Error,
                        ))));
                    }
                },
                AppEvent::OpenUrl(url) => handle_open_url(app, url),
                _ => {}
            }
            let _ = host.ui_events.send(ev.clone());
            if matches!(ev, AppEvent::Quit) {
                host.shutdown();
                return Ok(());
            }
        }

        if app.should_quit() {
            host.shutdown();
            return Ok(());
        }
    }
}

fn handle_export(app: &mut App, suggested_filename: &str, markdown: &str) {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "export aborted: cannot read cwd");
            app.dispatch(Action::View(ViewAction::ShowToast(Toast::new(
                format!("Failed to read cwd while exporting: {e}"),
                ToastVariant::Error,
            ))));
            return;
        }
    };
    let target = cwd.join(suggested_filename);
    match std::fs::write(&target, markdown) {
        Ok(()) => {
            tracing::info!(path = %target.display(), "session exported");
            app.dispatch(Action::View(ViewAction::ShowToast(Toast::new(
                format!("Session exported to {suggested_filename}"),
                ToastVariant::Success,
            ))));
        }
        Err(e) => {
            tracing::warn!(path = %target.display(), error = %e, "export write failed");
            app.dispatch(Action::View(ViewAction::ShowToast(Toast::new(
                format!("Failed to write {}: {e}", target.display()),
                ToastVariant::Error,
            ))));
        }
    }
}

fn handle_clipboard_copy(text: &str) -> io::Result<()> {
    let base64 = base64_encode(text.as_bytes());
    let osc52 = format!("\x1b]52;c;{base64}\x07");
    let passthrough = std::env::var_os("TMUX").is_some() || std::env::var_os("STY").is_some();
    let sequence = if passthrough {
        format!("\x1bPtmux;\x1b{osc52}\x1b\\")
    } else {
        osc52
    };
    use std::io::Write;
    let mut out = io::stdout();
    out.write_all(sequence.as_bytes())
        .and_then(|_| out.flush())?;
    tracing::info!(bytes = text.len(), "wrote clipboard via OSC 52");
    Ok(())
}

fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[(b0 >> 2) as usize] as char);
        out.push(ALPHABET[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(((b1 & 0b1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(b2 & 0b111111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn handle_open_url(app: &mut App, url: &str) {
    #[cfg(target_os = "macos")]
    let opener = ("open", &[] as &[&str]);
    #[cfg(target_os = "windows")]
    let opener = ("cmd", &["/C", "start", ""] as &[&str]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let opener = ("xdg-open", &[] as &[&str]);

    let (cmd, args) = opener;
    let mut command = Command::new(cmd);
    command.args(args).arg(url);
    match command.spawn() {
        Ok(_) => {
            tracing::info!(%url, "launched browser");
        }
        Err(e) => {
            tracing::warn!(%url, error = %e, "failed to open URL");
            app.dispatch(Action::View(ViewAction::ShowToast(Toast::new(
                format!("Failed to open {url}: {e}"),
                ToastVariant::Error,
            ))));
        }
    }
}

fn build_label() -> String {
    let hash = env!("RAIDER_GIT_SHORT_HASH");
    if hash.is_empty() {
        format!("raider {}", env!("CARGO_PKG_VERSION"))
    } else {
        format!("raider {hash}")
    }
}

fn current_cwd_display() -> String {
    match std::env::current_dir() {
        Ok(p) => collapse_home(&p),
        Err(_) => String::from("?"),
    }
}

fn current_branch() -> Option<String> {
    std::env::current_dir()
        .ok()
        .as_deref()
        .and_then(current_git_branch)
}

fn collapse_home(path: &Path) -> String {
    let s = path.display().to_string();
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy().into_owned();
        if !home.is_empty() && s.starts_with(&home) {
            return format!("~{}", &s[home.len()..]);
        }
    }
    s
}

fn current_git_branch(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn detect_mode_via_osc() -> ThemeMode {
    use terminal_colorsaurus::{theme_mode, QueryOptions, ThemeMode as TcsMode};
    let mut opts = QueryOptions::default();
    opts.timeout = Duration::from_millis(250);
    match theme_mode(opts) {
        Ok(TcsMode::Light) => ThemeMode::Light,
        Ok(TcsMode::Dark) => ThemeMode::Dark,
        Err(_) => ThemeMode::Dark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, parent: Option<&str>) -> Session {
        Session {
            id: SessionId::new(id),
            title: id.to_string(),
            parent_id: parent.map(SessionId::new),
            time: Default::default(),
            extra: Default::default(),
        }
    }

    #[test]
    fn cli_accepts_opencode_resume_flags() {
        let cli = Cli::try_parse_from(["raider", "-c", "--fork"]).unwrap();
        assert!(cli.continue_session);
        assert!(cli.fork);

        let cli = Cli::try_parse_from(["raider", "-s", "ses_123"]).unwrap();
        assert_eq!(cli.session.as_deref(), Some("ses_123"));
    }

    #[test]
    fn cli_rejects_single_dash_fork() {
        assert!(Cli::try_parse_from(["raider", "-fork"]).is_err());
    }

    #[test]
    fn continue_session_uses_first_root_session() {
        let sessions = vec![
            session("child", Some("parent")),
            session("root-a", None),
            session("root-b", None),
        ];

        let picked = select_continue_session(&sessions).expect("root session");
        assert_eq!(picked.id.as_str(), "root-a");
    }

    #[test]
    fn selection_extracts_single_line_and_strips_gutter() {
        let mrect = Rect::new(0, 0, 20, 1);
        let mut buffer = Buffer::empty(mrect);
        buffer.set_string(0, 0, "   hello world", Style::default());
        let snapshot = ScreenSnapshot::from_buffer(&buffer);

        let mut selection = MouseSelection::default();
        selection.start(3, 0, mrect, 0);
        selection.drag_to(7, 0, mrect, 0);
        selection.capture(&snapshot, mrect, 0);

        assert_eq!(
            selection.selected_text(mrect.width).as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn selection_gutter_is_stripped_even_when_dragged_from_col_zero() {
        let mrect = Rect::new(0, 0, 20, 1);
        let mut buffer = Buffer::empty(mrect);
        buffer.set_string(0, 0, "   hello world", Style::default());
        let snapshot = ScreenSnapshot::from_buffer(&buffer);

        let mut selection = MouseSelection::default();
        selection.start(0, 0, mrect, 0);
        selection.drag_to(7, 0, mrect, 0);
        selection.capture(&snapshot, mrect, 0);

        assert_eq!(
            selection.selected_text(mrect.width).as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn selection_extracts_multiline_full_lines_backwards_dedented() {
        let mrect = Rect::new(0, 0, 8, 2);
        let mut buffer = Buffer::empty(mrect);
        buffer.set_string(0, 0, "   abcde", Style::default());
        buffer.set_string(0, 1, "   fghij", Style::default());
        let snapshot = ScreenSnapshot::from_buffer(&buffer);

        let mut selection = MouseSelection::default();
        selection.start(7, 1, mrect, 0);
        selection.drag_to(0, 0, mrect, 0);
        selection.capture(&snapshot, mrect, 0);

        assert_eq!(
            selection.selected_text(mrect.width).as_deref(),
            Some("abcde\nfghij")
        );
    }

    #[test]
    fn selection_preserves_leading_digits_of_line_numbers() {
        let mrect = Rect::new(0, 0, 12, 1);
        let mut buffer = Buffer::empty(mrect);
        buffer.set_string(0, 0, "   10 foo", Style::default());
        let snapshot = ScreenSnapshot::from_buffer(&buffer);

        let mut selection = MouseSelection::default();
        selection.start(3, 0, mrect, 0);
        selection.drag_to(4, 0, mrect, 0);
        selection.capture(&snapshot, mrect, 0);

        assert_eq!(selection.selected_text(mrect.width).as_deref(), Some("10"));
    }

    #[test]
    fn selection_keeps_diff_line_numbers_with_bar_prefix() {
        let mrect = Rect::new(0, 0, 16, 2);
        let mut buffer = Buffer::empty(mrect);
        buffer.set_string(0, 0, "┃  9  ctx_a", Style::default());
        buffer.set_string(0, 1, "┃ 10  ctx_b", Style::default());
        let snapshot = ScreenSnapshot::from_buffer(&buffer);

        let mut selection = MouseSelection::default();
        selection.start(0, 0, mrect, 0);
        selection.drag_to(15, 1, mrect, 0);
        selection.capture(&snapshot, mrect, 0);

        assert_eq!(
            selection.selected_text(mrect.width).as_deref(),
            Some(" 9  ctx_a\n10  ctx_b")
        );
    }

    #[test]
    fn strip_common_gutter_keeps_relative_indentation() {
        let lines = vec![
            "   fn foo() {".to_string(),
            "       bar();".to_string(),
            "   }".to_string(),
        ];
        assert_eq!(
            strip_common_gutter(lines),
            vec![
                "fn foo() {".to_string(),
                "    bar();".to_string(),
                "}".to_string(),
            ]
        );
    }

    #[test]
    fn selection_uses_content_coordinates_across_scroll() {
        let mrect = Rect::new(0, 0, 20, 1);
        let mut selection = MouseSelection::default();
        selection.start(3, 0, mrect, 5);
        selection.drag_to(7, 0, mrect, 5);

        let mut buffer = Buffer::empty(mrect);
        buffer.set_string(0, 0, "   hello world", Style::default());
        let snapshot = ScreenSnapshot::from_buffer(&buffer);
        selection.capture(&snapshot, mrect, 5);

        assert_eq!(
            selection.selected_text(mrect.width).as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn mouse_selection_paints_active_drag() {
        let mrect = Rect::new(0, 0, 8, 1);
        let mut buffer = Buffer::empty(mrect);
        buffer.set_string(0, 0, "   abcde", Style::default());
        let mut selection = MouseSelection::default();
        selection.start(4, 0, mrect, 0);
        selection.drag_to(6, 0, mrect, 0);

        selection.paint(&mut buffer, mrect, 0);

        assert!(buffer
            .cell((5, 0))
            .expect("selected cell")
            .modifier
            .contains(Modifier::REVERSED));
        assert!(!buffer
            .cell((0, 0))
            .expect("gutter cell")
            .modifier
            .contains(Modifier::REVERSED));
    }
}
