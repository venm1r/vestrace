//! Governed acceptance for embedding jobs.
//!
//! Acceptance only. Rediscovery, dispatch and recovery are
//! `PgProviderDispatchRepository`'s, because they are the Run step's too: one
//! dispatch authority with two callers is the property P04 is built around, and
//! it stops being checkable the moment embeddings acquire a second one.

use async_trait::async_trait;
use vestrace_application::{
    AcceptEmbeddingJob, ApplicationError, EmbeddingJobRepository, EmbeddingJobTerminationReceipt,
    ExternalEffectRepository, GovernedMutation, GovernedMutationApply, GovernedMutationReceipt,
    GovernedMutationRepository, InstallationMutationPermit, PermitMode, PreDispatchTerminalState,
    PreDispatchTerminationEvidence, RequestContext, TerminateEmbeddingJobPreDispatch, UnitOfWork,
};

use super::{
    PgExternalEffectRepository, PgGovernedMutationRepository, PgScopedTransaction, PgStore,
};

pub struct PgEmbeddingJobRepository {
    permit: super::PgInstallationMutationPermit,
    governed_mutations: PgGovernedMutationRepository,
    effects: PgExternalEffectRepository,
}

impl PgEmbeddingJobRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            permit: super::PgInstallationMutationPermit::new(store.clone()),
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

    async fn terminate_pre_dispatch(
        &self,
        context: RequestContext,
        command: TerminateEmbeddingJobPreDispatch,
    ) -> Result<EmbeddingJobTerminationReceipt, ApplicationError> {
        if command.idempotency_key.trim().is_empty() {
            return Err(ApplicationError::Policy(
                "embedding pre-dispatch termination requires an idempotency key".to_owned(),
            ));
        }
        let (evidence_kind, evidence_id, policy_version, capability, operation, scope, risk) =
            match &command.evidence {
                PreDispatchTerminationEvidence::CancellationAuthorization(decision) => {
                    let request = vestrace_domain::AuthorizationRequest::new(
                        vestrace_domain::Capability::ExecutionWrite,
                        "embedding.job.cancel",
                        vestrace_domain::ResourceScope::workspace().to_string(),
                        vestrace_domain::RiskCategory::Low,
                    );
                    vestrace_application::embedding::job::validate_cancellation_decision(
                        &context, &request, decision,
                    )?;
                    if command.terminal_state != PreDispatchTerminalState::Cancelled {
                        return Err(ApplicationError::Policy(
                            "cancellation authorization cannot prove definite failure".to_owned(),
                        ));
                    }
                    (
                        command.evidence.kind(),
                        command.evidence.id(),
                        Some(decision.policy_version.as_str()),
                        Some("execution.write"),
                        Some("embedding.job.cancel"),
                        Some("workspace://"),
                        Some("low"),
                    )
                }
                PreDispatchTerminationEvidence::ExternalEffectDenied { .. }
                | PreDispatchTerminationEvidence::AdmissionTimeout { .. } => {
                    if command.terminal_state != PreDispatchTerminalState::FailedDefinite {
                        return Err(ApplicationError::Policy(
                            "durable failure evidence cannot cancel an embedding job".to_owned(),
                        ));
                    }
                    (
                        command.evidence.kind(),
                        command.evidence.id(),
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                }
            };

        let expected_version = i64::try_from(command.expected_version).map_err(|_| {
            ApplicationError::Policy(
                "embedding pre-dispatch termination version is out of range".to_owned(),
            )
        })?;
        let mut permit = self.permit.acquire(PermitMode::Shared, &context).await?;
        let row = sqlx::query_as::<_, EmbeddingJobTerminationReceiptRow>(
            "SELECT * FROM vestrace_terminate_embedding_job_pre_dispatch(\
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(command.receipt_id)
        .bind(context.workspace_id.as_uuid())
        .bind(context.principal_id.as_uuid())
        .bind(command.job_id.as_uuid())
        .bind(expected_version)
        .bind(&command.idempotency_key)
        .bind(command.terminal_state.as_str())
        .bind(evidence_kind)
        .bind(evidence_id)
        .bind(policy_version)
        .bind(capability)
        .bind(operation)
        .bind(scope)
        .bind(risk)
        .fetch_one(
            permit
                .unit_of_work_mut()
                .as_any_mut()
                .downcast_mut::<PgScopedTransaction>()
                .ok_or_else(|| {
                    ApplicationError::Internal("expected PostgreSQL transaction".to_owned())
                })?
                .connection(),
        )
        .await
        .map_err(map_termination_error)?;
        if row.created {
            let audit = vestrace_domain::AuditEvent::new(
                vestrace_domain::id::AuditEventId::new(),
                context.workspace_id,
                context.principal_id,
                match command.terminal_state {
                    PreDispatchTerminalState::Cancelled => "embedding.job.cancelled",
                    PreDispatchTerminalState::FailedDefinite => "embedding.job.failed_definite",
                },
                "embedding_job",
                command.job_id.as_uuid(),
                serde_json::json!({
                    "receipt_id": row.receipt_id,
                    "evidence_kind": row.evidence_kind,
                    "evidence_id": row.evidence_id,
                    "terminal_version": row.version,
                }),
                vestrace_domain::time::now(),
            )
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
            self.governed_mutations
                .commit_in(
                    permit.unit_of_work_mut(),
                    GovernedMutation {
                        context: context.clone(),
                        audit,
                        idempotency: None,
                        outbox: Vec::new(),
                        apply: TerminationAuditMutation,
                    },
                )
                .await?;
        }
        permit.commit().await?;
        let terminal_state = match row.terminal_state.as_str() {
            "cancelled" => PreDispatchTerminalState::Cancelled,
            "failed_definite" => PreDispatchTerminalState::FailedDefinite,
            _ => {
                return Err(ApplicationError::Storage(
                    "termination function returned an unknown terminal state".to_owned(),
                ));
            }
        };
        let version = u64::try_from(row.version).map_err(|_| {
            ApplicationError::Storage("termination function returned an invalid version".to_owned())
        })?;
        Ok(EmbeddingJobTerminationReceipt {
            receipt_id: row.receipt_id,
            job_id: vestrace_domain::EmbeddingJobId::from_uuid(row.job_id),
            version,
            terminal_state,
            evidence_kind: row.evidence_kind,
            evidence_id: row.evidence_id,
        })
    }
}

#[derive(sqlx::FromRow)]
struct EmbeddingJobTerminationReceiptRow {
    receipt_id: uuid::Uuid,
    job_id: uuid::Uuid,
    version: i64,
    terminal_state: String,
    evidence_kind: String,
    evidence_id: uuid::Uuid,
    created: bool,
}

struct TerminationAuditMutation;

#[async_trait]
impl GovernedMutationApply for TerminationAuditMutation {
    async fn apply(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<(), ApplicationError> {
        Ok(())
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

fn map_termination_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("40001") | Some("23505") => {
            ApplicationError::Conflict("EMBEDDING_JOB_PRE_DISPATCH_TERMINATION_CONFLICT".to_owned())
        }
        Some("22023") | Some("23514") | Some("42501") => {
            ApplicationError::Policy("EMBEDDING_JOB_PRE_DISPATCH_TERMINATION_REFUSED".to_owned())
        }
        _ => ApplicationError::Storage(error.to_string()),
    }
}
