use async_trait::async_trait;

use raider_opencode::{types::provider::ProviderList, Error};

#[async_trait]
pub trait ProviderBackend: Send + Sync + 'static {
    async fn provider_list(&self) -> Result<ProviderList, Error>;
}
