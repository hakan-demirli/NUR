use raider_tui::action::ChildToolRef;
use raider_tui::{ToolCall, ToolStatus};

use super::helpers::{
    capitalize_first, format_input_primitives, normalize_workspace_path, tail_bytes,
};
use super::MAX_TOOL_OUTPUT_BYTES;

pub(crate) fn child_tool_ref_from_part(
    part: &raider_opencode::types::message::ToolPart,
) -> ChildToolRef {
    let state = &part.state;
    let input_obj = state.input.as_object();
    let metadata_obj = state.metadata.as_object();
    let file_path = input_obj
        .and_then(|m| m.get("filePath"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let command = input_obj
        .and_then(|m| m.get("command"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let title = synthesize_tool_title(&part.tool_name, &state.title, input_obj, metadata_obj);
    ChildToolRef {
        part_id: part.id.as_str().to_string(),
        name: part.tool_name.clone(),
        status: ToolStatus::from_wire(&state.status),
        file_path,
        command,
        title,
    }
}

pub(crate) fn tool_part_to_call(part: &raider_opencode::types::message::ToolPart) -> ToolCall {
    let state = &part.state;
    let input_obj = state.input.as_object();
    let metadata_obj = state.metadata.as_object();
    // wording (user-reported BUG11: read tools rendered as `# Read`
    let title = synthesize_tool_title(&part.tool_name, &state.title, input_obj, metadata_obj);
    let command = input_obj
        .and_then(|m| m.get("command"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let metadata_output = state
        .metadata
        .as_object()
        .and_then(|m| m.get("output"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let output = metadata_output
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| state.output.clone());
    let output = tail_bytes(&output, MAX_TOOL_OUTPUT_BYTES);
    // `state.error` is `Option<String>` post-BUG1 fix (was
    let error = state.error.as_ref().filter(|s| !s.is_empty()).cloned();
    let file_path = input_obj
        .and_then(|m| m.get("filePath"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let diff = if part.tool_name == "edit" {
        state
            .metadata
            .as_object()
            .and_then(|m| m.get("diff"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    } else if part.tool_name == "write" {
        if matches!(ToolStatus::from_wire(&state.status), ToolStatus::Completed) {
            input_obj
                .and_then(|m| m.get("content"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|content| {
                    let mut buf = String::with_capacity(content.len() + content.len() / 40);
                    for line in content.lines() {
                        buf.push('+');
                        buf.push_str(line);
                        buf.push('\n');
                    }
                    buf
                })
        } else {
            None
        }
    } else {
        None
    };
    let todos: Vec<raider_tui::sidebar::TodoEntry> = if part.tool_name == "todowrite" {
        input_obj
            .and_then(|m| m.get("todos"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_object())
                    .map(|obj| {
                        let content = obj
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let status = obj
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        raider_tui::sidebar::TodoEntry::new(content, status)
                    })
                    .filter(|t| !t.content.is_empty() || !t.status.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let loaded: Vec<String> = if part.tool_name == "read" {
        metadata_obj
            .and_then(|m| m.get("loaded"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let patches: Vec<raider_tui::PatchFile> = if part.tool_name == "apply_patch" {
        metadata_obj
            .and_then(|m| m.get("files"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_object())
                    .map(|obj| {
                        let type_str = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        let kind = match type_str {
                            "add" => raider_tui::PatchKind::Created,
                            "delete" => raider_tui::PatchKind::Deleted,
                            "move" => raider_tui::PatchKind::Moved,
                            _ => raider_tui::PatchKind::Patched,
                        };
                        let file_path = obj
                            .get("filePath")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let relative_path = obj
                            .get("relativePath")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let diff = obj
                            .get("patch")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string());
                        match kind {
                            raider_tui::PatchKind::Moved => {
                                let move_path = obj
                                    .get("movePath")
                                    .and_then(|v| v.as_str())
                                    .filter(|s| !s.is_empty())
                                    .map(|s| s.to_string());
                                let new_path = if !relative_path.is_empty() {
                                    Some(relative_path)
                                } else {
                                    move_path
                                };
                                raider_tui::PatchFile {
                                    kind,
                                    path: file_path,
                                    new_path,
                                    diff,
                                }
                            }
                            _ => {
                                let path = if !relative_path.is_empty() {
                                    relative_path
                                } else {
                                    file_path
                                };
                                raider_tui::PatchFile {
                                    kind,
                                    path,
                                    new_path: None,
                                    diff,
                                }
                            }
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let (questions, answers): (Vec<raider_tui::Question>, Vec<Vec<String>>) =
        if part.tool_name == "question" {
            let qs = input_obj
                .and_then(|m| m.get("questions"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.as_object())
                        .map(|obj| {
                            let text = obj
                                .get("question")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let options = obj
                                .get("options")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|opt| {
                                            opt.as_str().map(|s| s.to_string()).or_else(|| {
                                                opt.as_object().and_then(|o| {
                                                    o.get("header")
                                                        .and_then(|v| v.as_str())
                                                        .map(|s| s.to_string())
                                                })
                                            })
                                        })
                                        .collect::<Vec<String>>()
                                })
                                .unwrap_or_default();
                            raider_tui::Question { text, options }
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let ans = metadata_obj
                .and_then(|m| m.get("answers"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|item| {
                            item.as_array()
                                .map(|inner| {
                                    inner
                                        .iter()
                                        .filter_map(|s| s.as_str().map(|s| s.to_string()))
                                        .collect::<Vec<String>>()
                                })
                                .unwrap_or_default()
                        })
                        .collect::<Vec<Vec<String>>>()
                })
                .unwrap_or_default();
            (qs, ans)
        } else {
            (Vec::new(), Vec::new())
        };
    ToolCall {
        id: Some(part.id.as_str().to_string()),
        name: part.tool_name.clone(),
        status: ToolStatus::from_wire(&state.status),
        title,
        command,
        output,
        error,
        todos,
        file_path,
        diff,
        loaded,
        patches,
        questions,
        answers,
        expanded: false,
        current_child: None,
        child_tool_count: 0,
        started_at_ms: None,
        completed_at_ms: None,
    }
}

pub(crate) fn synthesize_tool_title(
    tool_name: &str,
    server_title: &str,
    input: Option<&serde_json::Map<String, serde_json::Value>>,
    metadata: Option<&serde_json::Map<String, serde_json::Value>>,
) -> String {
    let str_in = |k: &str| {
        input
            .and_then(|m| m.get(k))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    let num_meta = |k: &str| metadata.and_then(|m| m.get(k)).and_then(|v| v.as_u64());

    match tool_name {
        "bash" => {
            if !server_title.is_empty() {
                server_title.to_string()
            } else {
                str_in("description").unwrap_or_else(|| "Shell".to_string())
            }
        }
        "glob" => {
            let pattern = str_in("pattern").unwrap_or_default();
            let path = str_in("path");
            let count = num_meta("count");
            let mut out = format!("Glob \"{pattern}\"");
            if let Some(p) = path {
                out.push_str(&format!(" in {}", normalize_workspace_path(&p)));
            }
            if let Some(n) = count {
                let plural = if n == 1 { "match" } else { "matches" };
                out.push_str(&format!(" ({n} {plural})"));
            }
            out
        }
        "read" => {
            let file_path = str_in("filePath").unwrap_or_default();
            let extras = format_input_primitives(input, &["filePath"]);
            let mut out = format!("Read {}", normalize_workspace_path(&file_path));
            if !extras.is_empty() {
                out.push(' ');
                out.push_str(&extras);
            }
            out
        }
        "grep" => {
            let pattern = str_in("pattern").unwrap_or_default();
            let path = str_in("path");
            let matches = num_meta("matches");
            let mut out = format!("Grep \"{pattern}\"");
            if let Some(p) = path {
                out.push_str(&format!(" in {}", normalize_workspace_path(&p)));
            }
            if let Some(n) = matches {
                let plural = if n == 1 { "match" } else { "matches" };
                out.push_str(&format!(" ({n} {plural})"));
            }
            out
        }
        "webfetch" => {
            let url = str_in("url").unwrap_or_default();
            format!("WebFetch {url}")
        }
        "websearch" => {
            let provider = metadata
                .and_then(|m| m.get("provider"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "WebSearch".to_string());
            let query = str_in("query").unwrap_or_default();
            let num_results = num_meta("numResults");
            let mut out = format!("{provider} \"{query}\"");
            if let Some(n) = num_results {
                out.push_str(&format!(" ({n} results)"));
            }
            out
        }
        "write" => {
            let file_path = str_in("filePath").unwrap_or_default();
            format!("Write {}", normalize_workspace_path(&file_path))
        }
        "edit" => {
            let file_path = str_in("filePath").unwrap_or_default();
            let extras = format_input_primitives(input, &["filePath"]);
            let mut out = format!("Edit {}", normalize_workspace_path(&file_path));
            if !extras.is_empty() {
                out.push(' ');
                out.push_str(&extras);
            }
            out
        }
        "todowrite" => "Todos".to_string(),
        "skill" => {
            let name = str_in("name").unwrap_or_default();
            format!("Skill \"{name}\"")
        }
        "task" => {
            let description = str_in("description").unwrap_or_default();
            let subagent = str_in("subagent_type").unwrap_or_else(|| "General".to_string());
            let subagent_titled = capitalize_first(&subagent);
            if description.is_empty() {
                format!("{subagent_titled} Task")
            } else {
                format!("{subagent_titled} Task — {description}")
            }
        }
        "question" => {
            let n = input
                .and_then(|m| m.get("questions"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0);
            if n == 0 {
                "Question".to_string()
            } else {
                let plural = if n == 1 { "question" } else { "questions" };
                format!("Asked {n} {plural}")
            }
        }
        _ => {
            let extras = format_input_primitives(input, &[]);
            if extras.is_empty() {
                tool_name.to_string()
            } else {
                format!("{tool_name} {extras}")
            }
        }
    }
}
