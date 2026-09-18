//! PostgreSQL implementation of the guarded q1 qualification lifecycle.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::FromRow;
use vestrace_application::{
    ApplicationError, ConnectionAuth, ConnectionKind, ExternalEffectRepository, IdempotencyRecord,
    InstallationMutationPermit, OutboxMessage, PermitMode, ProviderDispatchAuthority,
    ProviderDispatchCause, ProviderDispatchCredential, ProviderDispatchOutcome,
    ProviderDispatchRequest, ProviderPostNetworkCompletion, Q1ProbeFailure, Q1ProbeRequest,
    Q1ProbeResponse, QualificationFinalization, QualificationJobRecord, QualificationJobRepository,
    QualificationJobRequest, QualificationProbeCompletion, QualificationProbeRunner,
    RequestContext, SharedProviderDispatchRepository, qualification_state_from_storage,
};
use vestrace_domain::{
    AdapterDispatchResult, AuditEvent, AuditEventId, Capability, ConnectionId,
    ConnectionRevisionId, CredentialSlotId, DeliverySemantics, DryRunMode, EffectAuthorization,
    EffectPrecondition, EffectReversibility, ExternalEffectAdapter,
    ExternalEffectAdapterDescriptor, ExternalEffectIntent, IdempotencyProfile,
    ModelRequestEvidenceId, OutboxId, QualificationJobId, QualificationJobState,
    QualificationProbeResult, QualificationTargetBinding, RiskCategory, WorkerId,
};

use crate::providers::openai_compatible::OpenAiCompatibleClient;

use super::{
    PgExternalEffectRepository, PgInstallationMutationPermit, PgScopedTransaction, PgStore,
};

/// The repository always acquires a `Shared` permit before it reads or writes
/// the target tuple.  A caller receives no transaction handle and therefore
/// cannot reverse the permit/guard order with q1-specific SQL.
#[derive(Clone, Debug)]
pub struct PgQualificationJobRepository {
    permit: PgInstallationMutationPermit,
}

impl PgQualificationJobRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            permit: PgInstallationMutationPermit::new(store),
        }
    }

    async fn with_shared_permit<T>(
        &self,
        context: &RequestContext,
        operation: impl AsyncFnOnce(&mut PgScopedTransaction) -> Result<T, ApplicationError>,
    ) -> Result<T, ApplicationError> {
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = permit
            .unit_of_work_mut()
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| {
                ApplicationError::Internal(
                    "expected PostgreSQL qualification transaction".to_owned(),
                )
            })?;
        let value = operation(transaction).await?;
        permit.commit().await?;
        Ok(value)
    }
}

#[async_trait]
impl QualificationJobRepository for PgQualificationJobRepository {
    async fn request(
        &self,
        context: &RequestContext,
        request: QualificationJobRequest,
    ) -> Result<QualificationJobRecord, ApplicationError> {
        self.with_shared_permit(context, async |transaction| {
            let (
                branch,
                credential_revision_id,
                credential_slot_id,
                activation_guard_id,
                expected_slot_version,
                no_auth_binding_revision_id,
            ) = target_values(&request.target)?;
            let job_id: uuid::Uuid = sqlx::query_scalar(
                "SELECT vestrace_request_qualification_job(\
                 $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
            )
            .bind(request.job_id.as_uuid())
            .bind(request.target_binding_id)
            .bind(context.workspace_id.as_uuid())
            .bind(request.connection_id.as_uuid())
            .bind(request.connection_revision_id.as_uuid())
            .bind(vestrace_application::OPENAI_Q1_PROFILE_REVISION)
            .bind(branch)
            .bind(credential_revision_id)
            .bind(credential_slot_id)
            .bind(activation_guard_id)
            .bind(expected_slot_version)
            .bind(no_auth_binding_revision_id)
            .bind(request.chat_model_revision_id.as_uuid())
            .bind(request.embedding_model_revision_id.as_uuid())
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)?;
            Ok(QualificationJobRecord {
                id: vestrace_domain::QualificationJobId::from_uuid(job_id),
                target_binding_id: request.target_binding_id,
                state: QualificationJobState::Requested,
            })
        })
        .await
    }

    async fn record_probe_result(
        &self,
        context: &RequestContext,
        completion: QualificationProbeCompletion,
    ) -> Result<QualificationJobState, ApplicationError> {
        self.with_shared_permit(context, async |transaction| {
            let state: String = sqlx::query_scalar(
                "SELECT vestrace_record_qualification_probe_result($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(completion.probe_result_id)
            .bind(context.workspace_id.as_uuid())
            .bind(completion.job_id.as_uuid())
            .bind(completion.ordinal)
            .bind(probe_result(completion.result))
            .bind(completion.external_effect_id)
            .bind(completion.model_request_evidence_id)
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)?;
            qualification_state_from_storage(&state)
        })
        .await
    }

    async fn cancel_between_probes(
        &self,
        context: &RequestContext,
        job_id: vestrace_domain::QualificationJobId,
    ) -> Result<(), ApplicationError> {
        self.with_shared_permit(context, async |transaction| {
            sqlx::query_scalar::<_, ()>("SELECT vestrace_cancel_qualification_job($1,$2)")
                .bind(context.workspace_id.as_uuid())
                .bind(job_id.as_uuid())
                .fetch_one(transaction.connection())
                .await
                .map_err(storage_error)
        })
        .await
    }

    async fn recover_lost_dispatch(
        &self,
        context: &RequestContext,
        job_id: vestrace_domain::QualificationJobId,
        ordinal: &str,
    ) -> Result<(), ApplicationError> {
        self.with_shared_permit(context, async |transaction| {
            sqlx::query_scalar::<_, ()>(
                "SELECT vestrace_recover_qualification_dispatch_unknown($1,$2,$3)",
            )
            .bind(context.workspace_id.as_uuid())
            .bind(job_id.as_uuid())
            .bind(ordinal)
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)
        })
        .await
    }

    async fn finalize_success(
        &self,
        context: &RequestContext,
        finalization: QualificationFinalization,
    ) -> Result<(), ApplicationError> {
        self.with_shared_permit(context, async |transaction| {
            sqlx::query_scalar::<_, ()>(
                "SELECT vestrace_finalize_qualification_job($1,$2,$3,$4,$5)",
            )
            .bind(context.workspace_id.as_uuid())
            .bind(finalization.job_id.as_uuid())
            .bind(finalization.connection_qualification_revision_id.as_uuid())
            .bind(finalization.chat_model_qualification_revision_id.as_uuid())
            .bind(
                finalization
                    .embedding_model_qualification_revision_id
                    .as_uuid(),
            )
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)
        })
        .await
    }
}

/// Qualification-only adapter seam.  Its concrete implementation is the
/// hardened OpenAI-compatible client; tests use a closed fake and never open a
/// socket.  This is deliberately not Task 11's worker/server wiring.
#[async_trait]
pub trait QualificationQ1Adapter: Send + Sync {
    async fn execute(
        &self,
        kind: ConnectionKind,
        runtime_base_url: &str,
        auth: ConnectionAuth,
        request: Q1ProbeRequest,
    ) -> Result<Q1ProbeResponse, Q1ProbeFailure>;
}

#[derive(Default)]
pub struct OpenAiQualificationQ1Adapter;

#[async_trait]
impl QualificationQ1Adapter for OpenAiQualificationQ1Adapter {
    async fn execute(
        &self,
        kind: ConnectionKind,
        runtime_base_url: &str,
        auth: ConnectionAuth,
        request: Q1ProbeRequest,
    ) -> Result<Q1ProbeResponse, Q1ProbeFailure> {
        let client = OpenAiCompatibleClient::for_governed_connection(kind, runtime_base_url)
            .map_err(|_| Q1ProbeFailure::TransportFailure)?;
        client
            .execute_q1_for_qualification_once(auth, request)
            .await
    }
}

#[derive(FromRow)]
struct Q1RoutingRow {
    target_binding_id: uuid::Uuid,
    connection_id: uuid::Uuid,
    connection_revision_id: uuid::Uuid,
    runtime_base_url: String,
    branch: String,
    credential_revision_id: Option<uuid::Uuid>,
    credential_slot_id: Option<uuid::Uuid>,
    credential_activation_guard_id: Option<uuid::Uuid>,
    auth_mode: String,
}

/// Production q1 composition boundary.  It creates/recovers the original
/// durable intent+Complete MRE, uses the shared dispatch transaction, hands
/// the returned q1 request unchanged to one hardened adapter invocation, then
/// uses the shared post-network completion before returning the guarded probe
/// tuple to the application service.
pub struct PgQualificationProbeRunner {
    store: PgStore,
    dispatch: SharedProviderDispatchRepository,
    worker_id: WorkerId,
    adapter: Arc<dyn QualificationQ1Adapter>,
}

impl PgQualificationProbeRunner {
    pub fn new(
        store: PgStore,
        dispatch: SharedProviderDispatchRepository,
        worker_id: WorkerId,
    ) -> Self {
        Self::with_adapter(
            store,
            dispatch,
            worker_id,
            Arc::new(OpenAiQualificationQ1Adapter),
        )
    }

    pub fn with_adapter(
        store: PgStore,
        dispatch: SharedProviderDispatchRepository,
        worker_id: WorkerId,
        adapter: Arc<dyn QualificationQ1Adapter>,
    ) -> Self {
        Self {
            store,
            dispatch,
            worker_id,
            adapter,
        }
    }

    async fn routing(
        &self,
        context: &RequestContext,
        job_id: QualificationJobId,
    ) -> Result<Q1RoutingRow, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query_as::<_, Q1RoutingRow>(
            "SELECT binding.id AS target_binding_id, binding.connection_id,
                    binding.connection_revision_id, revision.runtime_base_url, binding.branch,
                    binding.credential_revision_id, binding.credential_slot_id,
                    binding.credential_activation_guard_id, revision.auth_mode
               FROM qualification_target_bindings AS binding
               JOIN connection_revisions AS revision
                 ON revision.workspace_id=binding.workspace_id
                AND revision.id=binding.connection_revision_id
                AND revision.connection_id=binding.connection_id
              WHERE binding.workspace_id=$1 AND binding.qualification_job_id=$2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(job_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?
        .ok_or_else(|| {
            ApplicationError::Policy("qualification job has no exact target routing".into())
        })?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(row)
    }

    async fn durable_q1_input(
        &self,
        context: &RequestContext,
        job_id: QualificationJobId,
        ordinal: &str,
        routing: &Q1RoutingRow,
    ) -> Result<(ExternalEffectIntent, ModelRequestEvidenceId), ApplicationError> {
        let candidate = qualification_intent(context, routing)?;
        let candidate_effect_id = candidate.id();
        let candidate_evidence_id = ModelRequestEvidenceId::new();
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let (existing_effect_id, existing_evidence_id, existing_payload): (
            Option<uuid::Uuid>,
            Option<uuid::Uuid>,
            Option<serde_json::Value>,
        ) = sqlx::query_as(
            "SELECT external_effect_id, model_request_evidence_id, intent_payload
               FROM vestrace_prepare_qualification_probe_dispatch($1,$2,$3,NULL,NULL)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(job_id.as_uuid())
        .bind(ordinal)
        .fetch_one(transaction.connection())
        .await
        .map_err(storage_error)?;
        let (effect_id, evidence_id, persisted_payload) =
            match (existing_effect_id, existing_evidence_id, existing_payload) {
                (Some(effect_id), Some(evidence_id), Some(payload)) => {
                    (effect_id, evidence_id, payload)
                }
                (None, None, None) => {
                    PgExternalEffectRepository::new(self.store.clone())
                        .save_intent_in(context, &mut transaction, &candidate)
                        .await?;
                    sqlx::query_as(
                        "SELECT external_effect_id, model_request_evidence_id, intent_payload
                       FROM vestrace_prepare_qualification_probe_dispatch($1,$2,$3,$4,$5)",
                    )
                    .bind(context.workspace_id.as_uuid())
                    .bind(job_id.as_uuid())
                    .bind(ordinal)
                    .bind(candidate_effect_id.as_uuid())
                    .bind(candidate_evidence_id.as_uuid())
                    .fetch_one(transaction.connection())
                    .await
                    .map_err(storage_error)?
                }
                _ => {
                    return Err(ApplicationError::Storage(
                        "qualification dispatch reservation returned a partial durable tuple"
                            .into(),
                    ));
                }
            };
        transaction.commit().await.map_err(storage_error)?;
        let intent: ExternalEffectIntent =
            serde_json::from_value(persisted_payload).map_err(|_| {
                ApplicationError::Storage("stored qualification effect intent is malformed".into())
            })?;
        if intent.id().as_uuid() != effect_id
            || intent.workspace_id() != context.workspace_id
            || intent.adapter() != "openai-compatible"
        {
            return Err(ApplicationError::Storage(
                "stored qualification effect intent indexed fields disagree".into(),
            ));
        }
        Ok((intent, ModelRequestEvidenceId::from_uuid(evidence_id)))
    }

    async fn skips_prerequisite(
        &self,
        context: &RequestContext,
        job_id: QualificationJobId,
        ordinal: &str,
    ) -> Result<bool, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let skipped: bool = sqlx::query_scalar(
            "SELECT CASE
                WHEN $3='35' THEN EXISTS (
                    SELECT 1 FROM qualification_probe_results
                     WHERE workspace_id=$1 AND qualification_job_id=$2
                       AND probe_ordinal='30' AND result <> 'pass'
                )
                WHEN $3 IN ('50','60') THEN EXISTS (
                    SELECT 1 FROM qualification_probe_results
                     WHERE workspace_id=$1 AND qualification_job_id=$2
                       AND probe_ordinal='40' AND result <> 'pass'
                )
                ELSE FALSE END",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(job_id.as_uuid())
        .bind(ordinal)
        .fetch_one(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(skipped)
    }
}

#[async_trait]
impl QualificationProbeRunner for PgQualificationProbeRunner {
    async fn run_network_probe(
        &self,
        context: &RequestContext,
        job_id: QualificationJobId,
        ordinal: &str,
    ) -> Result<QualificationProbeCompletion, ApplicationError> {
        let routing = self.routing(context, job_id).await?;
        let (intent, evidence_id) = self
            .durable_q1_input(context, job_id, ordinal, &routing)
            .await?;
        let request = provider_dispatch_request(
            context,
            &intent,
            evidence_id,
            job_id,
            ordinal,
            &routing,
            self.worker_id,
        )?;
        let ProviderDispatchOutcome::Prepared {
            authority,
            target,
            q1_request,
            auth,
            ..
        } = self.dispatch.prepare_dispatch(request).await?
        else {
            return Err(ApplicationError::Conflict(
                "qualification q1 dispatch was not admitted for its original effect".into(),
            ));
        };
        let q1_request = q1_request.ok_or_else(|| {
            ApplicationError::Policy(
                "shared dispatch omitted the q1 request for qualification".into(),
            )
        })?;
        let skipped = self.skips_prerequisite(context, job_id, ordinal).await?;
        let (result, adapter_result) = if skipped {
            (
                QualificationProbeResult::SkippedPrerequisite,
                AdapterDispatchResult::acknowledged(
                    "skipped_prerequisite",
                    None,
                    None,
                    vec![format!("q1_probe:{ordinal}")],
                ),
            )
        } else {
            q1_outcome(
                ordinal,
                self.adapter
                    .execute(target.kind, &target.runtime_base_url, auth, *q1_request)
                    .await,
            )
        };
        let receipt = q1_receipt(&intent, &authority, adapter_result)?;
        self.dispatch
            .complete_post_network(
                context,
                ProviderPostNetworkCompletion {
                    authority: *authority,
                    receipt,
                    throttle: None,
                },
            )
            .await?;
        Ok(QualificationProbeCompletion {
            probe_result_id: uuid::Uuid::now_v7(),
            job_id,
            ordinal: ordinal.to_owned(),
            result,
            external_effect_id: Some(intent.id().as_uuid()),
            model_request_evidence_id: Some(evidence_id.as_uuid()),
        })
    }
}

fn qualification_intent(
    context: &RequestContext,
    routing: &Q1RoutingRow,
) -> Result<ExternalEffectIntent, ApplicationError> {
    ExternalEffectIntent::new(
        "workspace://",
        context.workspace_id,
        context.principal_id,
        "openai-compatible",
        "qualification_probe",
        routing.runtime_base_url.clone(),
        "sha256:openai-q1-pinned-manifest-v1",
        "execute one immutable OpenAI q1 probe",
        vec![
            EffectPrecondition::new(
                "qualification_target_binding",
                routing.target_binding_id.to_string(),
            )
            .map_err(|error| ApplicationError::Policy(error.to_string()))?,
        ],
        "sha256:qualification-target-binding-v1",
        RiskCategory::Medium,
        EffectReversibility::Unknown,
        IdempotencyProfile::ProviderKey,
        DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        None::<String>,
        None::<String>,
        Utc::now(),
    )
    .map_err(|error| ApplicationError::Policy(error.to_string()))
}

fn provider_dispatch_request(
    context: &RequestContext,
    intent: &ExternalEffectIntent,
    evidence_id: ModelRequestEvidenceId,
    job_id: QualificationJobId,
    ordinal: &str,
    routing: &Q1RoutingRow,
    worker_id: WorkerId,
) -> Result<ProviderDispatchRequest, ApplicationError> {
    let now = Utc::now();
    let credential = match routing.branch.as_str() {
        "no_auth" => None,
        "credential" => Some(ProviderDispatchCredential {
            lease_id: uuid::Uuid::now_v7(),
            credential_slot_id: CredentialSlotId::from_uuid(
                routing.credential_slot_id.ok_or_else(|| {
                    ApplicationError::Policy(
                        "qualification credential target lacks its slot".into(),
                    )
                })?,
            ),
            credential_revision_id: routing.credential_revision_id.ok_or_else(|| {
                ApplicationError::Policy(
                    "qualification credential target lacks its revision".into(),
                )
            })?,
            credential_activation_guard_id: routing.credential_activation_guard_id.ok_or_else(
                || {
                    ApplicationError::Policy(
                        "qualification credential target lacks its guard".into(),
                    )
                },
            )?,
            destination_authority: destination_authority(&routing.runtime_base_url)?,
            auth_mode: routing.auth_mode.clone(),
        }),
        _ => {
            return Err(ApplicationError::Policy(
                "qualification target has an unknown branch".into(),
            ));
        }
    };
    Ok(ProviderDispatchRequest {
        context: context.clone(),
        intent: intent.clone(),
        model_request_evidence_id: evidence_id,
        connection_id: ConnectionId::from_uuid(routing.connection_id),
        connection_revision_id: ConnectionRevisionId::from_uuid(routing.connection_revision_id),
        cause: ProviderDispatchCause::QualificationProbe {
            qualification_job_id: job_id,
            qualification_target_id: routing.target_binding_id,
            probe_ordinal: ordinal.to_owned(),
        },
        admission_id: uuid::Uuid::now_v7(),
        wait_id: uuid::Uuid::now_v7(),
        concurrency_lease_id: uuid::Uuid::now_v7(),
        dispatch_ttl_seconds: 60,
        worker_id,
        credential,
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "provider.qualification.dispatch_prepared",
            "external_effect",
            intent.id().as_uuid(),
            serde_json::json!({"phase":"qualification_q1_pre_dispatch","probe_ordinal":ordinal}),
            now,
        )
        .map_err(|error| ApplicationError::Policy(error.to_string()))?,
        idempotency: Some(IdempotencyRecord {
            idempotency_key: format!("provider-dispatch:{}", intent.id()),
            workspace_id: context.workspace_id,
            request_hash: "qualification-q1-dispatch-v1".into(),
            response_payload: None,
            status: "completed".into(),
            created_at: now,
            expires_at: now + chrono::Duration::hours(1),
        }),
        outbox: vec![OutboxMessage {
            id: OutboxId::new(),
            workspace_id: context.workspace_id,
            topic: "provider.qualification.dispatch_prepared".into(),
            payload: serde_json::json!({"effect_id":intent.id().as_uuid(),"probe_ordinal":ordinal}),
            created_at: now,
            attempts: 0,
        }],
    })
}

/// The safe authority a credential lease is bound to.
///
/// Shared with governed Run-step dispatch: both branches must derive the same
/// authority from the same pinned revision URL, or a lease issued for one
/// could be presented against the other.
pub(crate) fn destination_authority(runtime_base_url: &str) -> Result<String, ApplicationError> {
    let parsed = reqwest::Url::parse(runtime_base_url)
        .map_err(|_| ApplicationError::Policy("qualification runtime URL is malformed".into()))?;
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ApplicationError::Policy(
            "qualification runtime URL contains forbidden authority components".into(),
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| ApplicationError::Policy("qualification runtime URL lacks its host".into()))?
        .to_ascii_lowercase();
    Ok(match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

fn q1_outcome(
    ordinal: &str,
    response: Result<Q1ProbeResponse, Q1ProbeFailure>,
) -> (QualificationProbeResult, AdapterDispatchResult) {
    match response {
        Ok(_) => (
            QualificationProbeResult::Pass,
            AdapterDispatchResult::acknowledged(
                "q1_oracle_pass",
                None,
                None,
                vec![format!("q1_probe:{ordinal}")],
            ),
        ),
        Err(Q1ProbeFailure::HttpStatus { status }) => {
            let optional = matches!(ordinal, "30" | "35" | "40" | "50" | "60" | "70" | "80")
                && matches!(status, 400 | 404 | 405 | 415 | 422);
            if optional {
                // The provider definitely answered this optional request. The
                // probe is unsupported, but the external effect itself is an
                // acknowledged observation rather than a failed delivery.
                (
                    QualificationProbeResult::UnsupportedDefinite,
                    AdapterDispatchResult::acknowledged(
                        format!("http_status_{status}"),
                        None,
                        None,
                        vec![format!("q1_probe:{ordinal}")],
                    ),
                )
            } else {
                (
                    QualificationProbeResult::FailedDefinite,
                    AdapterDispatchResult::failed(
                        format!("http_status_{status}"),
                        vec![format!("q1_probe:{ordinal}")],
                    ),
                )
            }
        }
        Err(Q1ProbeFailure::OracleViolation) => (
            QualificationProbeResult::FailedDefinite,
            AdapterDispatchResult::failed(
                "q1_oracle_violation",
                vec![format!("q1_probe:{ordinal}")],
            ),
        ),
        Err(Q1ProbeFailure::StructuralFailure) => (
            QualificationProbeResult::FailedDefinite,
            AdapterDispatchResult::failed(
                "q1_structural_failure",
                vec![format!("q1_probe:{ordinal}")],
            ),
        ),
        Err(Q1ProbeFailure::TransportFailure) => (
            QualificationProbeResult::InconclusiveUnknown,
            AdapterDispatchResult::unknown(
                "q1_transport_failure",
                vec![format!("q1_probe:{ordinal}")],
            ),
        ),
    }
}

fn q1_receipt(
    intent: &ExternalEffectIntent,
    authority: &ProviderDispatchAuthority,
    result: AdapterDispatchResult,
) -> Result<vestrace_domain::ExternalEffectReceipt, ApplicationError> {
    let authorization = EffectAuthorization::allow(
        authority.authorization_id.to_string(),
        "provider-dispatch-v1",
        intent.workspace_id(),
        intent.actor_id(),
        intent.required_capability(),
        intent.operation(),
        intent.target(),
    );
    let authorized = intent
        .authorize(&authorization)
        .map_err(|error| ApplicationError::Policy(error.to_string()))?;
    let adapter = RecordedQ1EffectAdapter::new(intent, result)?;
    authorized
        .dispatch(&adapter, intent.precondition_digest(), Utc::now())
        .map_err(|error| {
            ApplicationError::Policy(format!("q1 receipt construction refused: {error:?}"))
        })
}

struct RecordedQ1EffectAdapter {
    descriptor: ExternalEffectAdapterDescriptor,
    result: AdapterDispatchResult,
}

impl RecordedQ1EffectAdapter {
    fn new(
        intent: &ExternalEffectIntent,
        result: AdapterDispatchResult,
    ) -> Result<Self, ApplicationError> {
        Ok(Self {
            descriptor: ExternalEffectAdapterDescriptor::new(
                intent.adapter(),
                None,
                intent.delivery_semantics(),
                intent.idempotency_profile(),
                intent.reversibility(),
                DryRunMode::Unsupported,
                true,
                false,
                intent.required_capability(),
            )
            .map_err(|error| ApplicationError::Policy(error.to_string()))?,
            result,
        })
    }
}

impl ExternalEffectAdapter for RecordedQ1EffectAdapter {
    fn descriptor(&self) -> &ExternalEffectAdapterDescriptor {
        &self.descriptor
    }

    fn dispatch(
        &self,
        _intent: &ExternalEffectIntent,
    ) -> Result<AdapterDispatchResult, vestrace_domain::AdapterError> {
        Ok(self.result.clone())
    }
}

type TargetValues = (
    &'static str,
    Option<uuid::Uuid>,
    Option<uuid::Uuid>,
    Option<uuid::Uuid>,
    Option<i64>,
    Option<uuid::Uuid>,
);

fn target_values(target: &QualificationTargetBinding) -> Result<TargetValues, ApplicationError> {
    match target {
        QualificationTargetBinding::Credential {
            revision_id,
            slot_id,
            activation_guard_id,
            expected_slot_version,
        } => Ok((
            "credential",
            Some(revision_id.as_uuid()),
            Some(slot_id.as_uuid()),
            Some(activation_guard_id.as_uuid()),
            Some(i64::try_from(*expected_slot_version).map_err(|_| {
                ApplicationError::Policy("qualification slot version exceeds BIGINT".to_owned())
            })?),
            None,
        )),
        QualificationTargetBinding::NoAuth {
            binding_revision_id,
        } => Ok((
            "no_auth",
            None,
            None,
            None,
            None,
            Some(binding_revision_id.as_uuid()),
        )),
    }
}

fn probe_result(value: vestrace_domain::QualificationProbeResult) -> &'static str {
    match value {
        vestrace_domain::QualificationProbeResult::Pass => "pass",
        vestrace_domain::QualificationProbeResult::UnsupportedDefinite => "unsupported_definite",
        vestrace_domain::QualificationProbeResult::FailedDefinite => "failed_definite",
        vestrace_domain::QualificationProbeResult::InconclusiveUnknown => "inconclusive_unknown",
        vestrace_domain::QualificationProbeResult::SkippedPrerequisite => "skipped_prerequisite",
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
