mod api;
pub mod config;
mod events_link;
mod transport;
mod verbs;

use std::sync::Arc;

use crate::error::Result;

pub use config::ClientConfig;
pub(crate) use transport::ClientInner;

#[derive(Clone)]
pub struct Client {
    pub(crate) inner: Arc<ClientInner>,
}

impl Client {
    pub fn connect(config: ClientConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()?;

        let sse_http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .pool_max_idle_per_host(0)
            .build()?;
        Ok(Self {
            inner: Arc::new(ClientInner {
                config,
                http,
                sse_http,
            }),
        })
    }

    pub fn config(&self) -> &ClientConfig {
        &self.inner.config
    }
}
