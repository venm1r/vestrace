use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{ApplicationError, JobRepository};
use vestrace_domain::{
    Job, JobState,
    id::{JobId, WorkspaceId},
};

pub struct PgJobRepository {
    pool: PgPool,
}

impl PgJobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn job_state_from_str(value: &str) -> Result<JobState, ApplicationError> {
    match value {
        "pending" => Ok(JobState::Pending),
        "leased" => Ok(JobState::Leased),
        "completed" => Ok(JobState::Completed),
        "failed" => Ok(JobState::Failed),
        "dead_letter" => Ok(JobState::DeadLetter),
        _ => Err(ApplicationError::Storage(format!(
            "stored job state '{value}' is not supported"
        ))),
    }
}

#[async_trait]
impl JobRepository for PgJobRepository {
    async fn enqueue(&self, job: &Job) -> Result<(), ApplicationError> {
        let state_str = match job.state {
            JobState::Pending => "pending",
            JobState::Leased => "leased",
            JobState::Completed => "completed",
            JobState::Failed => "failed",
            JobState::DeadLetter => "dead_letter",
        };

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
        .bind(state_str)
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

    async fn lease_next(&self) -> Result<Option<Job>, ApplicationError> {
        let mut tx = self.pool.begin().await.map_err(storage_error)?;

        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, job_type, payload, state, attempts, max_attempts, run_at, leased_until, created_at, updated_at
            FROM jobs
            WHERE state = 'pending' AND run_at <= NOW()
            ORDER BY run_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT 1
            "#,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;

        let row = match row {
            Some(row) => row,
            None => {
                tx.rollback().await.map_err(storage_error)?;
                return Ok(None);
            }
        };

        let id: uuid::Uuid = row.try_get("id").map_err(storage_error)?;
        let workspace_id: uuid::Uuid = row.try_get("workspace_id").map_err(storage_error)?;
        let job_type: String = row.try_get("job_type").map_err(storage_error)?;
        let payload: serde_json::Value = row.try_get("payload").map_err(storage_error)?;
        let state_str: String = row.try_get("state").map_err(storage_error)?;
        let attempts: i32 = row.try_get("attempts").map_err(storage_error)?;
        let max_attempts: i32 = row.try_get("max_attempts").map_err(storage_error)?;
        let run_at = row
            .try_get::<chrono::DateTime<chrono::Utc>, _>("run_at")
            .map_err(storage_error)?;
        let leased_until: Option<chrono::DateTime<chrono::Utc>> =
            row.try_get("leased_until").map_err(storage_error)?;
        let created_at = row
            .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            .map_err(storage_error)?;
        let updated_at = row
            .try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
            .map_err(storage_error)?;

        sqlx::query("UPDATE jobs SET state = 'leased', updated_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;

        tx.commit().await.map_err(storage_error)?;

        Ok(Some(Job {
            id: JobId::from_uuid(id),
            workspace_id: WorkspaceId::from_uuid(workspace_id),
            job_type,
            payload,
            state: job_state_from_str(&state_str)?,
            attempts: u32::try_from(attempts)
                .map_err(|e| ApplicationError::Storage(e.to_string()))?,
            max_attempts: u32::try_from(max_attempts)
                .map_err(|e| ApplicationError::Storage(e.to_string()))?,
            run_at,
            leased_until,
            created_at,
            updated_at,
        }))
    }

    async fn complete(&self, id: JobId) -> Result<(), ApplicationError> {
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
