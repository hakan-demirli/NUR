//! -- @version 0.3.1
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
}

impl PluginManifest {
    pub fn parse(source: &str) -> Self {
        let mut manifest = PluginManifest::default();
        for line in source.lines() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                continue;
            }
            let Some(rest) = trimmed.strip_prefix("--") else {
                break;
            };
            if rest.starts_with('[') {
                break;
            }
            let body = rest.trim();
            let Some(directive) = body.strip_prefix('@') else {
                continue;
            };
            let (key, value) = match directive.split_once(char::is_whitespace) {
                Some((key, value)) => (key.trim(), value.trim()),
                None => (directive.trim(), ""),
            };
            if value.is_empty() {
                continue;
            }
            let value = value.to_string();
            match key {
                "id" | "name" => {
                    if manifest.id.is_none() {
                        manifest.id = Some(value);
                    }
                }
                "title" => {
                    if manifest.title.is_none() {
                        manifest.title = Some(value);
                    }
                }
                "description" | "desc" | "summary" => {
                    if manifest.description.is_none() {
                        manifest.description = Some(value);
                    }
                }
                "version" => {
                    if manifest.version.is_none() {
                        manifest.version = Some(value);
                    }
                }
                _ => {}
            }
        }
        manifest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_supported_fields() {
        let source = "-- @id   judge.daemon\n\
                      -- @title Judge Daemon\n\
                      -- @description Supervise the judge process\n\
                      -- @version 1.2.0\n\
                      \n\
                      api.command.register({})\n";
        let manifest = PluginManifest::parse(source);
        assert_eq!(manifest.id.as_deref(), Some("judge.daemon"));
        assert_eq!(manifest.title.as_deref(), Some("Judge Daemon"));
        assert_eq!(
            manifest.description.as_deref(),
            Some("Supervise the judge process")
        );
        assert_eq!(manifest.version.as_deref(), Some("1.2.0"));
    }

    #[test]
    fn stops_at_first_code_line() {
        let source = "-- @id foo\nlocal x = 1\n-- @title ignored\n";
        let manifest = PluginManifest::parse(source);
        assert_eq!(manifest.id.as_deref(), Some("foo"));
        assert_eq!(manifest.title, None);
    }

    #[test]
    fn returns_empty_for_no_directives() {
        let source = "local x = 1\n";
        assert_eq!(PluginManifest::parse(source), PluginManifest::default());
    }

    #[test]
    fn accepts_name_alias_for_id() {
        let source = "-- @name foo.bar\n";
        assert_eq!(PluginManifest::parse(source).id.as_deref(), Some("foo.bar"));
    }

    #[test]
    fn long_bracket_comment_ends_scan() {
        let source = "-- @id foo\n--[[ block comment with @title NotPicked ]]\n-- @title later\n";
        let manifest = PluginManifest::parse(source);
        assert_eq!(manifest.id.as_deref(), Some("foo"));
        assert_eq!(manifest.title, None);
    }
}
