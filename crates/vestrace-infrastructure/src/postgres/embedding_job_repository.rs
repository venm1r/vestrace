//! Governed acceptance for embedding jobs.
//!
//! Acceptance only. Rediscovery, dispatch and recovery are
//! `PgProviderDispatchRepository`'s, because they are the Run step's too: one
//! dispatch authority with two callers is the property P04 is built around, and
//! it stops being checkable the moment embeddings acquire a second one.

use async_trait::async_trait;
use vestrace_application::{
    AcceptEmbeddingJob, ApplicationError, EmbeddingJobRepository, ExternalEffectRepository,
    GovernedMutation, GovernedMutationApply, GovernedMutationReceipt, GovernedMutationRepository,
    RequestContext, UnitOfWork,
};

use super::{
    PgExternalEffectRepository, PgGovernedMutationRepository, PgScopedTransaction, PgStore,
};

pub struct PgEmbeddingJobRepository {
    governed_mutations: PgGovernedMutationRepository,
    effects: PgExternalEffectRepository,
}

impl PgEmbeddingJobRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            governed_mutations: PgGovernedMutationRepository::new(store.clone()),
            effects: PgExternalEffectRepository::new(store),
        }
    }
}

#[async_trait]
impl EmbeddingJobRepository for PgEmbeddingJobRepository {
    async fn accept_governed(
        &self,
        context: RequestContext,
        mut command: AcceptEmbeddingJob,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        if command.intent.workspace_id() != context.workspace_id
            || command.audit.workspace_id != context.workspace_id
            || command.audit.principal_id != context.principal_id
            || command
                .idempotency
                .as_ref()
                .is_some_and(|record| record.workspace_id != context.workspace_id)
            || command
                .outbox
                .iter()
                .any(|message| message.workspace_id != context.workspace_id)
        {
            return Err(ApplicationError::Policy(
                "embedding job acceptance identity must match its request context".to_owned(),
            ));
        }
        let audit = command.audit.clone();
        let idempotency = command.idempotency.take();
        let outbox = std::mem::take(&mut command.outbox);
        self.governed_mutations
            .commit(GovernedMutation {
                context,
                audit,
                idempotency,
                outbox,
                apply: AcceptEmbeddingJobMutation {
                    command,
                    effects: self.effects.clone(),
                },
            })
            .await
    }
}

struct AcceptEmbeddingJobMutation {
    command: AcceptEmbeddingJob,
    effects: PgExternalEffectRepository,
}

#[async_trait]
impl GovernedMutationApply for AcceptEmbeddingJobMutation {
    async fn apply(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<(), ApplicationError> {
        // The effect intent must be durable before the job that owns it, because
        // the job's foreign keys and every later dispatch converge on that exact
        // effect id.
        self.effects
            .save_intent_in(context, unit_of_work, &self.command.intent)
            .await?;
        let transaction = unit_of_work
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| {
                ApplicationError::Internal("expected PostgreSQL transaction".to_owned())
            })?;
        let accepted: Result<uuid::Uuid, sqlx::Error> =
            sqlx::query_scalar("SELECT vestrace_accept_embedding_job($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                .bind(self.command.job_id.as_uuid())
                .bind(context.workspace_id.as_uuid())
                .bind(self.command.space_registration_id.as_uuid())
                .bind(self.command.kind.as_str())
                .bind(self.command.model_binding_snapshot_id)
                .bind(self.command.intent.id().as_uuid())
                .bind(self.command.model_request_evidence_id.as_uuid())
                .bind(
                    self.command
                        .retries_unknown_embedding_job_id
                        .map(|id| id.as_uuid()),
                )
                .bind(
                    self.command
                        .expected_predecessor_version
                        .map(|version| version as i64),
                )
                .fetch_one(transaction.connection())
                .await;
        match accepted {
            Ok(id) if id == self.command.job_id.as_uuid() => Ok(()),
            // The guarded function returns the converged row's id. A different
            // id would mean it converged on a job this command did not name,
            // which is a conflict rather than a success worth reporting.
            Ok(_) => Err(ApplicationError::Conflict(
                "EMBEDDING_JOB_IDENTITY_CONFLICT".to_owned(),
            )),
            Err(error) => Err(map_acceptance_error(error)),
        }
    }
}

fn map_acceptance_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        // 23514 is every guarded structural refusal: an absent snapshot, a
        // missing connection or activation guard, an unauthorized space, a
        // conflicting existing tuple, or a predecessor that is not a terminal
        // ambiguity head. 23505 is the partial unique successor index, and
        // 22023 is a malformed argument.
        Some("23514") | Some("23505") | Some("22023") => {
            ApplicationError::Policy("EMBEDDING_JOB_ACCEPTANCE_REFUSED".to_owned())
        }
        _ => ApplicationError::Storage(error.to_string()),
    }
}
