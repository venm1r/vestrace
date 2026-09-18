//! PostgreSQL lease/claim over qualification_jobs, mirroring
//! embedding_work_repository.rs's claim/finish shape for one work kind.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use vestrace_application::{
    ApplicationError, InstallationMutationPermit, PermitMode, QualificationWorkClaim,
    QualificationWorkOutcome, QualificationWorkRepository, RequestContext,
};
use vestrace_domain::QualificationJobId;

use super::{PgInstallationMutationPermit, PgScopedTransaction, PgStore};

const CLAIM_CONFLICT: &str = "qualification work claim conflict";

pub struct PgQualificationWorkRepository {
    permit: PgInstallationMutationPermit,
}

impl PgQualificationWorkRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            permit: PgInstallationMutationPermit::new(store),
        }
    }
}

#[derive(FromRow)]
struct ClaimRow {
    job_id: uuid::Uuid,
    claim_owner: String,
    claim_deadline: DateTime<Utc>,
}

fn claim(row: ClaimRow) -> Result<QualificationWorkClaim, ApplicationError> {
    Ok(QualificationWorkClaim {
        job_id: QualificationJobId::from_uuid(row.job_id),
        owner: row.claim_owner,
        claim_deadline: row.claim_deadline,
    })
}

fn storage(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl QualificationWorkRepository for PgQualificationWorkRepository {
    async fn claim(
        &self,
        context: &RequestContext,
        owner: &str,
        limit: u32,
    ) -> Result<Vec<QualificationWorkClaim>, ApplicationError> {
        if owner.trim().is_empty() || owner.len() > 128 || limit == 0 || limit > 128 {
            return Err(ApplicationError::Conflict(CLAIM_CONFLICT.into()));
        }
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = permit
            .unit_of_work_mut()
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".into()))?;
        let rows = sqlx::query_as::<_, ClaimRow>(
            "SELECT * FROM vestrace_claim_qualification_work($1,$2,$3)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(owner)
        .bind(i32::try_from(limit).map_err(|_| ApplicationError::Conflict(CLAIM_CONFLICT.into()))?)
        .fetch_all(transaction.connection())
        .await
        .map_err(storage)?;
        permit.commit().await?;
        rows.into_iter().map(claim).collect()
    }

    async fn finish(
        &self,
        context: &RequestContext,
        claim: &QualificationWorkClaim,
        outcome: QualificationWorkOutcome,
    ) -> Result<(), ApplicationError> {
        if claim.owner.trim().is_empty() {
            return Err(ApplicationError::Conflict(CLAIM_CONFLICT.into()));
        }
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = permit
            .unit_of_work_mut()
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".into()))?;
        let finished: bool =
            sqlx::query_scalar("SELECT vestrace_finish_qualification_work($1,$2,$3,$4)")
                .bind(context.workspace_id.as_uuid())
                .bind(claim.job_id.as_uuid())
                .bind(&claim.owner)
                .bind(match outcome {
                    QualificationWorkOutcome::Completed => "completed",
                    QualificationWorkOutcome::RetryableFailure => "retryable_failure",
                    QualificationWorkOutcome::DefiniteFailure => "definite_failure",
                })
                .fetch_one(transaction.connection())
                .await
                .map_err(storage)?;
        if !finished {
            return Err(ApplicationError::Conflict(CLAIM_CONFLICT.into()));
        }
        permit.commit().await
    }
}
