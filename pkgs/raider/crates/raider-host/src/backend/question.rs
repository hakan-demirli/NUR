use async_trait::async_trait;

use raider_opencode::{types::question::QuestionRequest, Error};

#[async_trait]
pub trait QuestionBackend: Send + Sync + 'static {
    async fn question_list(&self) -> Result<Vec<QuestionRequest>, Error>;

    async fn question_reply(
        &self,
        request_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<(), Error>;

    async fn question_reject(&self, request_id: &str) -> Result<(), Error>;
}
