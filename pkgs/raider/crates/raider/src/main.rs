use std::{
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
    layout::{Position, Rect},
    style::{Modifier, Style},
    Terminal,
};

use raider_host::{default_lua_plugin_paths, HostHandle, OpencodeBackend, Runtime, RuntimeConfig};
use raider_opencode::{Client, ClientConfig};
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

    #[arg(long)]
    session: Option<String>,

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

    let host = match build_host(&cli, &directory_str) {
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

fn build_host(cli: &Cli, directory: &str) -> Result<HostHandle, Box<dyn Error>> {
    let config = ClientConfig::new(&cli.server, directory)?.with_token(cli.token.clone());
    let client = Client::connect(config)?;
    let backend = Arc::new(OpencodeBackend::new(client));
    let lua_plugin_paths = if cli.plugins.is_empty() {
        default_lua_plugin_paths()
    } else {
        cli.plugins.clone()
    };
    let runtime_config = RuntimeConfig {
        initial_session: cli.session.as_deref().map(raider_opencode::SessionId::new),
        workspace_directory: Some(directory.to_string()),
        lua_plugin_paths,
        disable_plugins: cli.no_plugins,
        ..Default::default()
    };
    Ok(Runtime::spawn(backend, runtime_config))
}

#[derive(Default)]
struct MouseSelection {
    anchor: Option<Position>,
    focus: Option<Position>,
    dragging: bool,
}

impl MouseSelection {
    fn start(&mut self, column: u16, row: u16) {
        let point = Position { x: column, y: row };
        self.anchor = Some(point);
        self.focus = Some(point);
        self.dragging = false;
    }

    fn drag_to(&mut self, column: u16, row: u16) {
        let Some(anchor) = self.anchor else {
            return;
        };
        let point = Position { x: column, y: row };
        if point != anchor {
            self.dragging = true;
        }
        self.focus = Some(point);
    }

    fn finish(&mut self, column: u16, row: u16) -> Option<(Position, Position)> {
        self.drag_to(column, row);
        let range = match (self.dragging, self.anchor, self.focus) {
            (true, Some(anchor), Some(focus)) if anchor != focus => Some((anchor, focus)),
            _ => None,
        };
        self.clear();
        range
    }

    fn clear(&mut self) {
        self.anchor = None;
        self.focus = None;
        self.dragging = false;
    }

    fn paint(&self, buffer: &mut Buffer) {
        let Some(range) = selected_range(self.anchor, self.focus, buffer.area) else {
            return;
        };
        if !self.dragging {
            return;
        }

        let style = Style::default().add_modifier(Modifier::REVERSED);
        for y in range.start.y..=range.end.y {
            let row_start = if y == range.start.y {
                range.start.x
            } else {
                buffer.area.x
            };
            let row_end = if y == range.end.y {
                range.end.x
            } else {
                buffer
                    .area
                    .x
                    .saturating_add(buffer.area.width.saturating_sub(1))
            };
            for x in row_start..=row_end {
                if let Some(cell) = buffer.cell_mut((x, y)) {
                    cell.set_style(style);
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct SelectionRange {
    start: Position,
    end: Position,
}

fn selected_range(
    anchor: Option<Position>,
    focus: Option<Position>,
    area: Rect,
) -> Option<SelectionRange> {
    let anchor = clamp_to_area(anchor?, area)?;
    let focus = clamp_to_area(focus?, area)?;
    if anchor == focus {
        return None;
    }
    let (start, end) = if (focus.y, focus.x) < (anchor.y, anchor.x) {
        (focus, anchor)
    } else {
        (anchor, focus)
    };
    Some(SelectionRange { start, end })
}

fn clamp_to_area(point: Position, area: Rect) -> Option<Position> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let max_x = area.x.saturating_add(area.width.saturating_sub(1));
    let max_y = area.y.saturating_add(area.height.saturating_sub(1));
    Some(Position {
        x: point.x.clamp(area.x, max_x),
        y: point.y.clamp(area.y, max_y),
    })
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

    fn selected_text(&self, anchor: Position, focus: Position) -> Option<String> {
        let range = selected_range(Some(anchor), Some(focus), self.area)?;
        let mut lines = Vec::new();

        for y in range.start.y..=range.end.y {
            let Some(row) = self.rows.get((y.saturating_sub(self.area.y)) as usize) else {
                continue;
            };
            if row.is_empty() {
                lines.push(String::new());
                continue;
            }

            let row_start = if y == range.start.y {
                range.start.x
            } else {
                self.area.x
            };
            let row_end = if y == range.end.y {
                range.end.x
            } else {
                self.area
                    .x
                    .saturating_add(self.area.width.saturating_sub(1))
            };
            let start_idx = row_start.saturating_sub(self.area.x) as usize;
            let end_idx = row_end.saturating_sub(self.area.x) as usize;
            let end_idx = end_idx.min(row.len().saturating_sub(1));

            let mut line = String::new();
            if start_idx <= end_idx {
                for cell in &row[start_idx..=end_idx] {
                    line.push_str(cell);
                }
            }
            while line.ends_with(' ') {
                line.pop();
            }
            lines.push(line);
        }

        let text = lines.join("\n");
        if text.trim().is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

async fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut host: HostHandle,
) -> io::Result<()> {
    let tick = Duration::from_millis(50);
    let mut last_tick = Instant::now();
    let mut selection = MouseSelection::default();
    let mut last_screen: Option<ScreenSnapshot> = None;
    const SCROLL_LINES_PER_TICK: i32 = 3;

    let mut dirty = true;

    loop {
        if dirty {
            let frame = terminal.draw(|f| {
                ui(f, app);
                selection.paint(f.buffer_mut());
            })?;
            last_screen = Some(ScreenSnapshot::from_buffer(frame.buffer));
            dirty = false;
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
                    selection.clear();
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
                            selection.start(mev.column, mev.row);
                        }
                        MouseEventKind::Drag(MouseButton::Left) => {
                            selection.drag_to(mev.column, mev.row);
                            dirty = true;
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            let copied_selection = if let Some((anchor, focus)) =
                                selection.finish(mev.column, mev.row)
                            {
                                if let Some(text) = last_screen
                                    .as_ref()
                                    .and_then(|screen| screen.selected_text(anchor, focus))
                                {
                                    let _ = handle_clipboard_copy(&text);
                                }
                                dirty = true;
                                true
                            } else {
                                false
                            };

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
            app.dispatch(Action::Lifecycle(Lifecycle::Tick));
            last_tick = Instant::now();
            dirty = true;
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

    #[test]
    fn screen_snapshot_extracts_single_line_selection() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 2));
        buffer.set_string(0, 0, "hello world", Style::default());
        let snapshot = ScreenSnapshot::from_buffer(&buffer);

        assert_eq!(
            snapshot
                .selected_text(Position { x: 0, y: 0 }, Position { x: 4, y: 0 })
                .as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn screen_snapshot_extracts_multiline_selection_backwards() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 2));
        buffer.set_string(0, 0, "abcde", Style::default());
        buffer.set_string(0, 1, "fghij", Style::default());
        let snapshot = ScreenSnapshot::from_buffer(&buffer);

        assert_eq!(
            snapshot
                .selected_text(Position { x: 1, y: 1 }, Position { x: 3, y: 0 })
                .as_deref(),
            Some("de\nfg")
        );
    }

    #[test]
    fn mouse_selection_paints_active_drag() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
        buffer.set_string(0, 0, "abcde", Style::default());
        let mut selection = MouseSelection::default();
        selection.start(1, 0);
        selection.drag_to(3, 0);

        selection.paint(&mut buffer);

        assert!(buffer
            .cell((2, 0))
            .expect("selected cell")
            .modifier
            .contains(Modifier::REVERSED));
        assert!(!buffer
            .cell((0, 0))
            .expect("unselected cell")
            .modifier
            .contains(Modifier::REVERSED));
    }
}
