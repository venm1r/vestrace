use crate::{error::ApplicationError, jobs::ports::JobRepository};

pub struct Worker<J> {
    job_repo: J,
}

impl<J> Worker<J>
where
    J: JobRepository,
{
    pub fn new(job_repo: J) -> Self {
        Self { job_repo }
    }

    pub async fn process_one(&self) -> Result<bool, ApplicationError> {
        if let Some(job) = self.job_repo.lease_next().await? {
            // Process job execution
            self.job_repo.complete(job.id).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
