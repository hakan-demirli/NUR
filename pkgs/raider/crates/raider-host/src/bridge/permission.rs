use raider_opencode::types::permission::PermissionRequest;

pub fn permission_to_prompt(req: &PermissionRequest) -> raider_tui::PermissionPrompt {
    use raider_tui::PermissionPrompt;
    let view = build_permission_view(req);
    PermissionPrompt {
        id: req.id.clone(),
        session_id: req.session_id.as_str().to_string(),
        permission: req.permission.clone(),
        patterns: req.patterns.clone(),
        metadata: req.metadata.clone(),
        always: req.always.clone(),
        view,
    }
}

fn meta_str<'a>(
    meta: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    meta.get(key).and_then(|v| v.as_str())
}

fn parent_dir(p: &str) -> String {
    match p.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => p[..i].to_string(),
        None => ".".to_string(),
    }
}

fn titlecase(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut up = true;
    for c in s.chars() {
        if matches!(c, '-' | '_' | ' ') {
            out.push(c);
            up = true;
        } else if up {
            for u in c.to_uppercase() {
                out.push(u);
            }
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn build_permission_view(req: &PermissionRequest) -> raider_tui::PermissionView {
    use raider_tui::PermissionView;
    let meta = &req.metadata;
    let normalize = raider_tui::path_format::normalize;
    match req.permission.as_str() {
        "edit" => {
            let filepath = meta_str(meta, "filepath").unwrap_or("");
            PermissionView {
                icon: "→".into(),
                title: format!("Edit {}", normalize(filepath)),
                detail: Vec::new(),
            }
        }
        "read" => {
            let path = meta_str(meta, "filePath")
                .or_else(|| meta_str(meta, "filepath"))
                .or_else(|| req.patterns.first().map(|s| s.as_str()))
                .unwrap_or("");
            PermissionView {
                icon: "→".into(),
                title: format!("Read {}", normalize(path)),
                detail: if path.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("Path: {}", normalize(path))]
                },
            }
        }
        "glob" => {
            let pattern = req
                .patterns
                .first()
                .map(|s| s.as_str())
                .or_else(|| meta_str(meta, "pattern"))
                .unwrap_or("");
            PermissionView {
                icon: "✱".into(),
                title: format!("Glob \"{pattern}\""),
                detail: if pattern.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("Pattern: {pattern}")]
                },
            }
        }
        "grep" => {
            let pattern = req
                .patterns
                .first()
                .map(|s| s.as_str())
                .or_else(|| meta_str(meta, "pattern"))
                .unwrap_or("");
            PermissionView {
                icon: "✱".into(),
                title: format!("Grep \"{pattern}\""),
                detail: if pattern.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("Pattern: {pattern}")]
                },
            }
        }
        "list" => {
            let dir = meta_str(meta, "path").unwrap_or("");
            PermissionView {
                icon: "→".into(),
                title: format!("List {}", normalize(dir)),
                detail: if dir.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("Path: {}", normalize(dir))]
                },
            }
        }
        "bash" => {
            let title = meta_str(meta, "description")
                .filter(|s| !s.is_empty())
                .unwrap_or("Shell command");
            let command = meta_str(meta, "command").unwrap_or("");
            PermissionView {
                icon: "#".into(),
                title: title.to_string(),
                detail: if command.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("$ {command}")]
                },
            }
        }
        "task" => {
            let ty = meta_str(meta, "subagent_type").unwrap_or("Unknown");
            let desc = meta_str(meta, "description").unwrap_or("");
            PermissionView {
                icon: "#".into(),
                title: format!("{} Task", titlecase(ty)),
                detail: if desc.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("◉ {desc}")]
                },
            }
        }
        "webfetch" => {
            let url = meta_str(meta, "url").unwrap_or("");
            PermissionView {
                icon: "%".into(),
                title: format!("WebFetch {url}"),
                detail: if url.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("URL: {url}")]
                },
            }
        }
        "websearch" => {
            let query = meta_str(meta, "query").unwrap_or("");
            let provider = meta_str(meta, "provider").unwrap_or("WebSearch");
            PermissionView {
                icon: "◈".into(),
                title: format!("{provider} \"{query}\""),
                detail: if query.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("Query: {query}")]
                },
            }
        }
        "external_directory" => {
            let parent = meta_str(meta, "parentDir");
            let filepath = meta_str(meta, "filepath");
            let pattern = req.patterns.first().map(|s| s.as_str());
            let derived = pattern.map(|p| {
                if p.contains('*') {
                    parent_dir(p)
                } else {
                    p.to_string()
                }
            });
            let raw = parent
                .map(|s| s.to_string())
                .or_else(|| filepath.map(|s| s.to_string()))
                .or(derived)
                .unwrap_or_default();
            let mut detail: Vec<String> = Vec::new();
            if !req.patterns.is_empty() {
                detail.push("Patterns".to_string());
                for p in &req.patterns {
                    detail.push(format!("- {p}"));
                }
            }
            PermissionView {
                icon: "←".into(),
                title: format!("Access external directory {}", normalize(&raw)),
                detail,
            }
        }
        "doom_loop" => PermissionView {
            icon: "⟳".into(),
            title: "Continue after repeated failures".into(),
            detail: vec!["This keeps the session running despite repeated failures.".into()],
        },
        other => PermissionView {
            icon: "⚙".into(),
            title: format!("Call tool {other}"),
            detail: vec![format!("Tool: {other}")],
        },
    }
}
