use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

use super::theme::Theme;

#[derive(Clone, Copy)]
pub struct SyntaxCtx<'a> {
    pub ps: &'a SyntaxSet,
    pub ts: &'a ThemeSet,
    pub filetype: &'a str,
    pub mode: super::theme::Mode,
    pub synth_theme: Option<&'a syntect::highlighting::Theme>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Added,
    Removed,
    Context,
    Hunk,
    NoNewline,
}

#[derive(Debug)]
struct Row {
    kind: Kind,
    old_line: Option<u32>,
    new_line: Option<u32>,
    content: String,
}

fn parse(diff: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut old_line: u32 = 0;
    let mut new_line: u32 = 0;
    let mut seen_first_hunk = false;

    for raw in diff.lines() {
        if !seen_first_hunk {
            if raw.starts_with("@@") {
                seen_first_hunk = true;
            } else {
                continue;
            }
        }

        if raw.starts_with("@@") {
            if let Some((o, n)) = parse_hunk_header(raw) {
                old_line = o;
                new_line = n;
            }
            rows.push(Row {
                kind: Kind::Hunk,
                old_line: None,
                new_line: None,
                content: raw.to_string(),
            });
            continue;
        }

        if raw.starts_with("+++") || raw.starts_with("---") {
            seen_first_hunk = false;
            continue;
        }

        if let Some(content) = raw.strip_prefix('+') {
            rows.push(Row {
                kind: Kind::Added,
                old_line: None,
                new_line: Some(new_line),
                content: content.to_string(),
            });
            new_line += 1;
        } else if let Some(content) = raw.strip_prefix('-') {
            rows.push(Row {
                kind: Kind::Removed,
                old_line: Some(old_line),
                new_line: None,
                content: content.to_string(),
            });
            old_line += 1;
        } else if raw.starts_with('\\') {
            rows.push(Row {
                kind: Kind::NoNewline,
                old_line: None,
                new_line: None,
                content: raw.to_string(),
            });
        } else {
            let content = if raw.is_empty() {
                String::new()
            } else if let Some(content) = raw.strip_prefix(' ') {
                content.to_string()
            } else {
                raw.to_string()
            };
            rows.push(Row {
                kind: Kind::Context,
                old_line: Some(old_line),
                new_line: Some(new_line),
                content,
            });
            old_line += 1;
            new_line += 1;
        }
    }
    rows
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] != b'-' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    i += 1;
    let old_start = read_number(&line[i..])?;

    while i < bytes.len() && bytes[i] != b'+' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    i += 1;
    let new_start = read_number(&line[i..])?;

    Some((old_start, new_start))
}

fn read_number(s: &str) -> Option<u32> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    s[..end].parse().ok()
}

pub fn render_diff_block<'a>(
    diff: &str,
    theme: &Theme,
    bar: Span<'a>,
    gap: Span<'a>,
) -> Vec<Line<'a>> {
    render_diff_block_with_width(diff, theme, bar, gap, 0, None)
}

pub fn render_diff_block_with_width<'a>(
    diff: &str,
    theme: &Theme,
    bar: Span<'a>,
    gap: Span<'a>,
    content_width: u16,
    syntax: Option<SyntaxCtx<'_>>,
) -> Vec<Line<'a>> {
    let rows = parse(diff);

    let max_num = rows
        .iter()
        .flat_map(|r| [r.old_line, r.new_line])
        .flatten()
        .max()
        .unwrap_or(1);
    let width = max_num.to_string().len().max(1);

    // PERFORMANCE: build the syntect highlighter ONCE per diff block.
    let mut hl = syntax.and_then(|s| build_highlighter(&s));

    if content_width > 120 {
        return render_split(
            &rows,
            width,
            theme,
            (bar.clone(), gap.clone()),
            content_width,
            syntax,
            hl.as_mut(),
        );
    }

    let mut out: Vec<Line<'a>> = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(render_row(
            &row,
            width,
            theme,
            bar.clone(),
            gap.clone(),
            syntax,
            hl.as_mut(),
        ));
    }
    out
}

fn build_highlighter<'a>(ctx: &SyntaxCtx<'a>) -> Option<HighlightLines<'a>> {
    if ctx.filetype.is_empty() {
        return None;
    }
    let synref = ctx.ps.find_syntax_by_token(ctx.filetype)?;
    if let Some(synth) = ctx.synth_theme {
        return Some(HighlightLines::new(synref, synth));
    }
    let theme_name = match ctx.mode {
        super::theme::Mode::Dark => "base16-ocean.dark",
        super::theme::Mode::Light => "InspiredGitHub",
    };
    let synth_theme = ctx
        .ts
        .themes
        .get(theme_name)
        .or_else(|| ctx.ts.themes.get("base16-ocean.dark"))
        .or_else(|| ctx.ts.themes.values().next())?;
    Some(HighlightLines::new(synref, synth_theme))
}

fn render_split<'a>(
    rows: &[Row],
    num_width: usize,
    theme: &Theme,
    indent: (Span<'a>, Span<'a>),
    content_width: u16,
    syntax: Option<SyntaxCtx<'_>>,
    hl: Option<&mut HighlightLines<'_>>,
) -> Vec<Line<'a>> {
    let mut groups: Vec<SplitGroup> = Vec::new();
    let mut current: Vec<SplitRow> = Vec::new();
    let mut current_hunk_header: Option<String> = None;
    let flush =
        |groups: &mut Vec<SplitGroup>, buf: &mut Vec<SplitRow>, header: &mut Option<String>| {
            if !buf.is_empty() || header.is_some() {
                groups.push(SplitGroup {
                    hunk: header.take(),
                    rows: std::mem::take(buf),
                });
            }
        };

    let mut i = 0;
    while i < rows.len() {
        let row = &rows[i];
        match row.kind {
            Kind::Hunk => {
                flush(&mut groups, &mut current, &mut current_hunk_header);
                current_hunk_header = Some(row.content.clone());
                i += 1;
            }
            Kind::Context => {
                current.push(SplitRow {
                    left: Some(SplitCell {
                        line: row.old_line,
                        content: row.content.clone(),
                        kind: SplitKind::Context,
                    }),
                    right: Some(SplitCell {
                        line: row.new_line,
                        content: row.content.clone(),
                        kind: SplitKind::Context,
                    }),
                });
                i += 1;
            }
            Kind::NoNewline => {
                i += 1;
            }
            Kind::Added | Kind::Removed => {
                let mut removes: Vec<(Option<u32>, String)> = Vec::new();
                let mut adds: Vec<(Option<u32>, String)> = Vec::new();
                while i < rows.len() {
                    match rows[i].kind {
                        Kind::Removed => {
                            removes.push((rows[i].old_line, rows[i].content.clone()));
                            i += 1;
                        }
                        Kind::Added => {
                            adds.push((rows[i].new_line, rows[i].content.clone()));
                            i += 1;
                        }
                        _ => break,
                    }
                }
                let max = removes.len().max(adds.len());
                for j in 0..max {
                    let left = removes.get(j).map(|(line, content)| SplitCell {
                        line: *line,
                        content: content.clone(),
                        kind: SplitKind::Removed,
                    });
                    let right = adds.get(j).map(|(line, content)| SplitCell {
                        line: *line,
                        content: content.clone(),
                        kind: SplitKind::Added,
                    });
                    current.push(SplitRow { left, right });
                }
            }
        }
    }
    flush(&mut groups, &mut current, &mut current_hunk_header);

    let total_minus_divider = content_width.saturating_sub(2 + 1);
    let half = (total_minus_divider / 2).max(20) as usize;

    let mut out: Vec<Line<'a>> = Vec::new();
    let mut hl_owned = hl;
    for group in groups {
        let _ = &group.hunk;
        for split_row in group.rows {
            out.push(render_split_row(
                &split_row,
                num_width,
                half,
                theme,
                (indent.0.clone(), indent.1.clone()),
                syntax,
                hl_owned.as_deref_mut(),
            ));
        }
    }
    out
}

#[derive(Debug)]
struct SplitGroup {
    hunk: Option<String>,
    rows: Vec<SplitRow>,
}

#[derive(Debug)]
struct SplitRow {
    left: Option<SplitCell>,
    right: Option<SplitCell>,
}

#[derive(Debug, Clone)]
struct SplitCell {
    line: Option<u32>,
    content: String,
    kind: SplitKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitKind {
    Added,
    Removed,
    Context,
}

fn render_split_row<'a>(
    row: &SplitRow,
    num_width: usize,
    half_width: usize,
    theme: &Theme,
    indent: (Span<'a>, Span<'a>),
    syntax: Option<SyntaxCtx<'_>>,
    mut hl: Option<&mut HighlightLines<'_>>,
) -> Line<'a> {
    let line_number_fg = theme.diff_line_number;
    let divider_bg = theme.background_panel;

    let left_spans = render_split_cell(
        row.left.as_ref(),
        num_width,
        half_width,
        line_number_fg,
        theme,
        syntax,
        hl.as_deref_mut(),
    );
    let right_spans = render_split_cell(
        row.right.as_ref(),
        num_width,
        half_width,
        line_number_fg,
        theme,
        syntax,
        hl,
    );

    let divider = Span::styled(" ", Style::default().bg(divider_bg));

    let mut spans: Vec<Span<'a>> = Vec::with_capacity(2 + left_spans.len() + 1 + right_spans.len());
    spans.push(indent.0);
    spans.push(indent.1);
    spans.extend(left_spans);
    spans.push(divider);
    spans.extend(right_spans);
    Line::from(spans)
}

fn render_split_cell<'a>(
    cell: Option<&SplitCell>,
    num_width: usize,
    half_width: usize,
    line_number_fg: Color,
    theme: &Theme,
    syntax: Option<SyntaxCtx<'_>>,
    hl: Option<&mut HighlightLines<'_>>,
) -> Vec<Span<'a>> {
    let (sign, sign_fg, content_fg, content_bg, num_bg, line_num) = match cell {
        Some(c) => match c.kind {
            SplitKind::Added => (
                '+',
                theme.diff_highlight_added,
                theme.diff_added,
                theme.diff_added_bg,
                theme.diff_added_line_number_bg,
                c.line,
            ),
            SplitKind::Removed => (
                '-',
                theme.diff_highlight_removed,
                theme.diff_removed,
                theme.diff_removed_bg,
                theme.diff_removed_line_number_bg,
                c.line,
            ),
            SplitKind::Context => (
                ' ',
                theme.diff_context,
                theme.diff_context,
                theme.diff_context_bg,
                theme.diff_context_bg,
                c.line,
            ),
        },
        None => (
            ' ',
            theme.diff_context,
            theme.diff_context,
            theme.diff_context_bg,
            theme.diff_context_bg,
            None,
        ),
    };

    let overhead = num_width + 3;
    let body_width = half_width.saturating_sub(overhead).max(1);
    let content_raw = match cell {
        Some(c) => c.content.clone(),
        None => String::new(),
    };
    let body = fit_to_width(&content_raw, body_width);

    let num_cell = format_num_cell(line_num, num_width, line_number_fg, num_bg);
    let sep_after_num = Span::styled(" ", Style::default().bg(num_bg));
    let sign_span = Span::styled(
        sign.to_string(),
        Style::default().fg(sign_fg).bg(content_bg),
    );
    let sign_to_content = Span::styled(" ", Style::default().bg(content_bg));
    let content_spans = highlight_content(&body, syntax, hl, content_fg, content_bg);
    let mut spans: Vec<Span<'a>> = Vec::with_capacity(4 + content_spans.len());
    spans.push(num_cell);
    spans.push(sep_after_num);
    spans.push(sign_span);
    spans.push(sign_to_content);
    spans.extend(content_spans);
    spans
}

fn fit_to_width(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() >= width {
        chars.into_iter().take(width).collect()
    } else {
        let mut out: String = chars.into_iter().collect();
        while out.chars().count() < width {
            out.push(' ');
        }
        out
    }
}

fn render_row<'a>(
    row: &Row,
    num_width: usize,
    theme: &Theme,
    bar: Span<'a>,
    gap: Span<'a>,
    syntax: Option<SyntaxCtx<'_>>,
    hl: Option<&mut HighlightLines<'_>>,
) -> Line<'a> {
    let line_number_fg = theme.diff_line_number;

    match row.kind {
        Kind::Hunk => Line::from(vec![
            bar,
            gap,
            Span::styled(row.content.clone(), Style::default().fg(theme.text_muted)),
        ]),
        Kind::NoNewline => Line::from(vec![
            bar,
            gap,
            Span::styled(row.content.clone(), Style::default().fg(theme.text_muted)),
        ]),
        Kind::Added | Kind::Removed | Kind::Context => {
            let (sign, sign_fg, content_fg, content_bg, num_bg) = match row.kind {
                Kind::Added => (
                    '+',
                    theme.diff_highlight_added,
                    theme.diff_added,
                    theme.diff_added_bg,
                    theme.diff_added_line_number_bg,
                ),
                Kind::Removed => (
                    '-',
                    theme.diff_highlight_removed,
                    theme.diff_removed,
                    theme.diff_removed_bg,
                    theme.diff_removed_line_number_bg,
                ),
                Kind::Context => (
                    ' ',
                    theme.diff_context,
                    theme.diff_context,
                    theme.diff_context_bg,
                    theme.diff_context_bg,
                ),
                _ => unreachable!(),
            };

            let old_cell = format_num_cell(row.old_line, num_width, line_number_fg, num_bg);
            let new_cell = format_num_cell(row.new_line, num_width, line_number_fg, num_bg);
            let sep_num_to_sign = Span::styled("  ", Style::default().bg(num_bg));
            let sign_span = Span::styled(
                sign.to_string(),
                Style::default().fg(sign_fg).bg(content_bg),
            );
            let sign_to_content = Span::styled(" ", Style::default().bg(content_bg));
            let content_spans = highlight_content(&row.content, syntax, hl, content_fg, content_bg);

            let mut spans: Vec<Span<'a>> = Vec::with_capacity(7 + content_spans.len());
            spans.push(bar);
            spans.push(gap);
            spans.push(old_cell);
            spans.push(Span::styled("  ", Style::default().bg(num_bg)));
            spans.push(new_cell);
            spans.push(sep_num_to_sign);
            spans.push(sign_span);
            spans.push(sign_to_content);
            spans.extend(content_spans);
            Line::from(spans)
        }
    }
}

fn highlight_content<'a>(
    content: &str,
    syntax: Option<SyntaxCtx<'_>>,
    hl: Option<&mut HighlightLines<'_>>,
    content_fg: Color,
    content_bg: Color,
) -> Vec<Span<'a>> {
    let fallback = || {
        vec![Span::styled(
            content.to_string(),
            Style::default().fg(content_fg).bg(content_bg),
        )]
    };
    let (Some(ctx), Some(hl)) = (syntax, hl) else {
        return fallback();
    };
    if content.is_empty() {
        return fallback();
    }
    let with_nl = format!("{content}\n");
    let ranges = match hl.highlight_line(&with_nl, ctx.ps) {
        Ok(r) => r,
        Err(_) => return fallback(),
    };
    let mut out: Vec<Span<'a>> = Vec::with_capacity(ranges.len());
    for (style, text) in ranges {
        let body = text.trim_end_matches(['\r', '\n']).to_string();
        if body.is_empty() {
            continue;
        }
        let fg = style.foreground;
        out.push(Span::styled(
            body,
            Style::default()
                .fg(Color::Rgb(fg.r, fg.g, fg.b))
                .bg(content_bg),
        ));
    }
    if out.is_empty() {
        return fallback();
    }
    out
}

fn format_num_cell<'a>(n: Option<u32>, width: usize, fg: Color, bg: Color) -> Span<'a> {
    let text = match n {
        Some(v) => format!("{:>width$}", v, width = width),
        None => " ".repeat(width),
    };
    Span::styled(text, Style::default().fg(fg).bg(bg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hunk_header_extracts_old_and_new_start() {
        assert_eq!(parse_hunk_header("@@ -10,5 +20,5 @@"), Some((10, 20)));
        assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some((1, 1)));
        assert_eq!(
            parse_hunk_header("@@ -0,0 +1,3 @@ fn foo() {"),
            Some((0, 1))
        );
    }

    #[test]
    fn parse_skips_envelope_and_assigns_numbers() {
        let diff = "\
Index: /tmp/foo
===
--- a/foo
+++ b/foo
@@ -10,3 +10,3 @@
 ctx1
-removed
+added
 ctx2
";
        let rows = parse(diff);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].kind, Kind::Hunk);
        assert_eq!(rows[1].kind, Kind::Context);
        assert_eq!(rows[1].old_line, Some(10));
        assert_eq!(rows[1].new_line, Some(10));
        assert_eq!(rows[2].kind, Kind::Removed);
        assert_eq!(rows[2].old_line, Some(11));
        assert_eq!(rows[2].new_line, None);
        assert_eq!(rows[3].kind, Kind::Added);
        assert_eq!(rows[3].old_line, None);
        assert_eq!(rows[3].new_line, Some(11));
        assert_eq!(rows[4].kind, Kind::Context);
        assert_eq!(rows[4].old_line, Some(12));
        assert_eq!(rows[4].new_line, Some(12));
    }

    #[test]
    fn parse_strips_no_newline_marker_into_its_own_row() {
        let diff = "@@ -1,1 +1,1 @@\n-foo\n+bar\n\\ No newline at end of file\n";
        let rows = parse(diff);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[3].kind, Kind::NoNewline);
    }

    #[test]
    fn parse_multifile_patch_resets_on_secondary_envelope() {
        let diff = "\
@@ -1,1 +1,1 @@
-a
+b
--- a/other
+++ b/other
@@ -5,1 +5,1 @@
-c
+d
";
        let rows = parse(diff);
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].kind, Kind::Hunk);
        assert_eq!(rows[3].kind, Kind::Hunk);
        assert_eq!(rows[4].kind, Kind::Removed);
        assert_eq!(rows[4].old_line, Some(5));
    }
}
