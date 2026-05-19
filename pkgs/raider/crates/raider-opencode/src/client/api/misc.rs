use crate::error::Result;
use crate::types::provider::ProviderList;

use super::super::Client;

impl Client {
    pub async fn mcp_status(&self) -> Result<crate::types::mcp::McpRegistry> {
        self.get_json("/mcp").await
    }

    pub async fn lsp_status(&self) -> Result<Vec<crate::types::lsp::LspStatus>> {
        self.get_json("/lsp").await
    }

    pub async fn config_get(&self) -> Result<crate::types::config::AppConfig> {
        self.get_json("/config").await
    }

    pub async fn provider_list(&self) -> Result<ProviderList> {
        self.get_json("/provider").await
    }

    pub async fn sync_start(&self, _directory: Option<&str>) -> Result<bool> {
        let path = "/sync/start";
        let url = self.inner.build_url(path)?;
        let req = self.inner.apply_headers(
            self.inner
                .http
                .post(url)
                .header("accept", "application/json"),
        );
        let resp = req.send().await?;
        self.inner.ensure_ok(&resp, path)?;
        let bytes = resp.bytes().await?;
        let val: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Bool(true));
        Ok(val.as_bool().unwrap_or(true))
    }
}
