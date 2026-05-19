use std::time::Duration;

use raider_host::backend::{EventBackend, PromptBackend, ProviderBackend, SessionBackend};
use raider_host::OpencodeBackend;
use raider_opencode::events::{ServerEvent, StreamItem};
use raider_opencode::types::common::MessageId;
use raider_opencode::types::session::{
    PromptModel, PromptPart, PromptPayload, PromptTextPart, SessionCreateModel,
    SessionCreatePayload,
};
use raider_opencode::{Client, ClientConfig};

use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = ClientConfig::new("http://127.0.0.1:4096", "/home/emre/Desktop/raider")?;
    let client = Client::connect(cfg)?;
    let backend = OpencodeBackend::new(client);

    let providers = backend.provider_list().await?;
    eprintln!(
        "PROVIDERS: {} entries, default opencode={:?}",
        providers.all.len(),
        providers.default.get("opencode")
    );

    let session = backend
        .session_create(&SessionCreatePayload {
            agent: Some("build".into()),
            model: Some(SessionCreateModel {
                id: "deepseek-v4-flash-free".into(),
                provider_id: "opencode".into(),
                variant: None,
            }),
            title: None,
        })
        .await?;
    eprintln!("SESSION: id={}", session.id);
    let sid = session.id.clone();

    let mut stream = backend.events();

    backend
        .session_prompt(
            &sid,
            &PromptPayload {
                message_id: Some(MessageId::new(raider_opencode::ascending(
                    raider_opencode::Prefix::Message,
                ))),
                model: Some(PromptModel {
                    provider_id: "opencode".into(),
                    model_id: "deepseek-v4-flash-free".into(),
                }),
                agent: Some("build".into()),
                variant: None,
                parts: vec![PromptPart::Text(PromptTextPart {
                    id: None,
                    text: "reply with just OK".into(),
                })],
            },
        )
        .await?;
    eprintln!("PROMPT: submitted");

    let mut accumulated = String::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let mut idle_seen = false;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(2), stream.next()).await {
            Ok(Some(StreamItem::Event(ev))) => match ev.as_ref() {
                ServerEvent::MessagePartDelta(p) => {
                    if p.session_id == sid && p.field == "text" {
                        accumulated.push_str(&p.delta);
                        eprint!(".");
                    }
                }
                ServerEvent::SessionIdle(p) if p.session_id == sid => {
                    eprintln!("\nIDLE");
                    idle_seen = true;
                    break;
                }
                ServerEvent::SessionError(p) => {
                    eprintln!("\nSESSION_ERROR: {:?}", p.error);
                }
                _ => {}
            },
            Ok(Some(StreamItem::Error(e))) => eprintln!("\nERR: {e}"),
            Ok(Some(StreamItem::Reconnecting { attempt })) => eprintln!("\nRECONNECT {attempt}"),
            Ok(None) => {
                eprintln!("\nSTREAM_END");
                break;
            }
            Err(_) => continue,
        }
    }
    println!("ACCUMULATED_ASSISTANT_TEXT: {accumulated:?}");
    println!("IDLE_SEEN: {idle_seen}");
    if accumulated.is_empty() {
        return Err("no text received from server".into());
    }
    Ok(())
}
