use crate::model::{Message, Sender};

pub fn export_markdown(messages: &[Message]) -> String {
    let mut out = String::from("# Session\n\n");
    for msg in messages {
        if msg.content.trim().is_empty() && msg.thoughts.trim().is_empty() {
            continue;
        }
        let sender = match msg.sender {
            Sender::User => "User",
            Sender::Assistant => "Assistant",
            Sender::System => "System",
        };
        out.push_str(&format!("## {sender} {}\n\n", msg.timestamp));
        if !msg.thoughts.trim().is_empty() {
            out.push_str("> _thinking_\n>\n");
            for line in msg.thoughts.lines() {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
        if !msg.content.trim().is_empty() {
            out.push_str(msg.content.trim_end());
            out.push_str("\n\n");
        }
    }
    out
}
