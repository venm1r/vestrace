//! One-transaction PostgreSQL preparation of a provider dispatch.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use vestrace_application::run::WorkItem;
use vestrace_application::{
    ApplicationError, ConnectionAuth, CredentialDispatchLeaseRepository,
    CredentialDispatchLeaseRequest, EmbeddingJobAttemptRecovery, ExternalEffectRepository,
    GovernedMutation, GovernedMutationRepository, InstallationMutationPermit,
    ModelDataPolicyDecisionRepository, ModelRequestEvidenceRepository, ModelRequestReconstruction,
    NoProviderDispatchFaults, PROVIDER_ADMISSION_CONFLICT, PROVIDER_RESULT_CONFLICT, PermitMode,
    ProviderDispatchAuthority, ProviderDispatchCause, ProviderDispatchCredential,
    ProviderDispatchFaultInjector, ProviderDispatchFaultPoint, ProviderDispatchGovernedApply,
    ProviderDispatchOutcome, ProviderDispatchPolicyEvaluator, ProviderDispatchRepository,
    ProviderDispatchRequest, ProviderDispatchTarget, ProviderLostDispatchRecovery,
    ProviderPostNetworkCompletion, RunStepAttemptRecovery, RunStepExecutionAttempt,
};
use vestrace_domain::embedding::UnknownEmbeddingJobKind;

use super::PgScopedTransaction;

#[derive(FromRow)]
struct AdmissionRow {
    decision: String,
    retry_after_seconds: Option<i32>,
    concurrency_lease_id: Option<uuid::Uuid>,
    wait_deadline_at: Option<DateTime<Utc>>,
    dispatch_expires_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct RoutingRow {
    branch: String,
    credential_revision_id: Option<uuid::Uuid>,
    credential_slot_id: Option<uuid::Uuid>,
    credential_activation_guard_id: Option<uuid::Uuid>,
    expected_slot_version: Option<i64>,
    no_auth_binding_revision_id: Option<uuid::Uuid>,
    runtime_base_url: String,
    auth_mode: String,
    revision_credential_slot_id: Option<uuid::Uuid>,
}

#[derive(FromRow)]
struct ConnectionTargetRow {
    kind: String,
    runtime_base_url: String,
}

#[derive(FromRow)]
struct CompletionCauseRow {
    cause_kind: String,
    qualification_job_id: Option<uuid::Uuid>,
    qualification_probe_ordinal: Option<String>,
}

#[derive(FromRow)]
struct ProviderResultPublicationAuthorityRow {
    external_effect_id: uuid::Uuid,
    phase: String,
    has_receipt: bool,
    has_result_preparation: bool,
}

#[derive(FromRow)]
struct RunStepAttemptRecoveryAuthorityRow {
    attempt_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    run_id: uuid::Uuid,
    step_id: uuid::Uuid,
    model_binding_snapshot_id: uuid::Uuid,
    input_material_intent_id: uuid::Uuid,
    input_content_material_id: uuid::Uuid,
    input_material_key_id: uuid::Uuid,
    input_intent_nonce: uuid::Uuid,
    input_prepared_attachment_id: uuid::Uuid,
    external_effect_id: uuid::Uuid,
    model_request_evidence_id: uuid::Uuid,
    phase: String,
    authorization_id: Option<uuid::Uuid>,
    connection_id: Option<uuid::Uuid>,
    connection_revision_id: Option<uuid::Uuid>,
    concurrency_lease_id: Option<uuid::Uuid>,
    credential_lease_id: Option<uuid::Uuid>,
    dispatch_transition_id: Option<uuid::Uuid>,
    dispatch_expires_at: Option<DateTime<Utc>>,
    has_receipt: bool,
    has_result_preparation: bool,
}

impl RunStepAttemptRecoveryAuthorityRow {
    fn attempt(&self) -> RunStepExecutionAttempt {
        RunStepExecutionAttempt {
            id: self.attempt_id,
            workspace_id: vestrace_domain::WorkspaceId::from_uuid(self.workspace_id),
            run_id: vestrace_domain::AgentRunId::from_uuid(self.run_id),
            step_id: vestrace_domain::RunStepId::from_uuid(self.step_id),
            model_binding_snapshot_id: self.model_binding_snapshot_id,
            input_material_intent_id: vestrace_domain::MaterialKeyCreationIntentId::from_uuid(
                self.input_material_intent_id,
            ),
            input_content_material_id: vestrace_domain::ContentMaterialId::from_uuid(
                self.input_content_material_id,
            ),
            input_material_key_id: vestrace_domain::MaterialKeyId::from_uuid(
                self.input_material_key_id,
            ),
            input_intent_nonce: vestrace_domain::IntentNonce::from_uuid(self.input_intent_nonce),
            input_prepared_attachment_id: vestrace_domain::PreparedMaterialAttachmentId::from_uuid(
                self.input_prepared_attachment_id,
            ),
            external_effect_id: vestrace_domain::ExternalEffectId::from_uuid(
                self.external_effect_id,
            ),
            model_request_evidence_id: vestrace_domain::ModelRequestEvidenceId::from_uuid(
                self.model_request_evidence_id,
            ),
        }
    }

    fn authority(&self) -> Result<ProviderDispatchAuthority, ApplicationError> {
        Ok(ProviderDispatchAuthority {
            effect_id: vestrace_domain::ExternalEffectId::from_uuid(self.external_effect_id),
            authorization_id: vestrace_domain::PolicyDecisionId::from_uuid(
                self.authorization_id
                    .ok_or_else(run_step_attempt_recovery_refused)?,
            ),
            connection_id: vestrace_domain::ConnectionId::from_uuid(
                self.connection_id
                    .ok_or_else(run_step_attempt_recovery_refused)?,
            ),
            connection_revision_id: vestrace_domain::ConnectionRevisionId::from_uuid(
                self.connection_revision_id
                    .ok_or_else(run_step_attempt_recovery_refused)?,
            ),
            concurrency_lease_id: self
                .concurrency_lease_id
                .ok_or_else(run_step_attempt_recovery_refused)?,
            credential_lease_id: self.credential_lease_id,
            dispatch_transition_id: vestrace_domain::ExternalEffectLifecycleTransitionId::from_uuid(
                self.dispatch_transition_id
                    .ok_or_else(run_step_attempt_recovery_refused)?,
            ),
            dispatch_expires_at: self
                .dispatch_expires_at
                .ok_or_else(run_step_attempt_recovery_refused)?,
        })
    }

    fn has_admission_authority(&self) -> bool {
        self.authorization_id.is_some()
            && self.connection_id.is_some()
            && self.connection_revision_id.is_some()
            && self.concurrency_lease_id.is_some()
    }
}

/// Composition root for the Task 10C transaction. Every collaborator exposes
/// a caller-owned-transaction entry point; none may commit independently here.
pub struct PgProviderDispatchRepository {
    permit: Arc<dyn InstallationMutationPermit>,
    evidence: Arc<dyn ModelRequestEvidenceRepository>,
    effects: Arc<dyn ExternalEffectRepository>,
    credential_leases: Arc<dyn CredentialDispatchLeaseRepository>,
    data_policy: Arc<dyn ModelDataPolicyDecisionRepository>,
    governed: Arc<dyn GovernedMutationRepository<ProviderDispatchGovernedApply>>,
    policy: Arc<dyn ProviderDispatchPolicyEvaluator>,
    faults: Arc<dyn ProviderDispatchFaultInjector>,
}

impl PgProviderDispatchRepository {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        permit: Arc<dyn InstallationMutationPermit>,
        evidence: Arc<dyn ModelRequestEvidenceRepository>,
        effects: Arc<dyn ExternalEffectRepository>,
        credential_leases: Arc<dyn CredentialDispatchLeaseRepository>,
        data_policy: Arc<dyn ModelDataPolicyDecisionRepository>,
        governed: Arc<dyn GovernedMutationRepository<ProviderDispatchGovernedApply>>,
        policy: Arc<dyn ProviderDispatchPolicyEvaluator>,
    ) -> Self {
        Self::with_faults(
            permit,
            evidence,
            effects,
            credential_leases,
            data_policy,
            governed,
            policy,
            Arc::new(NoProviderDispatchFaults),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_faults(
        permit: Arc<dyn InstallationMutationPermit>,
        evidence: Arc<dyn ModelRequestEvidenceRepository>,
        effects: Arc<dyn ExternalEffectRepository>,
        credential_leases: Arc<dyn CredentialDispatchLeaseRepository>,
        data_policy: Arc<dyn ModelDataPolicyDecisionRepository>,
        governed: Arc<dyn GovernedMutationRepository<ProviderDispatchGovernedApply>>,
        policy: Arc<dyn ProviderDispatchPolicyEvaluator>,
        faults: Arc<dyn ProviderDispatchFaultInjector>,
    ) -> Self {
        Self {
            permit,
            evidence,
            effects,
            credential_leases,
            data_policy,
            governed,
            policy,
            faults,
        }
    }

    fn fault(&self, point: ProviderDispatchFaultPoint) -> Result<(), ApplicationError> {
        self.faults.check(point)
    }
}

#[async_trait]
impl ProviderDispatchRepository for PgProviderDispatchRepository {
    async fn load_run_step_attempt_in(
        &self,
        context: &vestrace_application::RequestContext,
        unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        run_id: vestrace_domain::AgentRunId,
        step_id: vestrace_domain::RunStepId,
    ) -> Result<Option<RunStepExecutionAttempt>, ApplicationError> {
        // A plain workspace-scoped read of the row the guarded reserver owns.
        // The runtime role already holds SELECT on this table; nothing here
        // mutates, so it needs no guarded function of its own.
        let row = sqlx::query_as::<_, RunStepAttemptRow>(
            "SELECT id, workspace_id, run_id, step_id, model_binding_snapshot_id,
                    input_material_intent_id, input_content_material_id,
                    input_material_key_id, input_intent_nonce,
                    input_prepared_attachment_id, external_effect_id,
                    model_request_evidence_id
               FROM run_step_execution_attempts
              WHERE workspace_id=$1 AND run_id=$2 AND step_id=$3",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .bind(step_id.as_uuid())
        .fetch_optional(postgres_transaction(unit_of_work)?.connection())
        .await
        .map_err(storage_error)?;
        Ok(row.map(|row| RunStepExecutionAttempt {
            id: row.id,
            workspace_id: vestrace_domain::WorkspaceId::from_uuid(row.workspace_id),
            run_id: vestrace_domain::AgentRunId::from_uuid(row.run_id),
            step_id: vestrace_domain::RunStepId::from_uuid(row.step_id),
            model_binding_snapshot_id: row.model_binding_snapshot_id,
            input_material_intent_id: vestrace_domain::MaterialKeyCreationIntentId::from_uuid(
                row.input_material_intent_id,
            ),
            input_content_material_id: vestrace_domain::ContentMaterialId::from_uuid(
                row.input_content_material_id,
            ),
            input_material_key_id: vestrace_domain::MaterialKeyId::from_uuid(
                row.input_material_key_id,
            ),
            input_intent_nonce: vestrace_domain::IntentNonce::from_uuid(row.input_intent_nonce),
            input_prepared_attachment_id: vestrace_domain::PreparedMaterialAttachmentId::from_uuid(
                row.input_prepared_attachment_id,
            ),
            external_effect_id: vestrace_domain::ExternalEffectId::from_uuid(
                row.external_effect_id,
            ),
            model_request_evidence_id: vestrace_domain::ModelRequestEvidenceId::from_uuid(
                row.model_request_evidence_id,
            ),
        }))
    }

    async fn load_run_step_dispatch_plan(
        &self,
        context: &vestrace_application::RequestContext,
        run_id: vestrace_domain::AgentRunId,
        step_id: vestrace_domain::RunStepId,
    ) -> Result<vestrace_application::RunStepDispatchPlan, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let attempt = self
            .load_run_step_attempt_in(context, permit.unit_of_work_mut(), run_id, step_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Conflict(
                    "the Run step has no reserved governed execution attempt".to_owned(),
                )
            })?;
        let transaction = postgres_transaction(permit.unit_of_work_mut())?;
        // The pinned snapshot is the whole routing decision. Reading it here
        // rather than accepting it from the work item is what stops a replayed
        // or forged item from naming a different Connection for the same step.
        let routing = sqlx::query_as::<_, RunStepRoutingRow>(
            "SELECT snapshot.connection_id, snapshot.connection_revision_id, snapshot.branch,
                    snapshot.credential_revision_id, snapshot.credential_slot_id,
                    snapshot.credential_activation_guard_id, revision.runtime_base_url,
                    revision.auth_mode
               FROM model_binding_snapshots AS snapshot
               JOIN connection_revisions AS revision
                 ON revision.workspace_id=snapshot.workspace_id
                AND revision.id=snapshot.connection_revision_id
                AND revision.connection_id=snapshot.connection_id
              WHERE snapshot.workspace_id=$1 AND snapshot.id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(attempt.model_binding_snapshot_id)
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?
        .ok_or_else(|| {
            ApplicationError::Conflict(
                "the Run-step attempt names a model binding snapshot that is not current"
                    .to_owned(),
            )
        })?;
        let payload: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT payload FROM external_effect_intents WHERE id=$1 AND workspace_id=$2",
        )
        .bind(attempt.external_effect_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        permit.commit().await?;

        let intent: vestrace_domain::ExternalEffectIntent =
            serde_json::from_value(payload.ok_or_else(|| {
                ApplicationError::Conflict(
                    "the Run-step attempt has no persisted effect intent to dispatch".to_owned(),
                )
            })?)
            .map_err(|_| {
                ApplicationError::Storage("stored Run-step effect intent is malformed".to_owned())
            })?;
        // The stored payload is the authority for what is dispatched, so its
        // indexed identity is re-proved rather than trusted: a row whose JSON
        // disagrees with its own key would otherwise dispatch one effect under
        // another effect's admission.
        if intent.id() != attempt.external_effect_id
            || intent.workspace_id() != context.workspace_id
        {
            return Err(ApplicationError::Storage(
                "stored Run-step effect intent indexed fields disagree".to_owned(),
            ));
        }

        let credential = match routing.branch.as_str() {
            "no_auth" => None,
            "credential" => Some(vestrace_application::ProviderDispatchCredential {
                lease_id: uuid::Uuid::now_v7(),
                credential_slot_id: vestrace_domain::CredentialSlotId::from_uuid(
                    routing.credential_slot_id.ok_or_else(|| {
                        ApplicationError::Policy(
                            "the pinned credential binding lacks its slot".to_owned(),
                        )
                    })?,
                ),
                credential_revision_id: routing.credential_revision_id.ok_or_else(|| {
                    ApplicationError::Policy(
                        "the pinned credential binding lacks its revision".to_owned(),
                    )
                })?,
                credential_activation_guard_id: routing.credential_activation_guard_id.ok_or_else(
                    || {
                        ApplicationError::Policy(
                            "the pinned credential binding lacks its activation guard".to_owned(),
                        )
                    },
                )?,
                destination_authority: super::qualification_job_repository::destination_authority(
                    &routing.runtime_base_url,
                )?,
                auth_mode: routing.auth_mode.clone(),
            }),
            _ => {
                return Err(ApplicationError::Policy(
                    "the pinned model binding snapshot has an unknown auth branch".to_owned(),
                ));
            }
        };

        Ok(vestrace_application::RunStepDispatchPlan {
            attempt,
            intent,
            connection_id: vestrace_domain::ConnectionId::from_uuid(routing.connection_id),
            connection_revision_id: vestrace_domain::ConnectionRevisionId::from_uuid(
                routing.connection_revision_id,
            ),
            credential,
        })
    }

    async fn reserve_run_step_attempt_in(
        &self,
        _context: &vestrace_application::RequestContext,
        unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        attempt: &RunStepExecutionAttempt,
    ) -> Result<(), ApplicationError> {
        let returned_id: uuid::Uuid = sqlx::query_scalar(
            "SELECT vestrace_reserve_run_step_execution_attempt(\
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(attempt.id)
        .bind(attempt.workspace_id.as_uuid())
        .bind(attempt.run_id.as_uuid())
        .bind(attempt.step_id.as_uuid())
        .bind(attempt.model_binding_snapshot_id)
        .bind(attempt.input_material_intent_id.as_uuid())
        .bind(attempt.input_content_material_id.as_uuid())
        .bind(attempt.input_material_key_id.as_uuid())
        .bind(attempt.input_intent_nonce.as_uuid())
        .bind(attempt.input_prepared_attachment_id.as_uuid())
        .bind(attempt.external_effect_id.as_uuid())
        .bind(attempt.model_request_evidence_id.as_uuid())
        .fetch_one(postgres_transaction(unit_of_work)?.connection())
        .await
        .map_err(storage_error)?;
        if returned_id != attempt.id {
            return Err(ApplicationError::Conflict(
                "Run-step attempt reservation returned a substituted identity".to_owned(),
            ));
        }
        Ok(())
    }

    async fn enqueue_run_step_after_input_ready(
        &self,
        context: &vestrace_application::RequestContext,
        attempt: &RunStepExecutionAttempt,
        item: &WorkItem,
    ) -> Result<(), ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        sqlx::query("SELECT vestrace_enqueue_run_step_after_input_ready($1,$2,$3,$4,$5,$6,$7)")
            .bind(attempt.workspace_id.as_uuid())
            .bind(attempt.run_id.as_uuid())
            .bind(attempt.step_id.as_uuid())
            .bind(attempt.id)
            .bind(item.id.as_uuid())
            .bind(item.expected_run_version.value() as i64)
            .bind(&item.idempotency_key)
            .execute(postgres_transaction(permit.unit_of_work_mut())?.connection())
            .await
            .map_err(storage_error)?;
        permit.commit().await
    }

    async fn prepare_dispatch(
        &self,
        input: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchOutcome, ApplicationError> {
        let ProviderDispatchRequest {
            context,
            intent,
            model_request_evidence_id,
            connection_id,
            connection_revision_id,
            cause,
            admission_id,
            wait_id,
            concurrency_lease_id,
            dispatch_ttl_seconds,
            worker_id,
            credential,
            audit,
            idempotency,
            outbox,
        } = input;
        if intent.workspace_id() != context.workspace_id
            || intent.actor_id() != context.principal_id
            || !(1..=900).contains(&dispatch_ttl_seconds)
        {
            return Err(ApplicationError::Policy(
                "provider dispatch request is not bound to its request context".to_owned(),
            ));
        }
        let effect_id = intent.id();
        let mut permit = self.permit.acquire(PermitMode::Shared, &context).await?;

        let target = validate_and_lock_routing(
            permit.unit_of_work_mut(),
            &context,
            effect_id,
            model_request_evidence_id,
            connection_id,
            connection_revision_id,
            &cause,
            credential.as_ref(),
        )
        .await?;

        let (effective_request, q1_request) =
            if matches!(cause, ProviderDispatchCause::QualificationProbe { .. }) {
                let request = self
                    .evidence
                    .reconstruct_q1_in(
                        permit.unit_of_work_mut(),
                        context.workspace_id,
                        model_request_evidence_id,
                    )
                    .await?;
                // Qualification has no governed user content. The existing policy
                // interface still needs a structural request carrier to issue its
                // authorization record; the adapter receives only `q1_request`.
                (
                    vestrace_application::EffectiveModelRequest::models_list(),
                    Some(Box::new(request)),
                )
            } else {
                let request = match self
                    .evidence
                    .reconstruct_current_in(
                        permit.unit_of_work_mut(),
                        context.workspace_id,
                        model_request_evidence_id,
                    )
                    .await?
                {
                    ModelRequestReconstruction::Complete(request) => request,
                    ModelRequestReconstruction::Incomplete(_) => {
                        return Err(ApplicationError::Conflict(
                            vestrace_application::MODEL_REQUEST_EVIDENCE_INCOMPLETE.to_owned(),
                        ));
                    }
                    ModelRequestReconstruction::Expired => {
                        return Err(ApplicationError::Conflict(
                            vestrace_application::MODEL_REQUEST_EVIDENCE_CONFLICT.to_owned(),
                        ));
                    }
                };
                (request, None)
            };

        let evaluation = self
            .policy
            .evaluate(&context, &intent, &cause, &target, &effective_request)
            .await?;
        validate_evaluation(&context, &intent, &cause, &evaluation)?;
        let is_allowed = evaluation.authorization.is_allowed();

        if !is_allowed {
            if let Some(record) = evaluation.model_data_policy.as_ref() {
                self.fault(ProviderDispatchFaultPoint::BeforePolicyRecord)?;
                self.data_policy
                    .record_in(permit.unit_of_work_mut(), record)
                    .await?;
                self.fault(ProviderDispatchFaultPoint::AfterPolicyRecord)?;
            }
            self.fault(ProviderDispatchFaultPoint::BeforeIntent)?;
            self.effects
                .save_intent_in(&context, permit.unit_of_work_mut(), &intent)
                .await?;
            self.fault(ProviderDispatchFaultPoint::AfterIntent)?;
            self.fault(ProviderDispatchFaultPoint::BeforeAuthorization)?;
            self.effects
                .record_authorization_in(
                    &context,
                    permit.unit_of_work_mut(),
                    effect_id,
                    &evaluation.authorization,
                )
                .await?;
            self.fault(ProviderDispatchFaultPoint::AfterAuthorization)?;
            let authorization_id = evaluation.authorization.id;
            self.fault(ProviderDispatchFaultPoint::BeforeGovernedCommit)?;
            self.governed
                .commit_in(
                    permit.unit_of_work_mut(),
                    GovernedMutation {
                        context: context.clone(),
                        audit,
                        idempotency,
                        outbox,
                        apply: ProviderDispatchGovernedApply,
                    },
                )
                .await?;
            self.fault(ProviderDispatchFaultPoint::AfterGovernedCommit)?;
            permit.commit().await?;
            return Ok(ProviderDispatchOutcome::Denied { authorization_id });
        }

        self.fault(ProviderDispatchFaultPoint::BeforeAdmission)?;
        let admission = admit(
            permit.unit_of_work_mut(),
            &context,
            admission_id,
            wait_id,
            concurrency_lease_id,
            connection_id,
            connection_revision_id,
            effect_id,
            model_request_evidence_id,
            &cause,
            dispatch_ttl_seconds,
        )
        .await?;
        self.fault(ProviderDispatchFaultPoint::AfterAdmission)?;

        if admission.decision != "admitted" {
            if admission.decision != "conflict" && admission.decision != "throttled" {
                return Err(ApplicationError::Conflict(
                    PROVIDER_ADMISSION_CONFLICT.to_owned(),
                ));
            }
            let retry_after_seconds = admission
                .retry_after_seconds
                .map(u32::try_from)
                .transpose()
                .map_err(|_| {
                    ApplicationError::Storage(
                        "provider admission returned a negative retry interval".to_owned(),
                    )
                })?;
            permit.commit().await?;
            return Ok(ProviderDispatchOutcome::Conflict {
                wait_deadline_at: admission.wait_deadline_at,
                retry_after_seconds,
            });
        }
        let admitted_lease_id = admission.concurrency_lease_id.ok_or_else(|| {
            ApplicationError::Storage(
                "admitted provider dispatch returned no concurrency lease".to_owned(),
            )
        })?;
        let dispatch_expires_at = admission.dispatch_expires_at.ok_or_else(|| {
            ApplicationError::Storage(
                "admitted provider dispatch returned no dispatch deadline".to_owned(),
            )
        })?;
        if admitted_lease_id != concurrency_lease_id {
            return Err(ApplicationError::Conflict(
                PROVIDER_ADMISSION_CONFLICT.to_owned(),
            ));
        }

        if let Some(record) = evaluation.model_data_policy.as_ref() {
            self.fault(ProviderDispatchFaultPoint::BeforePolicyRecord)?;
            self.data_policy
                .record_in(permit.unit_of_work_mut(), record)
                .await?;
            self.fault(ProviderDispatchFaultPoint::AfterPolicyRecord)?;
        }
        self.fault(ProviderDispatchFaultPoint::BeforeIntent)?;
        self.effects
            .save_intent_in(&context, permit.unit_of_work_mut(), &intent)
            .await?;
        self.fault(ProviderDispatchFaultPoint::AfterIntent)?;
        self.fault(ProviderDispatchFaultPoint::BeforeAuthorization)?;
        self.effects
            .record_authorization_in(
                &context,
                permit.unit_of_work_mut(),
                effect_id,
                &evaluation.authorization,
            )
            .await?;
        self.fault(ProviderDispatchFaultPoint::AfterAuthorization)?;

        let (credential_lease_id, auth) = if let Some(credential) = credential {
            self.fault(ProviderDispatchFaultPoint::BeforeCredentialIssue)?;
            let lease = self
                .credential_leases
                .issue_in(
                    &context,
                    permit.unit_of_work_mut(),
                    CredentialDispatchLeaseRequest {
                        lease_id: credential.lease_id,
                        connection_id,
                        external_effect_id: effect_id.as_uuid(),
                        authorization_id: evaluation.authorization.id.as_uuid(),
                        credential_slot_id: credential.credential_slot_id,
                        credential_revision_id: credential.credential_revision_id,
                        credential_activation_guard_id: credential.credential_activation_guard_id,
                        destination_authority: credential.destination_authority,
                        auth_mode: credential.auth_mode,
                        expires_at: dispatch_expires_at,
                    },
                )
                .await?;
            self.fault(ProviderDispatchFaultPoint::AfterCredentialIssue)?;
            self.fault(ProviderDispatchFaultPoint::BeforeCredentialConsume)?;
            let auth = self
                .credential_leases
                .consume_for_dispatch(&context, permit.unit_of_work_mut(), &lease)
                .await?;
            self.faults.observe_auth(&auth);
            self.fault(ProviderDispatchFaultPoint::AfterCredentialConsume)?;
            (Some(lease.id), auth)
        } else {
            (None, ConnectionAuth::None)
        };

        advance_run_step_execution_attempt_in(
            permit.unit_of_work_mut(),
            &context,
            &cause,
            "admitted",
        )
        .await?;

        let recorded_at = database_now(permit.unit_of_work_mut()).await?;
        self.fault(ProviderDispatchFaultPoint::BeforeDispatching)?;
        let dispatch_transition_id = self
            .effects
            .record_dispatch_started_in(
                &context,
                permit.unit_of_work_mut(),
                effect_id,
                worker_id,
                dispatch_expires_at,
                recorded_at,
            )
            .await?;
        advance_run_step_execution_attempt_in(
            permit.unit_of_work_mut(),
            &context,
            &cause,
            "dispatching",
        )
        .await?;
        self.fault(ProviderDispatchFaultPoint::AfterDispatching)?;

        self.fault(ProviderDispatchFaultPoint::BeforeGovernedCommit)?;
        self.governed
            .commit_in(
                permit.unit_of_work_mut(),
                GovernedMutation {
                    context: context.clone(),
                    audit,
                    idempotency,
                    outbox,
                    apply: ProviderDispatchGovernedApply,
                },
            )
            .await?;
        self.fault(ProviderDispatchFaultPoint::AfterGovernedCommit)?;
        let authority = ProviderDispatchAuthority {
            effect_id,
            authorization_id: evaluation.authorization.id,
            connection_id,
            connection_revision_id,
            concurrency_lease_id: admitted_lease_id,
            credential_lease_id,
            dispatch_transition_id,
            dispatch_expires_at,
        };
        permit.commit().await?;
        Ok(ProviderDispatchOutcome::Prepared {
            authority: Box::new(authority),
            target,
            q1_request,
            request: effective_request,
            auth,
        })
    }

    async fn complete_post_network(
        &self,
        context: &vestrace_application::RequestContext,
        completion: ProviderPostNetworkCompletion,
    ) -> Result<(), ApplicationError> {
        if completion.receipt.effect_id() != completion.authority.effect_id {
            return Err(ApplicationError::Policy(
                "provider completion receipt is not bound to its dispatch effect".to_owned(),
            ));
        }
        if let Some(throttle) = completion.throttle {
            if completion.receipt.response_class() != "http_429"
                || !matches!(
                    completion.receipt.outcome_status(),
                    vestrace_domain::EffectLifecycleStatus::Acknowledged
                        | vestrace_domain::EffectLifecycleStatus::Failed
                )
                || throttle.retry_after_seconds == 0
            {
                return Err(ApplicationError::Policy(
                    "provider throttle is not bound to an exact 429 receipt".to_owned(),
                ));
            }
        }

        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        completion_authority_is_original(permit.unit_of_work_mut(), context, &completion.authority)
            .await?;
        self.effects
            .insert_receipt_in(context, permit.unit_of_work_mut(), &completion.receipt)
            .await?;
        release_provider_dispatch_in(
            permit.unit_of_work_mut(),
            context,
            &completion.authority,
            completion.receipt.id(),
        )
        .await?;
        if let Some(throttle) = completion.throttle {
            let transaction = postgres_transaction(permit.unit_of_work_mut())?;
            sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT vestrace_record_provider_throttle($1,$2,$3,$4,$5)",
            )
            .bind(throttle.observation_id)
            .bind(context.workspace_id.as_uuid())
            .bind(completion.authority.effect_id.as_uuid())
            .bind(completion.receipt.id().as_uuid())
            .bind(i32::from(throttle.retry_after_seconds))
            .fetch_one(transaction.connection())
            .await
            .map_err(map_completion_error)?;
        }
        permit.commit().await
    }

    async fn release_after_provider_result_in(
        &self,
        context: &vestrace_application::RequestContext,
        unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        authority: &ProviderDispatchAuthority,
        receipt: &vestrace_domain::ExternalEffectReceipt,
    ) -> Result<(), ApplicationError> {
        if receipt.effect_id() != authority.effect_id
            || !matches!(
                receipt.outcome_status(),
                vestrace_domain::EffectLifecycleStatus::Acknowledged
            )
        {
            return Err(ApplicationError::Policy(
                "provider result release must use its acknowledged dispatch receipt".to_owned(),
            ));
        }
        completion_authority_is_original(unit_of_work, context, authority).await?;
        release_provider_dispatch_in(unit_of_work, context, authority, receipt.id()).await
    }

    async fn lock_provider_result_completion_authority_in(
        &self,
        context: &vestrace_application::RequestContext,
        unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        authority: &ProviderDispatchAuthority,
    ) -> Result<(), ApplicationError> {
        let cause = completion_authority_is_original(unit_of_work, context, authority).await?;
        if cause.cause_kind != "run_step"
            || cause.qualification_job_id.is_some()
            || cause.qualification_probe_ordinal.is_some()
        {
            return Err(ApplicationError::Policy(
                "RUN_PROVIDER_RESULT_DISPATCH_REFUSED".to_owned(),
            ));
        }
        Ok(())
    }

    async fn lock_provider_result_publication_authority_in(
        &self,
        context: &vestrace_application::RequestContext,
        unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        effect_id: vestrace_domain::ExternalEffectId,
        run_id: vestrace_domain::AgentRunId,
        step_id: vestrace_domain::RunStepId,
    ) -> Result<(), ApplicationError> {
        let transaction = postgres_transaction(unit_of_work)?;
        let row = sqlx::query_as::<_, ProviderResultPublicationAuthorityRow>(
            "SELECT external_effect_id,phase,has_receipt,has_result_preparation \
             FROM vestrace_lock_run_step_attempt_recovery_authority($1,$2,$3)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .bind(step_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(map_provider_result_publication_authority_error)?;
        if row.external_effect_id != effect_id.as_uuid()
            || !matches!(row.phase.as_str(), "result_prepared" | "published")
            || !row.has_receipt
            || !row.has_result_preparation
        {
            return Err(ApplicationError::Conflict(
                PROVIDER_RESULT_CONFLICT.to_owned(),
            ));
        }
        Ok(())
    }

    async fn recover_lost_post_network(
        &self,
        context: &vestrace_application::RequestContext,
        authority: &ProviderDispatchAuthority,
        recovered_at: DateTime<Utc>,
    ) -> Result<ProviderLostDispatchRecovery, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let cause =
            completion_authority_is_original(permit.unit_of_work_mut(), context, authority).await?;
        self.effects
            .adopt_lost_dispatch_in(
                context,
                permit.unit_of_work_mut(),
                authority.effect_id,
                authority.dispatch_transition_id,
                recovered_at,
            )
            .await?;

        let outcome = if cause.cause_kind == "qualification_probe" {
            let job_id = cause.qualification_job_id.ok_or_else(|| {
                ApplicationError::Policy(
                    "qualification completion cause is missing its job identity".to_owned(),
                )
            })?;
            let ordinal = cause.qualification_probe_ordinal.ok_or_else(|| {
                ApplicationError::Policy(
                    "qualification completion cause is missing its probe ordinal".to_owned(),
                )
            })?;
            let transaction = postgres_transaction(permit.unit_of_work_mut())?;
            sqlx::query_scalar::<_, ()>(
                "SELECT vestrace_recover_qualification_dispatch_unknown($1,$2,$3)",
            )
            .bind(context.workspace_id.as_uuid())
            .bind(job_id)
            .bind(ordinal)
            .fetch_one(transaction.connection())
            .await
            .map_err(map_completion_error)?;
            ProviderLostDispatchRecovery::QualificationInconclusive
        } else {
            ProviderLostDispatchRecovery::RunStep
        };
        permit.commit().await?;
        Ok(outcome)
    }

    async fn load_embedding_dispatch_plan(
        &self,
        context: &vestrace_application::RequestContext,
        job_id: vestrace_domain::EmbeddingJobId,
    ) -> Result<vestrace_application::EmbeddingJobDispatchPlan, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = postgres_transaction(permit.unit_of_work_mut())?;
        let job = sqlx::query_as::<_, EmbeddingJobRow>(
            "SELECT id, workspace_id, space_registration_id, kind, model_binding_snapshot_id,
                    external_effect_id, model_request_evidence_id
               FROM embedding_jobs WHERE workspace_id=$1 AND id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(job_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?
        .ok_or_else(|| {
            ApplicationError::Conflict("the embedding job has no accepted attempt".to_owned())
        })?;
        // The same read the Run-step twin performs, for the same reason: the
        // pinned snapshot is the whole routing decision, and reading it here
        // rather than accepting it from the work item is what stops a replayed
        // item from naming a different Connection for the same job.
        let routing = sqlx::query_as::<_, RunStepRoutingRow>(
            "SELECT snapshot.connection_id, snapshot.connection_revision_id, snapshot.branch,
                    snapshot.credential_revision_id, snapshot.credential_slot_id,
                    snapshot.credential_activation_guard_id, revision.runtime_base_url,
                    revision.auth_mode
               FROM model_binding_snapshots AS snapshot
               JOIN connection_revisions AS revision
                 ON revision.workspace_id=snapshot.workspace_id
                AND revision.id=snapshot.connection_revision_id
                AND revision.connection_id=snapshot.connection_id
              WHERE snapshot.workspace_id=$1 AND snapshot.id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(job.model_binding_snapshot_id)
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?
        .ok_or_else(|| {
            ApplicationError::Conflict(
                "the embedding job names a model binding snapshot that is not current".to_owned(),
            )
        })?;
        let payload: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT payload FROM external_effect_intents WHERE id=$1 AND workspace_id=$2",
        )
        .bind(job.external_effect_id)
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        permit.commit().await?;

        let intent: vestrace_domain::ExternalEffectIntent =
            serde_json::from_value(payload.ok_or_else(|| {
                ApplicationError::Conflict(
                    "the embedding job has no persisted effect intent to dispatch".to_owned(),
                )
            })?)
            .map_err(|_| {
                ApplicationError::Storage(
                    "stored embedding job effect intent is malformed".to_owned(),
                )
            })?;
        if intent.id().as_uuid() != job.external_effect_id
            || intent.workspace_id() != context.workspace_id
        {
            return Err(ApplicationError::Storage(
                "stored embedding job effect intent indexed fields disagree".to_owned(),
            ));
        }

        Ok(vestrace_application::EmbeddingJobDispatchPlan {
            attempt: job.attempt(context)?,
            intent,
            connection_id: vestrace_domain::ConnectionId::from_uuid(routing.connection_id),
            connection_revision_id: vestrace_domain::ConnectionRevisionId::from_uuid(
                routing.connection_revision_id,
            ),
            credential: embedding_credential(&routing)?,
        })
    }

    async fn recover_embedding_job_attempt(
        &self,
        context: &vestrace_application::RequestContext,
        job_id: vestrace_domain::EmbeddingJobId,
        recovered_at: DateTime<Utc>,
    ) -> Result<EmbeddingJobAttemptRecovery, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = postgres_transaction(permit.unit_of_work_mut())?;
        let row = sqlx::query_as::<_, EmbeddingJobRecoveryAuthorityRow>(
            "SELECT * FROM vestrace_lock_embedding_job_recovery_authority($1,$2)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(job_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(map_embedding_job_recovery_error)?;
        if row.workspace_id != context.workspace_id.as_uuid() || row.job_id != job_id.as_uuid() {
            return Err(embedding_job_recovery_refused());
        }

        // Every arm states the whole evidence tuple it expects, not just the
        // phase. The phase is derived from that evidence, so re-stating it here
        // is what makes a derivation defect a refusal instead of a
        // misclassification.
        let outcome = match row.phase.as_str() {
            "reserved"
                if row.connection_id.is_none()
                    && row.dispatch_transition_id.is_none()
                    && !row.has_receipt
                    && !row.has_result_preparation =>
            {
                EmbeddingJobAttemptRecovery::ResumeReserved
            }
            "admitted"
                if row.connection_id.is_some()
                    && row.dispatch_transition_id.is_none()
                    && !row.has_receipt
                    && !row.has_result_preparation =>
            {
                EmbeddingJobAttemptRecovery::ResumeAdmitted
            }
            "dispatching"
                if row.connection_id.is_some()
                    && row.dispatch_transition_id.is_some()
                    && !row.has_receipt
                    && !row.has_result_preparation =>
            {
                let expires_at = row
                    .dispatch_expires_at
                    .ok_or_else(embedding_job_recovery_refused)?;
                if recovered_at < expires_at {
                    // Line 219: loss after `Dispatching` remains deadline-gated.
                    // Before the deadline there is nothing to decide and nothing
                    // to adopt.
                    EmbeddingJobAttemptRecovery::AwaitDispatchDeadline
                } else {
                    // Line 245: one recovery winner leaves the effect `Unknown`
                    // and finalizes the job `InconclusiveUnknown`. Both writes
                    // happen here, in the caller's transaction, and neither
                    // calls an adapter: whether the provider ran is exactly what
                    // is unknown, so there is nothing to ask.
                    let adoption = self
                        .effects
                        .adopt_lost_dispatch_in(
                            context,
                            permit.unit_of_work_mut(),
                            vestrace_domain::ExternalEffectId::from_uuid(row.external_effect_id),
                            vestrace_domain::ExternalEffectLifecycleTransitionId::from_uuid(
                                row.dispatch_transition_id
                                    .ok_or_else(embedding_job_recovery_refused)?,
                            ),
                            recovered_at,
                        )
                        .await?;
                    if adoption != vestrace_application::LostDispatchAdoption::Adopted {
                        return Err(embedding_job_recovery_refused());
                    }
                    let transaction = postgres_transaction(permit.unit_of_work_mut())?;
                    let finalized: i64 =
                        sqlx::query_scalar("SELECT vestrace_finalize_embedding_job_unknown($1,$2)")
                            .bind(context.workspace_id.as_uuid())
                            .bind(job_id.as_uuid())
                            .fetch_one(transaction.connection())
                            .await
                            .map_err(map_embedding_job_recovery_error)?;
                    if finalized <= 0 {
                        return Err(embedding_job_recovery_refused());
                    }
                    EmbeddingJobAttemptRecovery::AdoptedUnknown
                }
            }
            "unknown" if row.dispatch_transition_id.is_some() && !row.has_result_preparation => {
                EmbeddingJobAttemptRecovery::AlreadyUnknown
            }
            "result_prepared" if row.has_receipt && row.has_result_preparation => {
                EmbeddingJobAttemptRecovery::ResumeResultPrepared {
                    effect_id: vestrace_domain::ExternalEffectId::from_uuid(row.external_effect_id),
                }
            }
            "succeeded" if row.has_receipt && row.has_result_preparation => {
                EmbeddingJobAttemptRecovery::Succeeded
            }
            _ => return Err(embedding_job_recovery_refused()),
        };
        permit.commit().await?;
        Ok(outcome)
    }

    async fn recover_run_step_attempt(
        &self,
        context: &vestrace_application::RequestContext,
        run_id: vestrace_domain::AgentRunId,
        step_id: vestrace_domain::RunStepId,
        recovered_at: DateTime<Utc>,
    ) -> Result<RunStepAttemptRecovery, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = postgres_transaction(permit.unit_of_work_mut())?;
        let row = sqlx::query_as::<_, RunStepAttemptRecoveryAuthorityRow>(
            "SELECT * FROM vestrace_lock_run_step_attempt_recovery_authority($1,$2,$3)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .bind(step_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(map_run_step_attempt_recovery_error)?;
        let attempt = row.attempt();
        if attempt.workspace_id != context.workspace_id
            || attempt.run_id != run_id
            || attempt.step_id != step_id
        {
            return Err(run_step_attempt_recovery_refused());
        }

        let outcome = match row.phase.as_str() {
            "reserved"
                if !row.has_admission_authority()
                    && row.dispatch_transition_id.is_none()
                    && row.dispatch_expires_at.is_none()
                    && !row.has_receipt
                    && !row.has_result_preparation =>
            {
                RunStepAttemptRecovery::ResumeReserved
            }
            "admitted"
                if row.has_admission_authority()
                    && row.dispatch_transition_id.is_none()
                    && row.dispatch_expires_at.is_none()
                    && !row.has_receipt
                    && !row.has_result_preparation =>
            {
                RunStepAttemptRecovery::ResumeAdmitted
            }
            "dispatching"
                if row.has_admission_authority()
                    && row.dispatch_transition_id.is_some()
                    && row.dispatch_expires_at.is_some()
                    && !row.has_receipt
                    && !row.has_result_preparation =>
            {
                let authority = row.authority()?;
                if recovered_at < authority.dispatch_expires_at {
                    RunStepAttemptRecovery::AwaitDispatchDeadline
                } else {
                    let cause = completion_authority_is_original(
                        permit.unit_of_work_mut(),
                        context,
                        &authority,
                    )
                    .await?;
                    if cause.cause_kind != "run_step"
                        || cause.qualification_job_id.is_some()
                        || cause.qualification_probe_ordinal.is_some()
                    {
                        return Err(run_step_attempt_recovery_refused());
                    }
                    let adoption = self
                        .effects
                        .adopt_lost_dispatch_in(
                            context,
                            permit.unit_of_work_mut(),
                            authority.effect_id,
                            authority.dispatch_transition_id,
                            recovered_at,
                        )
                        .await?;
                    if adoption != vestrace_application::LostDispatchAdoption::Adopted {
                        return Err(run_step_attempt_recovery_refused());
                    }
                    let transaction = postgres_transaction(permit.unit_of_work_mut())?;
                    let transitioned: uuid::Uuid = sqlx::query_scalar(
                        "SELECT vestrace_transition_run_step_execution_attempt($1,$2,$3,'unknown')",
                    )
                    .bind(context.workspace_id.as_uuid())
                    .bind(run_id.as_uuid())
                    .bind(step_id.as_uuid())
                    .fetch_one(transaction.connection())
                    .await
                    .map_err(map_run_step_attempt_recovery_error)?;
                    if transitioned != attempt.id {
                        return Err(run_step_attempt_recovery_refused());
                    }
                    RunStepAttemptRecovery::AdoptedUnknown
                }
            }
            "unknown"
                if row.has_admission_authority()
                    && row.dispatch_transition_id.is_some()
                    && row.dispatch_expires_at.is_some()
                    && !row.has_receipt
                    && !row.has_result_preparation =>
            {
                RunStepAttemptRecovery::AlreadyUnknown
            }
            "result_prepared"
                if row.has_admission_authority()
                    && row.dispatch_transition_id.is_some()
                    && row.dispatch_expires_at.is_some()
                    && row.has_receipt
                    && row.has_result_preparation =>
            {
                RunStepAttemptRecovery::ResumeResultPrepared {
                    effect_id: attempt.external_effect_id,
                }
            }
            "published"
                if row.has_admission_authority()
                    && row.dispatch_transition_id.is_some()
                    && row.dispatch_expires_at.is_some()
                    && row.has_receipt
                    && row.has_result_preparation =>
            {
                RunStepAttemptRecovery::Published
            }
            _ => return Err(run_step_attempt_recovery_refused()),
        };
        permit.commit().await?;
        Ok(outcome)
    }
}

async fn advance_run_step_execution_attempt_in(
    unit_of_work: &mut dyn vestrace_application::UnitOfWork,
    context: &vestrace_application::RequestContext,
    cause: &ProviderDispatchCause,
    target_phase: &'static str,
) -> Result<(), ApplicationError> {
    let ProviderDispatchCause::RunStep {
        run_id, step_id, ..
    } = cause
    else {
        return Ok(());
    };
    let transaction = postgres_transaction(unit_of_work)?;
    sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT vestrace_transition_run_step_execution_attempt($1,$2,$3,$4)",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .bind(step_id.as_uuid())
    .bind(target_phase)
    .fetch_one(transaction.connection())
    .await
    .map_err(map_run_step_attempt_dispatch_error)?;
    Ok(())
}

async fn release_provider_dispatch_in(
    unit_of_work: &mut dyn vestrace_application::UnitOfWork,
    context: &vestrace_application::RequestContext,
    authority: &ProviderDispatchAuthority,
    receipt_id: vestrace_domain::ExternalEffectReceiptId,
) -> Result<(), ApplicationError> {
    let transaction = postgres_transaction(unit_of_work)?;
    sqlx::query_scalar::<_, uuid::Uuid>("SELECT vestrace_release_provider_dispatch($1,$2,$3)")
        .bind(context.workspace_id.as_uuid())
        .bind(authority.effect_id.as_uuid())
        .bind(receipt_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(map_completion_error)?;
    Ok(())
}

async fn completion_authority_is_original(
    unit_of_work: &mut dyn vestrace_application::UnitOfWork,
    context: &vestrace_application::RequestContext,
    authority: &ProviderDispatchAuthority,
) -> Result<CompletionCauseRow, ApplicationError> {
    let transaction = postgres_transaction(unit_of_work)?;
    sqlx::query_as::<_, CompletionCauseRow>(
        "SELECT cause_kind, qualification_job_id, qualification_probe_ordinal
           FROM vestrace_lock_provider_dispatch_completion_authority(
                $1,$2,$3,$4,$5,$6)",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(authority.effect_id.as_uuid())
    .bind(authority.connection_id.as_uuid())
    .bind(authority.connection_revision_id.as_uuid())
    .bind(authority.dispatch_transition_id.as_uuid())
    .bind(authority.concurrency_lease_id)
    .fetch_one(transaction.connection())
    .await
    .map_err(map_completion_error)
}

/// The pinned routing a Run step inherits from its `ModelBindingSnapshot`.
///
/// Every column here is immutable once the snapshot exists, which is why the
/// worker may read it without a lock: there is no later value to race with.
#[derive(sqlx::FromRow)]
struct RunStepRoutingRow {
    connection_id: uuid::Uuid,
    connection_revision_id: uuid::Uuid,
    branch: String,
    credential_revision_id: Option<uuid::Uuid>,
    credential_slot_id: Option<uuid::Uuid>,
    credential_activation_guard_id: Option<uuid::Uuid>,
    runtime_base_url: String,
    auth_mode: String,
}

#[derive(sqlx::FromRow)]
struct EmbeddingJobRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    space_registration_id: uuid::Uuid,
    kind: String,
    model_binding_snapshot_id: uuid::Uuid,
    external_effect_id: uuid::Uuid,
    model_request_evidence_id: uuid::Uuid,
}

impl EmbeddingJobRow {
    fn attempt(
        &self,
        context: &vestrace_application::RequestContext,
    ) -> Result<vestrace_application::EmbeddingJobExecutionAttempt, ApplicationError> {
        if self.workspace_id != context.workspace_id.as_uuid() {
            return Err(ApplicationError::Storage(
                "stored embedding job belongs to another workspace".to_owned(),
            ));
        }
        // The column is a closed set in SQL and a closed enum in Rust. Parsing
        // rather than carrying the string is what keeps a value that satisfied
        // the CHECK but no variant from reaching a match site.
        let kind = self
            .kind
            .parse()
            .map_err(|error: UnknownEmbeddingJobKind| {
                ApplicationError::Storage(error.to_string())
            })?;
        Ok(vestrace_application::EmbeddingJobExecutionAttempt {
            job_id: vestrace_domain::EmbeddingJobId::from_uuid(self.id),
            workspace_id: context.workspace_id,
            space_registration_id: vestrace_domain::EmbeddingSpaceId::from_uuid(
                self.space_registration_id,
            ),
            kind,
            model_binding_snapshot_id: self.model_binding_snapshot_id,
            external_effect_id: vestrace_domain::ExternalEffectId::from_uuid(
                self.external_effect_id,
            ),
            model_request_evidence_id: vestrace_domain::ModelRequestEvidenceId::from_uuid(
                self.model_request_evidence_id,
            ),
        })
    }
}

#[derive(sqlx::FromRow)]
struct EmbeddingJobRecoveryAuthorityRow {
    job_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    #[allow(dead_code)]
    space_registration_id: uuid::Uuid,
    #[allow(dead_code)]
    kind: String,
    #[allow(dead_code)]
    state: String,
    #[allow(dead_code)]
    model_binding_snapshot_id: uuid::Uuid,
    external_effect_id: uuid::Uuid,
    #[allow(dead_code)]
    model_request_evidence_id: uuid::Uuid,
    phase: String,
    connection_id: Option<uuid::Uuid>,
    #[allow(dead_code)]
    connection_revision_id: Option<uuid::Uuid>,
    dispatch_transition_id: Option<uuid::Uuid>,
    dispatch_expires_at: Option<DateTime<Utc>>,
    has_receipt: bool,
    has_result_preparation: bool,
}

/// The pinned auth branch, read from the snapshot rather than chosen.
///
/// Shared shape with the Run-step twin because the snapshot is the same kind of
/// authority for both: `no_auth` yields `None`, and a credential branch that
/// cannot produce its slot, revision and guard is an error rather than a silent
/// downgrade to no authentication.
fn embedding_credential(
    routing: &RunStepRoutingRow,
) -> Result<Option<vestrace_application::ProviderDispatchCredential>, ApplicationError> {
    match routing.branch.as_str() {
        "no_auth" => Ok(None),
        "credential" => Ok(Some(vestrace_application::ProviderDispatchCredential {
            lease_id: uuid::Uuid::now_v7(),
            credential_slot_id: vestrace_domain::CredentialSlotId::from_uuid(
                routing.credential_slot_id.ok_or_else(|| {
                    ApplicationError::Policy(
                        "the pinned credential binding lacks its slot".to_owned(),
                    )
                })?,
            ),
            credential_revision_id: routing.credential_revision_id.ok_or_else(|| {
                ApplicationError::Policy(
                    "the pinned credential binding lacks its revision".to_owned(),
                )
            })?,
            credential_activation_guard_id: routing.credential_activation_guard_id.ok_or_else(
                || {
                    ApplicationError::Policy(
                        "the pinned credential binding lacks its activation guard".to_owned(),
                    )
                },
            )?,
            destination_authority: super::qualification_job_repository::destination_authority(
                &routing.runtime_base_url,
            )?,
            auth_mode: routing.auth_mode.clone(),
        })),
        _ => Err(ApplicationError::Policy(
            "the pinned model binding snapshot has an unknown auth branch".to_owned(),
        )),
    }
}

fn embedding_job_recovery_refused() -> ApplicationError {
    ApplicationError::Conflict(
        "the embedding job is not in a recoverable governed state".to_owned(),
    )
}

fn map_embedding_job_recovery_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("23514") | Some("22023") => embedding_job_recovery_refused(),
        Some("40001") => ApplicationError::Conflict(
            "the embedding job advanced while recovery was acquiring its lock".to_owned(),
        ),
        _ => storage_error(error),
    }
}

#[derive(sqlx::FromRow)]
struct RunStepAttemptRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    run_id: uuid::Uuid,
    step_id: uuid::Uuid,
    model_binding_snapshot_id: uuid::Uuid,
    input_material_intent_id: uuid::Uuid,
    input_content_material_id: uuid::Uuid,
    input_material_key_id: uuid::Uuid,
    input_intent_nonce: uuid::Uuid,
    input_prepared_attachment_id: uuid::Uuid,
    external_effect_id: uuid::Uuid,
    model_request_evidence_id: uuid::Uuid,
}

fn validate_evaluation(
    context: &vestrace_application::RequestContext,
    intent: &vestrace_domain::ExternalEffectIntent,
    cause: &ProviderDispatchCause,
    evaluation: &vestrace_application::ProviderDispatchPolicyEvaluation,
) -> Result<(), ApplicationError> {
    let authorization = &evaluation.authorization;
    if authorization.workspace_id != context.workspace_id
        || authorization.subject_id != context.principal_id
        || authorization.capability != intent.required_capability()
        || authorization.operation != intent.operation()
        || authorization.resource_scope != intent.target()
    {
        return Err(ApplicationError::Policy(
            "provider authorization is not bound to the exact effect".to_owned(),
        ));
    }
    match (cause, evaluation.model_data_policy.as_ref()) {
        (
            ProviderDispatchCause::RunStep {
                run_id, step_id, ..
            },
            Some(record),
        ) if record.run_id == *run_id
            && record.step_id == *step_id
            && !(authorization.is_allowed()
                && !record.allowed
                && record.mode == vestrace_application::ModelDataPolicyMode::Enforce) =>
        {
            Ok(())
        }
        (ProviderDispatchCause::QualificationProbe { .. }, None) => Ok(()),
        // An embedding job's disclosure decision is the embedding data policy's,
        // recorded by `EmbeddingDataPolicyGate` before the job is accepted. The
        // model data policy record belongs to a Run step and names its run and
        // step; requiring one here would either be absent or be another step's.
        (ProviderDispatchCause::EmbeddingJob { .. }, None) => Ok(()),
        _ => Err(ApplicationError::Policy(
            "provider policy decisions are not bound to the exact dispatch cause".to_owned(),
        )),
    }
}

#[allow(clippy::too_many_arguments)]
async fn validate_and_lock_routing(
    unit_of_work: &mut dyn vestrace_application::UnitOfWork,
    context: &vestrace_application::RequestContext,
    effect_id: vestrace_domain::ExternalEffectId,
    evidence_id: vestrace_domain::ModelRequestEvidenceId,
    connection_id: vestrace_domain::ConnectionId,
    connection_revision_id: vestrace_domain::ConnectionRevisionId,
    cause: &ProviderDispatchCause,
    credential: Option<&ProviderDispatchCredential>,
) -> Result<ProviderDispatchTarget, ApplicationError> {
    let transaction = postgres_transaction(unit_of_work)?;
    let (cause_kind, run_id, step_id, snapshot_id, job_id, target_id) = match cause {
        ProviderDispatchCause::RunStep {
            run_id,
            step_id,
            snapshot_id,
        } => (
            "run_step",
            Some(run_id.as_uuid()),
            Some(step_id.as_uuid()),
            Some(*snapshot_id),
            None,
            None,
        ),
        ProviderDispatchCause::QualificationProbe {
            qualification_job_id,
            qualification_target_id,
            probe_ordinal: _,
        } => (
            "qualification_probe",
            None,
            None,
            None,
            Some(qualification_job_id.as_uuid()),
            Some(*qualification_target_id),
        ),
        // The job id is deliberately not passed: the guarded function finds the
        // job by its own unique effect and checks that the snapshot named here
        // is the one the job pinned at acceptance.
        ProviderDispatchCause::EmbeddingJob {
            job_id: _,
            snapshot_id,
        } => ("embedding_job", None, None, Some(*snapshot_id), None, None),
    };
    let row = sqlx::query_as::<_, RoutingRow>(
        "SELECT branch, credential_revision_id, credential_slot_id,
                credential_activation_guard_id, expected_slot_version,
                no_auth_binding_revision_id, runtime_base_url, auth_mode,
                revision_credential_slot_id
           FROM vestrace_lock_provider_dispatch_routing(
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(connection_id.as_uuid())
    .bind(connection_revision_id.as_uuid())
    .bind(effect_id.as_uuid())
    .bind(evidence_id.as_uuid())
    .bind(cause_kind)
    .bind(run_id)
    .bind(step_id)
    .bind(snapshot_id)
    .bind(job_id)
    .bind(target_id)
    .fetch_one(transaction.connection())
    .await
    .map_err(map_routing_error)?;

    if let ProviderDispatchCause::QualificationProbe {
        qualification_job_id,
        probe_ordinal,
        ..
    } = cause
    {
        let pinned_ordinal: Option<String> = sqlx::query_scalar(
            "SELECT node.safe_ordinal
               FROM model_request_evidence_nodes AS node
              WHERE node.workspace_id=$1 AND node.evidence_root_id=$2
                AND node.reference_kind='qualification_probe'
                AND node.reference_id=$3",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(evidence_id.as_uuid())
        .bind(qualification_job_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(map_routing_error)?
        .flatten();
        if pinned_ordinal.as_deref() != Some(probe_ordinal.as_str()) {
            return Err(routing_refused());
        }
    }

    match (row.branch.as_str(), credential) {
        ("no_auth", None)
            if row.auth_mode == "none"
                && row.revision_credential_slot_id.is_none()
                && row.no_auth_binding_revision_id.is_some() => {}
        ("credential", Some(credential))
            if row.auth_mode != "none"
                && row.credential_revision_id == Some(credential.credential_revision_id)
                && row.credential_slot_id == Some(credential.credential_slot_id.as_uuid())
                && row.credential_activation_guard_id
                    == Some(credential.credential_activation_guard_id)
                && row.revision_credential_slot_id
                    == Some(credential.credential_slot_id.as_uuid())
                && row.auth_mode == credential.auth_mode
                && normalized_destination_authority(&row.runtime_base_url).as_deref()
                    == Some(credential.destination_authority.as_str()) =>
        {
            row.expected_slot_version
                .map(|_| ())
                .ok_or_else(routing_refused)?;
        }
        _ => return Err(routing_refused()),
    }

    // `vestrace_lock_provider_dispatch_routing` has already proved this exact
    // immutable revision belongs to the pinned snapshot/target and acquired
    // the governing lock chain.  This is a read of only the two adapter-safe
    // fields in that same transaction; it is not a mutable-head lookup.
    let target = sqlx::query_as::<_, ConnectionTargetRow>(
        "SELECT kind, runtime_base_url
           FROM connection_revisions
          WHERE workspace_id=$1 AND connection_id=$2 AND id=$3",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(connection_id.as_uuid())
    .bind(connection_revision_id.as_uuid())
    .fetch_optional(transaction.connection())
    .await
    .map_err(map_routing_error)?
    .ok_or_else(routing_refused)?;
    let kind = match target.kind.as_str() {
        "lm_studio_local" => vestrace_domain::ConnectionKind::LMStudioLocal,
        "open_ai_chat_completions_v1" => vestrace_domain::ConnectionKind::OpenAiChatCompletionsV1,
        _ => return Err(routing_refused()),
    };
    if target.runtime_base_url != row.runtime_base_url {
        return Err(routing_refused());
    }
    // The disclosure class of the pinned endpoint is settled here, in the same
    // transaction that proved the revision, so the policy evaluator never has
    // to parse a URL or acquire routing authority of its own.
    let destination =
        crate::providers::openai_compatible::pinned_destination(kind, &target.runtime_base_url)
            .ok_or_else(routing_refused)?;
    Ok(ProviderDispatchTarget {
        kind,
        runtime_base_url: target.runtime_base_url,
        destination,
    })
}

fn normalized_destination_authority(runtime_base_url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(runtime_base_url).ok()?;
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    Some(match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

fn routing_refused() -> ApplicationError {
    ApplicationError::Policy("PROVIDER_DISPATCH_ROUTING_REFUSED".to_owned())
}

fn map_routing_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("22023") | Some("23514") | Some("42501") => routing_refused(),
        _ => storage_error(error),
    }
}

#[allow(clippy::too_many_arguments)]
async fn admit(
    unit_of_work: &mut dyn vestrace_application::UnitOfWork,
    context: &vestrace_application::RequestContext,
    admission_id: uuid::Uuid,
    wait_id: uuid::Uuid,
    concurrency_lease_id: uuid::Uuid,
    connection_id: vestrace_domain::ConnectionId,
    connection_revision_id: vestrace_domain::ConnectionRevisionId,
    effect_id: vestrace_domain::ExternalEffectId,
    evidence_id: vestrace_domain::ModelRequestEvidenceId,
    cause: &ProviderDispatchCause,
    dispatch_ttl_seconds: u16,
) -> Result<AdmissionRow, ApplicationError> {
    let transaction = postgres_transaction(unit_of_work)?;
    let (cause_kind, run_id, step_id, snapshot_id, job_id, target_id, ordinal) = match cause {
        ProviderDispatchCause::RunStep {
            run_id,
            step_id,
            snapshot_id,
        } => (
            "run_step",
            Some(run_id.as_uuid()),
            Some(step_id.as_uuid()),
            Some(*snapshot_id),
            None,
            None,
            None,
        ),
        ProviderDispatchCause::QualificationProbe {
            qualification_job_id,
            qualification_target_id,
            probe_ordinal,
        } => (
            "qualification_probe",
            None,
            None,
            None,
            Some(qualification_job_id.as_uuid()),
            Some(*qualification_target_id),
            Some(probe_ordinal.as_str()),
        ),
        ProviderDispatchCause::EmbeddingJob {
            job_id: _,
            snapshot_id,
        } => (
            "embedding_job",
            None,
            None,
            Some(*snapshot_id),
            None,
            None,
            None,
        ),
    };
    sqlx::query_as::<_, AdmissionRow>(
        "SELECT decision, retry_after_seconds, concurrency_lease_id,
                wait_deadline_at, dispatch_expires_at
           FROM vestrace_try_admit_provider_dispatch(
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
    )
    .bind(admission_id)
    .bind(wait_id)
    .bind(concurrency_lease_id)
    .bind(context.workspace_id.as_uuid())
    .bind(connection_id.as_uuid())
    .bind(connection_revision_id.as_uuid())
    .bind(effect_id.as_uuid())
    .bind(evidence_id.as_uuid())
    .bind(cause_kind)
    .bind(run_id)
    .bind(step_id)
    .bind(snapshot_id)
    .bind(job_id)
    .bind(target_id)
    .bind(ordinal)
    .bind(i32::from(dispatch_ttl_seconds))
    .fetch_one(transaction.connection())
    .await
    .map_err(map_admission_error)
}

async fn database_now(
    unit_of_work: &mut dyn vestrace_application::UnitOfWork,
) -> Result<DateTime<Utc>, ApplicationError> {
    sqlx::query_scalar("SELECT NOW()")
        .fetch_one(postgres_transaction(unit_of_work)?.connection())
        .await
        .map_err(storage_error)
}

fn postgres_transaction(
    unit_of_work: &mut dyn vestrace_application::UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| {
            ApplicationError::Internal("expected PostgreSQL provider dispatch transaction".into())
        })
}

fn map_admission_error(error: sqlx::Error) -> ApplicationError {
    if error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
        == Some("23514")
    {
        ApplicationError::Conflict(PROVIDER_ADMISSION_CONFLICT.to_owned())
    } else {
        storage_error(error)
    }
}

fn map_completion_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("22023") | Some("23514") | Some("42501") => {
            ApplicationError::Policy("PROVIDER_POST_NETWORK_COMPLETION_REFUSED".to_owned())
        }
        _ => storage_error(error),
    }
}

fn map_provider_result_publication_authority_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("22023") | Some("23514") | Some("40001") | Some("42501") => {
            ApplicationError::Conflict(PROVIDER_RESULT_CONFLICT.to_owned())
        }
        _ => storage_error(error),
    }
}

fn map_run_step_attempt_dispatch_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("22023") | Some("23514") | Some("42501") => {
            ApplicationError::Policy("RUN_STEP_ATTEMPT_DISPATCH_REFUSED".to_owned())
        }
        _ => storage_error(error),
    }
}

fn run_step_attempt_recovery_refused() -> ApplicationError {
    ApplicationError::Policy("RUN_STEP_ATTEMPT_RECOVERY_REFUSED".to_owned())
}

fn map_run_step_attempt_recovery_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        // The SQL recovery seam detects a phase change while a pre-dispatch
        // reader was waiting for the attempt lock. That is a retryable
        // serialization conflict, never an opaque storage failure.
        Some("40001") => ApplicationError::Conflict("RUN_STEP_ATTEMPT_RECOVERY_RETRY".to_owned()),
        Some("22023") | Some("23514") | Some("42501") => run_step_attempt_recovery_refused(),
        _ => storage_error(error),
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
