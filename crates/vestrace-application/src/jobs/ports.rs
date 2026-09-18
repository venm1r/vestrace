use crate::ApplicationError;
use async_trait::async_trait;
use vestrace_domain::{Job, id::JobId};

#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn enqueue(&self, job: &Job) -> Result<(), ApplicationError>;
    async fn lease_next(&self) -> Result<Option<Job>, ApplicationError>;
    async fn complete(&self, id: JobId) -> Result<(), ApplicationError>;
}
