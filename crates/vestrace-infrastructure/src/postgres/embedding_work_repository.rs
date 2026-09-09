//! PostgreSQL implementation of bounded embedding-worker claims.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use vestrace_application::embedding::{
    EmbeddingWorkClaim, EmbeddingWorkKind, EmbeddingWorkOutcome, EmbeddingWorkRepository,
};
use vestrace_application::{
    ApplicationError, InstallationMutationPermit, PermitMode, RequestContext,
};
use vestrace_domain::EmbeddingJobId;

use super::{PgInstallationMutationPermit, PgScopedTransaction, PgStore};

const CLAIM_CONFLICT: &str = "EMBEDDING_WORK_CLAIM_CONFLICT";

pub struct PgEmbeddingWorkRepository {
    permit: PgInstallationMutationPermit,
}

impl PgEmbeddingWorkRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            permit: PgInstallationMutationPermit::new(store),
        }
    }
}

#[derive(FromRow)]
struct ClaimRow {
    job_id: uuid::Uuid,
    kind: String,
    claim_owner: String,
    claim_deadline: DateTime<Utc>,
}

#[async_trait]
impl EmbeddingWorkRepository for PgEmbeddingWorkRepository {
    async fn claim(
        &self,
        context: &RequestContext,
        kind: EmbeddingWorkKind,
        owner: &str,
        limit: u32,
    ) -> Result<Vec<EmbeddingWorkClaim>, ApplicationError> {
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
            "SELECT * FROM vestrace_claim_embedding_work($1,$2,$3,$4)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(kind.as_str())
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
        claim: &EmbeddingWorkClaim,
        outcome: EmbeddingWorkOutcome,
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
            sqlx::query_scalar("SELECT vestrace_finish_embedding_work($1,$2,$3,$4,$5)")
                .bind(context.workspace_id.as_uuid())
                .bind(claim.job_id.as_uuid())
                .bind(claim.kind.as_str())
                .bind(&claim.owner)
                .bind(match outcome {
                    EmbeddingWorkOutcome::Completed => "completed",
                    EmbeddingWorkOutcome::RetryableFailure => "retryable_failure",
                    EmbeddingWorkOutcome::DefiniteFailure => "definite_failure",
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

fn claim(row: ClaimRow) -> Result<EmbeddingWorkClaim, ApplicationError> {
    let kind = match row.kind.as_str() {
        "dispatch" => EmbeddingWorkKind::Dispatch,
        "reconcile_keys" => EmbeddingWorkKind::ReconcileKeys,
        "finalize_result" => EmbeddingWorkKind::FinalizeResult,
        "build_index" => EmbeddingWorkKind::BuildIndex,
        "coordinate_transition" => EmbeddingWorkKind::CoordinateTransition,
        "propagate_erasure" => EmbeddingWorkKind::PropagateErasure,
        _ => {
            return Err(ApplicationError::Storage(
                "unknown embedding work kind".into(),
            ));
        }
    };
    Ok(EmbeddingWorkClaim {
        job_id: EmbeddingJobId::from_uuid(row.job_id),
        kind,
        owner: row.claim_owner,
        claim_deadline: row.claim_deadline,
    })
}

fn storage(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("22023") | Some("23514") | Some("42501") => {
            ApplicationError::Conflict(CLAIM_CONFLICT.into())
        }
        _ => ApplicationError::Storage(error.to_string()),
    }
}
