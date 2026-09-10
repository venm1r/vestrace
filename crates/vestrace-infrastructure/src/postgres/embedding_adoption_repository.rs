//! PostgreSQL authority for legacy embedding adoption plans and their cutover.
//!
//! Every state change here is one guarded function call. Nothing in this file
//! decides whether a member may advance, whether a plan is ready, or whether
//! the installation gate may commit: the database refuses, and this adapter
//! translates the refusal.

use async_trait::async_trait;
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, RequestContext,
    embedding::{
        EmbeddingLegacyAdoptionRepository, LegacyAdoptionBlocker, LegacyAdoptionBlockerRecord,
        LegacyAdoptionMember, LegacyAdoptionMemberState, LegacyAdoptionProgress,
        MaterializedSource, StartLegacyAdoption,
    },
};
use vestrace_domain::{EmbeddingJobId, embedding::LegacyAdoptionState, id::LegacyAdoptionId};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgEmbeddingLegacyAdoptionRepository {
    store: PgStore,
}

impl PgEmbeddingLegacyAdoptionRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn adoption_state(value: &str) -> Result<LegacyAdoptionState, ApplicationError> {
    LegacyAdoptionState::ALL
        .into_iter()
        .find(|state| state.as_str() == value)
        .ok_or_else(|| {
            ApplicationError::Storage(format!("stored legacy adoption state {value} is unknown"))
        })
}

fn non_negative(value: i64, what: &str) -> Result<u64, ApplicationError> {
    u64::try_from(value)
        .map_err(|_| ApplicationError::Internal(format!("{what} must be non-negative")))
}

fn map_adoption_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("40001") | Some("23505") => {
            ApplicationError::Conflict("EMBEDDING_LEGACY_ADOPTION_CONFLICT".to_owned())
        }
        Some("23514") | Some("22023") => ApplicationError::Policy(
            error
                .as_database_error()
                .map(|database| database.message().to_owned())
                .unwrap_or_else(|| "legacy adoption refused".to_owned()),
        ),
        Some("42501") => ApplicationError::Unavailable(
            "governed legacy adoption authority is unavailable".to_owned(),
        ),
        _ => ApplicationError::Storage(error.to_string()),
    }
}

impl PgEmbeddingLegacyAdoptionRepository {
    /// Reads the plan back from the rows the database holds, so a caller never
    /// reports counts it accumulated itself.
    async fn read_progress(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        connection: &mut sqlx::PgConnection,
    ) -> Result<LegacyAdoptionProgress, ApplicationError> {
        let plan: (String, i64) = sqlx::query_as(
            "SELECT state, version FROM embedding_legacy_adoptions \
             WHERE workspace_id=$1 AND id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(plan_id.as_uuid())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_adoption_error)?
        .ok_or_else(|| ApplicationError::Policy("legacy adoption plan is absent".to_owned()))?;

        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT count(*), \
                    count(*) FILTER (WHERE state='satisfied'), \
                    count(*) FILTER (WHERE state='blocked') \
               FROM embedding_legacy_adoption_members \
              WHERE workspace_id=$1 AND adoption_id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(plan_id.as_uuid())
        .fetch_one(&mut *connection)
        .await
        .map_err(map_adoption_error)?;

        let blocker_rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT member_ordinal, reason FROM embedding_legacy_adoption_blockers \
              WHERE workspace_id=$1 AND adoption_id=$2 ORDER BY member_ordinal, reason",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(plan_id.as_uuid())
        .fetch_all(&mut *connection)
        .await
        .map_err(map_adoption_error)?;
        let mut blockers = Vec::with_capacity(blocker_rows.len());
        for (ordinal, reason) in blocker_rows {
            blockers.push(LegacyAdoptionBlockerRecord {
                ordinal: non_negative(ordinal, "member ordinal")?,
                reason: LegacyAdoptionBlocker::parse(&reason).ok_or_else(|| {
                    ApplicationError::Storage(format!(
                        "stored legacy adoption blocker {reason} is unknown"
                    ))
                })?,
            });
        }

        let deleted: Option<i64> = sqlx::query_scalar(
            "SELECT deleted_legacy_row_count FROM embedding_legacy_cutover_receipts \
              WHERE workspace_id=$1 AND adoption_id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(plan_id.as_uuid())
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_adoption_error)?;

        Ok(LegacyAdoptionProgress {
            plan_id,
            state: adoption_state(&plan.0)?,
            version: non_negative(plan.1, "plan version")?,
            total_members: non_negative(counts.0, "member count")?,
            satisfied_members: non_negative(counts.1, "satisfied count")?,
            blocked_members: non_negative(counts.2, "blocked count")?,
            blockers,
            deleted_legacy_rows: deleted
                .map(|value| non_negative(value, "deleted legacy row count"))
                .transpose()?,
        })
    }
}

#[async_trait]
impl EmbeddingLegacyAdoptionRepository for PgEmbeddingLegacyAdoptionRepository {
    async fn start_or_resume(
        &self,
        context: &RequestContext,
        command: StartLegacyAdoption,
    ) -> Result<LegacyAdoptionProgress, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let plan: Uuid =
            sqlx::query_scalar("SELECT vestrace_start_or_resume_legacy_adoption($1,$2,$3,$4,$5)")
                .bind(command.plan_id.as_uuid())
                .bind(context.workspace_id.as_uuid())
                .bind(command.legacy_space_registration_id)
                .bind(command.target_space_registration_id)
                .bind(command.idempotency_key.as_str())
                .fetch_one(transaction.connection())
                .await
                .map_err(map_adoption_error)?;
        let progress = self
            .read_progress(
                context,
                LegacyAdoptionId::from_uuid(plan),
                transaction.connection(),
            )
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(progress)
    }

    async fn progress(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
    ) -> Result<LegacyAdoptionProgress, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let progress = self
            .read_progress(context, plan_id, transaction.connection())
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(progress)
    }

    async fn unfinished_members(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        limit: u32,
    ) -> Result<Vec<LegacyAdoptionMember>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let rows: Vec<(
            i64,
            Uuid,
            Uuid,
            Uuid,
            Option<Uuid>,
            Option<Uuid>,
            Option<Uuid>,
            String,
        )> = sqlx::query_as(
            "SELECT member_ordinal, legacy_embedding_id, memory_id, memory_revision_id, \
                        source_material_id, source_intent_id, rebuild_job_id, state \
                   FROM embedding_legacy_adoption_members \
                  WHERE workspace_id=$1 AND adoption_id=$2 AND state IN ('planned','sourced') \
                  ORDER BY member_ordinal LIMIT $3",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(plan_id.as_uuid())
        .bind(i64::from(limit))
        .fetch_all(transaction.connection())
        .await
        .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        rows.into_iter()
            .map(|row| {
                Ok(LegacyAdoptionMember {
                    ordinal: non_negative(row.0, "member ordinal")?,
                    legacy_embedding_id: row.1,
                    memory_id: row.2,
                    memory_revision_id: row.3,
                    source_material_id: row.4,
                    source_intent_id: row.5,
                    rebuild_job_id: row.6,
                    state: LegacyAdoptionMemberState::parse(&row.7).ok_or_else(|| {
                        ApplicationError::Storage(format!(
                            "stored legacy adoption member state {} is unknown",
                            row.7
                        ))
                    })?,
                })
            })
            .collect()
    }

    async fn bind_source(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        source: MaterializedSource,
    ) -> Result<(), ApplicationError> {
        self.call(
            context,
            "SELECT vestrace_bind_legacy_adoption_source($1,$2,$3,$4,$5)",
            |query| {
                query
                    .bind(plan_id.as_uuid())
                    .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
                    .bind(source.material_id)
                    .bind(source.intent_id)
            },
        )
        .await
    }

    async fn bind_rebuild(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        job_id: EmbeddingJobId,
    ) -> Result<(), ApplicationError> {
        self.call(
            context,
            "SELECT vestrace_bind_legacy_adoption_rebuild($1,$2,$3,$4)",
            |query| {
                query
                    .bind(plan_id.as_uuid())
                    .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
                    .bind(job_id.as_uuid())
            },
        )
        .await
    }

    async fn satisfy_member(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        projection_entry_id: Uuid,
    ) -> Result<(), ApplicationError> {
        self.call(
            context,
            "SELECT vestrace_satisfy_legacy_adoption_member($1,$2,$3,$4)",
            |query| {
                query
                    .bind(plan_id.as_uuid())
                    .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
                    .bind(projection_entry_id)
            },
        )
        .await
    }

    async fn record_blocker(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        reason: LegacyAdoptionBlocker,
    ) -> Result<(), ApplicationError> {
        self.call(
            context,
            "SELECT vestrace_record_legacy_adoption_blocker($1,$2,$3,$4)",
            |query| {
                query
                    .bind(plan_id.as_uuid())
                    .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
                    .bind(reason.as_str())
            },
        )
        .await
    }

    async fn prove_ready(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        generation_id: Uuid,
    ) -> Result<u64, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let version: i64 =
            sqlx::query_scalar("SELECT vestrace_prove_legacy_adoption_ready($1,$2,$3)")
                .bind(context.workspace_id.as_uuid())
                .bind(plan_id.as_uuid())
                .bind(generation_id)
                .fetch_one(transaction.connection())
                .await
                .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        non_negative(version, "plan version")
    }

    async fn commit_cutover(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        receipt_id: Uuid,
        expected_version: u64,
        audit_event_id: Uuid,
    ) -> Result<u64, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let deleted: i64 =
            sqlx::query_scalar("SELECT vestrace_commit_legacy_adoption_cutover($1,$2,$3,$4,$5)")
                .bind(receipt_id)
                .bind(context.workspace_id.as_uuid())
                .bind(plan_id.as_uuid())
                .bind(i64::try_from(expected_version).unwrap_or(i64::MAX))
                .bind(audit_event_id)
                .fetch_one(transaction.connection())
                .await
                .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        non_negative(deleted, "deleted legacy row count")
    }

    async fn commit_plaintext_retirement(
        &self,
        context: &RequestContext,
    ) -> Result<u64, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let completed: i64 =
            sqlx::query_scalar("SELECT vestrace_commit_legacy_plaintext_retirement()")
                .fetch_one(transaction.connection())
                .await
                .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        non_negative(completed, "completed adoption count")
    }
}

impl PgEmbeddingLegacyAdoptionRepository {
    /// The shared shape of every void-returning guarded call: one scoped
    /// transaction, the workspace bound first, and the database's refusal
    /// carried through unchanged.
    async fn call<F>(
        &self,
        context: &RequestContext,
        sql: &str,
        bind: F,
    ) -> Result<(), ApplicationError>
    where
        F: for<'q> FnOnce(
            sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
        )
            -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        bind(sqlx::query(sql).bind(context.workspace_id.as_uuid()))
            .execute(transaction.connection())
            .await
            .map_err(map_adoption_error)?;
        transaction
            .commit()
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(())
    }
}
