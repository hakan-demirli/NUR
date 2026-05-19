// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn inline_tool_call_renders_icon_and_title() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Looking things up", "").with_tool(ToolCall {
            id: None,
            name: "glob".into(),
            status: ToolStatus::Completed,
            title: "Find tests".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Find tests"),
        "tool title must render inline:\n{snap}"
    );
    assert!(snap.contains("✱"), "glob icon must render:\n{snap}");
}

#[test]
fn bash_tool_call_renders_block_with_command_and_output() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Running checks", "").with_tool(ToolCall {
            id: None,
            name: "bash".into(),
            status: ToolStatus::Completed,
            title: "list files".into(),
            command: Some("ls /tmp".into()),
            output: "foo.txt\nbar.txt\n".into(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(snap.contains("# list files"), "header line:\n{snap}");
    assert!(snap.contains("$ ls /tmp"), "command line:\n{snap}");
    assert!(snap.contains("foo.txt"), "output line 1:\n{snap}");
    assert!(snap.contains("bar.txt"), "output line 2:\n{snap}");
}

#[test]
fn bash_tool_call_truncates_long_output_at_ten_lines() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(140, 30);
    let many_lines: String = (1..=20).map(|i| format!("line{i}\n")).collect();
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("ran", "").with_tool(ToolCall {
            id: None,
            name: "bash".into(),
            status: ToolStatus::Completed,
            title: "many lines".into(),
            command: Some("seq 1 20".into()),
            output: many_lines,
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("line10"),
        "line10 (within truncation window) must render:\n{snap}"
    );
    assert!(
        !snap.contains("line11"),
        "line11 must be elided beyond the 10-line truncation window:\n{snap}"
    );
    assert!(
        snap.contains('…'),
        "truncation `…` line must render after the 10-line slice:\n{snap}"
    );
    assert!(
        snap.contains("Click to expand"),
        "muted `Click to expand` hint must render below the truncated body:\n{snap}"
    );
    assert!(
        !snap.contains("more lines"),
        "the old `+N more lines` raider-ism must NOT render — opencode \
         emits a bare `…` + `Click to expand` pair:\n{snap}"
    );
}

#[test]
fn running_tool_call_renders_spinner_in_header() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Running bash", "").with_tool(ToolCall {
            id: None,
            name: "bash".into(),
            status: ToolStatus::Running,
            title: "list files".into(),
            command: Some("ls /tmp".into()),
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    assert!(
        frames.iter().any(|f| snap.contains(f)),
        "running tool must render a braille spinner frame in the header:\n{snap}"
    );
    assert!(
        !snap.contains("# list files"),
        "spinner header must replace the `# ` prefix while running:\n{snap}"
    );
}

#[test]
fn completed_tool_call_keeps_hash_prefix_no_spinner() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Done", "").with_tool(ToolCall {
            id: None,
            name: "bash".into(),
            status: ToolStatus::Completed,
            title: "list files".into(),
            command: Some("ls /tmp".into()),
            output: "a.txt\n".into(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("# list files"),
        "completed tool keeps `# ` block prefix:\n{snap}"
    );
    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    for f in spinner_frames {
        assert!(
            !snap.contains(f),
            "no spinner frame on a completed tool ({f} in snap):\n{snap}"
        );
    }
}

#[test]
fn errored_tool_call_renders_error_message() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("had a problem", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Error,
            title: "Apply patch".into(),
            command: None,
            output: String::new(),
            error: Some("file not found".into()),
            todos: vec![],
            file_path: Some("/tmp/foo.rs".into()),
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Edit") && snap.contains("foo.rs"),
        "edit tool title must read `Edit <path>` (no diff available so falls back to InlineTool):\n{snap}",
    );
    assert!(snap.contains("file not found"), "error message:\n{snap}");
}

#[test]
fn edit_tool_renders_unified_diff_with_per_line_colors() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    let diff = "\
Index: /tmp/foo.rs
===================================================================
--- /tmp/foo.rs
+++ /tmp/foo.rs
@@ -1,4 +1,4 @@
 fn main() {
-    println!(\"old\");
+    println!(\"new\");
 }
";
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Patching", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "Apply patch".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/foo.rs".into()),
            diff: Some(diff.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();

    assert!(
        snap.contains("← Edit") && snap.contains("foo.rs"),
        "edit block title must be `← Edit <path>`; snap:\n{snap}",
    );
    assert!(
        !snap.contains("Apply patch"),
        "server-supplied state.title must NOT leak into the diff-block; snap:\n{snap}",
    );

    assert!(
        !snap.contains("Index: /tmp/foo.rs"),
        "diff `Index:` envelope must be stripped; snap:\n{snap}",
    );
    assert!(
        !snap.contains("=====") && !snap.contains("--- /tmp/foo.rs"),
        "diff `===`/`---` envelope must be stripped; snap:\n{snap}",
    );

    assert!(
        !snap.contains("@@ -1,4 +1,4 @@"),
        "hunk header must NOT render (opencode parity); snap:\n{snap}",
    );

    assert!(
        snap.contains("println!(\"new\");"),
        "added line content must render; snap:\n{snap}",
    );
    assert!(
        snap.contains("println!(\"old\");"),
        "removed line content must render; snap:\n{snap}",
    );

    assert!(
        snap.contains("fn main()"),
        "context line must render; snap:\n{snap}",
    );

    let lines: Vec<&str> = snap.lines().collect();
    let plus_y = lines
        .iter()
        .position(|l| l.contains("println!(\"new\");"))
        .unwrap() as u16;
    let buf = h.terminal.backend().buffer();
    let theme = &h.app.theme.theme;
    let mut x_plus = None;
    for x in 0..buf.area.width {
        if buf[(x, plus_y)].symbol() == "+" {
            x_plus = Some(x);
            break;
        }
    }
    let x = x_plus.expect("`+` sign cell must exist on the added-line row");
    let cell = &buf[(x, plus_y)];
    assert_eq!(
        cell.style().fg.unwrap_or(ratatui::style::Color::Reset),
        theme.diff_highlight_added,
        "added sign glyph must use theme.diff_highlight_added; cell={cell:?}",
    );

    let minus_y = lines
        .iter()
        .position(|l| l.contains("println!(\"old\");"))
        .unwrap() as u16;
    let mut x_minus = None;
    for x in 0..buf.area.width {
        if buf[(x, minus_y)].symbol() == "-" {
            x_minus = Some(x);
            break;
        }
    }
    let x = x_minus.expect("`-` sign cell must exist on the removed-line row");
    let cell = &buf[(x, minus_y)];
    assert_eq!(
        cell.style().fg.unwrap_or(ratatui::style::Color::Reset),
        theme.diff_highlight_removed,
        "removed sign glyph must use theme.diff_highlight_removed; cell={cell:?}",
    );
}

#[test]
fn write_tool_renders_full_file_content_as_added_diff() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    let diff = "@@ -0,0 +1,3 @@\n\
+fn hello() {\n\
+    println!(\"hi\");\n\
+}\n";
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Creating file", "").with_tool(ToolCall {
            id: None,
            name: "write".into(),
            status: ToolStatus::Completed,
            title: "Create file".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/hello.rs".into()),
            diff: Some(diff.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("# Wrote") && snap.contains("hello.rs"),
        "write block title must be `# Wrote <path>`; snap:\n{snap}",
    );
    assert!(
        snap.contains("fn hello() {"),
        "first body line content must render; snap:\n{snap}",
    );
}

#[test]
fn edit_tool_without_diff_falls_back_to_inline_label() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Preparing edit", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Running,
            title: "Apply patch".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/foo.rs".into()),
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Edit") && snap.contains("foo.rs"),
        "running edit must show `Edit <path>` inline label; snap:\n{snap}",
    );
    assert!(
        !snap.contains("← Edit"),
        "without a diff payload the BlockTool `← Edit` title must NOT render; snap:\n{snap}",
    );
    assert!(
        !snap.contains("Apply patch"),
        "server-supplied state.title must NOT leak; snap:\n{snap}",
    );
}

#[test]
fn todowrite_tool_renders_structured_todos_not_raw_json() {
    // per todo. User-reported BUG4: raider was dumping the raw
    use raider_tui::{HostMessage, TodoEntry, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Updating my plan", "").with_tool(ToolCall {
            id: None,
            name: "todowrite".into(),
            status: ToolStatus::Completed,
            title: "Update todo list".into(),
            command: None,
            output: "# 0 todos\n[\n  {\n    \"content\": \"junk\"\n  }\n]".into(),
            error: None,
            todos: vec![
                TodoEntry::new("Fix BUG4 todowrite rendering", "in_progress"),
                TodoEntry::new("Audit other tool renderers", "pending"),
                TodoEntry::new("Ship orphan footer", "completed"),
                TodoEntry::new("Drop dead code", "cancelled"),
            ],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();

    assert!(
        snap.contains("# Todos"),
        "expected `# Todos` block header; snap:\n{snap}",
    );

    for content in [
        "Fix BUG4 todowrite rendering",
        "Audit other tool renderers",
        "Ship orphan footer",
        "Drop dead code",
    ] {
        assert!(
            snap.contains(content),
            "expected todo content `{content}` in snapshot:\n{snap}",
        );
    }

    let in_progress_row = snap
        .lines()
        .find(|l| l.contains("Fix BUG4 todowrite rendering"))
        .expect("in_progress row must render");
    assert!(
        in_progress_row.contains("[•]"),
        "in_progress todo must carry `[•]` glyph; row: {in_progress_row:?}",
    );
    let completed_row = snap
        .lines()
        .find(|l| l.contains("Ship orphan footer"))
        .expect("completed row must render");
    assert!(
        completed_row.contains("[✓]"),
        "completed todo must carry `[✓]` glyph; row: {completed_row:?}",
    );
    let cancelled_row = snap
        .lines()
        .find(|l| l.contains("Drop dead code"))
        .expect("cancelled row must render");
    assert!(
        cancelled_row.contains("[ ]"),
        "unknown/cancelled todo status must carry `[ ]` glyph; row: {cancelled_row:?}",
    );
    let pending_row = snap
        .lines()
        .find(|l| l.contains("Audit other tool renderers"))
        .expect("pending row must render");
    assert!(
        pending_row.contains("[ ]"),
        "pending todo must carry `[ ]` glyph; row: {pending_row:?}",
    );
    let in_progress_y = snap
        .lines()
        .position(|l| l.contains("Fix BUG4 todowrite rendering"))
        .expect("in_progress row index") as u16;
    let in_progress_x = in_progress_row
        .split("[•]")
        .next()
        .expect("prefix")
        .chars()
        .count() as u16
        + 1;
    let cell = h.terminal.backend().buffer()[(in_progress_x, in_progress_y)].clone();
    assert_eq!(
        cell.style().fg,
        Some(h.app.theme.theme.warning),
        "in_progress todo glyph/content should use theme.warning like opencode"
    );

    assert!(
        !snap.contains("\"content\": \"junk\""),
        "raw JSON from `tool.output` must NOT leak into the rendering:\n{snap}",
    );
    assert!(
        !snap.contains("+ more lines") && !snap.contains("more lines"),
        "todowrite must NOT show the generic overflow hint (it has its own \
         structured rendering); snap:\n{snap}",
    );
}

#[test]
fn todowrite_tool_running_renders_inline_updating_label() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Working", "").with_tool(ToolCall {
            id: None,
            name: "todowrite".into(),
            status: ToolStatus::Running,
            title: "Update todo list".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Updating todos..."),
        "running todowrite must surface opencode's `Updating todos...` label; \
         snap:\n{snap}",
    );
    assert!(
        !snap.contains("# Todos"),
        "running todowrite must not render the block header; snap:\n{snap}",
    );
}

#[test]
fn assistant_parts_have_blank_gap_between_reasoning_text_and_tool() {
    // BUG9 user-reported: raider's transcript was too dense — every
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.app.messages.toggle_thinking();
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("here is the body", "the chain of thought").with_tool(ToolCall {
            id: None,
            name: "glob".into(),
            status: ToolStatus::Completed,
            title: "Glob \"**/*.rs\" (3 matches)".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let reasoning_y = lines
        .iter()
        .position(|l| l.contains("chain of thought"))
        .unwrap_or_else(|| panic!("missing reasoning row:\n{snap}"));
    let text_y = lines
        .iter()
        .position(|l| l.contains("here is the body"))
        .unwrap_or_else(|| panic!("missing text row:\n{snap}"));
    let tool_y = lines
        .iter()
        .position(|l| l.contains("Glob \"**/*.rs\""))
        .unwrap_or_else(|| panic!("missing tool row:\n{snap}"));

    assert!(
        reasoning_y < text_y,
        "reasoning row must come before text row; r={reasoning_y} t={text_y}",
    );
    assert!(
        text_y < tool_y,
        "text row must come before tool row; t={text_y} u={tool_y}",
    );

    let gap_between_reasoning_and_text =
        (reasoning_y + 1..text_y).any(|y| lines[y].trim().is_empty());
    assert!(
        gap_between_reasoning_and_text,
        "expected at least one blank row between reasoning and text rows; \
         reasoning_y={reasoning_y} text_y={text_y} snap:\n{snap}",
    );

    let gap_between_text_and_tool = (text_y + 1..tool_y).any(|y| lines[y].trim().is_empty());
    assert!(
        gap_between_text_and_tool,
        "expected at least one blank row between text and tool rows; \
         text_y={text_y} tool_y={tool_y} snap:\n{snap}",
    );
}

#[test]
fn orphan_footer_margin_top_gap_is_not_doubled_when_tool_is_last_part() {
    // BUG9 follow-up: the orphan `▣ …` footer carries its own
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("done", "")
            .with_agent("build")
            .with_model("claude")
            .with_duration(std::time::Duration::from_millis(700))
            .with_tool(ToolCall {
                id: None,
                name: "glob".into(),
                status: ToolStatus::Completed,
                title: "Glob \"**/*.rs\" in src (3 matches)".into(),
                command: None,
                output: String::new(),
                error: None,
                todos: vec![],
                file_path: None,
                diff: None,
                loaded: vec![],
                patches: vec![],
                questions: vec![],
                answers: vec![],
                expanded: false,
                current_child: None,
                child_tool_count: 0,
                started_at_ms: None,
                completed_at_ms: None,
            }),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let footer_y = lines
        .iter()
        .position(|l| l.contains("▣"))
        .unwrap_or_else(|| panic!("no footer in snapshot:\n{snap}"));
    let above = lines[footer_y - 1];
    assert!(
        above.trim().is_empty(),
        "row directly above orphan footer must be the marginTop=1 gap; got: {above:?}\nsnap:\n{snap}",
    );
    let two_above = lines[footer_y - 2];
    assert!(
        !two_above.trim().is_empty(),
        "two rows above orphan footer must be the tool's last content row (no double marginTop=1); \
         got: {two_above:?}\nsnap:\n{snap}",
    );
}

#[test]
fn glob_tool_renders_inline_summary_not_block_dump() {
    // BUG11 user-reported: opencode renders glob as a single-line
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("hunting", "").with_tool(ToolCall {
            id: None,
            name: "glob".into(),
            status: ToolStatus::Completed,
            title: "Glob \"**/*.rs\" in src (17 matches)".into(),
            command: None,
            output: "src/foo.rs\nsrc/bar.rs\nsrc/baz.rs\n".into(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Glob \"**/*.rs\" in src (17 matches)"),
        "glob inline summary must render verbatim; snap:\n{snap}",
    );
    assert!(
        !snap.contains("src/foo.rs"),
        "glob's matched-file list (`state.output`) must NOT render — \
         opencode's InlineTool ignores it; snap:\n{snap}",
    );
    assert!(
        !snap.contains("# Glob"),
        "glob must render as InlineTool, never BlockTool; snap:\n{snap}",
    );
}

#[test]
fn grep_tool_renders_inline_summary_with_match_count() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("searching", "").with_tool(ToolCall {
            id: None,
            name: "grep".into(),
            status: ToolStatus::Completed,
            title: "Grep \"pattern\" in src (5 matches)".into(),
            command: None,
            output: "src/a.rs:12: hit\nsrc/b.rs:34: hit\n".into(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Grep \"pattern\" in src (5 matches)"),
        "grep inline summary must render; snap:\n{snap}",
    );
    assert!(
        !snap.contains("src/a.rs:12:"),
        "grep's match-lines payload must NOT surface; snap:\n{snap}",
    );
    assert!(
        !snap.contains("# Grep"),
        "grep must render as InlineTool, never BlockTool; snap:\n{snap}",
    );
}

#[test]
fn read_tool_renders_inline_with_filepath_and_extras() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("reading", "").with_tool(ToolCall {
            id: None,
            name: "read".into(),
            status: ToolStatus::Completed,
            title: "Read crates/raider-tui/src/ui.rs [offset=1855, limit=55]".into(),
            command: None,
            output: "use ratatui::Frame;\nfn ui() {}\n... (1000 more lines)".into(),
            error: None,
            todos: vec![],
            file_path: Some("crates/raider-tui/src/ui.rs".into()),
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Read crates/raider-tui/src/ui.rs"),
        "read tool must include `Read <path>` inline; snap:\n{snap}",
    );
    assert!(
        snap.contains("offset=1855"),
        "extras like `offset=1855` must surface in the title; snap:\n{snap}",
    );
    assert!(
        snap.contains("limit=55"),
        "extras like `limit=55` must surface in the title; snap:\n{snap}",
    );
    assert!(
        !snap.contains("use ratatui::Frame"),
        "read tool must NOT render the file's body; snap:\n{snap}",
    );
}

#[test]
fn glob_running_renders_tilde_not_spinner() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("hunting", "").with_tool(ToolCall {
            id: None,
            name: "glob".into(),
            status: ToolStatus::Running,
            title: "Glob \"**/*.rs\"".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("~ Glob \"**/*.rs\""),
        "glob running must render `~ Glob …`, not the braille spinner; snap:\n{snap}",
    );
    for frame in ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"] {
        assert!(
            !snap.contains(frame),
            "no braille spinner frame must appear next to a running glob; \
             found {frame:?} in snap:\n{snap}",
        );
    }
}

#[test]
fn bash_running_renders_spinner_not_tilde() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("kicking off a shell", "").with_tool(ToolCall {
            id: None,
            name: "bash".into(),
            status: ToolStatus::Running,
            title: "List files".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    let has_braille = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        .iter()
        .any(|f| snap.contains(*f));
    assert!(
        has_braille,
        "running bash must render a braille spinner frame as its leading glyph; snap:\n{snap}",
    );
    assert!(
        !snap.contains("~ List files"),
        "running bash must NOT use the `~` fallback glyph; snap:\n{snap}",
    );
}

#[test]
fn running_block_body_bash_does_not_double_its_title_after_cached_spinner_tick() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);

    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("running a long shell command", ""),
    )));

    h.dispatch(Action::Host(HostAction::UpsertToolCall(Box::new(
        ToolCall {
            id: Some("prt_bash_doubled_title".into()),
            name: "bash".into(),
            status: ToolStatus::Running,
            title: "Run with .vlt comment fix".into(),
            command: Some("make sim".into()),
            output: "starting build\n".into(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        },
    ))));

    h.draw();
    h.draw();

    let snap = h.snapshot();
    let title_occurrences = snap.matches("Run with .vlt comment fix").count();
    assert_eq!(
        title_occurrences, 1,
        "block-body running bash title must appear exactly ONCE in the \
         rendered snapshot (got {title_occurrences}); the spinner-tick \
         cache path must not paint a phantom header into the top pad row.\n\
         snap:\n{snap}",
    );

    let has_braille = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        .iter()
        .any(|f| snap.contains(*f));
    assert!(
        has_braille,
        "spinner frame must be present — otherwise we're not exercising \
         the cache-hit spinner-tick path:\n{snap}",
    );
}

#[test]
fn read_tool_renders_loaded_rows_under_main_row() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("reading", "").with_tool(ToolCall {
            id: None,
            name: "read".into(),
            status: ToolStatus::Completed,
            title: "Read /a".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/a".into()),
            diff: None,
            loaded: vec!["/a".into(), "/b".into()],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("↳ Loaded /a"),
        "first transitive `↳ Loaded /a` row missing; snap:\n{snap}",
    );
    assert!(
        snap.contains("↳ Loaded /b"),
        "second transitive `↳ Loaded /b` row missing; snap:\n{snap}",
    );
    let line_with_arrow = snap
        .lines()
        .find(|l| l.contains("↳ Loaded /a"))
        .unwrap_or_else(|| panic!("`↳ Loaded /a` row missing in snap:\n{snap}"));
    let leading_spaces = line_with_arrow.chars().take_while(|c| *c == ' ').count();
    assert!(
        leading_spaces >= 6,
        "`↳ Loaded /a` row must have at least 6 leading spaces (3 inline indent + 3 nested); \
         got {leading_spaces}; row: {line_with_arrow:?}",
    );
}

#[test]
fn stacked_inline_tools_have_no_blank_gap_between() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    let mk = |title: &str| ToolCall {
        id: None,
        name: "glob".into(),
        status: ToolStatus::Completed,
        title: title.into(),
        command: None,
        output: String::new(),
        error: None,
        todos: vec![],
        file_path: None,
        diff: None,
        loaded: vec![],
        patches: vec![],
        questions: vec![],
        answers: vec![],
        expanded: false,
        current_child: None,
        child_tool_count: 0,
        started_at_ms: None,
        completed_at_ms: None,
    };
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("scanning", "")
            .with_tool(mk("Glob \"first\""))
            .with_tool(mk("Glob \"second\""))
            .with_tool(mk("Glob \"third\"")),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let row_idx = |needle: &str| -> usize {
        lines
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("needle {needle:?} missing in snap:\n{snap}"))
    };
    let first = row_idx("Glob \"first\"");
    let second = row_idx("Glob \"second\"");
    let third = row_idx("Glob \"third\"");
    assert_eq!(
        second,
        first + 1,
        "stacked InlineTools must sit on adjacent rows (no marginTop gap); \
         first={first}, second={second}\nsnap:\n{snap}",
    );
    assert_eq!(
        third,
        second + 1,
        "stacked InlineTools must sit on adjacent rows (no marginTop gap); \
         second={second}, third={third}\nsnap:\n{snap}",
    );
}

#[test]
fn inline_tool_after_text_still_has_margin_top_gap() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Some assistant prose that lands before the tool row.", "")
            .with_tool(ToolCall {
                id: None,
                name: "glob".into(),
                status: ToolStatus::Completed,
                title: "Glob \"after-text\"".into(),
                command: None,
                output: String::new(),
                error: None,
                todos: vec![],
                file_path: None,
                diff: None,
                loaded: vec![],
                patches: vec![],
                questions: vec![],
                answers: vec![],
                expanded: false,
                current_child: None,
                child_tool_count: 0,
                started_at_ms: None,
                completed_at_ms: None,
            }),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let text_row = lines
        .iter()
        .position(|l| l.contains("Some assistant prose"))
        .unwrap_or_else(|| panic!("text row missing; snap:\n{snap}"));
    let tool_row = lines
        .iter()
        .position(|l| l.contains("Glob \"after-text\""))
        .unwrap_or_else(|| panic!("tool row missing; snap:\n{snap}"));
    assert!(
        tool_row > text_row + 1,
        "inline tool after text must be separated by at least one blank gap row; \
         text_row={text_row}, tool_row={tool_row}\nsnap:\n{snap}",
    );
    let gap_row = lines[text_row + 1];
    assert!(
        gap_row.trim().is_empty(),
        "row between text and inline tool must be the marginTop=1 gap; got: {gap_row:?}\nsnap:\n{snap}",
    );
}

#[test]
fn inline_tool_after_block_tool_has_margin_top_gap() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("doing things", "")
            .with_tool(ToolCall {
                id: None,
                name: "bash".into(),
                status: ToolStatus::Completed,
                title: "List".into(),
                command: Some("ls".into()),
                output: "Cargo.toml\nsrc\n".into(),
                error: None,
                todos: vec![],
                file_path: None,
                diff: None,
                loaded: vec![],
                patches: vec![],
                questions: vec![],
                answers: vec![],
                expanded: false,
                current_child: None,
                child_tool_count: 0,
                started_at_ms: None,
                completed_at_ms: None,
            })
            .with_tool(ToolCall {
                id: None,
                name: "glob".into(),
                status: ToolStatus::Completed,
                title: "Glob \"after-bash\"".into(),
                command: None,
                output: String::new(),
                error: None,
                todos: vec![],
                file_path: None,
                diff: None,
                loaded: vec![],
                patches: vec![],
                questions: vec![],
                answers: vec![],
                expanded: false,
                current_child: None,
                child_tool_count: 0,
                started_at_ms: None,
                completed_at_ms: None,
            }),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let bash_last = lines
        .iter()
        .rposition(|l| l.contains("Cargo.toml") || l.contains("src"))
        .unwrap_or_else(|| panic!("bash output rows missing; snap:\n{snap}"));
    let glob_row = lines
        .iter()
        .position(|l| l.contains("Glob \"after-bash\""))
        .unwrap_or_else(|| panic!("glob row missing; snap:\n{snap}"));
    assert!(
        glob_row > bash_last + 1,
        "inline tool after block tool must have a marginTop=1 gap; \
         bash_last={bash_last}, glob_row={glob_row}\nsnap:\n{snap}",
    );
    let gap_row = lines[bash_last + 1];
    let gap_stripped = gap_row.trim_start_matches([' ', '┃']).trim();
    assert!(
        gap_stripped.is_empty(),
        "row between bash block and inline glob must be empty content (blank or bar-only); \
         got: {gap_row:?}\nsnap:\n{snap}",
    );
}

#[test]
fn host_upsert_tool_call_merges_by_part_id_across_state_transitions() {
    // BUG7 user-reported: streaming tool parts arrive via
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("working", ""),
    )));

    h.dispatch(Action::Host(HostAction::UpsertToolCall(Box::new(
        ToolCall {
            id: Some("prt_glob_1".into()),
            name: "glob".into(),
            status: ToolStatus::Running,
            title: "Glob \"**/*.rs\"".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        },
    ))));
    let last = h
        .app
        .messages
        .messages
        .last()
        .expect("assistant message exists after seed");
    assert_eq!(
        last.tool_calls.len(),
        1,
        "first upsert must create one row; got {} tools",
        last.tool_calls.len(),
    );
    assert_eq!(last.tool_calls[0].status, ToolStatus::Running);
    assert_eq!(last.tool_calls[0].title, "Glob \"**/*.rs\"");

    h.dispatch(Action::Host(HostAction::UpsertToolCall(Box::new(
        ToolCall {
            id: Some("prt_glob_1".into()),
            name: "glob".into(),
            status: ToolStatus::Completed,
            title: "Glob \"**/*.rs\" (17 matches)".into(),
            command: None,
            output: "src/foo.rs\nsrc/bar.rs\n".into(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        },
    ))));
    let last = h
        .app
        .messages
        .messages
        .last()
        .expect("assistant message persists after upsert");
    assert_eq!(
        last.tool_calls.len(),
        1,
        "second upsert with same PartId must MERGE onto the existing \
         row, not append a duplicate; got {} tools (titles: {:?})",
        last.tool_calls.len(),
        last.tool_calls.iter().map(|t| &t.title).collect::<Vec<_>>(),
    );
    assert_eq!(last.tool_calls[0].status, ToolStatus::Completed);
    assert_eq!(last.tool_calls[0].title, "Glob \"**/*.rs\" (17 matches)");
    assert_eq!(last.tool_calls[0].output, "src/foo.rs\nsrc/bar.rs\n");
}

#[test]
fn host_upsert_tool_call_distinct_part_ids_produce_separate_rows() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("working", ""),
    )));
    h.dispatch(Action::Host(HostAction::UpsertToolCall(Box::new(
        ToolCall {
            id: Some("prt_read_a".into()),
            name: "read".into(),
            status: ToolStatus::Completed,
            title: "Read /a.rs".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/a.rs".into()),
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        },
    ))));
    h.dispatch(Action::Host(HostAction::UpsertToolCall(Box::new(
        ToolCall {
            id: Some("prt_read_b".into()),
            name: "read".into(),
            status: ToolStatus::Completed,
            title: "Read /b.rs".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/b.rs".into()),
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        },
    ))));
    let last = h
        .app
        .messages
        .messages
        .last()
        .expect("assistant message exists");
    assert_eq!(
        last.tool_calls.len(),
        2,
        "two distinct PartIds must produce two rows (got {})",
        last.tool_calls.len(),
    );
    assert_eq!(last.tool_calls[0].id.as_deref(), Some("prt_read_a"));
    assert_eq!(last.tool_calls[1].id.as_deref(), Some("prt_read_b"));
}

#[test]
fn host_upsert_tool_call_without_id_falls_back_to_edit_write_filepath() {
    // before BUG7 plumbed the id through) must still merge for
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("working", ""),
    )));
    h.dispatch(Action::Host(HostAction::UpsertToolCall(Box::new(
        ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Running,
            title: "Edit /x.rs".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/x.rs".into()),
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        },
    ))));
    h.dispatch(Action::Host(HostAction::UpsertToolCall(Box::new(
        ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "Edit /x.rs".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/x.rs".into()),
            diff: Some("Index: x.rs\n--- a\n+++ b\n@@ -1 +1 @@\n-a\n+b\n".into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        },
    ))));
    let last = h
        .app
        .messages
        .messages
        .last()
        .expect("assistant message exists");
    assert_eq!(
        last.tool_calls.len(),
        1,
        "two id-less edits with same file_path must MERGE; got {} tools",
        last.tool_calls.len(),
    );
    assert_eq!(last.tool_calls[0].status, ToolStatus::Completed);
    assert!(last.tool_calls[0].diff.is_some());
}

#[test]
fn diff_layout_emits_old_new_sign_content_columns() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    let diff = "@@ -10,3 +10,3 @@\n ctx\n-old\n+new\n";
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Patch", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "Apply patch".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/foo.rs".into()),
            diff: Some(diff.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();

    let minus_row = find_row(&snap, "old");
    let plus_row = find_row(&snap, "new");
    let buf = h.terminal.backend().buffer();

    let minus_x = find_glyph_x(buf, minus_row, "-").expect("minus sign cell exists");
    let mut row_text = String::new();
    for x in 0..=minus_x {
        row_text.push_str(buf[(x, minus_row)].symbol());
    }
    assert!(
        row_text.contains("11"),
        "removed row must show old#=11 before the `-`; got {row_text:?}"
    );

    let plus_x = find_glyph_x(buf, plus_row, "+").expect("plus sign cell exists");
    let mut row_text = String::new();
    for x in 0..=plus_x {
        row_text.push_str(buf[(x, plus_row)].symbol());
    }
    assert!(
        row_text.contains("11"),
        "added row must show new#=11 before the `+`; got {row_text:?}"
    );
}

#[test]
fn diff_line_number_width_auto_sizes_to_max_digits() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};

    let mut h2 = Harness::new(100, 30);
    let diff_2d = "@@ -1,1 +1,1 @@\n-rmline\n+addline\n";
    h2.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "x".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/a".into()),
            diff: Some(diff_2d.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h2.draw();
    let snap2 = h2.snapshot();
    let row = find_row(&snap2, "rmline");
    let buf = h2.terminal.backend().buffer();
    let sign_x = find_glyph_x(buf, row, "-").expect("- cell");
    let mut leading = String::new();
    for x in 0..sign_x {
        leading.push_str(buf[(x, row)].symbol());
    }
    assert_eq!(
        leading.chars().count(),
        10,
        "1-digit line numbers must yield 10 cells before the sign; leading={leading:?}",
    );

    let mut h3 = Harness::new(100, 30);
    let diff_3d = "@@ -999,1 +999,1 @@\n-bigrm\n+bigadd\n";
    h3.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "x".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/a".into()),
            diff: Some(diff_3d.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h3.draw();
    let snap3 = h3.snapshot();
    let row = find_row(&snap3, "bigrm");
    let buf = h3.terminal.backend().buffer();
    let sign_x = find_glyph_x(buf, row, "-").expect("- cell");
    let mut leading = String::new();
    for x in 0..sign_x {
        leading.push_str(buf[(x, row)].symbol());
    }
    assert_eq!(
        leading.chars().count(),
        14,
        "3-digit line numbers must yield 14 cells before the sign; leading={leading:?}",
    );
    assert!(leading.contains("999"));
}

#[test]
fn diff_per_status_backgrounds_paint_content_and_line_number_cells() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(100, 30);
    let diff = "@@ -1,2 +1,2 @@\n-old\n+new\n";
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "x".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/a".into()),
            diff: Some(diff.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    let buf = h.terminal.backend().buffer();
    let theme = &h.app.theme.theme;

    let plus_row = find_row(&snap, "new");
    let plus_x = find_glyph_x(buf, plus_row, "+").expect("+");
    let sign_cell = &buf[(plus_x, plus_row)];
    assert_eq!(
        sign_cell.style().bg.unwrap_or(ratatui::style::Color::Reset),
        theme.diff_added_bg,
        "added sign cell bg must be diff_added_bg; got {:?}",
        sign_cell.style().bg,
    );
    let content_cell = &buf[(plus_x + 2, plus_row)];
    assert_eq!(
        content_cell
            .style()
            .bg
            .unwrap_or(ratatui::style::Color::Reset),
        theme.diff_added_bg,
        "added content cell bg must be diff_added_bg; got {:?}",
        content_cell.style().bg,
    );
    let new_num_cell = &buf[(plus_x - 3, plus_row)];
    assert_eq!(
        new_num_cell
            .style()
            .bg
            .unwrap_or(ratatui::style::Color::Reset),
        theme.diff_added_line_number_bg,
        "added new# cell bg must be diff_added_line_number_bg; got {:?}",
        new_num_cell.style().bg,
    );

    let minus_row = find_row(&snap, "old");
    let minus_x = find_glyph_x(buf, minus_row, "-").expect("-");
    let sign_cell = &buf[(minus_x, minus_row)];
    assert_eq!(
        sign_cell.style().bg.unwrap_or(ratatui::style::Color::Reset),
        theme.diff_removed_bg,
        "removed sign cell bg must be diff_removed_bg",
    );
    let mut found_num_x: Option<u16> = None;
    for x in (0..minus_x).rev() {
        let ch = buf[(x, minus_row)].symbol();
        if ch.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            found_num_x = Some(x);
            break;
        }
    }
    let nx = found_num_x.expect("a numeric line-number cell exists left of `-`");
    let old_num_cell = &buf[(nx, minus_row)];
    assert_eq!(
        old_num_cell
            .style()
            .bg
            .unwrap_or(ratatui::style::Color::Reset),
        theme.diff_removed_line_number_bg,
        "removed old# cell bg must be diff_removed_line_number_bg; got {:?}",
        old_num_cell.style().bg,
    );
}

#[test]
fn diff_envelope_index_minus_minus_minus_plus_plus_plus_is_stripped() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    let diff = "\
Index: /tmp/foo.rs
===================================================================
--- /tmp/foo.rs
+++ /tmp/foo.rs
@@ -1,1 +1,1 @@
-old
+new
";
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "x".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/foo.rs".into()),
            diff: Some(diff.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("Index: /tmp/foo.rs"),
        "Index: header must be stripped; snap:\n{snap}",
    );
    assert!(
        !snap.contains("====="),
        "===== separator must be stripped; snap:\n{snap}",
    );
    let lines: Vec<&str> = snap.lines().collect();
    let body_envelope_count = lines
        .iter()
        .filter(|l| l.contains("--- /tmp/foo.rs") || l.contains("+++ /tmp/foo.rs"))
        .count();
    assert_eq!(
        body_envelope_count, 0,
        "no `---`/`+++` envelope rows should appear in the body; snap:\n{snap}",
    );
}

#[test]
fn diff_hunk_header_is_suppressed_for_opencode_parity() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    let diff = "@@ -1,1 +1,1 @@\n-x\n+y\n";
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "x".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/a".into()),
            diff: Some(diff.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("@@ -1,1 +1,1 @@"),
        "hunk header must NOT render; snap:\n{snap}",
    );
    assert!(
        !snap.contains("@@"),
        "no `@@` glyph pair should leak into the rendered diff; snap:\n{snap}",
    );
    assert!(
        snap.contains('x') && snap.contains('y'),
        "diff body must still render; snap:\n{snap}"
    );
}

#[test]
fn apply_patch_tool_emits_one_block_per_file() {
    use raider_tui::{HostMessage, PatchFile, PatchKind, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Patching", "").with_tool(ToolCall {
            id: None,
            name: "apply_patch".into(),
            status: ToolStatus::Completed,
            title: "Apply patch".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![
                PatchFile {
                    kind: PatchKind::Patched,
                    path: "/a".into(),
                    new_path: None,
                    diff: None,
                },
                PatchFile {
                    kind: PatchKind::Created,
                    path: "/b".into(),
                    new_path: None,
                    diff: None,
                },
            ],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("← Patched /a"),
        "first patch must render as `← Patched /a`; snap:\n{snap}",
    );
    assert!(
        snap.contains("# Created /b"),
        "second patch must render as `# Created /b`; snap:\n{snap}",
    );
}

#[test]
fn apply_patch_created_uses_diff_added_accent() {
    use raider_tui::{HostMessage, PatchFile, PatchKind, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Creating", "").with_tool(ToolCall {
            id: None,
            name: "apply_patch".into(),
            status: ToolStatus::Completed,
            title: "Apply patch".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![PatchFile {
                kind: PatchKind::Created,
                path: "/some/new.rs".into(),
                new_path: None,
                diff: None,
            }],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let y = lines
        .iter()
        .position(|l| l.contains("# Created"))
        .unwrap_or_else(|| panic!("`# Created` title row must exist; snap:\n{snap}"))
        as u16;
    let buf = h.terminal.backend().buffer();
    let theme = &h.app.theme.theme;
    let mut x_c: Option<u16> = None;
    for x in 0..buf.area.width {
        if buf[(x, y)].symbol() == "C" {
            x_c = Some(x);
            break;
        }
    }
    let x = x_c.expect("`C` of Created must exist on the title row");
    let cell = &buf[(x, y)];
    assert_eq!(
        cell.style().fg.unwrap_or(ratatui::style::Color::Reset),
        theme.diff_added,
        "`Created` verb must use theme.diff_added accent; cell={cell:?}",
    );
}

#[test]
fn apply_patch_moved_shows_old_arrow_new() {
    use raider_tui::{HostMessage, PatchFile, PatchKind, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Renaming", "").with_tool(ToolCall {
            id: None,
            name: "apply_patch".into(),
            status: ToolStatus::Completed,
            title: "Apply patch".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![PatchFile {
                kind: PatchKind::Moved,
                path: "/old/path.rs".into(),
                new_path: Some("/new/path.rs".into()),
                diff: None,
            }],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("# Moved /old/path.rs → /new/path.rs"),
        "Moved title must show `<old> → <new>`; snap:\n{snap}",
    );
}

#[test]
fn apply_patch_diff_body_renders_with_line_numbers() {
    use raider_tui::{HostMessage, PatchFile, PatchKind, ToolCall, ToolStatus};
    let diff = "Index: /tmp/x.rs\n\
===================================================================\n\
--- /tmp/x.rs\n\
+++ /tmp/x.rs\n\
@@ -1,2 +1,2 @@\n\
-old\n\
+new\n";
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Patch", "").with_tool(ToolCall {
            id: None,
            name: "apply_patch".into(),
            status: ToolStatus::Completed,
            title: "Apply patch".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![PatchFile {
                kind: PatchKind::Patched,
                path: "/tmp/x.rs".into(),
                new_path: None,
                diff: Some(diff.into()),
            }],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("← Patched /tmp/x.rs"),
        "Patched title row must render; snap:\n{snap}",
    );
    assert!(
        !snap.contains("@@ -1,2 +1,2 @@"),
        "hunk header must NOT render under per-file block (opencode parity); snap:\n{snap}",
    );
    assert!(
        snap.contains("new"),
        "added-line content must render; snap:\n{snap}",
    );
    assert!(
        snap.contains("old"),
        "removed-line content must render; snap:\n{snap}",
    );
    let lines: Vec<&str> = snap.lines().collect();
    let added_y = lines.iter().position(|l| l.contains("new")).unwrap() as u16;
    let buf = h.terminal.backend().buffer();
    let mut saw_digit = false;
    for x in 0..buf.area.width {
        let sym = buf[(x, added_y)].symbol();
        if sym == "+" {
            break;
        }
        if sym == "1" {
            saw_digit = true;
        }
    }
    assert!(
        saw_digit,
        "added line must carry a line-number column before the `+` sign; snap:\n{snap}",
    );
}

#[test]
fn apply_patch_inline_fallback_when_patches_empty() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Preparing patch", "").with_tool(ToolCall {
            id: None,
            name: "apply_patch".into(),
            status: ToolStatus::Running,
            title: "Apply patch".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("~ Preparing patch..."),
        "inline-fallback row must surface opencode's pending label; snap:\n{snap}",
    );
    assert!(
        !snap.contains("Apply patch"),
        "inline fallback must not use the server-supplied title; snap:\n{snap}",
    );
    assert!(
        !snap.contains("← Patched")
            && !snap.contains("# Created")
            && !snap.contains("# Deleted")
            && !snap.contains("# Moved"),
        "no per-file block titles must appear when patches is empty; snap:\n{snap}",
    );
}

#[test]
fn apply_patch_error_wraps_instead_of_truncating_to_one_row() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(82, 36);
    let err = "apply_patch verification failed: Error: Failed to find expected lines in /home/emre/Desktop/raider/crates/raider-tui/src/app/state.rs:     /// Rotating placeholder examples surfaced in the empty prompt textarea. Mirrors opencode TUI routes/home.tsx";
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Patch failed", "").with_tool(ToolCall {
            id: None,
            name: "apply_patch".into(),
            status: ToolStatus::Error,
            title: "apply_patch [patchText=*** Begin Patch ... very long raw input]".into(),
            command: None,
            output: String::new(),
            error: Some(err.into()),
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("~ Preparing patch..."),
        "apply_patch error fallback must keep opencode's pending label; snap:\n{snap}",
    );
    assert!(
        !snap.contains("patchText="),
        "raw apply_patch input summary must not replace the pending label; snap:\n{snap}",
    );
    assert!(
        snap.contains("apply_patch verification failed"),
        "error prefix visible; snap:\n{snap}",
    );
    assert!(
        snap.contains("Rotating placeholder examples surfaced"),
        "wrapped tail of the error visible; snap:\n{snap}",
    );
    assert!(
        snap.lines().any(|l| l.contains("Failed to find expected"))
            && snap
                .lines()
                .any(|l| l.contains("Rotating placeholder examples surfaced")),
        "long error should occupy multiple readable rows; snap:\n{snap}",
    );
    assert!(
        !snap
            .lines()
            .any(|l| l.contains("apply_patch verification failed")
                && l.contains("Rotating placeholder examples surfaced")),
        "error must not be a single clipped row; snap:\n{snap}",
    );
}

#[test]
fn question_tool_answered_renders_block_with_qa_pairs() {
    use raider_tui::{HostMessage, Question, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Asking", "").with_tool(ToolCall {
            id: None,
            name: "question".into(),
            status: ToolStatus::Completed,
            title: "Asked 2 questions".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![
                Question {
                    text: "Which database driver should we use?".into(),
                    options: vec!["postgres".into(), "sqlite".into()],
                },
                Question {
                    text: "Enable tracing?".into(),
                    options: vec!["yes".into(), "no".into()],
                },
            ],
            answers: vec![vec!["postgres".into()], vec!["yes".into()]],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("# Questions"),
        "answered-question block must show `# Questions` header; snap:\n{snap}",
    );
    assert!(
        snap.contains("Q: Which database driver should we use?"),
        "first question text must render with `Q:` prefix; snap:\n{snap}",
    );
    assert!(
        snap.contains("A: postgres"),
        "first answer must render with `A:` prefix; snap:\n{snap}",
    );
    assert!(
        snap.contains("Q: Enable tracing?"),
        "second question text must render; snap:\n{snap}",
    );
    assert!(
        snap.contains("A: yes"),
        "second answer must render; snap:\n{snap}",
    );
    assert!(
        !snap.contains("→ Asked"),
        "InlineTool `→ Asked` must NOT render when answers are present; snap:\n{snap}",
    );
}

#[test]
fn question_tool_pending_renders_inline_summary() {
    use raider_tui::{HostMessage, Question, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Asking", "").with_tool(ToolCall {
            id: None,
            name: "question".into(),
            status: ToolStatus::Running,
            title: "Asked 2 questions".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![
                Question {
                    text: "q1".into(),
                    options: vec![],
                },
                Question {
                    text: "q2".into(),
                    options: vec![],
                },
            ],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Asked 2 questions"),
        "pending question must render inline summary; snap:\n{snap}",
    );
    assert!(
        !snap.contains("# Questions"),
        "no `# Questions` block must render until answers arrive; snap:\n{snap}",
    );
}

#[test]
fn question_tool_streaming_with_zero_questions_renders_asking_not_asked_zero() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Asking", "").with_tool(ToolCall {
            id: None,
            name: "question".into(),
            status: ToolStatus::Running,
            title: "Question".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Asking questions..."),
        "streaming question tool with empty `questions` must render \
         opencode's `Asking questions...` running label; snap:\n{snap}",
    );
    assert!(
        !snap.contains("Asked 0 questions"),
        "must NEVER render the misleading `Asked 0 questions` label \
         (the model has not asked anything yet); snap:\n{snap}",
    );
    assert!(
        !snap.contains("# Questions"),
        "no `# Questions` block while streaming; snap:\n{snap}",
    );
}

#[test]
fn click_to_expand_toggles_full_output() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    let many_lines: String = (1..=20)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("ran", "").with_tool(ToolCall {
            id: Some("prt_bash_xyz".into()),
            name: "bash".into(),
            status: ToolStatus::Completed,
            title: "many lines".into(),
            command: Some("seq 1 20".into()),
            output: many_lines,
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Click to expand"),
        "collapsed bash with overflow must surface the `Click to expand` hint; snap:\n{snap}",
    );
    assert!(
        !snap.contains("line15"),
        "collapsed bash must NOT render lines past the 10-line truncation window; snap:\n{snap}",
    );

    h.dispatch(Action::View(ViewAction::ToggleToolExpanded {
        id: "prt_bash_xyz".into(),
    }));
    let snap = h.snapshot();
    assert!(
        snap.contains("line15") && snap.contains("line20"),
        "expanded bash must render every output line; snap:\n{snap}",
    );
    assert!(
        snap.contains("Click to collapse"),
        "expanded bash must show the inverse `Click to collapse` hint; snap:\n{snap}",
    );
    assert!(
        !snap.contains("Click to expand"),
        "the `Click to expand` hint must NOT linger after expanding; snap:\n{snap}",
    );

    h.dispatch(Action::View(ViewAction::ToggleToolExpanded {
        id: "prt_bash_xyz".into(),
    }));
    let snap = h.snapshot();
    assert!(
        snap.contains("Click to expand"),
        "collapsing again must restore the `Click to expand` hint; snap:\n{snap}",
    );
    assert!(
        !snap.contains("line15"),
        "collapsing must hide lines past the truncation window; snap:\n{snap}",
    );
}

#[test]
fn click_to_expand_toggles_finalized_ordered_tool_part() {
    use raider_tui::{HostMessage, HostMessagePart, ToolCall, ToolStatus};

    let mut h = Harness::new(160, 30);
    let many_lines: String = (1..=20)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let tool = ToolCall {
        id: Some("prt_ordered_bash".into()),
        name: "bash".into(),
        status: ToolStatus::Completed,
        title: "many lines".into(),
        command: Some("seq 1 20".into()),
        output: many_lines,
        error: None,
        todos: vec![],
        file_path: None,
        diff: None,
        loaded: vec![],
        patches: vec![],
        questions: vec![],
        answers: vec![],
        expanded: false,
        current_child: None,
        child_tool_count: 0,
        started_at_ms: None,
        completed_at_ms: None,
    };
    let mut message = HostMessage::assistant("ran", "").with_tool(tool.clone());
    message.parts = vec![
        HostMessagePart::Text("ran".into()),
        HostMessagePart::Tool(Box::new(tool)),
    ];
    h.dispatch(Action::Host(HostAction::ReplaceMessages(vec![message])));

    let snap = h.snapshot();
    assert!(
        snap.contains("Click to expand") && !snap.contains("line15"),
        "finalized ordered tool should start collapsed; snap:\n{snap}",
    );

    h.dispatch(Action::View(ViewAction::ToggleToolExpanded {
        id: "prt_ordered_bash".into(),
    }));
    let snap = h.snapshot();
    assert!(
        snap.contains("line15") && snap.contains("Click to collapse"),
        "toggling a finalized ordered tool must update HostMessagePart::Tool too; snap:\n{snap}",
    );
}

#[test]
fn tool_block_rects_cached_for_click_hit_testing() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("ran", "").with_tool(ToolCall {
            id: Some("prt_bash_rect".into()),
            name: "bash".into(),
            status: ToolStatus::Completed,
            title: "rect test".into(),
            command: Some("echo hi".into()),
            output: "hi\n".into(),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    assert!(
        h.app
            .messages
            .tool_block_rects
            .iter()
            .any(|(id, r)| id == "prt_bash_rect" && r.width > 0 && r.height > 0),
        "render_messages must cache a non-empty rect for every tool with an id; \
         got: {:?}",
        h.app.messages.tool_block_rects,
    );
}

#[test]
fn diff_body_syntax_highlights_rust_keywords() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(100, 30);
    let diff = "@@ -1,1 +1,2 @@\n-old_line\n+fn highlighted_token() {}\n";
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "edit".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/file.rs".into()),
            diff: Some(diff.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("fn highlighted_token()"),
        "added line must surface; snap:\n{snap}",
    );
    let row = find_row(&snap, "highlighted_token");
    let buf = h.terminal.backend().buffer();
    let mut fgs: std::collections::HashSet<ratatui::style::Color> = Default::default();
    for x in 0..buf.area.width {
        let cell = &buf[(x, row)];
        let symbol = cell.symbol();
        if symbol == " " || symbol.is_empty() {
            continue;
        }
        if let Some(fg) = cell.style().fg {
            fgs.insert(fg);
        }
    }
    assert!(
        fgs.len() >= 2,
        "syntax-highlighted diff row must have at least 2 distinct foreground colors \
         across its tokens (keyword vs identifier vs punctuation); \
         got {} distinct fg colors on the row: {fgs:?}",
        fgs.len(),
    );
}

fn assert_diff_row_highlighted(
    h: &mut Harness,
    file_path: &str,
    diff: &str,
    needle: &str,
    lang_label: &str,
) {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "edit".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some(file_path.into()),
            diff: Some(diff.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    let row = find_row(&snap, needle);
    let buf = h.terminal.backend().buffer();
    let mut fgs: std::collections::HashSet<ratatui::style::Color> = Default::default();
    for x in 0..buf.area.width {
        let cell = &buf[(x, row)];
        let symbol = cell.symbol();
        if symbol == " " || symbol.is_empty() {
            continue;
        }
        if let Some(fg) = cell.style().fg {
            fgs.insert(fg);
        }
    }
    assert!(
        fgs.len() >= 2,
        "syntax-highlighted {lang_label} diff row must have at least 2 distinct \
         foreground colors across its tokens; got {} on row: {fgs:?}\nsnap:\n{snap}",
        fgs.len(),
    );
}

#[test]
fn diff_body_syntax_highlights_nix_keywords() {
    let mut h = Harness::new(120, 30);
    let diff = "@@ -1,1 +1,2 @@\n-old\n+let x = pkgs.gcc; in x\n";
    assert_diff_row_highlighted(&mut h, "/tmp/flake.nix", diff, "pkgs.gcc", "nix");
}

#[test]
fn diff_body_syntax_highlights_toml_keys() {
    let mut h = Harness::new(120, 30);
    let diff = "@@ -1,1 +1,2 @@\n-old\n+name = \"raider\"\n";
    assert_diff_row_highlighted(&mut h, "/tmp/Cargo.toml", diff, "raider", "toml");
}

#[test]
fn diff_body_syntax_highlights_typescript_keywords() {
    let mut h = Harness::new(120, 30);
    let diff = "@@ -1,1 +1,2 @@\n-old\n+const greet = (n: string) => n\n";
    assert_diff_row_highlighted(&mut h, "/tmp/x.ts", diff, "greet", "typescript");
}

#[test]
fn diff_body_falls_back_to_plain_when_no_extension() {
    // `Makefile`, `LICENSE`) must render as plain text without
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(100, 30);
    let diff = "@@ -1,1 +1,1 @@\n-old\n+new\n";
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(ToolCall {
            id: None,
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "edit".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/Makefile".into()),
            diff: Some(diff.into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("new"),
        "added line must render even without syntax; snap:\n{snap}"
    );
    assert!(
        snap.contains("old"),
        "removed line must render; snap:\n{snap}"
    );
}

#[test]
fn tool_render_cache_hits_on_unchanged_tool() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(120, 20);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(ToolCall {
            id: Some("prt_cache_hit".into()),
            name: "edit".into(),
            status: ToolStatus::Completed,
            title: "edit".into(),
            command: None,
            output: String::new(),
            error: None,
            todos: vec![],
            file_path: Some("/tmp/file.rs".into()),
            diff: Some("@@ -1,1 +1,1 @@\n-old\n+new\n".into()),
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let cache_present = h
        .app
        .messages
        .iter()
        .any(|m| m.tool_render_cache.contains_key("prt_cache_hit"));
    assert!(
        cache_present,
        "expected `prt_cache_hit` to land in the per-tool render cache; \
         cache keys: {:?}",
        h.app
            .messages
            .iter()
            .flat_map(|m| m.tool_render_cache.keys().cloned())
            .collect::<Vec<_>>(),
    );
}

#[test]
fn tool_render_cache_evicts_stale_ids_on_remove() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(120, 20);
    let mk = |id: &str| ToolCall {
        id: Some(id.into()),
        name: "edit".into(),
        status: ToolStatus::Completed,
        title: id.into(),
        command: None,
        output: String::new(),
        error: None,
        todos: vec![],
        file_path: Some("/tmp/file.rs".into()),
        diff: Some("@@ -1,1 +1,1 @@\n-old\n+new\n".into()),
        loaded: vec![],
        patches: vec![],
        questions: vec![],
        answers: vec![],
        expanded: false,
        current_child: None,
        child_tool_count: 0,
        started_at_ms: None,
        completed_at_ms: None,
    };
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(mk("prt_a")),
    )));
    h.draw();
    assert!(h
        .app
        .messages
        .iter()
        .any(|m| m.tool_render_cache.contains_key("prt_a")));
    if let Some(msg) = h.app.messages.messages.last_mut() {
        msg.tool_calls = vec![mk("prt_b")];
        msg.invalidate_render_cache();
    }
    h.draw();
    let stale = h
        .app
        .messages
        .iter()
        .any(|m| m.tool_render_cache.contains_key("prt_a"));
    let fresh = h
        .app
        .messages
        .iter()
        .any(|m| m.tool_render_cache.contains_key("prt_b"));
    assert!(
        !stale,
        "stale `prt_a` must be evicted after the tool list churns"
    );
    assert!(
        fresh,
        "fresh `prt_b` must populate the cache on the next render"
    );
}

#[test]
fn toggle_tool_expanded_with_unknown_id_is_noop() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("ran", "").with_tool(ToolCall {
            id: Some("prt_real".into()),
            name: "bash".into(),
            status: ToolStatus::Completed,
            title: "real".into(),
            command: Some("true".into()),
            output: (1..=20)
                .map(|i| format!("line{i}"))
                .collect::<Vec<_>>()
                .join("\n"),
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    h.dispatch(Action::View(ViewAction::ToggleToolExpanded {
        id: "prt_nonexistent".into(),
    }));
    let snap = h.snapshot();
    assert!(
        snap.contains("Click to expand"),
        "unknown-id toggle must NOT expand any tool; snap:\n{snap}",
    );
}

fn empty_tool(name: &str) -> raider_tui::ToolCall {
    use raider_tui::{ToolCall, ToolStatus};
    ToolCall {
        id: None,
        name: name.into(),
        status: ToolStatus::Running,
        title: String::new(),
        command: None,
        output: String::new(),
        error: None,
        todos: vec![],
        file_path: None,
        diff: None,
        loaded: vec![],
        patches: vec![],
        questions: vec![],
        answers: vec![],
        expanded: false,
        current_child: None,
        child_tool_count: 0,
        started_at_ms: None,
        completed_at_ms: None,
    }
}

fn assert_inline_label(tool: raider_tui::ToolCall, expected: &str) {
    use raider_tui::HostMessage;
    let mut h = Harness::new(160, 24);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("p", "").with_tool(tool),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains(expected),
        "inline tool must show {expected:?}; snap:\n{snap}",
    );
}

#[test]
fn bash_pending_inline_label_matches_opencode_writing_command() {
    let mut t = empty_tool("bash");
    t.title = "Bash".to_string();
    assert_inline_label(t, "Writing command...");
}

#[test]
fn read_pending_inline_label_matches_opencode_reading_file() {
    let mut t = empty_tool("read");
    t.title = "Read".to_string();
    assert_inline_label(t, "Reading file...");
}

#[test]
fn write_pending_inline_label_matches_opencode_preparing_write() {
    let mut t = empty_tool("write");
    t.title = "Write".to_string();
    assert_inline_label(t, "Preparing write...");
}

#[test]
fn edit_pending_inline_label_matches_opencode_preparing_edit() {
    let mut t = empty_tool("edit");
    t.title = "Edit".to_string();
    assert_inline_label(t, "Preparing edit...");
}

#[test]
fn glob_pending_inline_label_matches_opencode_finding_files() {
    let mut t = empty_tool("glob");
    t.title = "Glob".to_string();
    assert_inline_label(t, "Finding files...");
}

#[test]
fn grep_pending_inline_label_matches_opencode_searching_content() {
    let mut t = empty_tool("grep");
    t.title = "Grep".to_string();
    assert_inline_label(t, "Searching content...");
}

#[test]
fn webfetch_pending_inline_label_matches_opencode_fetching_from_the_web() {
    let mut t = empty_tool("webfetch");
    t.title = "WebFetch".to_string();
    assert_inline_label(t, "Fetching from the web...");
}

#[test]
fn websearch_pending_inline_label_matches_opencode_searching_web() {
    let mut t = empty_tool("websearch");
    t.title = "WebSearch".to_string();
    assert_inline_label(t, "Searching web...");
}

#[test]
fn todowrite_pending_inline_label_matches_opencode_updating_todos() {
    let mut t = empty_tool("todowrite");
    t.title = "TodoWrite".to_string();
    assert_inline_label(t, "Updating todos...");
}

#[test]
fn task_pending_inline_label_matches_opencode_delegating() {
    let mut t = empty_tool("task");
    t.title = "Task".to_string();
    assert_inline_label(t, "Delegating...");
}

#[test]
fn skill_pending_inline_label_matches_opencode_loading_skill() {
    let mut t = empty_tool("skill");
    t.title = "Skill".to_string();
    assert_inline_label(t, "Loading skill...");
}

#[test]
fn apply_patch_pending_inline_label_matches_opencode_preparing_patch() {
    let mut t = empty_tool("apply_patch");
    t.title = "Patch".to_string();
    assert_inline_label(t, "Preparing patch...");
}

#[test]
fn write_with_filepath_uses_complete_label_not_pending() {
    use raider_tui::ToolStatus;
    let mut t = empty_tool("write");
    t.title = "Write".to_string();
    t.file_path = Some("/tmp/x.rs".into());
    t.status = ToolStatus::Running;
    assert_inline_label(t, "Write /tmp/x.rs");
}

#[test]
fn edit_with_filepath_uses_complete_label_not_pending() {
    use raider_tui::ToolStatus;
    let mut t = empty_tool("edit");
    t.title = "Edit".to_string();
    t.file_path = Some("/tmp/y.ts".into());
    t.status = ToolStatus::Running;
    assert_inline_label(t, "Edit /tmp/y.ts");
}

#[test]
fn read_with_rich_host_title_is_preserved() {
    use raider_tui::ToolStatus;
    let mut t = empty_tool("read");
    t.title = "Read foo.rs [offset=10, limit=20]".into();
    t.file_path = Some("foo.rs".into());
    t.status = ToolStatus::Completed;
    assert_inline_label(t, "[offset=10, limit=20]");
}

#[test]
fn toggling_tool_expansion_does_not_force_scroll_to_bottom() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(160, 24);
    for i in 0..30 {
        h.dispatch(Action::Host(HostAction::AppendMessage(HostMessage::user(
            format!("filler user message {i}"),
        ))));
    }
    let many_lines: String = (1..=20)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("ran", "").with_tool(ToolCall {
            id: Some("prt_target".into()),
            name: "bash".into(),
            status: ToolStatus::Completed,
            title: "target".into(),
            command: Some("seq 1 20".into()),
            output: many_lines,
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    h.dispatch(Action::User(UserAction::MouseScroll { lines: 5 }));
    assert!(
        !h.app.scroll.scroll_stick_to_bottom,
        "test setup: scrolling up must disable stick-to-bottom",
    );
    let pre_offset = h.app.scroll.list_state.offset();

    h.dispatch(Action::View(ViewAction::ToggleToolExpanded {
        id: "prt_target".into(),
    }));

    assert!(
        !h.app.scroll.scroll_stick_to_bottom,
        "toggling tool expansion must NOT force scroll back to the bottom \
         (was the reported user bug); scroll_stick_to_bottom must stay false",
    );
    let post_offset = h.app.scroll.list_state.offset();
    assert_eq!(
        post_offset, pre_offset,
        "list_state offset must be preserved across a tool-expand toggle; \
         pre={pre_offset} post={post_offset}",
    );
}

#[test]
fn expanded_bash_output_capped_at_256_lines() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(120, 60);
    let many: String = (1..=10_000)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("running yes", "").with_tool(ToolCall {
            id: Some("prt_runaway".into()),
            name: "bash".into(),
            status: ToolStatus::Completed,
            title: "yes loop".into(),
            command: Some("yes &; sleep 5; kill yes".into()),
            output: many,
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: true,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();

    for old in ["line1\n", "line500\n", "line5000\n", "line9700\n"] {
        assert!(
            !snap.contains(old),
            "expanded bash output must drop old lines past the 256-line \
             tail cap; found `{}` in snap:\n{snap}",
            old.trim(),
        );
    }
    assert!(
        snap.contains("line10000"),
        "expanded bash output must preserve the trailing lines; snap:\n{snap}",
    );
    assert!(
        snap.contains("line9999"),
        "expanded bash output must preserve the trailing lines; snap:\n{snap}",
    );
}

#[test]
fn collapsed_bash_with_huge_output_still_shows_first_10_lines() {
    use raider_tui::{HostMessage, ToolCall, ToolStatus};
    let mut h = Harness::new(120, 30);
    let many: String = (1..=500)
        .map(|i| format!("tailline{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("running", "").with_tool(ToolCall {
            id: Some("prt_collapsed".into()),
            name: "bash".into(),
            status: ToolStatus::Running,
            title: "live tail".into(),
            command: Some("yes".into()),
            output: many,
            error: None,
            todos: vec![],
            file_path: None,
            diff: None,
            loaded: vec![],
            patches: vec![],
            questions: vec![],
            answers: vec![],
            expanded: false,
            current_child: None,
            child_tool_count: 0,
            started_at_ms: None,
            completed_at_ms: None,
        }),
    )));
    h.draw();
    let snap = h.snapshot();
    for i in 1..=10 {
        assert!(
            snap.contains(&format!("tailline{i}\n"))
                || snap.contains(&format!("tailline{i} "))
                || snap.contains(&format!("tailline{i}")),
            "collapsed bash must show first 10 lines of the tail window; \
             missing `tailline{i}` in snap:\n{snap}",
        );
    }
    assert!(
        !snap.contains("tailline11"),
        "collapsed bash must NOT exceed the 10-line truncation window; \
         found `tailline11` in snap:\n{snap}",
    );
    assert!(
        snap.contains('…'),
        "collapsed bash with overflow must show the `…` truncation marker; snap:\n{snap}",
    );
    assert!(
        snap.contains("Click to expand"),
        "collapsed bash with overflow must show the `Click to expand` hint; snap:\n{snap}",
    );
}

#[test]
fn host_remove_tool_call_drops_matching_tool_id() {
    use raider_tui::action::{ToolCall, ToolStatus};
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    let tool_keep = ToolCall {
        id: Some("prt-keep".into()),
        name: "bash".into(),
        status: ToolStatus::Completed,
        title: "echo keep".into(),
        command: Some("echo keep".into()),
        output: "keep".into(),
        error: None,
        todos: vec![],
        file_path: None,
        diff: None,
        loaded: vec![],
        patches: vec![],
        questions: vec![],
        answers: vec![],
        expanded: false,
        current_child: None,
        child_tool_count: 0,
        started_at_ms: None,
        completed_at_ms: None,
    };
    let tool_drop = ToolCall {
        id: Some("prt-drop".into()),
        name: "bash".into(),
        status: ToolStatus::Completed,
        title: "echo drop".into(),
        command: Some("echo drop".into()),
        output: "drop".into(),
        error: None,
        todos: vec![],
        file_path: None,
        diff: None,
        loaded: vec![],
        patches: vec![],
        questions: vec![],
        answers: vec![],
        expanded: false,
        current_child: None,
        child_tool_count: 0,
        started_at_ms: None,
        completed_at_ms: None,
    };
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::assistant("reply", "")
            .with_tool(tool_keep)
            .with_tool(tool_drop),
    )));
    assert_eq!(h.app.messages.messages[0].tool_calls.len(), 2);
    h.dispatch(Action::Host(HostAction::RemoveToolCall("prt-drop".into())));
    assert_eq!(h.app.messages.messages[0].tool_calls.len(), 1);
    assert_eq!(
        h.app.messages.messages[0].tool_calls[0].id.as_deref(),
        Some("prt-keep"),
    );
    h.dispatch(Action::Host(HostAction::RemoveToolCall("prt-never".into())));
    assert_eq!(h.app.messages.messages[0].tool_calls.len(), 1);
}
