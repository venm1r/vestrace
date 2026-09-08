//! Atomic preparation of a governed provider dispatch.
//!
//! This boundary stops at the durable `Dispatching` transition. It never calls
//! a provider adapter and it deliberately returns the single reconstructed
//! request by value only after the transaction has committed.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use vestrace_domain::{
    AgentRunId, AuthorizationRequest, ConnectionId, ConnectionKind, ConnectionRevisionId,
    ContentMaterialId, CredentialSlotId, DataDestination, EmbeddingJobId, EmbeddingSpaceId,
    ExternalEffectId, ExternalEffectIntent, ExternalEffectLifecycleTransitionId,
    ExternalEffectReceipt, IntentNonce, MaterialKeyCreationIntentId, MaterialKeyId,
    ModelRequestEvidenceId, PolicyDecision, PolicyDecisionId, PolicyDecisionReason,
    PolicyDecisionResult, PreparedMaterialAttachmentId, PrincipalId, QualificationJobId, RunStepId,
    WorkerId, WorkspaceId,
    embedding::EmbeddingJobKind,
    run::{RunActorRef, RunVersion},
    trust::{DataClassification, evaluate_model_boundary},
};

use crate::run::{CommitRun, ConfidentialRunInput, RunSnapshot, RunStorePort, WorkItem};
use crate::{
    ApplicationError, AuditEntry, ConnectionAuth, EffectiveModelRequest, GovernedMutationApply,
    IdempotencyRecord, ModelDataPolicyDecisionRecord, ModelDataPolicyMode, ModelDataPolicySettings,
    OutboxMessage, Q1ProbeRequest, RequestContext, SharedPolicyDecisionEngine, UnitOfWork,
};
use crate::{MaterialIntentRepository, TransactionManager};
use vestrace_domain::{
    MaterialKeyCreationIntent,
    run::{RunEvent, RunEventPayload, RunStep},
};

pub const PROVIDER_ADMISSION_CONFLICT: &str = "PROVIDER_ADMISSION_CONFLICT";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderDispatchCause {
    RunStep {
        run_id: AgentRunId,
        step_id: RunStepId,
        snapshot_id: uuid::Uuid,
    },
    QualificationProbe {
        qualification_job_id: QualificationJobId,
        qualification_target_id: uuid::Uuid,
        probe_ordinal: String,
    },
    /// Spec line 219: an embedding job is "the durable non-Run owner for
    /// production Embeddings", so it is a third cause of the same dispatch
    /// rather than a second dispatch path.
    ///
    /// It carries no run, step or probe. `job_id` is here because Rust callers
    /// legitimately know which job they are dispatching; the guarded SQL does
    /// not take it, deriving the job from its own unique effect instead, so the
    /// worker cannot name a job the effect does not belong to.
    EmbeddingJob {
        job_id: EmbeddingJobId,
        snapshot_id: uuid::Uuid,
    },
}

#[derive(Clone, Debug)]
pub struct ProviderDispatchCredential {
    pub lease_id: uuid::Uuid,
    pub credential_slot_id: CredentialSlotId,
    pub credential_revision_id: uuid::Uuid,
    pub credential_activation_guard_id: uuid::Uuid,
    pub destination_authority: String,
    pub auth_mode: String,
}

pub struct ProviderDispatchRequest {
    pub context: RequestContext,
    pub intent: ExternalEffectIntent,
    pub model_request_evidence_id: ModelRequestEvidenceId,
    pub connection_id: ConnectionId,
    pub connection_revision_id: ConnectionRevisionId,
    pub cause: ProviderDispatchCause,
    pub admission_id: uuid::Uuid,
    pub wait_id: uuid::Uuid,
    pub concurrency_lease_id: uuid::Uuid,
    pub dispatch_ttl_seconds: u16,
    pub worker_id: WorkerId,
    pub credential: Option<ProviderDispatchCredential>,
    pub audit: AuditEntry,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
}

pub struct ProviderDispatchPolicyEvaluation {
    pub authorization: PolicyDecision,
    /// Run-step model disclosure has one dedicated durable decision. Structural
    /// qualification probes have no Run/step identity and therefore no row.
    pub model_data_policy: Option<ModelDataPolicyDecisionRecord>,
}

#[async_trait]
pub trait ProviderDispatchPolicyEvaluator: Send + Sync {
    async fn evaluate(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
        cause: &ProviderDispatchCause,
        target: &ProviderDispatchTarget,
        request: &EffectiveModelRequest,
    ) -> Result<ProviderDispatchPolicyEvaluation, ApplicationError>;
}

/// The one production [`ProviderDispatchPolicyEvaluator`].
///
/// It answers two questions in a fixed order and nothing else: may this
/// subject exercise the effect's exact capability, and does the deployment's
/// data policy permit disclosure to the destination the pinned connection
/// revision already chose. It allocates decision identities, and it selects no
/// connection, revision, model, binding, credential, URL, or route — every one
/// of those arrives already fixed in the cause and the target.
pub struct ConfiguredProviderDispatchPolicyEvaluator {
    engine: SharedPolicyDecisionEngine,
    data_policy: ModelDataPolicySettings,
}

impl ConfiguredProviderDispatchPolicyEvaluator {
    pub fn new(engine: SharedPolicyDecisionEngine, data_policy: ModelDataPolicySettings) -> Self {
        Self {
            engine,
            data_policy,
        }
    }
}

#[async_trait]
impl ProviderDispatchPolicyEvaluator for ConfiguredProviderDispatchPolicyEvaluator {
    async fn evaluate(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
        cause: &ProviderDispatchCause,
        target: &ProviderDispatchTarget,
        _request: &EffectiveModelRequest,
    ) -> Result<ProviderDispatchPolicyEvaluation, ApplicationError> {
        // The capability tuple is taken from the intent, never rebuilt from the
        // request: the dispatch repository refuses any decision whose
        // capability, operation, or resource scope is not the effect's own.
        let authorization = self
            .engine
            .decide(
                context,
                AuthorizationRequest::new(
                    intent.required_capability(),
                    intent.operation(),
                    intent.target(),
                    intent.risk(),
                ),
            )
            .await?;

        // A structural qualification probe carries no governed content and has
        // no Run or step identity to bind a disclosure decision to, so the data
        // boundary is not consulted and no Run decision row exists for it.
        let ProviderDispatchCause::RunStep {
            run_id, step_id, ..
        } = cause
        else {
            return Ok(ProviderDispatchPolicyEvaluation {
                authorization,
                model_data_policy: None,
            });
        };

        let classification = DataClassification::source(
            self.data_policy.classification,
            "run-step-model-channel",
            "deployment configuration policy.data.classification",
        )?;
        let destination = target.destination;
        let boundary = evaluate_model_boundary(
            &self.data_policy.policy,
            &classification,
            destination,
            authorization.is_allowed(),
        );
        let reason = if boundary.is_allowed() {
            boundary.reason().to_owned()
        } else {
            format!("{}; destination {destination:?}", boundary.reason())
        };
        let record = ModelDataPolicyDecisionRecord {
            id: uuid::Uuid::now_v7(),
            run_id: *run_id,
            step_id: *step_id,
            destination,
            classification: self.data_policy.classification,
            allowed: boundary.is_allowed(),
            reason,
            policy_version: boundary.policy_version().to_owned(),
            mode: self.data_policy.mode,
            decided_at: vestrace_domain::time::now(),
        };

        // Enforce means the refusal is load-bearing, so the authorization it
        // overrides has to say so. Leaving an allowed authorization beside a
        // denied enforced record would be refused by the dispatch repository
        // as a decision not bound to its cause — and, worse, would describe a
        // dispatch that policy did not actually permit.
        let authorization = if authorization.is_allowed()
            && !record.allowed
            && self.data_policy.mode == ModelDataPolicyMode::Enforce
        {
            PolicyDecision {
                result: PolicyDecisionResult::Deny,
                reason: PolicyDecisionReason::ConditionNotSatisfied,
                ..authorization
            }
        } else {
            authorization
        };

        Ok(ProviderDispatchPolicyEvaluation {
            authorization,
            model_data_policy: Some(record),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderDispatchFaultPoint {
    BeforePolicyRecord,
    AfterPolicyRecord,
    BeforeAdmission,
    AfterAdmission,
    BeforeIntent,
    AfterIntent,
    BeforeAuthorization,
    AfterAuthorization,
    BeforeCredentialIssue,
    AfterCredentialIssue,
    BeforeCredentialConsume,
    AfterCredentialConsume,
    BeforeDispatching,
    AfterDispatching,
    BeforeGovernedCommit,
    AfterGovernedCommit,
}

pub trait ProviderDispatchFaultInjector: Send + Sync {
    fn check(&self, point: ProviderDispatchFaultPoint) -> Result<(), ApplicationError>;

    fn observe_auth(&self, _auth: &ConnectionAuth) {}
}

#[derive(Default)]
pub struct NoProviderDispatchFaults;

impl ProviderDispatchFaultInjector for NoProviderDispatchFaults {
    fn check(&self, _point: ProviderDispatchFaultPoint) -> Result<(), ApplicationError> {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDispatchAuthority {
    pub effect_id: ExternalEffectId,
    pub authorization_id: PolicyDecisionId,
    pub connection_id: ConnectionId,
    pub connection_revision_id: ConnectionRevisionId,
    pub concurrency_lease_id: uuid::Uuid,
    pub credential_lease_id: Option<uuid::Uuid>,
    pub dispatch_transition_id: ExternalEffectLifecycleTransitionId,
    pub dispatch_expires_at: DateTime<Utc>,
}

/// The closed, immutable adapter destination selected by the Run's pinned
/// connection revision.  It is returned with the one reconstructed request so
/// callers cannot route a governed dispatch through `config.model`, a mutable
/// Connection head, or a legacy provider row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDispatchTarget {
    pub kind: ConnectionKind,
    pub runtime_base_url: String,
    /// The disclosure class of `runtime_base_url`, classified once where the
    /// pinned revision is locked. It travels with the target so a policy
    /// evaluator can weigh the boundary without parsing a URL, resolving a
    /// host, or otherwise acquiring routing authority of its own.
    pub destination: DataDestination,
}

pub enum ProviderDispatchOutcome {
    Prepared {
        authority: Box<ProviderDispatchAuthority>,
        target: ProviderDispatchTarget,
        /// Present only for a `QualificationProbe`. The shared dispatch
        /// transaction reconstructed this exact closed request from a Complete
        /// MRE; it must be handed unchanged to the q1 adapter.
        q1_request: Option<Box<Q1ProbeRequest>>,
        request: EffectiveModelRequest,
        auth: ConnectionAuth,
    },
    Conflict {
        wait_deadline_at: Option<DateTime<Utc>>,
        retry_after_seconds: Option<u32>,
    },
    Denied {
        authorization_id: PolicyDecisionId,
    },
}

/// One completed provider attempt.  The caller supplies the closed, safe
/// receipt produced by the adapter; the repository owns the one transaction
/// that persists it, releases admission, and records any 429 throttle.
#[derive(Clone, Debug)]
pub struct ProviderPostNetworkCompletion {
    pub authority: ProviderDispatchAuthority,
    pub receipt: ExternalEffectReceipt,
    pub throttle: Option<ProviderDispatchThrottle>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderDispatchThrottle {
    pub observation_id: uuid::Uuid,
    pub retry_after_seconds: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderLostDispatchRecovery {
    RunStep,
    QualificationInconclusive,
}

/// The immutable opaque identity of a single governed Run-step execution.
/// None of these fields carries plaintext provider input; the material tuple
/// exists only so a restart can converge on the original effect and evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunStepExecutionAttempt {
    pub id: uuid::Uuid,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub model_binding_snapshot_id: uuid::Uuid,
    pub input_material_intent_id: MaterialKeyCreationIntentId,
    pub input_content_material_id: ContentMaterialId,
    pub input_material_key_id: MaterialKeyId,
    pub input_intent_nonce: IntentNonce,
    pub input_prepared_attachment_id: PreparedMaterialAttachmentId,
    pub external_effect_id: ExternalEffectId,
    pub model_request_evidence_id: ModelRequestEvidenceId,
}

/// One request-owned, move-only input acceptance command. Its input is never
/// serialised or retained by this command boundary.
pub struct PrepareGovernedRunStepInput {
    pub run_id: AgentRunId,
    pub expected_run_version: RunVersion,
    pub step_id: RunStepId,
    pub assigned_actor: RunActorRef,
    pub plan_step_reference: Option<String>,
    pub input: ConfidentialRunInput,
    /// The effect intent this step's attempt adopted its id from.
    ///
    /// The boundary constructs it, so `ExternalEffectIntent::new` keeps its
    /// single allocation site and no fixed-id constructor is admitted. The
    /// attempt then carries that same id, which is what lets the MRE root
    /// reference an effect row that already exists.
    pub intent: ExternalEffectIntent,
    /// The pinned revisions and the canonical request values this step's
    /// evidence is authored from. The boundary supplies them because it is
    /// what resolved the binding; the dispatch transaction independently
    /// re-proves that the connection revision belongs to the pinned snapshot
    /// before anything is disclosed.
    pub connection_revision_id: ConnectionRevisionId,
    pub connection_qualification_revision_id: uuid::Uuid,
    pub model_revision_id: vestrace_domain::ModelRevisionId,
    pub model_qualification_revision_id: uuid::Uuid,
    pub sampling: crate::EffectiveSampling,
    pub limits: crate::EffectiveRequestLimits,
    pub identities: RunStepExecutionAttempt,
    pub audit: AuditEntry,
    pub idempotency: IdempotencyRecord,
    pub outbox: Vec<OutboxMessage>,
}

pub struct GovernedRunStepInputOutcome {
    pub run: RunSnapshot,
    pub attempt: RunStepExecutionAttempt,
}

#[async_trait]
pub trait GovernedRunStepInputAuthority: Send + Sync {
    async fn accept(
        &self,
        context: &RequestContext,
        command: PrepareGovernedRunStepInput,
    ) -> Result<GovernedRunStepInputOutcome, ApplicationError>;
}

/// The durable first leg of governed Run-step acceptance. It is intentionally
/// limited to reservation: vault and material publication happen after this
/// transaction has committed.
/// Seals one governed Run-step input under a borrowed DEK.
///
/// The codec that performs this lives in infrastructure, and
/// `crates/vestrace-application/src/material/vault.rs` is outside the P03
/// change scope, so the port is declared here beside its only caller rather
/// than added to the material module.
///
/// It takes the DEK by reference and returns only ciphertext: the plaintext is
/// borrowed for the call and never stored by the sealer.
/// Lets a shared vault satisfy the vault bound directly.
///
/// The acceptance service is generic over its vault so it can be composed
/// without boxing, but every production and test composition holds one behind
/// an `Arc`. Declared here because `material/vault.rs` is outside the P03
/// change scope; the trait is ours, so the impl is well-formed anywhere in
/// this crate.
impl<T: crate::MaterialKeyVault + ?Sized> crate::MaterialKeyVault for std::sync::Arc<T> {
    fn bind_embedding_output(
        &self,
        binding: &crate::EmbeddingOutputKeyBinding,
        preparation: crate::EmbeddingResultPreparationId,
    ) -> Result<vestrace_domain::MaterialKeyBindingReceipt, crate::VaultError> {
        (**self).bind_embedding_output(binding, preparation)
    }

    fn with_bound_embedding_output_key(
        &self,
        binding: &crate::EmbeddingOutputKeyBinding,
        preparation: crate::EmbeddingResultPreparationId,
        receipt: vestrace_domain::MaterialKeyBindingReceipt,
        use_dek: &mut dyn FnMut(&vestrace_domain::ZeroizingDek),
    ) -> Result<(), crate::VaultError> {
        (**self).with_bound_embedding_output_key(binding, preparation, receipt, use_dek)
    }

    fn create_if_absent(
        &self,
        key_id: MaterialKeyId,
        nonce: IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, crate::VaultError> {
        (**self).create_if_absent(key_id, nonce)
    }

    fn create_embedding_output_if_absent(
        &self,
        binding: &crate::EmbeddingOutputKeyBinding,
    ) -> Result<vestrace_domain::VaultReceipt, crate::VaultError> {
        (**self).create_embedding_output_if_absent(binding)
    }

    fn retire_embedding_output(
        &self,
        binding: &crate::EmbeddingOutputKeyBinding,
    ) -> Result<vestrace_domain::ErasureReceipt, crate::VaultError> {
        (**self).retire_embedding_output(binding)
    }

    fn with_embedding_output_key(
        &self,
        binding: &crate::EmbeddingOutputKeyBinding,
        use_dek: &mut dyn FnMut(&vestrace_domain::ZeroizingDek),
    ) -> Result<(), crate::VaultError> {
        (**self).with_embedding_output_key(binding, use_dek)
    }

    fn unwrap(
        &self,
        key_id: MaterialKeyId,
        use_dek: &mut dyn FnMut(&vestrace_domain::ZeroizingDek),
    ) -> Result<(), crate::VaultError> {
        (**self).unwrap(key_id, use_dek)
    }

    fn prepare_erasure(
        &self,
        key_id: MaterialKeyId,
    ) -> Result<crate::FenceReceipt, crate::VaultError> {
        (**self).prepare_erasure(key_id)
    }

    fn erase(
        &self,
        key_id: MaterialKeyId,
    ) -> Result<vestrace_domain::ErasureReceipt, crate::VaultError> {
        (**self).erase(key_id)
    }
}

pub trait GovernedInputSealer: Send + Sync {
    fn seal(
        &self,
        workspace_id: WorkspaceId,
        material_id: ContentMaterialId,
        material_key_id: MaterialKeyId,
        dek: &vestrace_domain::ZeroizingDek,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, ApplicationError>;
}

pub struct GovernedRunStepInputReservation<T, S, M, P, G, E, V, C, R> {
    transactions: T,
    runs: S,
    materials: M,
    dispatch: P,
    governed: G,
    effects: E,
    vault: V,
    sealer: C,
    evidence: R,
}

impl<T, S, M, P, G, E, V, C, R> GovernedRunStepInputReservation<T, S, M, P, G, E, V, C, R> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        transactions: T,
        runs: S,
        materials: M,
        dispatch: P,
        governed: G,
        effects: E,
        vault: V,
        sealer: C,
        evidence: R,
    ) -> Self {
        Self {
            transactions,
            runs,
            materials,
            dispatch,
            governed,
            effects,
            vault,
            sealer,
            evidence,
        }
    }
}

#[async_trait]
impl<T, S, M, P, G, E, V, C, R> GovernedRunStepInputAuthority
    for GovernedRunStepInputReservation<T, S, M, P, G, E, V, C, R>
where
    T: TransactionManager,
    S: RunStorePort,
    M: MaterialIntentRepository,
    P: ProviderDispatchRepository,
    G: crate::GovernedMutationRepository<ProviderDispatchGovernedApply>,
    E: crate::ExternalEffectRepository,
    V: crate::MaterialKeyVault,
    C: GovernedInputSealer,
    R: crate::ModelRequestEvidenceRepository,
{
    async fn accept(
        &self,
        context: &RequestContext,
        command: PrepareGovernedRunStepInput,
    ) -> Result<GovernedRunStepInputOutcome, ApplicationError> {
        if !matches!(command.assigned_actor, RunActorRef::AgentSnapshot(_)) {
            return Err(ApplicationError::Policy(
                "governed Run-step input requires an agent snapshot".to_owned(),
            ));
        }
        if command.identities.workspace_id != context.workspace_id
            || command.identities.run_id != command.run_id
            || command.identities.step_id != command.step_id
        {
            return Err(ApplicationError::Policy(
                "governed Run-step input identities are not bound to the command".to_owned(),
            ));
        }
        // The attempt must adopt the effect id from the intent it was built
        // beside. Any other pairing would durably reference an effect row this
        // acceptance never writes, and the MRE root's foreign key would then
        // fail at a point where the plaintext has already been consumed.
        if command.intent.id() != command.identities.external_effect_id
            || command.intent.workspace_id() != context.workspace_id
            || command.intent.execution_run_id() != Some(command.run_id)
        {
            return Err(ApplicationError::Policy(
                "governed Run-step effect intent is not bound to its attempt".to_owned(),
            ));
        }
        let snapshot = self
            .runs
            .load(context, command.run_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Domain(vestrace_domain::DomainError::NotFound(
                    "run not found".into(),
                ))
            })?;
        if snapshot.run.version != command.expected_run_version {
            return Err(ApplicationError::Conflict(
                "governed Run-step input run version conflicts".to_owned(),
            ));
        }
        let at = chrono::Utc::now();
        let step = RunStep::create(
            vestrace_domain::run::NewRunStep {
                id: command.step_id,
                run_id: command.run_id,
                plan_step_reference: command.plan_step_reference,
                assigned_actor: command.assigned_actor.clone(),
                input_references: vec![],
            },
            at,
        )?;
        let next_version = command.expected_run_version.next()?;
        let event = RunEvent::new(
            command.run_id,
            context.workspace_id,
            next_version,
            vestrace_domain::run::ResumeCursor::from_version(next_version),
            RunActorRef::Principal(context.principal_id),
            RunEventPayload::StepsAdded {
                steps: vec![step.clone()],
            },
            vestrace_domain::CorrelationId::new(),
            None,
            at,
        )?;
        let mut run = snapshot.run;
        run.version = next_version;
        run.updated_at = at;
        let intent = MaterialKeyCreationIntent::reserve(
            command.identities.input_material_intent_id,
            context.workspace_id,
            command.identities.input_content_material_id,
            command.identities.input_material_key_id,
            command.identities.input_intent_nonce,
            "model_request_input",
            PrincipalId::from_uuid(command.step_id.as_uuid()),
            0,
        );
        let mut unit_of_work = self.transactions.begin(context).await?;
        // A replay of the same step must converge on the identities already
        // reserved. The guarded reserver refuses a substituted tuple, so
        // writing a second intent here would turn a lawful replay into an
        // identity conflict.
        let reserved = self
            .dispatch
            .load_run_step_attempt_in(
                context,
                unit_of_work.as_mut(),
                command.run_id,
                command.step_id,
            )
            .await?;
        if let Some(existing) = reserved {
            if existing != command.identities {
                return Err(ApplicationError::Conflict(
                    "governed Run-step input replays a different attempt identity".to_owned(),
                ));
            }
            unit_of_work.rollback().await?;
            let snapshot = self
                .runs
                .load(context, command.run_id)
                .await?
                .ok_or_else(|| {
                    ApplicationError::Domain(vestrace_domain::DomainError::NotFound(
                        "run not found".into(),
                    ))
                })?;
            let _input = command.input;
            return Ok(GovernedRunStepInputOutcome {
                run: snapshot,
                attempt: existing,
            });
        }
        // The effect row must exist before the MRE root can reference it, and
        // before the attempt that names it is durable.
        self.effects
            .save_intent_in(context, unit_of_work.as_mut(), &command.intent)
            .await?;
        let commit = CommitRun {
            run,
            event,
            new_steps: vec![step],
            checkpoint: None,
            work_items: vec![],
        };
        let run = self
            .runs
            .commit_in(context, unit_of_work.as_mut(), commit)
            .await?;
        self.dispatch
            .reserve_run_step_attempt_in(context, unit_of_work.as_mut(), &command.identities)
            .await?;
        self.materials
            .reserve_in(context, unit_of_work.as_mut(), &intent)
            .await?;
        self.governed
            .commit_in(
                unit_of_work.as_mut(),
                crate::GovernedMutation {
                    context: context.clone(),
                    audit: command.audit,
                    idempotency: Some(command.idempotency),
                    outbox: command.outbox,
                    apply: ProviderDispatchGovernedApply,
                },
            )
            .await?;
        unit_of_work.commit().await?;

        // Everything below is post-commit and idempotent on the identities the
        // transaction above made durable. No vault call happens while a
        // transaction is open, and no work item exists until the last step.
        let attempt = command.identities;

        let receipt = self
            .vault
            .create_if_absent(attempt.input_material_key_id, attempt.input_intent_nonce)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        self.materials
            .record_provisional_created(context, attempt.input_material_intent_id)
            .await?;
        self.materials
            .record_provisional_receipt(context, attempt.input_material_intent_id, receipt)
            .await?;

        // The plaintext is borrowed for exactly this callback and the DEK never
        // leaves it. `sealed` receives ciphertext only.
        let mut sealed: Option<Result<Vec<u8>, ApplicationError>> = None;
        self.vault
            .unwrap(attempt.input_material_key_id, &mut |dek| {
                sealed = Some(command.input.with_bytes(|plaintext| {
                    self.sealer.seal(
                        context.workspace_id,
                        attempt.input_content_material_id,
                        attempt.input_material_key_id,
                        dek,
                        plaintext,
                    )
                }));
            })
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let ciphertext = sealed.ok_or_else(|| {
            ApplicationError::Internal(
                "the material vault returned without sealing the governed input".to_owned(),
            )
        })??;
        // The input has done its work; drop it before anything else can fail.
        drop(command.input);

        self.materials
            .prepare_content(
                context,
                attempt.input_material_intent_id,
                attempt.input_prepared_attachment_id,
                &ciphertext,
                vestrace_domain::size_class_for(ciphertext.len()),
            )
            .await?;
        self.materials
            .bind(
                context,
                attempt.input_material_intent_id,
                vestrace_domain::MaterialKeyBindingReceipt::new(),
            )
            .await?;
        self.materials
            .finalize_bound(context, attempt.input_material_intent_id)
            .await?;

        let mut evidence_work = self.transactions.begin(context).await?;
        let creation = crate::CreateModelRequestEvidence::for_governed_run_step(
            attempt.model_request_evidence_id.as_uuid(),
            context.workspace_id,
            attempt.external_effect_id.as_uuid(),
            attempt.model_binding_snapshot_id,
            command.connection_revision_id.as_uuid(),
            command.connection_qualification_revision_id,
            command.model_revision_id.as_uuid(),
            command.model_qualification_revision_id,
            attempt.step_id.as_uuid(),
            attempt.input_content_material_id.as_uuid(),
            command.sampling,
            command.limits,
        )?;
        self.evidence
            .create_in(evidence_work.as_mut(), &creation)
            .await?;
        // Creating the root does not prove it can be rebuilt. This appends the
        // Complete check by actually reconstructing the request from the
        // durable ciphertext, so a work item is never scheduled against
        // evidence that cannot produce a request.
        match self
            .evidence
            .reconstruct_current_in(
                evidence_work.as_mut(),
                context.workspace_id,
                attempt.model_request_evidence_id,
            )
            .await?
        {
            crate::ModelRequestReconstruction::Complete(_) => {}
            crate::ModelRequestReconstruction::Incomplete(_) => {
                return Err(ApplicationError::Conflict(
                    crate::MODEL_REQUEST_EVIDENCE_INCOMPLETE.to_owned(),
                ));
            }
            crate::ModelRequestReconstruction::Expired => {
                return Err(ApplicationError::Conflict(
                    crate::MODEL_REQUEST_EVIDENCE_CONFLICT.to_owned(),
                ));
            }
        }
        evidence_work.commit().await?;

        // Last, and only now. The guarded 0186 function re-proves Live input
        // material and a Complete exact MRE itself, so this cannot schedule
        // work against an attempt that is not ready even if the caller is
        // wrong about the order.
        self.dispatch
            .enqueue_run_step_after_input_ready(
                context,
                &attempt,
                &WorkItem {
                    id: vestrace_domain::WorkItemId::from_uuid(attempt.id),
                    run_id: attempt.run_id,
                    kind: crate::run::WorkItemKind::ExecuteStep {
                        step_id: attempt.step_id,
                    },
                    expected_run_version: run.run.version,
                    available_at: chrono::Utc::now(),
                    // Derived from the attempt, so a replay of the same
                    // acceptance converges on the same work item rather than
                    // scheduling the step twice.
                    idempotency_key: format!("governed-run-step-execute:{}", attempt.id),
                    attempt: 0,
                },
            )
            .await?;

        Ok(GovernedRunStepInputOutcome { run, attempt })
    }
}

/// Closed recovery actions for a durable Run-step attempt.  The repository
/// never calls an adapter while deciding one of these actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunStepAttemptRecovery {
    ResumeReserved,
    ResumeAdmitted,
    AwaitDispatchDeadline,
    AdoptedUnknown,
    AlreadyUnknown,
    ResumeResultPrepared { effect_id: ExternalEffectId },
    Published,
}

/// Everything durable one governed Run-step attempt needs before it may ask
/// for dispatch.
///
/// The worker rediscovers this rather than being told it. Nothing here is
/// chosen at execution time: the attempt fixes the identities, the pinned
/// `ModelBindingSnapshot` fixes the Connection and its revision, and the same
/// snapshot's auth branch fixes whether a credential participates at all. A
/// worker that could name any of these itself would be routing, which is the
/// authority this package spent ten tasks removing from it.
///
/// The intent is the one already persisted under the attempt's effect id, not
/// a freshly constructed equivalent: `ExternalEffectIntent::new` allocates, so
/// building one here would present a different effect to a dispatch that must
/// converge on the original.
/// Safe to render: every field is an opaque identity or immutable routing
/// metadata. The governed input itself never travels here — it is reconstructed
/// inside the dispatch transaction and handed out only as a zeroizing request.
#[derive(Debug)]
pub struct RunStepDispatchPlan {
    pub attempt: RunStepExecutionAttempt,
    pub intent: ExternalEffectIntent,
    pub connection_id: ConnectionId,
    pub connection_revision_id: ConnectionRevisionId,
    /// `None` is the pinned `no_auth` branch, not an absent lookup. A
    /// credential branch that could not produce its slot, revision and guard
    /// is an error rather than a silent downgrade to no authentication.
    pub credential: Option<ProviderDispatchCredential>,
}

/// Closed recovery actions for a durable embedding job attempt.
///
/// The same seven shapes as `RunStepAttemptRecovery`, and deliberately so: the
/// two callers share one dispatch authority, and a recovery vocabulary that
/// diverged would be the first place they stopped sharing it. The repository
/// never calls an adapter while deciding one of these actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingJobAttemptRecovery {
    /// A durable pre-dispatch cancellation receipt proves no provider outcome
    /// can be recovered or resumed.
    Cancelled,
    /// A durable denial or expired admission wait proved definite failure
    /// before the provider boundary.
    FailedDefinite,
    ResumeReserved,
    ResumeAdmitted,
    AwaitDispatchDeadline,
    AdoptedUnknown,
    AlreadyUnknown,
    ResumeResultPrepared {
        effect_id: ExternalEffectId,
    },
    Succeeded,
}

/// The immutable opaque identity of one governed embedding job attempt.
///
/// This is the durable `embedding_jobs` row and nothing else. Spec line 219
/// fixes what it owns: "exactly one immutable embedding-kind
/// `ModelBindingSnapshot`, one external-effect identity, and one
/// `ModelRequestEvidence` identity", resolved at acceptance and never changed.
/// No field here carries plaintext to embed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddingJobExecutionAttempt {
    pub job_id: EmbeddingJobId,
    pub workspace_id: WorkspaceId,
    pub space_registration_id: EmbeddingSpaceId,
    pub kind: EmbeddingJobKind,
    pub model_binding_snapshot_id: uuid::Uuid,
    pub external_effect_id: ExternalEffectId,
    pub model_request_evidence_id: ModelRequestEvidenceId,
}

/// Everything durable one governed embedding job needs before it may ask for
/// dispatch.
///
/// The mirror of `RunStepDispatchPlan`, field for field where the two share a
/// meaning. The worker rediscovers this rather than being told it: the job row
/// fixes the identities, the pinned `ModelBindingSnapshot` fixes the Connection
/// and its revision, and the same snapshot's auth branch fixes whether a
/// credential participates at all.
#[derive(Debug)]
pub struct EmbeddingJobDispatchPlan {
    pub attempt: EmbeddingJobExecutionAttempt,
    pub intent: ExternalEffectIntent,
    pub connection_id: ConnectionId,
    pub connection_revision_id: ConnectionRevisionId,
    /// `None` is the pinned `no_auth` branch, not an absent lookup. Line 219:
    /// the snapshot "structurally pins exactly one auth branch ... never both or
    /// neither", so a credential branch that could not produce its slot,
    /// revision and guard is an error rather than a silent downgrade to no
    /// authentication.
    pub credential: Option<ProviderDispatchCredential>,
}

#[async_trait]
pub trait ProviderDispatchRepository: Send + Sync {
    /// Reserve the immutable attempt tuple in the caller's transaction.
    async fn reserve_run_step_attempt_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
        _attempt: &RunStepExecutionAttempt,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "transaction-bound Run-step attempt reservation is not configured".to_owned(),
        ))
    }

    /// Read the exact reserved attempt for a Run step inside the caller's
    /// transaction.
    ///
    /// Acceptance calls this before it authors anything. `ExternalEffectIntent`
    /// allocates a fresh id on construction, so a service that built an intent
    /// unconditionally would present a different `external_effect_id` on a
    /// same-Request-Id replay and turn a lawful replay into the identity
    /// conflict the guarded reserver raises.
    ///
    /// The default refuses rather than answering `None`: reporting absence
    /// from an unconfigured repository would make acceptance author a second
    /// intent for a step that already has one.
    async fn load_run_step_attempt_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
        _run_id: AgentRunId,
        _step_id: RunStepId,
    ) -> Result<Option<RunStepExecutionAttempt>, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "transaction-bound Run-step attempt lookup is not configured".to_owned(),
        ))
    }

    /// Schedule only after the durable input material and MRE prerequisites
    /// have been completed. This method intentionally has no caller UoW: its
    /// implementation performs the guarded final enqueue as a short command.
    async fn enqueue_run_step_after_input_ready(
        &self,
        _context: &RequestContext,
        _attempt: &RunStepExecutionAttempt,
        _item: &WorkItem,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed Run-step enqueue is not configured".to_owned(),
        ))
    }

    /// Rediscover the one durable dispatch plan for a Run step.
    ///
    /// The default refuses rather than answering absence: a worker that read
    /// `None` from an unconfigured repository would conclude the step has no
    /// attempt and fail a step whose provider call may already be in flight.
    async fn load_run_step_dispatch_plan(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
        _step_id: RunStepId,
    ) -> Result<RunStepDispatchPlan, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed Run-step dispatch plan lookup is not configured".to_owned(),
        ))
    }

    /// Rediscover the one durable dispatch plan for an embedding job.
    ///
    /// The embedding twin of `load_run_step_dispatch_plan`, on this trait rather
    /// than on a parallel one. That placement is the point: "one dispatch
    /// authority, two callers" is a claim a reviewer can check by reading this
    /// trait, and it stops being checkable the moment embedding dispatch grows
    /// its own repository.
    ///
    /// The default refuses rather than answering absence, for the same reason
    /// the Run-step twin does: a worker that read `None` from an unconfigured
    /// repository would conclude the job has no attempt and fail a job whose
    /// provider call may already be in flight.
    async fn load_embedding_dispatch_plan(
        &self,
        _context: &RequestContext,
        _job_id: EmbeddingJobId,
    ) -> Result<EmbeddingJobDispatchPlan, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding dispatch plan lookup is not configured".to_owned(),
        ))
    }

    /// Classifies one immutable embedding job attempt using its existing durable
    /// effect. Implementations must not issue a second admission or invoke an
    /// adapter during recovery.
    ///
    /// Line 219: "Neither workers nor recovery automatically retry a job after
    /// `Dispatching`." Recovery decides what already happened; it never makes
    /// something happen again.
    async fn recover_embedding_job_attempt(
        &self,
        _context: &RequestContext,
        _job_id: EmbeddingJobId,
        _recovered_at: DateTime<Utc>,
    ) -> Result<EmbeddingJobAttemptRecovery, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "embedding job attempt recovery is not configured".to_owned(),
        ))
    }

    async fn prepare_dispatch(
        &self,
        request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchOutcome, ApplicationError>;

    /// Idempotent post-network completion for the original effect.  This is
    /// deliberately shared by ordinary provider execution and q1; neither
    /// caller may release admission or persist a receipt independently.
    async fn complete_post_network(
        &self,
        context: &RequestContext,
        completion: ProviderPostNetworkCompletion,
    ) -> Result<(), ApplicationError>;

    /// Releases the exact admission lease after the provider-result authority
    /// has inserted its canonical acknowledged receipt.  Keeping this in the
    /// dispatch authority avoids a second release implementation in result
    /// persistence and lets `ResultPrepared`, receipt witness, and release
    /// share one caller-owned transaction.
    async fn release_after_provider_result_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        authority: &ProviderDispatchAuthority,
        receipt: &ExternalEffectReceipt,
    ) -> Result<(), ApplicationError>;

    /// Locks and revalidates the original pre-network authority inside a
    /// caller-owned result transaction before any Run/step parent locks are
    /// acquired. Implementations that do not own the governed dispatch
    /// authority must fail closed rather than letting a result compose around
    /// an unvalidated adapter effect.
    async fn lock_provider_result_completion_authority_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
        _authority: &ProviderDispatchAuthority,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "provider-result completion authority is not configured".to_owned(),
        ))
    }

    /// Locks the original embedding-job dispatch cause for a delivery
    /// result-preparation transaction. This is intentionally separate from the
    /// Run/Step completion lock above: widening that method would let an
    /// embedding result borrow Run publication authority.
    async fn lock_embedding_result_completion_authority_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
        _authority: &ProviderDispatchAuthority,
        _job_id: EmbeddingJobId,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "embedding result completion authority is not configured".to_owned(),
        ))
    }

    /// Locks and revalidates the original Run-step attempt before provider
    /// result publication acquires its Run, step, or preparation parents.
    /// This is deliberately a caller-owned transaction seam: a result
    /// finalizer may not open a second transaction around its publication.
    async fn lock_provider_result_publication_authority_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
        _effect_id: ExternalEffectId,
        _run_id: AgentRunId,
        _step_id: RunStepId,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "provider-result publication authority is not configured".to_owned(),
        ))
    }

    /// Adopts the original lost `Dispatching` transition without creating a
    /// new effect or adapter attempt.  Qualification causes become terminal
    /// `InconclusiveUnknown` in the same transaction.
    async fn recover_lost_post_network(
        &self,
        context: &RequestContext,
        authority: &ProviderDispatchAuthority,
        recovered_at: DateTime<Utc>,
    ) -> Result<ProviderLostDispatchRecovery, ApplicationError>;

    /// Classifies one immutable Run-step attempt using its existing durable
    /// effect. Implementations must not issue a second admission or invoke an
    /// adapter during recovery.
    async fn recover_run_step_attempt(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
        _step_id: RunStepId,
        _recovered_at: DateTime<Utc>,
    ) -> Result<RunStepAttemptRecovery, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "run-step attempt recovery is not configured".to_owned(),
        ))
    }
}

pub type SharedProviderDispatchRepository = Arc<dyn ProviderDispatchRepository>;

/// The effect/admission writes are complete before the governed evidence leg;
/// this typed no-op lets the existing governed-mutation authority append its
/// Audit/idempotency/outbox/watermark evidence without duplicating SQL.
pub struct ProviderDispatchGovernedApply;

#[async_trait]
impl GovernedMutationApply for ProviderDispatchGovernedApply {
    async fn apply(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

#[cfg(test)]
mod run_step_attempt_lookup {
    use super::*;

    /// Acceptance must resolve an already-reserved attempt before it authors
    /// anything. `ExternalEffectIntent::new` allocates a fresh effect id on
    /// every call, so an acceptance service that built an intent
    /// unconditionally would present a different `external_effect_id` on a
    /// same-Request-Id replay and turn a lawful replay into the `23514`
    /// identity conflict the guarded reserver raises.
    struct UnconfiguredDispatch;

    #[async_trait]
    impl ProviderDispatchRepository for UnconfiguredDispatch {
        async fn prepare_dispatch(
            &self,
            _request: ProviderDispatchRequest,
        ) -> Result<ProviderDispatchOutcome, ApplicationError> {
            unreachable!("this fixture only exercises the attempt lookup default")
        }

        async fn complete_post_network(
            &self,
            _context: &RequestContext,
            _completion: ProviderPostNetworkCompletion,
        ) -> Result<(), ApplicationError> {
            unreachable!("this fixture only exercises the attempt lookup default")
        }

        async fn release_after_provider_result_in(
            &self,
            _context: &RequestContext,
            _unit_of_work: &mut dyn UnitOfWork,
            _authority: &ProviderDispatchAuthority,
            _receipt: &ExternalEffectReceipt,
        ) -> Result<(), ApplicationError> {
            unreachable!("this fixture only exercises the attempt lookup default")
        }

        async fn recover_lost_post_network(
            &self,
            _context: &RequestContext,
            _authority: &ProviderDispatchAuthority,
            _recovered_at: DateTime<Utc>,
        ) -> Result<ProviderLostDispatchRecovery, ApplicationError> {
            unreachable!("this fixture only exercises the attempt lookup default")
        }
    }

    /// The default must refuse rather than report "no attempt". Reporting
    /// absence from an unconfigured repository would make acceptance author a
    /// second intent for a step that already has one.
    #[tokio::test]
    async fn an_unconfigured_attempt_lookup_refuses_instead_of_reporting_absence() {
        let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
        let mut unit_of_work = NoUnitOfWork;

        let result = UnconfiguredDispatch
            .load_run_step_attempt_in(
                &context,
                &mut unit_of_work,
                AgentRunId::new(),
                RunStepId::new(),
            )
            .await;

        assert!(
            matches!(result, Err(ApplicationError::Unavailable(_))),
            "an unconfigured lookup must fail closed, not answer None"
        );
    }

    struct NoUnitOfWork;

    #[async_trait]
    impl UnitOfWork for NoUnitOfWork {
        async fn commit(self: Box<Self>) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn rollback(self: Box<Self>) -> Result<(), ApplicationError> {
            Ok(())
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }
}
