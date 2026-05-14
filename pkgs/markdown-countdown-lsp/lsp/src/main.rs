use std::sync::Arc;
use std::time::Duration;

use chrono::{
    DateTime, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use regex::Regex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{async_trait, Client, LanguageServer, LspService, Server};
use tracing::{info, warn, Level};

static DATE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"@(\d{1,2})[^\d\w]+(\d{1,2})[^\d\w]+(\d{2,4})(?:[^\d\w]+(\d{1,2}))?(?:[^\d\w]+(\d{1,2}))?",
    )
    .unwrap()
});

struct ParsedDate {
    dt: DateTime<Utc>,
    position: Position,
}

type DocumentStore = Arc<DashMap<Url, String>>;

#[derive(Debug)]
struct Backend {
    client: Client,
    document_store: DocumentStore,
}

fn format_duration_custom(duration: ChronoDuration) -> String {
    let total_hours = duration.num_hours();
    let remaining_minutes = duration.num_minutes() - (total_hours * 60);

    let mut parts = Vec::new();

    if total_hours > 0 {
        parts.push(format!("{}h", total_hours));
    }

    if remaining_minutes > 0 {
        parts.push(format!("{}m", remaining_minutes));
    }

    if parts.is_empty() {
        return "0m".to_string();
    }

    parts.join("")
}

impl Backend {
    /// Parses a single line of text and returns any found dates.
    fn parse_line(&self, text: &str, line_num: u32) -> Vec<ParsedDate> {
        let mut dates = Vec::new();
        for caps in DATE_RE.captures_iter(text) {
            let day = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
            let month = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
            let year = caps.get(3).and_then(|m| m.as_str().parse::<i32>().ok());

            let hour = caps
                .get(4)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            let minute = caps
                .get(5)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);

            if let (Some(d), Some(m), Some(mut y)) = (day, month, year) {
                if y < 100 {
                    y += 2000;
                }

                if let (Some(date), Some(time)) = (
                    NaiveDate::from_ymd_opt(y, m, d),
                    NaiveTime::from_hms_opt(hour, minute, 0),
                ) {
                    let naive_dt = NaiveDateTime::new(date, time);

                    if let Some(local_dt) = Local.from_local_datetime(&naive_dt).single() {
                        let utc_dt: DateTime<Utc> = local_dt.into();
                        dates.push(ParsedDate {
                            dt: utc_dt,
                            position: Position::new(line_num, text.chars().count() as u32),
                        });
                    } else {
                        warn!(
                            "Could not convert naive datetime to local time: {}",
                            naive_dt
                        );
                    }
                }
            }
        }
        dates
    }
}

#[async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        info!("Initializing server...");
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
                    InlayHintOptions {
                        resolve_provider: Some(false),
                        ..Default::default()
                    },
                ))),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..ServerCapabilities::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Markdown countdown server initialized!")
            .await;
        info!("Server initialized!");
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down server.");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        info!("File opened: {}", params.text_document.uri);
        self.document_store
            .insert(params.text_document.uri.clone(), params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        info!("File changed: {}", params.text_document.uri);
        let full_text = params.content_changes[0].text.clone();
        self.document_store
            .insert(params.text_document.uri, full_text);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        info!("File closed: {}", params.text_document.uri);
        self.document_store.remove(&params.text_document.uri);
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        info!("inlay_hint request for {}", uri);

        let Some(doc_entry) = self.document_store.get(&uri) else {
            warn!("inlay_hint requested for a document not in store: {}", uri);
            return Ok(None);
        };

        let text = doc_entry.value().clone();
        let mut hints = Vec::new();
        let now = Utc::now();

        for (line_num, line_text) in text.lines().enumerate() {
            for date in self.parse_line(line_text, line_num as u32) {
                if date.dt > now {
                    let duration = date.dt - now;
                    let custom_format = format_duration_custom(duration);
                    let label = format!("{}", custom_format);

                    hints.push(InlayHint {
                        position: date.position,
                        label: InlayHintLabel::String(label),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: Some(true),
                        padding_right: None,
                        data: None,
                    });
                }
            }
        }

        Ok(Some(hints))
    }
}

#[tokio::main]
async fn main() {
    let log_file = tracing_appender::rolling::daily("/tmp", "markdown-countdown-lsp.log");
    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(log_file);

    tracing_subscriber::fmt()
        .with_writer(non_blocking_writer)
        .with_target(true)
        .with_line_number(true)
        .with_max_level(Level::ERROR)
        .init();

    info!("Starting markdown-countdown-lsp server");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| {
        let client_clone = client.clone();
        tokio::spawn(async move {
            info!("Spawning periodic inlay hint refresh task.");
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                info!("Timer ticked, sending inlay hint refresh request.");
                if client_clone.inlay_hint_refresh().await.is_err() {
                    info!("Client disconnected, stopping refresh task.");
                    break;
                }
            }
        });

        Backend {
            client,
            document_store: Arc::new(DashMap::new()),
        }
    });

    Server::new(stdin, stdout, socket).serve(service).await;
    info!("Server has shut down.");
}
