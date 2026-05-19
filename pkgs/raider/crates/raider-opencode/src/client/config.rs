use url::Url;

use crate::error::{Error, Result};
use crate::types::common::Directory;

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub base_url: Url,
    pub directory: Directory,
    pub token: Option<String>,
    pub request_timeout: std::time::Duration,
}

impl ClientConfig {
    pub fn new(base_url: impl AsRef<str>, directory: impl Into<Directory>) -> Result<Self> {
        let parsed = Url::parse(base_url.as_ref()).map_err(|e| Error::BadUrl(format!("{e}")))?;
        Ok(Self {
            base_url: parsed,
            directory: directory.into(),
            token: None,
            request_timeout: std::time::Duration::from_secs(30),
        })
    }

    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token.filter(|t| !t.is_empty());
        self
    }

    pub fn with_request_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.request_timeout = timeout;
        self
    }
}
