use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid base URL: {0}")]
    BadUrl(String),

    #[error("HTTP transport error: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("HTTP {status} from {path}: {body}")]
    Http {
        status: u16,
        path: String,
        body: String,
    },

    #[error("failed to decode JSON from {path}: {source}")]
    Decode {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("SSE stream closed by server")]
    StreamClosed,

    #[error("SSE frame malformed: {0}")]
    BadFrame(String),

    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;
