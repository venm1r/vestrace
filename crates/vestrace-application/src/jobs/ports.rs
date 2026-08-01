use async_trait::async_trait;
use vestrace_domain::{id::JobId, Job, JobState};
use crate::ApplicationError;

#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn enqueue(&mut self, job: &Job) -> Result<(), ApplicationError>;
    async fn lease_next(&mut self) -> Result<Option<Job>, ApplicationError>;
    async fn complete(&mut self, id: JobId) -> Result<(), ApplicationError>;
}
