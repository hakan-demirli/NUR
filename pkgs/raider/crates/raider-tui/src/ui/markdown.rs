use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet, util::LinesWithEndings,
};

use super::theme::Theme;

pub fn render_markdown(
    content: &str,
    width: usize,
    ps: &SyntaxSet,
    ts: &ThemeSet,
    app_theme: &Theme,
    default_style: Style,
    suppress_style: bool,
) -> Vec<Line<'static>> {
    render_markdown_with_synth(
        content,
        width,
        ps,
        ts,
        app_theme,
        MarkdownRenderOptions {
            synth_theme: None,
            default_style,
            suppress_style,
        },
    )
}

pub struct MarkdownRenderOptions<'a> {
    pub synth_theme: Option<&'a syntect::highlighting::Theme>,
    pub default_style: Style,
    pub suppress_style: bool,
}

pub fn render_markdown_with_synth<'a>(
    content: &'a str,
    width: usize,
    ps: &'a SyntaxSet,
    ts: &'a ThemeSet,
    app_theme: &'a Theme,
    options: MarkdownRenderOptions<'a>,
) -> Vec<Line<'static>> {
    let mut ctx = RenderCtx {
        width: width.max(1),
        ps,
        ts,
        synth_theme: options.synth_theme,
        theme: app_theme,
        default_style: options.default_style,
        suppress_style: options.suppress_style,
        lines: Vec::new(),
        current: Vec::new(),
        styles: vec![options.default_style],
        list_stack: Vec::new(),
        blockquote_depth: 0,
        link_urls: Vec::new(),
        image_urls: Vec::new(),
        code_block: None,
        table: None,
    };

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    for event in Parser::new_ext(content, options) {
        ctx.event(event);
    }

    ctx.flush_current();

    while ctx.lines.last().is_some_and(line_is_blank) {
        ctx.lines.pop();
    }

    ctx.lines
}

struct RenderCtx<'a> {
    width: usize,
    ps: &'a SyntaxSet,
    ts: &'a ThemeSet,
    synth_theme: Option<&'a syntect::highlighting::Theme>,
    theme: &'a Theme,
    default_style: Style,
    suppress_style: bool,

    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    styles: Vec<Style>,
    list_stack: Vec<ListState>,
    blockquote_depth: usize,
    link_urls: Vec<String>,
    image_urls: Vec<String>,
    code_block: Option<CodeBlockState>,
    table: Option<TableBuilder>,
}

#[derive(Clone, Copy)]
struct ListState {
    ordered: bool,
    next: u64,
}

struct CodeBlockState {
    language: String,
    body: String,
}

impl<'a> RenderCtx<'a> {
    fn event(&mut self, event: Event<'_>) {
        if let Some(cb) = self.code_block.as_mut() {
            match event {
                Event::Text(t) | Event::Code(t) => cb.body.push_str(&t),
                Event::SoftBreak | Event::HardBreak => cb.body.push('\n'),
                Event::End(TagEnd::CodeBlock) => {
                    let cb = self.code_block.take().unwrap();
                    self.render_code_block(&cb.language, &cb.body);
                    self.push_blank_line();
                }
                _ => {}
            }
            return;
        }

        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.text(&t),
            Event::Code(t) => self.codespan(&t),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => self.rule(),
            Event::Html(html) | Event::InlineHtml(html) => self.html(&html),
            Event::TaskListMarker(checked) => self.task_marker(checked),
            Event::FootnoteReference(label) => self.footnote_ref(&label),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.flush_current();
                let style = self.heading_style();
                self.styles.push(style);
                self.ensure_blockquote_prefix();
                let hashes = "#".repeat(heading_level_to_u8(level) as usize);
                self.current.push(Span::styled(format!("{hashes} "), style));
            }
            Tag::BlockQuote => {
                self.flush_current();
                self.blockquote_depth += 1;
            }
            Tag::CodeBlock(kind) => {
                self.flush_current();
                let language = match kind {
                    CodeBlockKind::Fenced(l) => l.trim().to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code_block = Some(CodeBlockState {
                    language,
                    body: String::new(),
                });
            }
            Tag::List(start) => {
                self.flush_current();
                self.list_stack.push(ListState {
                    ordered: start.is_some(),
                    next: start.unwrap_or(1),
                });
            }
            Tag::Item => {
                self.flush_current();
                self.ensure_blockquote_prefix();
                let depth = self.list_stack.len().saturating_sub(1);
                let indent_len = depth * 2;
                if indent_len > 0 {
                    self.current
                        .push(Span::styled(" ".repeat(indent_len), self.default_style));
                }
                let (prefix, color) = if let Some(state) = self.list_stack.last_mut() {
                    if state.ordered {
                        let out = format!("{}. ", state.next);
                        state.next += 1;
                        (out, self.theme.markdown_list_enumeration)
                    } else {
                        ("• ".to_string(), self.theme.markdown_list_item)
                    }
                } else {
                    ("• ".to_string(), self.theme.markdown_list_item)
                };
                let style = if self.suppress_style {
                    self.default_style
                } else {
                    self.default_style.fg(color)
                };
                self.current.push(Span::styled(prefix, style));
            }
            Tag::Emphasis => {
                let mut style = self.current_style().add_modifier(Modifier::ITALIC);
                if !self.suppress_style {
                    style = style.fg(self.theme.markdown_emph);
                }
                self.styles.push(style);
            }
            Tag::Strong => {
                let mut style = self.current_style().add_modifier(Modifier::BOLD);
                if !self.suppress_style {
                    style = style.fg(self.theme.markdown_strong);
                }
                self.styles.push(style);
            }
            Tag::Strikethrough => {
                let style = self.current_style().add_modifier(Modifier::CROSSED_OUT);
                self.styles.push(style);
            }
            Tag::Link { dest_url, .. } => {
                self.link_urls.push(dest_url.to_string());
                let mut style = self.current_style().add_modifier(Modifier::UNDERLINED);
                if !self.suppress_style {
                    style = style.fg(self.theme.markdown_link_text);
                }
                self.styles.push(style);
            }
            Tag::Image { dest_url, .. } => {
                self.image_urls.push(dest_url.to_string());
                let style = if self.suppress_style {
                    self.current_style()
                } else {
                    self.current_style().fg(self.theme.markdown_image)
                };
                self.styles.push(style);
                if self.table.is_none() {
                    self.ensure_blockquote_prefix();
                }
                self.current.push(Span::styled("[image] ", style));
            }
            Tag::Table(alignments) => {
                self.flush_current();
                self.table = Some(TableBuilder::new(alignments));
            }
            Tag::TableHead => {
                if let Some(tbl) = self.table.as_mut() {
                    tbl.in_header = true;
                }
            }
            Tag::TableRow => {}
            Tag::TableCell => {
                self.current.clear();
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_current();
                self.push_blank_line();
            }
            TagEnd::Heading(_) => {
                self.flush_current();
                if self.styles.len() > 1 {
                    self.styles.pop();
                }
                self.push_blank_line();
            }
            TagEnd::BlockQuote => {
                self.flush_current();
                self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                self.push_blank_line();
            }
            TagEnd::List(_) => {
                self.flush_current();
                self.list_stack.pop();
                self.push_blank_line();
            }
            TagEnd::Item => self.flush_current(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                if self.styles.len() > 1 {
                    self.styles.pop();
                }
            }
            TagEnd::Link => {
                if self.styles.len() > 1 {
                    self.styles.pop();
                }
                if let Some(url) = self.link_urls.pop() {
                    let style = if self.suppress_style {
                        self.default_style
                    } else {
                        self.default_style.fg(self.theme.markdown_link)
                    };
                    self.current.push(Span::styled(format!(" ({url})"), style));
                }
            }
            TagEnd::Image => {
                if self.styles.len() > 1 {
                    self.styles.pop();
                }
                if let Some(url) = self.image_urls.pop() {
                    let style = if self.suppress_style {
                        self.default_style
                    } else {
                        self.default_style.fg(self.theme.markdown_image_text)
                    };
                    self.current.push(Span::styled(format!(" ({url})"), style));
                }
            }
            TagEnd::Table => {
                if let Some(tbl) = self.table.take() {
                    tbl.render(&mut self.lines, self.theme, self.suppress_style, self.width);
                }
                self.push_blank_line();
            }
            TagEnd::TableHead => {
                if let Some(tbl) = self.table.as_mut() {
                    tbl.finish_row();
                    tbl.in_header = false;
                }
            }
            TagEnd::TableRow => {
                if let Some(tbl) = self.table.as_mut() {
                    tbl.finish_row();
                }
            }
            TagEnd::TableCell => {
                if let Some(tbl) = self.table.as_mut() {
                    tbl.push_cell(std::mem::take(&mut self.current));
                }
            }
            TagEnd::CodeBlock => {}
            _ => {}
        }
    }

    fn text(&mut self, t: &str) {
        if self.table.is_some() {
            self.current
                .push(Span::styled(t.to_string(), self.current_style()));
            return;
        }
        self.ensure_blockquote_prefix();

        let style = self.current_style();
        let mut first = true;
        for word in t.split(' ') {
            if !first {
                if self.current_line_width() + 1 > self.width {
                    self.flush_current();
                    self.ensure_blockquote_prefix();
                } else {
                    self.current.push(Span::styled(" ", style));
                }
            }
            first = false;
            if word.is_empty() {
                continue;
            }
            let word_w = display_width(word);
            if !self.current.is_empty() && self.current_line_width() + word_w > self.width {
                self.flush_current();
                self.ensure_blockquote_prefix();
            }
            self.current.push(Span::styled(word.to_string(), style));
        }
    }

    fn codespan(&mut self, t: &str) {
        if self.table.is_none() {
            self.ensure_blockquote_prefix();
        }
        let style = if self.suppress_style {
            self.default_style
        } else {
            self.default_style.fg(self.theme.markdown_code)
        };
        let body = format!("`{t}`");
        if self.table.is_none()
            && !self.current.is_empty()
            && self.current_line_width() + display_width(&body) > self.width
        {
            self.flush_current();
            self.ensure_blockquote_prefix();
        }
        self.current.push(Span::styled(body, style));
    }

    fn soft_break(&mut self) {
        if self.table.is_some() {
            self.current.push(Span::raw(" "));
        } else {
            self.flush_current();
        }
    }

    fn hard_break(&mut self) {
        if self.table.is_some() {
            self.current.push(Span::raw(" "));
        } else {
            self.flush_current();
        }
    }

    fn rule(&mut self) {
        self.flush_current();
        let style = if self.suppress_style {
            self.default_style
        } else {
            self.default_style.fg(self.theme.markdown_horizontal_rule)
        };
        self.lines
            .push(Line::from(Span::styled("─".repeat(self.width), style)));
        self.push_blank_line();
    }

    fn html(&mut self, html: &str) {
        if self.table.is_none() {
            self.ensure_blockquote_prefix();
        }
        let style = if self.suppress_style {
            self.default_style
        } else {
            self.default_style.fg(self.theme.text_muted)
        };
        self.current.push(Span::styled(html.to_string(), style));
    }

    fn task_marker(&mut self, checked: bool) {
        if self.table.is_none() {
            self.ensure_blockquote_prefix();
        }
        let style = if self.suppress_style {
            self.default_style
        } else {
            self.default_style.fg(self.theme.markdown_list_item)
        };
        let mark = if checked { "[x] " } else { "[ ] " };
        self.current.push(Span::styled(mark, style));
    }

    fn footnote_ref(&mut self, label: &str) {
        if self.table.is_none() {
            self.ensure_blockquote_prefix();
        }
        let style = if self.suppress_style {
            self.default_style
        } else {
            self.default_style.fg(self.theme.text_muted)
        };
        self.current
            .push(Span::styled(format!("[^{label}]"), style));
    }

    fn render_code_block(&mut self, language: &str, code: &str) {
        if self.suppress_style {
            for line in code.lines() {
                self.lines.push(Line::from(Span::styled(
                    line.to_string(),
                    self.default_style,
                )));
            }
            return;
        }

        let border_style = self.default_style.fg(self.theme.markdown_horizontal_rule);
        let label_style = self.default_style.fg(self.theme.markdown_code_block);

        let lang_label = if language.is_empty() {
            "code"
        } else {
            language
        };
        let header_used = 1 + 1 + display_width(lang_label) + 1;
        let dashes = self.width.saturating_sub(header_used).max(3);
        self.lines.push(Line::from(vec![
            Span::styled("╭", border_style),
            Span::styled(format!(" {lang_label} "), label_style),
            Span::styled("─".repeat(dashes), border_style),
        ]));

        let preferred = match self.theme.mode {
            super::theme::Mode::Dark => "base16-ocean.dark",
            super::theme::Mode::Light => "InspiredGitHub",
        };
        let bundled = self
            .ts
            .themes
            .get(preferred)
            .or_else(|| self.ts.themes.get("base16-ocean.dark"))
            .unwrap_or_else(|| self.ts.themes.values().next().unwrap());
        let theme = self.synth_theme.unwrap_or(bundled);
        let syntax = self
            .ps
            .find_syntax_by_token(language)
            .unwrap_or_else(|| self.ps.find_syntax_plain_text());
        let mut hl = HighlightLines::new(syntax, theme);

        for line in LinesWithEndings::from(code) {
            let mut spans: Vec<Span<'static>> = vec![Span::styled("│ ", border_style)];
            match hl.highlight_line(line, self.ps) {
                Ok(ranges) => {
                    for (st, text) in ranges {
                        let fg = st.foreground;
                        let body = text.trim_end_matches(['\r', '\n']).to_string();
                        if body.is_empty() {
                            continue;
                        }
                        spans.push(Span::styled(
                            body,
                            Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b)),
                        ));
                    }
                }
                Err(_) => {
                    spans.push(Span::styled(
                        line.trim_end_matches(['\r', '\n']).to_string(),
                        self.default_style,
                    ));
                }
            }
            self.lines.push(Line::from(spans));
        }

        self.lines
            .push(Line::from(Span::styled("╰───".to_string(), border_style)));
    }

    fn current_style(&self) -> Style {
        *self.styles.last().unwrap_or(&self.default_style)
    }

    fn heading_style(&self) -> Style {
        if self.suppress_style {
            self.default_style.add_modifier(Modifier::BOLD)
        } else {
            self.default_style
                .fg(self.theme.markdown_heading)
                .add_modifier(Modifier::BOLD)
        }
    }

    fn flush_current(&mut self) {
        if self.current.is_empty() {
            return;
        }
        self.lines
            .push(Line::from(std::mem::take(&mut self.current)));
    }

    fn push_blank_line(&mut self) {
        if self.lines.last().is_some_and(line_is_blank) {
            return;
        }
        self.lines.push(Line::from(""));
    }

    fn ensure_blockquote_prefix(&mut self) {
        if !self.current.is_empty() || self.blockquote_depth == 0 {
            return;
        }
        let style = if self.suppress_style {
            self.default_style
        } else {
            self.default_style.fg(self.theme.markdown_block_quote)
        };
        for _ in 0..self.blockquote_depth {
            self.current.push(Span::styled("│ ", style));
        }
    }

    fn current_line_width(&self) -> usize {
        self.current.iter().map(|s| display_width(&s.content)).sum()
    }
}

struct TableBuilder {
    alignments: Vec<Alignment>,
    rows: Vec<TableRow>,
    current_row: Vec<Vec<Span<'static>>>,
    in_header: bool,
}

struct TableRow {
    is_header: bool,
    cells: Vec<Vec<Span<'static>>>,
}

impl TableBuilder {
    fn new(alignments: Vec<Alignment>) -> Self {
        Self {
            alignments,
            rows: Vec::new(),
            current_row: Vec::new(),
            in_header: false,
        }
    }

    fn push_cell(&mut self, cell: Vec<Span<'static>>) {
        self.current_row.push(cell);
    }

    fn finish_row(&mut self) {
        if self.current_row.is_empty() {
            return;
        }
        self.rows.push(TableRow {
            is_header: self.in_header,
            cells: std::mem::take(&mut self.current_row),
        });
    }

    fn render(
        mut self,
        lines: &mut Vec<Line<'static>>,
        theme: &Theme,
        suppress_style: bool,
        viewport_width: usize,
    ) {
        self.finish_row();
        if self.rows.is_empty() {
            return;
        }

        let col_count = self
            .rows
            .iter()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(0)
            .max(self.alignments.len());
        if col_count == 0 {
            return;
        }

        let mut widths = vec![1usize; col_count];
        for row in &self.rows {
            for (idx, cell) in row.cells.iter().enumerate() {
                widths[idx] = widths[idx].max(cell_span_width(cell).max(1));
            }
        }

        let overhead = 1 + col_count * 3;
        if viewport_width > overhead {
            let budget = viewport_width - overhead;
            let intrinsic_total: usize = widths.iter().sum();
            if intrinsic_total < budget {
                let extra = budget - intrinsic_total;
                let per = extra / col_count;
                let rem = extra - per * col_count;
                for w in widths.iter_mut() {
                    *w += per;
                }
                if let Some(last) = widths.last_mut() {
                    *last += rem;
                }
            }
        }

        let border_color = if suppress_style {
            None
        } else {
            Some(theme.markdown_horizontal_rule)
        };
        let border_style = match border_color {
            Some(c) => Style::default().fg(c),
            None => Style::default(),
        };

        lines.push(border_line('┌', '┬', '┐', &widths, border_style));

        for (idx, row) in self.rows.iter().enumerate() {
            lines.push(row_line(
                row,
                &widths,
                &self.alignments,
                theme,
                suppress_style,
            ));

            let is_header_break =
                row.is_header && self.rows.get(idx + 1).is_some_and(|next| !next.is_header);
            if is_header_break {
                lines.push(border_line('├', '┼', '┤', &widths, border_style));
            }
        }

        lines.push(border_line('└', '┴', '┘', &widths, border_style));
    }
}

fn border_line(
    left: char,
    middle: char,
    right: char,
    widths: &[usize],
    style: Style,
) -> Line<'static> {
    let mut s = String::new();
    s.push(left);
    for (i, w) in widths.iter().enumerate() {
        s.push_str(&"─".repeat(w + 2));
        if i + 1 < widths.len() {
            s.push(middle);
        }
    }
    s.push(right);
    Line::from(Span::styled(s, style))
}

fn row_line(
    row: &TableRow,
    widths: &[usize],
    alignments: &[Alignment],
    theme: &Theme,
    suppress_style: bool,
) -> Line<'static> {
    let border_style = if suppress_style {
        Style::default()
    } else {
        Style::default().fg(theme.markdown_horizontal_rule)
    };
    let header_style = if suppress_style {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.markdown_heading)
            .add_modifier(Modifier::BOLD)
    };
    let body_style = if suppress_style {
        Style::default()
    } else {
        Style::default().fg(theme.markdown_text)
    };
    let pad_style = if row.is_header {
        header_style
    } else {
        body_style
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled("│", border_style));
    for (idx, w) in widths.iter().enumerate() {
        let cell = row.cells.get(idx).cloned().unwrap_or_default();
        let cell_w = cell_span_width(&cell);
        let padding = w.saturating_sub(cell_w);
        let alignment = alignments.get(idx).copied().unwrap_or(Alignment::None);
        let (left_pad, right_pad) = match alignment {
            Alignment::Right => (padding, 0),
            Alignment::Center => (padding / 2, padding - padding / 2),
            _ => (0, padding),
        };

        spans.push(Span::styled(" ", pad_style));
        if left_pad > 0 {
            spans.push(Span::styled(" ".repeat(left_pad), pad_style));
        }
        if row.is_header {
            for span in cell {
                let merged = if suppress_style {
                    span.style.add_modifier(Modifier::BOLD)
                } else {
                    span.style
                        .fg(theme.markdown_heading)
                        .add_modifier(Modifier::BOLD)
                };
                spans.push(Span::styled(span.content.into_owned(), merged));
            }
        } else {
            for span in cell {
                let style = if span.style.fg.is_none() && !suppress_style {
                    span.style.fg(theme.markdown_text)
                } else {
                    span.style
                };
                spans.push(Span::styled(span.content.into_owned(), style));
            }
        }
        if right_pad > 0 {
            spans.push(Span::styled(" ".repeat(right_pad), pad_style));
        }
        spans.push(Span::styled(" ", pad_style));
        spans.push(Span::styled("│", border_style));
    }
    Line::from(spans)
}

fn cell_span_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(|s| display_width(&s.content)).sum()
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn line_is_blank(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .all(|s| s.content.as_ref().trim().is_empty())
}

fn display_width(s: &str) -> usize {
    s.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;
    use syntect::highlighting::ThemeSet;

    fn theme() -> Theme {
        Theme::default()
    }

    fn render(md: &str, width: usize) -> Vec<Line<'static>> {
        let ps = two_face::syntax::extra_newlines();
        let ts = ThemeSet::load_defaults();
        render_markdown(md, width, &ps, &ts, &theme(), Style::default(), false)
    }

    fn flatten(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn heading_emits_hash_prefix_and_bold() {
        let lines = render("# Hello", 80);
        let plain = flatten(&lines);
        assert!(
            plain.iter().any(|l| l.contains("# Hello")),
            "expected `# Hello`; got {plain:?}"
        );
        let first = lines
            .iter()
            .find(|l| l.spans.iter().any(|s| s.content.contains('#')))
            .unwrap();
        let span = first
            .spans
            .iter()
            .find(|s| s.content.contains('#'))
            .unwrap();
        assert!(
            span.style.add_modifier.contains(Modifier::BOLD),
            "heading span must be bold: {span:?}"
        );
    }

    #[test]
    fn bullet_list_emits_bullet_glyph() {
        let lines = render("- one\n- two\n", 80);
        let plain = flatten(&lines);
        assert!(
            plain.iter().any(|l| l.starts_with("• one")),
            "expected `• one`; got {plain:?}"
        );
        assert!(
            plain.iter().any(|l| l.starts_with("• two")),
            "expected `• two`; got {plain:?}"
        );
    }

    #[test]
    fn ordered_list_emits_numeric_prefixes() {
        let lines = render("1. first\n2. second\n", 80);
        let plain = flatten(&lines);
        assert!(
            plain.iter().any(|l| l.starts_with("1. first")),
            "expected `1. first`; got {plain:?}"
        );
        assert!(
            plain.iter().any(|l| l.starts_with("2. second")),
            "expected `2. second`; got {plain:?}"
        );
    }

    #[test]
    fn blockquote_emits_pipe_gutter() {
        let lines = render("> quoted text\n", 80);
        let plain = flatten(&lines);
        assert!(
            plain.iter().any(|l| l.starts_with("│ ")),
            "blockquote must start with `│ `; got {plain:?}"
        );
    }

    #[test]
    fn inline_code_wraps_in_literal_backticks() {
        let lines = render("call `frobnicate()` now", 80);
        let plain = flatten(&lines).join("\n");
        assert!(
            plain.contains("`frobnicate()`"),
            "inline code must keep backticks; got {plain:?}"
        );
    }

    #[test]
    fn fenced_code_block_renders_rounded_border_and_lang_label() {
        let md = "```rust\nfn main() {}\n```";
        let lines = render(md, 80);
        let plain = flatten(&lines);
        assert!(
            plain
                .iter()
                .any(|l| l.starts_with("╭") && l.contains("rust")),
            "code-block header must be `╭ rust ───…`; got {plain:?}"
        );
        assert!(
            plain.iter().any(|l| l.starts_with("│ ")),
            "code body lines must be prefixed with `│ `; got {plain:?}"
        );
        assert!(
            plain.iter().any(|l| l.starts_with("╰")),
            "code-block footer must be `╰───`; got {plain:?}"
        );
    }

    #[test]
    fn fenced_code_block_without_lang_uses_code_label() {
        let md = "```\nplain\n```";
        let lines = render(md, 80);
        let plain = flatten(&lines);
        assert!(
            plain.iter().any(|l| l.contains(" code ")),
            "no-lang code-block falls back to `code` label; got {plain:?}"
        );
    }

    #[test]
    fn horizontal_rule_fills_width() {
        let lines = render("---\n", 24);
        let plain = flatten(&lines);
        let rule = plain
            .iter()
            .find(|l| l.chars().all(|c| c == '─'))
            .unwrap_or_else(|| panic!("rule missing; got {plain:?}"));
        assert_eq!(rule.chars().count(), 24, "rule must span the full width");
    }

    #[test]
    fn task_list_marker_renders_unchecked() {
        let lines = render("- [ ] todo\n", 80);
        let plain = flatten(&lines).join("\n");
        assert!(plain.contains("[ ] todo"), "got {plain:?}");
    }

    #[test]
    fn task_list_marker_renders_checked() {
        let lines = render("- [x] done\n", 80);
        let plain = flatten(&lines).join("\n");
        assert!(plain.contains("[x] done"), "got {plain:?}");
    }

    #[test]
    fn link_renders_text_then_url_in_parens() {
        let lines = render("see [docs](https://example.com)", 80);
        let plain = flatten(&lines).join("\n");
        assert!(plain.contains("docs"), "label visible; got {plain:?}");
        assert!(
            plain.contains("(https://example.com)"),
            "url appended in parens; got {plain:?}"
        );
    }

    #[test]
    fn image_renders_image_prefix_and_url() {
        let lines = render("![banner](http://x.png)", 80);
        let plain = flatten(&lines).join("\n");
        assert!(plain.contains("[image]"), "`[image]` prefix; got {plain:?}");
        assert!(
            plain.contains("(http://x.png)"),
            "image url appended; got {plain:?}"
        );
    }

    #[test]
    fn strikethrough_applies_crossed_out_modifier() {
        let lines = render("~~gone~~", 80);
        let mut found = false;
        for line in &lines {
            for span in &line.spans {
                if span.content.contains("gone") {
                    assert!(
                        span.style.add_modifier.contains(Modifier::CROSSED_OUT),
                        "strikethrough must apply CROSSED_OUT; span={span:?}"
                    );
                    found = true;
                }
            }
        }
        assert!(found, "no `gone` span emitted: {lines:?}");
    }

    #[test]
    fn strong_applies_bold_modifier() {
        let lines = render("**loud**", 80);
        let mut found = false;
        for line in &lines {
            for span in &line.spans {
                if span.content.contains("loud") {
                    assert!(
                        span.style.add_modifier.contains(Modifier::BOLD),
                        "strong span must be bold: {span:?}"
                    );
                    found = true;
                }
            }
        }
        assert!(found, "no `loud` span emitted: {lines:?}");
    }

    #[test]
    fn emphasis_applies_italic_modifier() {
        let lines = render("*soft*", 80);
        let mut found = false;
        for line in &lines {
            for span in &line.spans {
                if span.content.contains("soft") {
                    assert!(
                        span.style.add_modifier.contains(Modifier::ITALIC),
                        "emphasis span must be italic: {span:?}"
                    );
                    found = true;
                }
            }
        }
        assert!(found, "no `soft` span emitted: {lines:?}");
    }

    #[test]
    fn table_renders_grid_with_single_box_drawing() {
        let md = "\
| File   | Lines |
|--------|-------|
| ui.rs  |   200 |
| app.rs |   150 |
";
        let lines = render(md, 60);
        let plain = flatten(&lines);
        let joined = plain.join("\n");

        assert!(
            plain.iter().any(|l| l.starts_with('┌') && l.contains('┬')),
            "top border `┌─┬─┐` missing; got {plain:?}"
        );
        assert!(
            plain.iter().any(|l| l.starts_with('├') && l.contains('┼')),
            "header separator `├─┼─┤` missing; got {plain:?}"
        );
        assert!(
            plain.iter().any(|l| l.starts_with('└') && l.contains('┴')),
            "bottom border `└─┴─┘` missing; got {plain:?}"
        );

        assert!(
            plain
                .iter()
                .any(|l| l.starts_with('│') && l.contains("ui.rs")),
            "body row `│ ui.rs … │` missing; got {plain:?}"
        );

        assert!(
            !joined.contains("|--------|"),
            "raw markdown delimiter row leaked: {joined}"
        );
    }

    #[test]
    fn table_header_row_is_bold() {
        let md = "| H1 | H2 |\n|----|----|\n| a  | b  |\n";
        let lines = render(md, 40);
        let mut found_bold = false;
        for line in &lines {
            for span in &line.spans {
                if span.content.contains("H1") {
                    assert!(
                        span.style.add_modifier.contains(Modifier::BOLD),
                        "header cell span must be bold: {span:?}"
                    );
                    found_bold = true;
                }
            }
        }
        assert!(found_bold, "no header span found");
    }

    #[test]
    fn table_renders_alignment_padding() {
        let md = "\
| L | R |
|:--|--:|
| a | b |
";
        let lines = render(md, 30);
        let body = lines
            .iter()
            .find(|l| l.spans.iter().any(|s| s.content.as_ref().contains('a')))
            .unwrap();
        let text: String = body.spans.iter().map(|s| s.content.as_ref()).collect();
        let b_idx = text.find('b').expect("b in row");
        assert!(
            text[..b_idx].ends_with(' '),
            "right-aligned `b` must be preceded by space padding: {text:?}"
        );
    }

    #[test]
    fn table_columns_fill_viewport_width() {
        let md = "| A | B |\n|---|---|\n| x | y |\n";
        let lines = render(md, 40);
        let top = lines
            .iter()
            .find(|l| l.spans.first().is_some_and(|s| s.content.starts_with('┌')))
            .expect("top border");
        let text: String = top.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(
            text.chars().count(),
            40,
            "top border row must equal viewport width; row was {text:?}"
        );
    }

    #[test]
    fn nested_emphasis_keeps_outer_style_after_inner_pop() {
        let lines = render("*one **two** three*", 80);
        let mut three_italic = false;
        for line in &lines {
            for span in &line.spans {
                if span.content.contains("three") {
                    three_italic |= span.style.add_modifier.contains(Modifier::ITALIC);
                }
            }
        }
        assert!(
            three_italic,
            "tail of emphasis must remain italic: {lines:?}"
        );
    }

    #[test]
    fn heading_color_is_suppressed_when_flag_set() {
        let ps = two_face::syntax::extra_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = theme();
        let default = Style::default().fg(Color::Magenta);
        let lines = render_markdown("# heading", 40, &ps, &ts, &theme, default, true);
        let span = lines[0]
            .spans
            .iter()
            .find(|s| s.content.contains('#'))
            .unwrap();
        assert_eq!(span.style.fg, Some(Color::Magenta), "{span:?}");
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn empty_input_yields_no_lines() {
        let lines = render("", 80);
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn soft_wrap_breaks_at_width() {
        let para = "alpha bravo charlie delta echo foxtrot golf hotel";
        let lines = render(para, 30);
        assert!(
            lines.len() > 1,
            "long paragraph must wrap to multiple lines; got {} line(s)",
            lines.len()
        );
        for l in &lines {
            let w: usize = l.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(
                w <= 30,
                "wrapped line exceeds viewport width: {} > 30 ({l:?})",
                w
            );
        }
    }
}
