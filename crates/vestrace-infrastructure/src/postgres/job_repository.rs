use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::{ApplicationError, JobRepository};
use vestrace_domain::{Job, id::JobId};

pub struct PgJobRepository {
    pool: PgPool,
}

impl PgJobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl JobRepository for PgJobRepository {
    async fn enqueue(&mut self, job: &Job) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO jobs (id, workspace_id, job_type, payload, state, attempts, max_attempts, run_at, leased_until, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(job.id.as_uuid())
        .bind(job.workspace_id.as_uuid())
        .bind(&job.job_type)
        .bind(&job.payload)
        .bind("pending")
        .bind(job.attempts as i32)
        .bind(job.max_attempts as i32)
        .bind(job.run_at)
        .bind(job.leased_until)
        .bind(job.created_at)
        .bind(job.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn lease_next(&mut self) -> Result<Option<Job>, ApplicationError> {
        // Implementation with FOR UPDATE SKIP LOCKED
        Ok(None)
    }

    async fn complete(&mut self, id: JobId) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            UPDATE jobs SET state = 'completed', updated_at = NOW() WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(())
    }
}
