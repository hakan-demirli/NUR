use super::diff::SyntaxCtx;
use super::theme::Theme;

#[derive(Clone, Copy)]
pub(crate) struct SyntaxResources<'a> {
    pub ps: &'a syntect::parsing::SyntaxSet,
    pub ts: &'a syntect::highlighting::ThemeSet,
    pub synth_theme: &'a syntect::highlighting::Theme,
}

pub(crate) fn build_syntax_ctx<'a>(
    ps: &'a syntect::parsing::SyntaxSet,
    ts: &'a syntect::highlighting::ThemeSet,
    theme: &'a Theme,
    synth_theme: &'a syntect::highlighting::Theme,
    path: &'a str,
) -> Option<SyntaxCtx<'a>> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if ext.is_empty() {
        return None;
    }
    let filetype: &'a str = match ext {
        "tsx" | "jsx" => "ts",
        other => other,
    };
    Some(SyntaxCtx {
        ps,
        ts,
        filetype,
        mode: theme.mode,
        synth_theme: Some(synth_theme),
    })
}
