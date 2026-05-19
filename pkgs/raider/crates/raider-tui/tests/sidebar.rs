// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn sidebar_hidden_by_default() {
    let h = Harness::new(120, 30);
    assert!(!h.app.sidebar.sidebar.visible, "default invisibility");
}

#[test]
fn sidebar_renders_title_and_section_lines_when_made_visible() {
    let mut h = Harness::new(120, 30);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Refactor raider TUI");
    h.app
        .sidebar
        .set_subtitle(Some("workspace: dotfiles".to_string()));
    h.app.sidebar.set_sections(vec![SidebarSection::new(
        "Context",
        ["src/app.rs", "src/ui.rs"],
    )]);
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Refactor raider TUI"),
        "title visible:\n{snap}"
    );
    assert!(
        snap.contains("workspace: dotfiles"),
        "subtitle visible:\n{snap}"
    );
    assert!(snap.contains("Context"), "section header visible:\n{snap}");
    assert!(
        !snap.contains("CONTEXT"),
        "section header must NOT be uppercased (opencode-parity regression):\n{snap}"
    );
    assert!(snap.contains("src/app.rs"), "section line visible:\n{snap}");
    assert!(
        snap.contains("raider v") || snap.contains("raider "),
        "footer with version visible:\n{snap}"
    );
}

#[test]
fn sidebar_section_header_is_bold_white_and_subitems_muted() {
    use ratatui::style::Modifier;
    let mut h = Harness::new(140, 30);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Some Session");
    h.app.sidebar.set_sections(vec![SidebarSection::new(
        "Context",
        ["12,345 tokens", "5% used", "$1.23 spent"],
    )]);
    h.draw();

    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let header_y = lines
        .iter()
        .position(|l| l.contains("Context"))
        .unwrap_or_else(|| panic!("Context heading missing in snapshot:\n{snap}"))
        as u16;
    let tokens_y = lines
        .iter()
        .position(|l| l.contains("12,345 tokens"))
        .unwrap_or_else(|| panic!("tokens line missing in snapshot:\n{snap}"))
        as u16;

    let buf = h.terminal.backend().buffer();
    let theme = &h.app.theme.theme;

    let first_content_x = |y: u16| -> u16 {
        for x in 0..buf.area.width {
            if buf[(x, y)].symbol() != " " && !buf[(x, y)].symbol().is_empty() {
                return x;
            }
        }
        0
    };

    let header_cell = &buf[(first_content_x(header_y), header_y)];
    let tokens_cell = &buf[(first_content_x(tokens_y), tokens_y)];

    assert_eq!(
        header_cell
            .style()
            .fg
            .unwrap_or(ratatui::style::Color::Reset),
        theme.text,
        "Context header fg must be theme.text (white); cell={header_cell:?}",
    );
    assert!(
        header_cell.style().add_modifier.contains(Modifier::BOLD),
        "Context header must be BOLD; cell={header_cell:?}",
    );

    assert_eq!(
        tokens_cell
            .style()
            .fg
            .unwrap_or(ratatui::style::Color::Reset),
        theme.text_muted,
        "Context sub-item (`12,345 tokens`) fg must be theme.text_muted; \
         cell={tokens_cell:?}",
    );
    assert!(
        !tokens_cell.style().add_modifier.contains(Modifier::BOLD),
        "Context sub-item must NOT be BOLD; cell={tokens_cell:?}",
    );
}

#[test]
fn sidebar_context_section_renders_with_bold_heading_and_muted_body() {
    use ratatui::style::Modifier;
    let mut h = Harness::new(140, 30);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Some Session");
    h.app.sidebar.set_sections(vec![SidebarSection::new(
        "Context",
        ["12,345 tokens", "45% used", "$0.23 spent"],
    )]);
    h.draw();

    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let buf = h.terminal.backend().buffer();
    let theme = &h.app.theme.theme;

    let first_content_x = |y: u16| -> u16 {
        for x in 0..buf.area.width {
            if buf[(x, y)].symbol() != " " && !buf[(x, y)].symbol().is_empty() {
                return x;
            }
        }
        0
    };

    let header_y = lines
        .iter()
        .position(|l| l.contains("Context"))
        .unwrap_or_else(|| panic!("Context heading missing in snapshot:\n{snap}"))
        as u16;
    let header_cell = &buf[(first_content_x(header_y), header_y)];
    assert_eq!(
        header_cell
            .style()
            .fg
            .unwrap_or(ratatui::style::Color::Reset),
        theme.text,
        "Context heading fg must be theme.text; cell={header_cell:?}",
    );
    assert!(
        header_cell.style().add_modifier.contains(Modifier::BOLD),
        "Context heading must be BOLD; cell={header_cell:?}",
    );

    for label in ["12,345 tokens", "45% used", "$0.23 spent"] {
        let y = lines
            .iter()
            .position(|l| l.contains(label))
            .unwrap_or_else(|| panic!("`{label}` body row missing in snapshot:\n{snap}"))
            as u16;
        let cell = &buf[(first_content_x(y), y)];
        assert_eq!(
            cell.style().fg.unwrap_or(ratatui::style::Color::Reset),
            theme.text_muted,
            "Context body row `{label}` fg must be theme.text_muted; cell={cell:?}",
        );
        assert!(
            !cell.style().add_modifier.contains(Modifier::BOLD),
            "Context body row `{label}` must NOT be BOLD; cell={cell:?}",
        );
    }
}

#[test]
fn sidebar_todo_and_files_headers_are_bold_white() {
    use raider_tui::{FileChange, TodoEntry};
    use ratatui::style::Modifier;
    let mut h = Harness::new(140, 40);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Some Session");
    h.app.sidebar.set_sections(vec![
        SidebarSection::todos(
            "Todo",
            vec![TodoEntry::new("write more tests", "in_progress")],
        ),
        SidebarSection::files("Modified Files", vec![FileChange::new("src/ui.rs", 10, 2)]),
    ]);
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let buf = h.terminal.backend().buffer();
    let theme = &h.app.theme.theme;

    for label in ["Todo", "Modified Files"] {
        let y = lines
            .iter()
            .position(|l| l.contains(label))
            .unwrap_or_else(|| panic!("`{label}` heading missing in snapshot:\n{snap}"))
            as u16;
        let first_char = label.chars().next().unwrap().to_string();
        let mut anchor_x = None;
        for x in 0..buf.area.width {
            let s = buf[(x, y)].symbol();
            if s == first_char {
                anchor_x = Some(x);
                break;
            }
        }
        let x = anchor_x
            .unwrap_or_else(|| panic!("could not find anchor cell for `{label}` on row {y}"));
        let cell = &buf[(x, y)];
        assert_eq!(
            cell.style().fg.unwrap_or(ratatui::style::Color::Reset),
            theme.text,
            "`{label}` header fg must be theme.text; cell={cell:?}",
        );
        assert!(
            cell.style().add_modifier.contains(Modifier::BOLD),
            "`{label}` header must be BOLD; cell={cell:?}",
        );
    }
}

#[test]
fn sidebar_title_wraps_to_multiple_visual_rows() {
    let mut h = Harness::new(120, 30);
    h.app.sidebar.set_visible(true);
    h.app
        .sidebar
        .set_title("This is a deliberately long session title that wraps across rows");
    h.draw();
    let snap = h.snapshot();

    assert!(
        snap.contains("This is a deliberately long session"),
        "first wrapped title row missing:\n{snap}",
    );
    assert!(
        snap.contains("title that wraps across rows"),
        "second wrapped title row missing (title must not be clipped to one row):\n{snap}",
    );
}

#[test]
fn sidebar_todo_content_wraps_with_fixed_checkbox_prefix() {
    use raider_tui::TodoEntry;

    let mut h = Harness::new(120, 34);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Session");
    h.app.sidebar.set_sections(vec![SidebarSection::todos(
        "Todo",
        vec![TodoEntry::new(
            "Implement selective sidebar wrapping so long todo text continues on the following visual row",
            "in_progress",
        )],
    )]);
    h.draw();
    let snap = h.snapshot();

    assert!(
        snap.contains("[•] Implement selective sidebar"),
        "first todo row must include the fixed status prefix:\n{snap}",
    );
    let continuation = snap
        .lines()
        .find(|line| line.contains("wrapping so long todo text"))
        .unwrap_or_else(|| panic!("wrapped todo continuation row missing:\n{snap}"));
    assert!(
        continuation.contains("    wrapping so long todo text"),
        "continuation row must align after the four-cell `[•] ` prefix: {continuation:?}\n{snap}",
    );
    assert!(
        !continuation.contains("[•]"),
        "continuation row must not repeat the checkbox prefix: {continuation:?}\n{snap}",
    );
}

#[test]
fn sidebar_modified_files_remain_single_line_and_truncated() {
    use raider_tui::FileChange;

    let mut h = Harness::new(120, 34);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Session");
    h.app.sidebar.set_sections(vec![SidebarSection::files(
        "Modified Files",
        vec![FileChange::new(
            "rtl/rv_core/vendor_xilinx_templates/really_long_directory/very_long_file_name_that_should_not_wrap.sv",
            77,
            3,
        )],
    )]);
    h.draw();
    let snap = h.snapshot();
    let file_rows: Vec<&str> = snap.lines().filter(|line| line.contains("+77")).collect();

    assert_eq!(
        file_rows.len(),
        1,
        "modified file entry must occupy exactly one visual row; rows={file_rows:?}\n{snap}",
    );
    assert!(
        file_rows[0].contains("-3"),
        "single modified-file row must keep additions/deletions stats together: {:?}\n{snap}",
        file_rows[0],
    );
    assert!(
        !snap.contains("very_long_file_name_that_should_not_wrap"),
        "modified file paths must be truncated, not wrapped into a suffix row:\n{snap}",
    );
}

#[test]
fn ctrl_b_toggles_sidebar() {
    let mut h = Harness::new(120, 30);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Visible-Title-Xyz");
    h.draw();
    assert!(
        h.snapshot().contains("Visible-Title-Xyz"),
        "title shown while visible"
    );

    h.dispatch(ctrl('b'));
    assert!(!h.app.sidebar.sidebar.visible);
    assert!(
        !h.snapshot().contains("Visible-Title-Xyz"),
        "title hidden after toggle"
    );

    h.dispatch(ctrl('b'));
    assert!(h.app.sidebar.sidebar.visible);
    assert!(
        h.snapshot().contains("Visible-Title-Xyz"),
        "title returns after second toggle"
    );
}

#[test]
fn slash_sidebar_toggles_sidebar() {
    let mut h = Harness::new(120, 30);
    assert!(!h.app.sidebar.sidebar.visible);
    h.dispatch(Action::View(ViewAction::Command("/sidebar".into())));
    assert!(h.app.sidebar.sidebar.visible);
    h.dispatch(Action::View(ViewAction::Command("/sidebar".into())));
    assert!(!h.app.sidebar.sidebar.visible);
    assert!(
        !h.events()
            .iter()
            .any(|e| matches!(e, Event::Command { name, .. } if name == "sidebar")),
        "internal command must not leak to host as Event::Command"
    );
}

#[test]
fn sidebar_collapses_on_narrow_terminals() {
    let mut h = Harness::new(60, 30);
    h.app.sidebar.set_title("Should-Not-Render-Xyz");
    h.draw();
    assert!(
        !h.snapshot().contains("Should-Not-Render-Xyz"),
        "sidebar should auto-collapse on narrow terminals"
    );
}

#[test]
fn sidebar_footer_pinned_to_bottom_of_panel() {
    let mut h = Harness::new(120, 40);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Pinned-Footer-Session");
    h.app.sidebar.set_subtitle(Some("ses_pinfoot".to_string()));
    h.app.sidebar.set_sections(vec![SidebarSection::new(
        "Context",
        (0..30).map(|i| format!("body line {i}")),
    )]);
    h.app.sidebar.set_footer("raider v9.9.9");
    h.app
        .sidebar
        .set_footer_path(Some("~/Desktop/raider:main".to_string()));
    h.draw();

    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let path_y = lines
        .iter()
        .position(|l| l.contains("~/Desktop/raider:main"))
        .unwrap_or_else(|| panic!("path row missing:\n{snap}"));
    let version_y = lines
        .iter()
        .position(|l| l.contains("raider v9.9.9"))
        .unwrap_or_else(|| panic!("version row missing:\n{snap}"));
    let buf = h.terminal.backend().buffer();
    let panel_bg = h.app.theme.theme.background_panel;
    let panel_bottom: u16 = {
        let mut bottom = 0u16;
        for y in 0..buf.area.height {
            let mut row_has_panel = false;
            for x in 0..buf.area.width {
                if buf[(x, y)].style().bg == Some(panel_bg) {
                    row_has_panel = true;
                    break;
                }
            }
            if row_has_panel {
                bottom = y;
            }
        }
        bottom
    };
    let footer_bottom = panel_bottom.saturating_sub(1);
    assert_eq!(
        version_y as u16, footer_bottom,
        "version row must be pinned to the footer's bottom row (footer_bottom={footer_bottom}, panel_bottom={panel_bottom}); snap:\n{snap}",
    );
    assert_eq!(
        path_y as u16,
        footer_bottom - 1,
        "path row must sit immediately above the version row; snap:\n{snap}",
    );
}

#[test]
fn sidebar_footer_pinned_to_bottom_when_body_is_short() {
    let mut h = Harness::new(120, 40);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Short-Body");
    h.app
        .sidebar
        .set_sections(vec![SidebarSection::new("Context", ["only one"])]);
    h.app.sidebar.set_footer("raider v9.9.9");
    h.app.sidebar.set_footer_path(Some("~/p:dev".to_string()));
    h.draw();

    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let version_y = lines
        .iter()
        .position(|l| l.contains("raider v9.9.9"))
        .expect("version row present");
    let buf = h.terminal.backend().buffer();
    let panel_bg = h.app.theme.theme.background_panel;
    let panel_bottom: u16 = {
        let mut bottom = 0u16;
        for y in 0..buf.area.height {
            let mut row_has_panel = false;
            for x in 0..buf.area.width {
                if buf[(x, y)].style().bg == Some(panel_bg) {
                    row_has_panel = true;
                    break;
                }
            }
            if row_has_panel {
                bottom = y;
            }
        }
        bottom
    };
    let footer_bottom = panel_bottom.saturating_sub(1);
    assert_eq!(
        version_y as u16, footer_bottom,
        "version row must be pinned to footer bottom even for a short body (footer_bottom={footer_bottom}); snap:\n{snap}",
    );
}

#[test]
fn sidebar_footer_includes_cwd_path_row_above_version() {
    let mut h = Harness::new(120, 30);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Path-Row");
    h.app.sidebar.set_footer("raider v0.2.0");
    h.app
        .sidebar
        .set_footer_path(Some("~/Desktop/raider:main".to_string()));
    h.draw();

    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let path_y = lines
        .iter()
        .position(|l| l.contains("~/Desktop/raider:main"))
        .unwrap_or_else(|| panic!("cwd:branch row missing in snapshot:\n{snap}"));
    let version_y = lines
        .iter()
        .position(|l| l.contains("raider v0.2.0"))
        .unwrap_or_else(|| panic!("version row missing in snapshot:\n{snap}"));
    assert!(
        path_y < version_y,
        "path row (y={path_y}) must be ABOVE the version row (y={version_y}); snap:\n{snap}",
    );
}

#[test]
fn sidebar_footer_path_styles_basename_brighter_than_parent() {
    let mut h = Harness::new(120, 30);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Path-Styling");
    h.app.sidebar.set_footer("raider v0.2.0");
    h.app
        .sidebar
        .set_footer_path(Some("~/Desktop/raider:main".to_string()));
    h.draw();

    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let path_y = lines
        .iter()
        .position(|l| l.contains("~/Desktop/raider"))
        .expect("path row present") as u16;
    let buf = h.terminal.backend().buffer();
    let theme = &h.app.theme.theme;

    let mut basename_x: Option<u16> = None;
    let mut parent_slash_x: Option<u16> = None;
    for x in 1..buf.area.width {
        let prev = buf[(x - 1, path_y)].symbol();
        let cur = buf[(x, path_y)].symbol();
        if prev == "/" && cur == "r" {
            basename_x = Some(x);
            parent_slash_x = Some(x - 1);
            break;
        }
    }
    let basename_x = basename_x
        .unwrap_or_else(|| panic!("could not locate `raider` basename on path row {path_y}"));
    let parent_x = parent_slash_x.unwrap();

    let basename_cell = &buf[(basename_x, path_y)];
    let parent_cell = &buf[(parent_x, path_y)];

    assert_eq!(
        basename_cell
            .style()
            .fg
            .unwrap_or(ratatui::style::Color::Reset),
        theme.text,
        "basename `raider` must use theme.text (brighter); cell={basename_cell:?}",
    );
    assert_eq!(
        parent_cell
            .style()
            .fg
            .unwrap_or(ratatui::style::Color::Reset),
        theme.text_muted,
        "parent `/` must use theme.text_muted (dimmer); cell={parent_cell:?}",
    );
}

#[test]
fn sidebar_inner_padding_is_2_cells_each_side() {
    let mut h = Harness::new(120, 30);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("PADDING-TITLE");
    h.draw();

    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let buf = h.terminal.backend().buffer();
    let theme = &h.app.theme.theme;
    let panel_bg = theme.background_panel;

    let title_y = lines
        .iter()
        .position(|l| l.contains("PADDING-TITLE"))
        .expect("title row present") as u16;
    let mut panel_left: Option<u16> = None;
    for x in 0..buf.area.width {
        if buf[(x, title_y)].style().bg == Some(panel_bg) {
            panel_left = Some(x);
            break;
        }
    }
    let panel_left = panel_left.expect("panel-bg cell present on title row");

    let mut title_x: Option<u16> = None;
    for x in panel_left..buf.area.width {
        let s = buf[(x, title_y)].symbol();
        if s == "P" {
            title_x = Some(x);
            break;
        }
    }
    let title_x = title_x.expect("`P` of `PADDING-TITLE` present");
    assert_eq!(
        title_x - panel_left,
        2,
        "left inset must be 2 cells (panel_left={panel_left}, title_x={title_x})",
    );

    let mut h2 = Harness::new(120, 30);
    h2.app.sidebar.set_visible(true);
    h2.app.sidebar.set_title("RP");
    h2.app.sidebar.set_sections(vec![SidebarSection::new(
        "Context",
        ["this row is long enough to butt against the right inset XXXX"],
    )]);
    h2.draw();
    let buf2 = h2.terminal.backend().buffer();
    let mut pl: Option<u16> = None;
    let mut pr: Option<u16> = None;
    for x in 0..buf2.area.width {
        if buf2[(x, 1)].style().bg == Some(panel_bg) {
            if pl.is_none() {
                pl = Some(x);
            }
            pr = Some(x);
        }
    }
    let pl = pl.expect("panel-bg present");
    let pr = pr.expect("panel-bg present");
    let panel_width = pr - pl + 1;
    assert!(
        panel_width >= 4,
        "panel must be wide enough to host 2-cell insets on both sides; got {panel_width}",
    );
    let _ = panel_width;
}

#[test]
fn sidebar_renders_lsp_entries_with_dots() {
    let mut h = Harness::new(140, 30);
    let entries = vec![
        raider_tui::LspEntry::new("rust-analyzer", "/repo", "connected"),
        raider_tui::LspEntry::new("tsserver", "/repo", "error"),
    ];
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::lsps(
            "LSP",
            entries,
            "LSPs will activate as files are read",
        )],
    );
    h.draw();
    let snap = h.snapshot();
    assert!(snap.contains("LSP"), "LSP header:\n{snap}");
    assert!(snap.contains("rust-analyzer /repo"));
    assert!(snap.contains("tsserver /repo"));
}

#[test]
fn sidebar_lsp_empty_list_renders_placeholder_line() {
    let mut h = Harness::new(140, 30);
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::lsps(
            "LSP",
            Vec::new(),
            "LSPs will activate as files are read",
        )],
    );
    h.draw();
    let snap = h.snapshot();
    assert!(snap.contains("LSP"));
    assert!(
        snap.contains("LSPs will activate as files are read"),
        "placeholder when list empty:\n{snap}"
    );
}

#[test]
fn sidebar_renders_mcp_entries_with_status_label() {
    let mut h = Harness::new(140, 30);
    let entries = vec![
        raider_tui::McpEntry::new("context7", "connected", ""),
        raider_tui::McpEntry::new("github", "needs_auth", ""),
        raider_tui::McpEntry::new("filesystem", "failed", "spawn failed"),
    ];
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::mcps("MCP", entries)],
    );
    h.draw();
    let snap = h.snapshot();
    assert!(snap.contains("MCP"), "MCP header:\n{snap}");
    assert!(
        snap.contains("context7 Connected"),
        "connected label:\n{snap}"
    );
    assert!(
        snap.contains("github Needs auth"),
        "needs_auth label:\n{snap}",
    );
    assert!(
        snap.contains("filesystem spawn failed"),
        "failed label surfaces error:\n{snap}",
    );
}

#[test]
fn sidebar_renders_todo_entries_with_status_glyphs() {
    let mut h = Harness::new(140, 30);
    let entries = vec![
        raider_tui::TodoEntry::new("Done thing", "completed"),
        raider_tui::TodoEntry::new("Working on this", "in_progress"),
        raider_tui::TodoEntry::new("Not yet", "pending"),
    ];
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::todos("Todo", entries)],
    );
    h.draw();
    let snap = h.snapshot();
    assert!(snap.contains("Todo"), "Todo header:\n{snap}");
    assert!(snap.contains("[✓]"), "completed gets ✓ glyph:\n{snap}",);
    assert!(snap.contains("[•]"), "in_progress gets • glyph:\n{snap}",);
    assert!(snap.contains("[ ]"), "pending gets empty glyph:\n{snap}",);
    assert!(snap.contains("Done thing"));
    assert!(snap.contains("Working on this"));
    assert!(snap.contains("Not yet"));
}

#[test]
fn sidebar_renders_modified_files_with_plus_minus_stats() {
    let mut h = Harness::new(140, 30);
    let entries = vec![
        raider_tui::FileChange::new("a.rs", 12, 3),
        raider_tui::FileChange::new("b.rs", 7, 0),
    ];
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::files("Modified Files", entries)],
    );
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Modified Files"),
        "Modified Files header must render:\n{snap}"
    );
    assert!(snap.contains("+12"), "additions for a.rs missing:\n{snap}");
    assert!(snap.contains("-3"), "deletions for a.rs missing:\n{snap}");
    assert!(snap.contains("+7"), "additions for b.rs missing:\n{snap}");
    assert!(
        snap.contains("a.rs"),
        "a.rs file path missing from sidebar:\n{snap}"
    );
    assert!(
        snap.contains("b.rs"),
        "b.rs file path missing from sidebar:\n{snap}"
    );
}

#[test]
fn sidebar_modified_files_long_path_truncates_from_end_not_start() {
    // BUG10 user-reported: long workspace-relative paths like
    let mut h = Harness::new(140, 30);
    let entries = vec![raider_tui::FileChange::new(
        "crates/raider-host/src/bridge.rs",
        575,
        93,
    )];
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::files("Modified Files", entries)],
    );
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("crates/raider-host/"),
        "workspace-relative prefix `crates/raider-host/` must be preserved \
         in the sidebar (right-truncation, not left); snap:\n{snap}",
    );
    let path_row = snap
        .lines()
        .find(|l| l.contains("crates/raider-host"))
        .unwrap_or_else(|| panic!("expected a row carrying the path:\n{snap}"));
    assert!(
        !path_row.contains('…'),
        "modified-files row must NOT carry an `…` ellipsis (opencode \
         clips silently); row: {path_row:?}",
    );
    assert!(
        path_row.contains("+575"),
        "additions stat must stay on the path row; row: {path_row:?}",
    );
}

#[test]
fn sidebar_modified_files_each_entry_renders_on_single_row() {
    let mut h = Harness::new(140, 20);
    let entries: Vec<_> = (0..30)
        .map(|i| {
            raider_tui::FileChange::new(format!("crates/raider-host/src/file_{i:02}.rs"), 5, 1)
        })
        .collect();
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::files("Modified Files", entries)],
    );
    h.draw();
    let snap = h.snapshot();

    let path_rows: Vec<&str> = snap
        .lines()
        .filter(|l| l.contains("crates/raider-host/src/file_"))
        .collect();
    assert!(
        !path_rows.is_empty(),
        "expected at least one visible file row; snap:\n{snap}"
    );
    for row in &path_rows {
        assert!(
            row.contains("+5"),
            "every file row must carry its additions stat on the SAME row \
             as the path (no 2-line wrap); offending row: {row:?}\nfull snap:\n{snap}",
        );
    }
}

#[test]
fn sidebar_is_flush_with_right_screen_edge() {
    let mut h = Harness::new(140, 20);
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::files(
            "Modified Files",
            vec![raider_tui::FileChange::new("foo.rs", 1, 0)],
        )],
    );
    h.draw();
    let snap = h.snapshot();

    let title_row = snap
        .lines()
        .find(|l| l.contains("Greeting"))
        .expect("sidebar title row must appear in snapshot");
    assert_eq!(
        title_row.chars().count(),
        140,
        "sidebar title row must extend to the full screen width; \
         row: {title_row:?}\nsnap:\n{snap}",
    );
    let rect = h
        .app
        .sidebar
        .last_sidebar_rect
        .expect("sidebar rect should be cached this frame");
    assert_eq!(
        rect.x + rect.width,
        140,
        "sidebar must touch the right edge: rect = {rect:?}, screen width = 140",
    );
}

#[test]
fn sidebar_modified_files_shows_collapse_glyph_above_two_entries() {
    let mut h = Harness::new(140, 30);
    let entries = vec![
        raider_tui::FileChange::new("a.rs", 1, 0),
        raider_tui::FileChange::new("b.rs", 2, 0),
        raider_tui::FileChange::new("c.rs", 3, 0),
    ];
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::files("Modified Files", entries)],
    );
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("▼"),
        "▼ collapse glyph must be present when entries > 2:\n{snap}"
    );
    assert!(snap.contains("Modified Files"), "header still visible");
}

#[test]
fn sidebar_modified_files_no_glyph_when_two_or_fewer_entries() {
    let mut h = Harness::new(140, 30);
    let entries = vec![
        raider_tui::FileChange::new("a.rs", 1, 0),
        raider_tui::FileChange::new("b.rs", 2, 0),
    ];
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::files("Modified Files", entries)],
    );
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("▼ Modified Files") && !snap.contains("▶ Modified Files"),
        "no collapse glyph when entries <= 2:\n{snap}"
    );
    assert!(snap.contains("Modified Files"));
}

#[test]
fn sidebar_collapse_state_survives_section_refresh() {
    let mut h = Harness::new(140, 30);
    let todo_slot = raider_tui::sidebar::slot::TODO;
    let first = vec![
        raider_tui::TodoEntry::new("first todo", "pending"),
        raider_tui::TodoEntry::new("second todo", "pending"),
        raider_tui::TodoEntry::new("third todo", "pending"),
    ];
    open_sidebar_with(
        &mut h,
        vec![raider_tui::SidebarSection::todos("Todo", first).with_order(todo_slot)],
    );

    h.dispatch(Action::View(ViewAction::ToggleSidebarSection(todo_slot)));
    h.draw();
    let collapsed = h.snapshot();
    assert!(
        collapsed.contains("▶ Todo"),
        "collapsed header:\n{collapsed}"
    );
    assert!(
        !collapsed.contains("first todo"),
        "collapsed body should be hidden:\n{collapsed}"
    );

    let refreshed = vec![
        raider_tui::TodoEntry::new("refreshed one", "pending"),
        raider_tui::TodoEntry::new("refreshed two", "pending"),
        raider_tui::TodoEntry::new("refreshed three", "pending"),
    ];
    h.dispatch(Action::Host(HostAction::SetSidebarSections(vec![
        raider_tui::SidebarSection::todos("Todo", refreshed).with_order(todo_slot),
    ])));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("▶ Todo"),
        "refresh must stay collapsed:\n{snap}"
    );
    assert!(
        !snap.contains("refreshed one"),
        "refreshed collapsed body should stay hidden:\n{snap}"
    );
}

#[test]
fn sidebar_scrollbar_renders_when_content_overflows() {
    let mut h = Harness::new(140, 20);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Scroll Test");
    h.app
        .sidebar
        .set_sections(vec![SidebarSection::new("Many", many_sidebar_entries(40))]);
    h.draw();

    let rect = h
        .app
        .sidebar
        .last_sidebar_rect
        .expect("sidebar must have a cached rect after draw");
    let scrollbar_x = rect.x + rect.width - 1 - 2;
    let mut any_thumb = false;
    for y in 0..rect.height {
        if cell_is_thumb(&h, scrollbar_x, rect.y + y) {
            any_thumb = true;
            break;
        }
    }
    assert!(
        any_thumb,
        "expected `█` scrollbar thumb in rightmost column ({scrollbar_x}) \
         of overflowing sidebar; rect={rect:?}\n{}",
        h.snapshot()
    );
}

#[test]
fn sidebar_scrollbar_not_rendered_when_content_fits() {
    let mut h = Harness::new(140, 30);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Fits");
    h.app.sidebar.set_sections(vec![SidebarSection::new(
        "Few",
        ["one".to_string(), "two".to_string(), "three".to_string()],
    )]);
    h.draw();

    let rect = h.app.sidebar.last_sidebar_rect.expect("cached rect");
    let scrollbar_x = rect.x + rect.width - 1 - 2;
    for y in 0..rect.height {
        assert!(
            !cell_is_thumb(&h, scrollbar_x, rect.y + y),
            "scrollbar thumb must not appear when content fits; \
             found `█` at ({scrollbar_x},{}); rect={rect:?}\n{}",
            rect.y + y,
            h.snapshot()
        );
    }
}

#[test]
fn sidebar_scroll_action_advances_offset() {
    let mut h = Harness::new(140, 20);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Scroll Action");
    h.app
        .sidebar
        .set_sections(vec![SidebarSection::new("Many", many_sidebar_entries(40))]);
    h.draw();
    assert_eq!(h.app.sidebar.sidebar.scroll_offset, 0);

    h.dispatch(Action::View(ViewAction::ScrollSidebar(3)));
    assert_eq!(
        h.app.sidebar.sidebar.scroll_offset, 3,
        "ScrollSidebar(3) must advance offset by 3 rows"
    );
}

#[test]
fn sidebar_scroll_offset_clamped_to_max() {
    let mut h = Harness::new(140, 20);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("Clamp");
    h.app
        .sidebar
        .set_sections(vec![SidebarSection::new("Many", many_sidebar_entries(40))]);
    h.draw();

    let total = h.app.sidebar.total_sidebar_content_lines;
    let body = h.app.sidebar.sidebar_body_height as usize;
    let max_offset = total.saturating_sub(body);
    assert!(
        max_offset > 0,
        "test precondition: content must overflow; total={total} body={body}"
    );

    h.dispatch(Action::View(ViewAction::ScrollSidebar(99_999)));
    assert_eq!(
        h.app.sidebar.sidebar.scroll_offset, max_offset,
        "scroll offset must clamp to total - body_height"
    );
}

#[test]
fn sidebar_scroll_changes_visible_window() {
    let mut h = Harness::new(140, 20);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("ScrollWindow");
    h.app.sidebar.set_sections(vec![SidebarSection::new(
        "Section",
        many_sidebar_entries(30),
    )]);
    h.draw();

    let snap_before = h.snapshot();
    assert!(
        snap_before.contains("entry 1 "),
        "expected `entry 1` to be visible before scrolling:\n{snap_before}"
    );

    h.dispatch(Action::View(ViewAction::ScrollSidebar(5)));
    let snap_after = h.snapshot();
    assert_eq!(
        h.app.sidebar.sidebar.scroll_offset, 5,
        "offset must reflect the dispatched delta"
    );
    assert!(
        !snap_after.contains("entry 1 "),
        "after scrolling 5 rows, `entry 1` must no longer be visible:\n{snap_after}"
    );
    let has_later_entry = (6..=15).any(|i| snap_after.contains(&format!("entry {i} ")));
    assert!(
        has_later_entry,
        "after scrolling, expected at least one of `entry 6..15` to be visible:\n{snap_after}"
    );
}

#[test]
fn mouse_wheel_over_sidebar_scrolls_sidebar_not_transcript() {
    use ratatui::layout::Rect;
    let mut h = Harness::new(180, 30);
    h.app.sidebar.set_visible(true);
    h.app.sidebar.set_title("MouseWheel");
    h.app
        .sidebar
        .set_sections(vec![SidebarSection::new("Many", many_sidebar_entries(40))]);
    h.draw();

    let list_before = h.app.scroll.list_state.selected();
    let sidebar_offset_before = h.app.sidebar.sidebar.scroll_offset;

    h.app.sidebar.last_sidebar_rect = Some(Rect {
        x: 100,
        y: 0,
        width: 50,
        height: 30,
    });
    let mouse_col: u16 = 110;
    let in_sidebar = h
        .app
        .sidebar
        .last_sidebar_rect
        .map(|r| mouse_col >= r.x && mouse_col < r.x.saturating_add(r.width) && r.height > 0)
        .unwrap_or(false);
    assert!(
        in_sidebar,
        "test precondition: mouse_col {mouse_col} must be inside cached sidebar rect"
    );
    h.dispatch(Action::View(ViewAction::ScrollSidebar(3)));

    assert!(
        h.app.sidebar.sidebar.scroll_offset > sidebar_offset_before,
        "sidebar offset must advance (was {sidebar_offset_before}, \
         now {})",
        h.app.sidebar.sidebar.scroll_offset
    );
    assert_eq!(
        h.app.scroll.list_state.selected(),
        list_before,
        "transcript list_state must NOT move when the wheel scrolls the sidebar"
    );
}
