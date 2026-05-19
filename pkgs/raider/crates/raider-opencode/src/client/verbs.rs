use reqwest::Method;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

use super::Client;

impl Client {
    pub(crate) async fn request_json<P: serde::Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&P>,
    ) -> Result<T> {
        let url = self.inner.build_url(path)?;
        let mut req = self
            .inner
            .http
            .request(method, url)
            .header("accept", "application/json");
        if let Some(body) = body {
            req = req.json(body);
        }
        let req = self.inner.apply_headers(req);
        let resp = req.send().await?;
        self.inner.ensure_ok(&resp, path)?;
        let bytes = resp.bytes().await?;
        let slice: &[u8] = if bytes.is_empty() { b"null" } else { &bytes };
        serde_json::from_slice(slice).map_err(|source| Error::Decode {
            path: path.to_string(),
            source,
        })
    }

    pub(crate) async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request_json::<serde_json::Value, T>(Method::GET, path, None)
            .await
    }

    pub(crate) async fn post_json<P: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        payload: &P,
    ) -> Result<T> {
        self.request_json(Method::POST, path, Some(payload)).await
    }

    pub(crate) async fn patch_json<P: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        payload: &P,
    ) -> Result<T> {
        self.request_json(Method::PATCH, path, Some(payload)).await
    }

    pub(crate) async fn delete_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request_json::<serde_json::Value, T>(Method::DELETE, path, None)
            .await
    }
}
