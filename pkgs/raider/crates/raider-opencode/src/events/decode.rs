use crate::error::{Error, Result};

use super::types::ServerEvent;

pub fn parse_frame(data: &str) -> Result<ServerEvent> {
    serde_json::from_str(data).map_err(|source| {
        let preview = event_type_for_log(data).unwrap_or_else(|| {
            let trimmed = data.trim();
            let snippet: String = trimmed.chars().take(120).collect();
            format!("<no `type` field; first 120 chars: {snippet}>")
        });
        Error::Decode {
            path: format!("/event ({preview})"),
            source,
        }
    })
}

fn event_type_for_log(data: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    let visit = |val: &serde_json::Value| -> Option<String> {
        val.get("type")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
    };
    visit(&v)
        .or_else(|| v.get("payload").and_then(visit))
        .or_else(|| v.get("syncEvent").and_then(visit))
}

pub(crate) fn find_frame_end(buf: &str) -> Option<usize> {
    buf.find("\n\n")
        .map(|i| {
            if let Some(crlf) = buf.find("\r\n\r\n") {
                i.min(crlf)
            } else {
                i
            }
        })
        .or_else(|| buf.find("\r\n\r\n"))
}

pub(crate) fn extract_data_field(frame: &str) -> Option<String> {
    let mut out = String::new();
    let mut any = false;
    for line in frame.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with(':') || line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            if any {
                out.push('\n');
            }
            out.push_str(rest.strip_prefix(' ').unwrap_or(rest));
            any = true;
        }
    }
    if any {
        Some(out)
    } else {
        None
    }
}

pub(crate) fn absorb(
    buffer: &mut String,
    chunk: &[u8],
    out: &mut std::collections::VecDeque<String>,
) {
    buffer.push_str(&String::from_utf8_lossy(chunk));

    loop {
        let Some(end) = find_frame_end(buffer) else {
            return;
        };
        let frame_raw = buffer[..end].to_string();
        let skip = if buffer[end..].starts_with("\r\n\r\n") {
            4
        } else {
            2
        };
        buffer.drain(..end + skip);

        if let Some(data) = extract_data_field(&frame_raw) {
            if !data.is_empty() {
                out.push_back(data);
            }
        }
    }
}
