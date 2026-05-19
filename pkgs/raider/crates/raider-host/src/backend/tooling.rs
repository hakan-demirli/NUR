use async_trait::async_trait;

use raider_opencode::{
    types::{config::AppConfig, lsp::LspStatus, mcp::McpRegistry},
    Error,
};

#[async_trait]
pub trait ToolingBackend: Send + Sync + 'static {
    async fn mcp_status(&self) -> Result<McpRegistry, Error>;

    async fn lsp_status(&self) -> Result<Vec<LspStatus>, Error>;

    async fn config_get(&self) -> Result<AppConfig, Error>;

    async fn sync_start(&self, directory: Option<&str>) -> Result<bool, Error>;
}
