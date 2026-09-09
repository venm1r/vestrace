//! PostgreSQL bridge for the embedding-owned output-key reconciler.
//!
//! Each method owns a short installation mutation permit.  The service calls
//! the host vault only after `claim_next` commits and before a later receipt
//! permit begins.

use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    AcceptDeliveryOutputs, AcceptEmbeddingJob, ApplicationError, DeliveryOutputAcceptanceReceipt,
    EmbeddingOutputKeyBinding, EmbeddingOutputKeyPlan, EmbeddingOutputKeyProgress,
    EmbeddingOutputKeyRepository, ExternalEffectRepository, GovernedMutation,
    GovernedMutationApply, GovernedMutationRepository, InstallationMutationPermit, PermitMode,
    PreDispatchTerminalState, PreDispatchTerminationEvidence, RequestContext,
    RequestEmbeddingOutputRetirement, UnitOfWork,
};
use vestrace_domain::{
    ContentMaterialId, EmbeddingJobId, ErasureReceipt, IntentNonce, MaterialKeyCreationIntentId,
    MaterialKeyId, VaultReceipt, WorkspaceId,
};

use super::{
    PgExternalEffectRepository, PgGovernedMutationRepository, PgInstallationMutationPermit,
    PgScopedTransaction, PgStore,
};

pub struct PgEmbeddingOutputKeyRepository {
    permit: PgInstallationMutationPermit,
    governed_mutations: PgGovernedMutationRepository,
    effects: PgExternalEffectRepository,
}

impl PgEmbeddingOutputKeyRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            permit: PgInstallationMutationPermit::new(store.clone()),
            governed_mutations: PgGovernedMutationRepository::new(store.clone()),
            effects: PgExternalEffectRepository::new(store),
        }
    }
}

#[async_trait]
impl EmbeddingOutputKeyRepository for PgEmbeddingOutputKeyRepository {
    async fn accept_delivery_outputs(
        &self,
        context: &RequestContext,
        command: AcceptDeliveryOutputs,
    ) -> Result<DeliveryOutputAcceptanceReceipt, ApplicationError> {
        if command.idempotency_key.trim().is_empty()
            || command.outputs.is_empty()
            || !matches!(
                command.acceptance.kind,
                vestrace_domain::embedding::EmbeddingJobKind::Delivery
                    | vestrace_domain::embedding::EmbeddingJobKind::Rebuild
            )
            || command.acceptance.intent.workspace_id() != context.workspace_id
            || command.acceptance.intent.actor_id() != context.principal_id
            || command.acceptance.audit.workspace_id != context.workspace_id
            || command.acceptance.audit.principal_id != context.principal_id
            || command
                .acceptance
                .idempotency
                .as_ref()
                .is_none_or(|record| {
                    record.workspace_id != context.workspace_id
                        || record.idempotency_key != command.idempotency_key
                })
            || command
                .acceptance
                .outbox
                .iter()
                .any(|message| message.workspace_id != context.workspace_id)
        {
            return Err(ApplicationError::Policy(
                "delivery output acceptance authority does not match its request context".into(),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        if command.outputs.iter().enumerate().any(|(ordinal, output)| {
            output.output_ordinal != ordinal as u64 || !seen.insert(output.output_ordinal)
        }) {
            return Err(ApplicationError::Policy(
                "delivery output ordinals must be contiguous and unique".into(),
            ));
        }

        let outputs = serde_json::to_value(&command.outputs)
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;
        let intent_payload = serde_json::to_value(&command.acceptance.intent)
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;
        let audit_payload = serde_json::to_value(&command.acceptance.audit)
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;
        let idempotency_payload = serde_json::to_value(&command.acceptance.idempotency)
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;
        let outbox_payload = serde_json::to_value(&command.acceptance.outbox)
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;
        let request_tuple = serde_json::json!({
            "job_id": command.acceptance.job_id.as_uuid(),
            "space_registration_id": command.acceptance.space_registration_id.as_uuid(),
            "kind": command.acceptance.kind.as_str(),
            "model_binding_snapshot_id": command.acceptance.model_binding_snapshot_id,
            "external_effect_intent": intent_payload,
            "model_request_evidence_id": command.acceptance.model_request_evidence_id.as_uuid(),
            "retries_unknown_embedding_job_id": command.acceptance.retries_unknown_embedding_job_id.map(|id| id.as_uuid()),
            "expected_predecessor_version": command.acceptance.expected_predecessor_version,
            "outputs": outputs,
            "audit": audit_payload,
            "idempotency": idempotency_payload,
            "outbox": outbox_payload,
        });
        let expected_predecessor_version = command
            .acceptance
            .expected_predecessor_version
            .map(i64::try_from)
            .transpose()
            .map_err(|_| {
                ApplicationError::Policy("embedding predecessor version is out of range".into())
            })?;
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let tx = permit
            .unit_of_work_mut()
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".into()))?;
        let gate = sqlx::query_as::<_, DeliveryAcceptanceGateRow>(
            "SELECT * FROM vestrace_begin_delivery_embedding_outputs(\
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(command.receipt_id)
        .bind(context.workspace_id.as_uuid())
        .bind(context.principal_id.as_uuid())
        .bind(&command.idempotency_key)
        .bind(command.acceptance.job_id.as_uuid())
        .bind(command.acceptance.space_registration_id.as_uuid())
        .bind(command.acceptance.kind.as_str())
        .bind(command.acceptance.model_binding_snapshot_id)
        .bind(command.acceptance.intent.id().as_uuid())
        .bind(command.acceptance.model_request_evidence_id.as_uuid())
        .bind(
            command
                .acceptance
                .retries_unknown_embedding_job_id
                .map(|id| id.as_uuid()),
        )
        .bind(expected_predecessor_version)
        .bind(request_tuple)
        .bind(&outputs)
        .fetch_one(tx.connection())
        .await
        .map_err(storage)?;

        let AcceptEmbeddingJob {
            job_id: _,
            space_registration_id: _,
            kind: _,
            model_binding_snapshot_id: _,
            intent,
            model_request_evidence_id: _,
            retries_unknown_embedding_job_id: _,
            expected_predecessor_version: _,
            idempotency,
            outbox,
            audit,
        } = command.acceptance;
        if gate.created {
            self.effects
                .save_intent_in(context, permit.unit_of_work_mut(), &intent)
                .await?;
            let tx = permit
                .unit_of_work_mut()
                .as_any_mut()
                .downcast_mut::<PgScopedTransaction>()
                .ok_or_else(|| {
                    ApplicationError::Internal("expected PostgreSQL transaction".into())
                })?;
            let finalized: serde_json::Value =
                sqlx::query_scalar("SELECT vestrace_finalize_delivery_embedding_outputs($1,$2,$3)")
                    .bind(context.workspace_id.as_uuid())
                    .bind(context.principal_id.as_uuid())
                    .bind(gate.receipt_id)
                    .fetch_one(tx.connection())
                    .await
                    .map_err(storage)?;
            if finalized != gate.accepted_outputs {
                return Err(ApplicationError::Storage(
                    "delivery acceptance returned inconsistent output identities".into(),
                ));
            }
            self.governed_mutations
                .commit_in(
                    permit.unit_of_work_mut(),
                    GovernedMutation {
                        context: context.clone(),
                        audit,
                        idempotency,
                        outbox,
                        apply: DeliveryAcceptanceAuditMutation,
                    },
                )
                .await?;
        }
        permit.commit().await?;
        let outputs = serde_json::from_value(gate.accepted_outputs)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(DeliveryOutputAcceptanceReceipt {
            receipt_id: gate.receipt_id,
            job_id: EmbeddingJobId::from_uuid(gate.job_id),
            outputs,
        })
    }

    async fn request_retirement(
        &self,
        context: &RequestContext,
        command: RequestEmbeddingOutputRetirement,
    ) -> Result<(), ApplicationError> {
        let termination = command.termination;
        if termination.idempotency_key.trim().is_empty() {
            return Err(ApplicationError::Policy(
                "embedding output retirement requires an idempotency key".into(),
            ));
        }
        let (evidence_kind, evidence_id, policy_version, capability, operation, scope, risk) =
            match &termination.evidence {
                PreDispatchTerminationEvidence::CancellationAuthorization(decision) => {
                    let request = vestrace_domain::AuthorizationRequest::new(
                        vestrace_domain::Capability::ExecutionWrite,
                        "embedding.job.cancel",
                        vestrace_domain::ResourceScope::workspace().to_string(),
                        vestrace_domain::RiskCategory::Low,
                    );
                    vestrace_application::embedding::job::validate_cancellation_decision(
                        context, &request, decision,
                    )?;
                    if termination.terminal_state != PreDispatchTerminalState::Cancelled {
                        return Err(ApplicationError::Policy(
                            "cancellation cannot authorize definite output failure".into(),
                        ));
                    }
                    (
                        termination.evidence.kind(),
                        termination.evidence.id(),
                        Some(decision.policy_version.as_str()),
                        Some("execution.write"),
                        Some("embedding.job.cancel"),
                        Some("workspace://"),
                        Some("low"),
                    )
                }
                PreDispatchTerminationEvidence::ExternalEffectDenied { .. }
                | PreDispatchTerminationEvidence::AdmissionTimeout { .. } => {
                    if termination.terminal_state != PreDispatchTerminalState::FailedDefinite {
                        return Err(ApplicationError::Policy(
                            "failure evidence cannot authorize output cancellation".into(),
                        ));
                    }
                    (
                        termination.evidence.kind(),
                        termination.evidence.id(),
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                }
            };
        let expected_version = i64::try_from(termination.expected_version).map_err(|_| {
            ApplicationError::Policy("embedding output retirement version is out of range".into())
        })?;
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = permit
            .unit_of_work_mut()
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".into()))?;
        let authority_id: uuid::Uuid = sqlx::query_scalar(
            "SELECT vestrace_request_embedding_output_retirement(\
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(termination.receipt_id)
        .bind(context.workspace_id.as_uuid())
        .bind(context.principal_id.as_uuid())
        .bind(termination.job_id.as_uuid())
        .bind(expected_version)
        .bind(&termination.idempotency_key)
        .bind(termination.terminal_state.as_str())
        .bind(evidence_kind)
        .bind(evidence_id)
        .bind(policy_version)
        .bind(capability)
        .bind(operation)
        .bind(scope)
        .bind(risk)
        .fetch_one(transaction.connection())
        .await
        .map_err(storage)?;
        if authority_id != termination.receipt_id {
            return Err(ApplicationError::Storage(
                "output retirement returned a different terminal authority".into(),
            ));
        }
        permit.commit().await
    }

    async fn claim_next(
        &self,
        context: &RequestContext,
    ) -> Result<Option<EmbeddingOutputKeyPlan>, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = permit
            .unit_of_work_mut()
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".into()))?;
        let row = sqlx::query("SELECT * FROM vestrace_claim_embedding_output_key($1)")
            .bind(context.workspace_id.as_uuid())
            .fetch_optional(transaction.connection())
            .await
            .map_err(storage)?;
        permit.commit().await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let workspace_id = WorkspaceId::from_uuid(row.get("workspace_id"));
        if workspace_id != context.workspace_id {
            return Err(ApplicationError::Storage(
                "output key authority returned a foreign workspace".into(),
            ));
        }
        let receipt: Option<uuid::Uuid> = row.get("vault_receipt");
        Ok(Some(EmbeddingOutputKeyPlan {
            binding: EmbeddingOutputKeyBinding {
                workspace_id,
                job_id: EmbeddingJobId::from_uuid(row.get("job_id")),
                intent_id: MaterialKeyCreationIntentId::from_uuid(row.get("intent_id")),
                material_id: ContentMaterialId::from_uuid(row.get("material_id")),
                key_id: MaterialKeyId::from_uuid(row.get("material_key_id")),
                nonce: IntentNonce::from_uuid(row.get("nonce")),
                output_ordinal: u64::try_from(row.get::<i64, _>("output_ordinal"))
                    .map_err(|_| ApplicationError::Storage("output ordinal is invalid".into()))?,
            },
            receipt: receipt.map(VaultReceipt::from_uuid),
            retirement_requested: row.get("retirement_requested"),
        }))
    }

    async fn record_receipt(
        &self,
        context: &RequestContext,
        binding: &EmbeddingOutputKeyBinding,
        receipt: VaultReceipt,
    ) -> Result<EmbeddingOutputKeyProgress, ApplicationError> {
        assert_context(context, binding)?;
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = permit
            .unit_of_work_mut()
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".into()))?;
        sqlx::query("SELECT vestrace_record_embedding_output_key_receipt($1,$2,$3)")
            .bind(context.workspace_id.as_uuid())
            .bind(binding.intent_id.as_uuid())
            .bind(receipt.as_uuid())
            .execute(transaction.connection())
            .await
            .map_err(storage)?;
        let aggregate: String =
            sqlx::query_scalar("SELECT vestrace_embedding_output_key_progress($1,$2)")
                .bind(context.workspace_id.as_uuid())
                .bind(binding.job_id.as_uuid())
                .fetch_one(transaction.connection())
                .await
                .map_err(storage)?;
        permit.commit().await?;
        match aggregate.as_str() {
            "waiting_for_result_keys" => Ok(EmbeddingOutputKeyProgress::WaitingForResultKeys),
            "prepared" => Ok(EmbeddingOutputKeyProgress::Prepared { receipt }),
            _ => Err(ApplicationError::Storage(
                "output key progress authority returned an unknown state".into(),
            )),
        }
    }

    async fn record_retirement(
        &self,
        context: &RequestContext,
        binding: &EmbeddingOutputKeyBinding,
        receipt: ErasureReceipt,
    ) -> Result<(), ApplicationError> {
        assert_context(context, binding)?;
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = permit
            .unit_of_work_mut()
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".into()))?;
        sqlx::query("SELECT vestrace_record_embedding_output_key_retirement($1,$2,$3)")
            .bind(context.workspace_id.as_uuid())
            .bind(binding.intent_id.as_uuid())
            .bind(receipt.as_uuid())
            .execute(transaction.connection())
            .await
            .map_err(storage)?;
        permit.commit().await
    }
}

#[derive(sqlx::FromRow)]
struct DeliveryAcceptanceGateRow {
    receipt_id: uuid::Uuid,
    job_id: uuid::Uuid,
    accepted_outputs: serde_json::Value,
    created: bool,
}

struct DeliveryAcceptanceAuditMutation;

#[async_trait]
impl GovernedMutationApply for DeliveryAcceptanceAuditMutation {
    async fn apply(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

fn assert_context(
    context: &RequestContext,
    binding: &EmbeddingOutputKeyBinding,
) -> Result<(), ApplicationError> {
    if context.workspace_id == binding.workspace_id {
        Ok(())
    } else {
        Err(ApplicationError::Policy(
            "embedding output key binding belongs to another workspace".into(),
        ))
    }
}

fn storage(error: sqlx::Error) -> ApplicationError {
    let code = error
        .as_database_error()
        .and_then(|database| database.code())
        .map(|code| code.into_owned());
    if code.as_deref() == Some("23514")
        && error.as_database_error().is_some_and(|database| {
            database.message()
                == "an existing embedding job cannot be backfilled with guessed outputs"
        })
    {
        return ApplicationError::Policy("EMBEDDING_JOB_ACCEPTANCE_REFUSED".into());
    }
    match code.as_deref() {
        Some("22023") | Some("23514") | Some("42501") => {
            ApplicationError::Policy("EMBEDDING_OUTPUT_KEY_REFUSED".into())
        }
        Some("23505") | Some("40001") => {
            ApplicationError::Conflict("EMBEDDING_OUTPUT_KEY_CONFLICT".into())
        }
        _ => ApplicationError::Storage(error.to_string()),
    }
}
