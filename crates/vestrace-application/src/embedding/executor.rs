//! The one production executor for a governed embedding-job attempt.

use std::sync::Arc;

use chrono::Utc;
use vestrace_domain::{
    AdapterDispatchResult, AuditEvent, EffectAuthorization, EmbeddingJobId, ExternalEffectAdapter,
    ExternalEffectAdapterDescriptor, ExternalEffectId, ExternalEffectReceipt, WorkerId,
};

use crate::run::GovernedModelAdapter;
use crate::{
    ApplicationError, EffectiveModelResponse, IdempotencyRecord, MaterialKeyVault, OutboxMessage,
    ProviderDispatchAuthority, ProviderDispatchCause, ProviderDispatchOutcome,
    ProviderDispatchRequest, ProviderError, ProviderPostNetworkCompletion, RequestContext,
    SharedProviderDispatchRepository,
};

use super::{
    EmbeddingResultCommitter, EmbeddingResultDispatchAuthority,
    EmbeddingResultFinalizationAuthority, EmbeddingResultFinalizationRepository,
    EmbeddingResultFinalizationService, EmbeddingResultPreparationId,
    EmbeddingResultPreparationOutcome, EmbeddingResultPreparationService,
    EmbeddingResultRepository, EmbeddingResultSealer,
};

pub const EMBEDDING_DISPATCH_TTL_SECONDS: u16 = 60;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingExecutionOutcome {
    Succeeded,
    Cancelled,
    FailedDefinite,
    RecoveredUnknown,
    AwaitingDispatchDeadline,
}

/// Executes exactly one durable embedding-job effect.  All persistent
/// authority remains in the collaborators: dispatch owns admission and the
/// effect lifecycle; preparation owns ciphertext; finalization owns Live
/// projection publication and job success.
pub struct EmbeddingExecutor<R, V, S, F, C> {
    dispatch: SharedProviderDispatchRepository,
    preparation: Arc<EmbeddingResultPreparationService<R, V, S>>,
    finalization: Arc<EmbeddingResultFinalizationService<F, V, C>>,
    adapter: Arc<dyn GovernedModelAdapter>,
    worker_id: WorkerId,
}

impl<R, V, S, F, C> EmbeddingExecutor<R, V, S, F, C>
where
    R: EmbeddingResultRepository,
    V: MaterialKeyVault,
    S: EmbeddingResultSealer,
    F: EmbeddingResultFinalizationRepository,
    C: EmbeddingResultCommitter,
{
    pub fn new(
        dispatch: SharedProviderDispatchRepository,
        preparation: Arc<EmbeddingResultPreparationService<R, V, S>>,
        finalization: Arc<EmbeddingResultFinalizationService<F, V, C>>,
        adapter: Arc<dyn GovernedModelAdapter>,
        worker_id: WorkerId,
    ) -> Self {
        Self {
            dispatch,
            preparation,
            finalization,
            adapter,
            worker_id,
        }
    }

    pub async fn execute(
        &self,
        context: &RequestContext,
        job_id: EmbeddingJobId,
    ) -> Result<EmbeddingExecutionOutcome, ApplicationError> {
        let recovery = self
            .dispatch
            .recover_embedding_job_attempt(context, job_id, Utc::now())
            .await?;
        match recovery {
            crate::EmbeddingJobAttemptRecovery::Succeeded => {
                return Ok(EmbeddingExecutionOutcome::Succeeded);
            }
            crate::EmbeddingJobAttemptRecovery::Cancelled => {
                return Ok(EmbeddingExecutionOutcome::Cancelled);
            }
            crate::EmbeddingJobAttemptRecovery::FailedDefinite => {
                return Ok(EmbeddingExecutionOutcome::FailedDefinite);
            }
            crate::EmbeddingJobAttemptRecovery::AdoptedUnknown
            | crate::EmbeddingJobAttemptRecovery::AlreadyUnknown => {
                return Ok(EmbeddingExecutionOutcome::RecoveredUnknown);
            }
            crate::EmbeddingJobAttemptRecovery::AwaitDispatchDeadline => {
                return Ok(EmbeddingExecutionOutcome::AwaitingDispatchDeadline);
            }
            crate::EmbeddingJobAttemptRecovery::ResumeResultPrepared { effect_id } => {
                self.finalization
                    .finalize_recovered(context, job_id, effect_id)
                    .await?;
                return Ok(EmbeddingExecutionOutcome::Succeeded);
            }
            crate::EmbeddingJobAttemptRecovery::ResumeReserved
            | crate::EmbeddingJobAttemptRecovery::ResumeAdmitted => {}
        }

        let plan = self
            .dispatch
            .load_embedding_dispatch_plan(context, job_id)
            .await?;
        if plan.attempt.job_id != job_id || plan.intent.id() != plan.attempt.external_effect_id {
            return Err(ApplicationError::Conflict(
                "the embedding dispatch plan does not describe its exact job effect".into(),
            ));
        }
        let effect_id = plan.attempt.external_effect_id;
        let now = Utc::now();
        let audit = AuditEvent::new(
            vestrace_domain::AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "provider.dispatch.prepared",
            "external_effect",
            effect_id.as_uuid(),
            serde_json::json!({"phase":"pre_dispatch","cause":"embedding_job","job_id":job_id.as_uuid()}),
            now,
        )
        .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        let dispatched = self
            .dispatch
            .prepare_dispatch(ProviderDispatchRequest {
                context: context.clone(),
                intent: plan.intent.clone(),
                model_request_evidence_id: plan.attempt.model_request_evidence_id,
                connection_id: plan.connection_id,
                connection_revision_id: plan.connection_revision_id,
                cause: ProviderDispatchCause::EmbeddingJob {
                    job_id,
                    snapshot_id: plan.attempt.model_binding_snapshot_id,
                },
                admission_id: uuid::Uuid::now_v7(),
                wait_id: uuid::Uuid::now_v7(),
                concurrency_lease_id: uuid::Uuid::now_v7(),
                dispatch_ttl_seconds: EMBEDDING_DISPATCH_TTL_SECONDS,
                worker_id: self.worker_id,
                credential: plan.credential,
                audit,
                idempotency: Some(IdempotencyRecord {
                    idempotency_key: format!("embedding-dispatch:{effect_id}"),
                    workspace_id: context.workspace_id,
                    request_hash: "safe-embedding-dispatch-tuple-v1".into(),
                    response_payload: None,
                    status: "completed".into(),
                    created_at: now,
                    expires_at: now + chrono::Duration::hours(1),
                }),
                outbox: vec![OutboxMessage::new(
                    context.workspace_id,
                    "provider.dispatch.prepared",
                    serde_json::json!({"effect_id":effect_id.as_uuid()}),
                    now,
                )],
            })
            .await?;
        let (authority, target, request, auth) = match dispatched {
            ProviderDispatchOutcome::Prepared {
                authority,
                target,
                request,
                auth,
                ..
            } => (*authority, target, request, auth),
            ProviderDispatchOutcome::Denied { .. } => {
                return Ok(EmbeddingExecutionOutcome::FailedDefinite);
            }
            ProviderDispatchOutcome::Conflict { .. } => {
                return Ok(EmbeddingExecutionOutcome::AwaitingDispatchDeadline);
            }
        };
        let response = self
            .adapter
            .execute(target.kind, &target.runtime_base_url, auth, request)
            .await;
        let response = match response {
            Ok(EffectiveModelResponse::Embeddings(response)) => response,
            Ok(_) => {
                self.record_non_success(
                    context,
                    &plan.intent,
                    authority,
                    AdapterDispatchResult::failed(
                        "provider_response_shape_mismatch",
                        vec!["embedding_job_dispatch".into()],
                    ),
                )
                .await?;
                return Err(ApplicationError::Internal(
                    "the governed adapter answered an embedding job with a non-embedding response"
                        .into(),
                ));
            }
            Err(error) => {
                let application_error = provider_failure(&error);
                let receipt =
                    observed_receipt(&plan.intent, &authority, non_success_result(&error))?;
                if let Err(completion_error) = self
                    .dispatch
                    .complete_post_network(
                        context,
                        ProviderPostNetworkCompletion {
                            authority,
                            receipt,
                            throttle: None,
                        },
                    )
                    .await
                {
                    tracing::error!(error=%completion_error, "embedding provider failure could not be persisted");
                }
                return Err(application_error);
            }
        };
        let prepared = self
            .preparation
            .prepare_with_generated_identities(
                context.clone(),
                EmbeddingResultDispatchAuthority {
                    job_id,
                    effect_id,
                    dispatch: authority,
                },
                response,
            )
            .await?;
        let preparation_id = match prepared {
            EmbeddingResultPreparationOutcome::Prepared { preparation_id }
            | EmbeddingResultPreparationOutcome::ConvergedExisting { preparation_id } => {
                preparation_id
            }
        };
        self.finalize(context, job_id, effect_id, preparation_id)
            .await
    }

    async fn finalize(
        &self,
        context: &RequestContext,
        job_id: EmbeddingJobId,
        effect_id: ExternalEffectId,
        preparation_id: EmbeddingResultPreparationId,
    ) -> Result<EmbeddingExecutionOutcome, ApplicationError> {
        let authority = EmbeddingResultFinalizationAuthority {
            preparation_id,
            job_id,
            effect_id,
        };
        self.finalization.finalize(context, &authority).await?;
        Ok(EmbeddingExecutionOutcome::Succeeded)
    }

    async fn record_non_success(
        &self,
        context: &RequestContext,
        intent: &vestrace_domain::ExternalEffectIntent,
        authority: ProviderDispatchAuthority,
        result: AdapterDispatchResult,
    ) -> Result<(), ApplicationError> {
        self.dispatch
            .complete_post_network(
                context,
                ProviderPostNetworkCompletion {
                    authority: authority.clone(),
                    receipt: observed_receipt(intent, &authority, result)?,
                    throttle: None,
                },
            )
            .await
    }
}

fn provider_failure(error: &ProviderError) -> ApplicationError {
    match error {
        ProviderError::Timeout | ProviderError::RateLimited | ProviderError::Unavailable(_) => {
            ApplicationError::Unavailable(error.to_string())
        }
        ProviderError::CredentialRejected(message) => ApplicationError::InvalidConfiguration(
            format!("the model provider rejected the pinned credential ({message})"),
        ),
        ProviderError::InvalidResponse(message) => ApplicationError::Internal(format!(
            "model provider returned an unusable response: {message}"
        )),
    }
}

fn non_success_result(error: &ProviderError) -> AdapterDispatchResult {
    match error {
        ProviderError::Timeout => AdapterDispatchResult::unknown(
            "provider_timeout",
            vec!["embedding_job_dispatch".into()],
        ),
        ProviderError::RateLimited => AdapterDispatchResult::failed(
            "provider_rate_limited",
            vec!["embedding_job_dispatch".into()],
        ),
        ProviderError::Unavailable(_) => AdapterDispatchResult::failed(
            "provider_unavailable",
            vec!["embedding_job_dispatch".into()],
        ),
        ProviderError::CredentialRejected(_) => AdapterDispatchResult::failed(
            "provider_credential_rejected",
            vec!["embedding_job_dispatch".into()],
        ),
        ProviderError::InvalidResponse(_) => AdapterDispatchResult::failed(
            "provider_invalid_response",
            vec!["embedding_job_dispatch".into()],
        ),
    }
}

struct ObservedProviderAttempt {
    descriptor: ExternalEffectAdapterDescriptor,
    result: AdapterDispatchResult,
}

impl ObservedProviderAttempt {
    fn new(
        intent: &vestrace_domain::ExternalEffectIntent,
        result: AdapterDispatchResult,
    ) -> Result<Self, ApplicationError> {
        Ok(Self {
            descriptor: ExternalEffectAdapterDescriptor::new(
                intent.adapter(),
                None,
                intent.delivery_semantics(),
                intent.idempotency_profile(),
                intent.reversibility(),
                vestrace_domain::DryRunMode::Unsupported,
                true,
                false,
                intent.required_capability(),
            )
            .map_err(|error| ApplicationError::Policy(error.to_string()))?,
            result,
        })
    }
}

impl ExternalEffectAdapter for ObservedProviderAttempt {
    fn descriptor(&self) -> &ExternalEffectAdapterDescriptor {
        &self.descriptor
    }

    fn dispatch(
        &self,
        _intent: &vestrace_domain::ExternalEffectIntent,
    ) -> Result<AdapterDispatchResult, vestrace_domain::AdapterError> {
        Ok(self.result.clone())
    }
}

fn observed_receipt(
    intent: &vestrace_domain::ExternalEffectIntent,
    authority: &ProviderDispatchAuthority,
    result: AdapterDispatchResult,
) -> Result<ExternalEffectReceipt, ApplicationError> {
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
    let adapter = ObservedProviderAttempt::new(intent, result)?;
    authorized
        .dispatch(&adapter, intent.precondition_digest(), Utc::now())
        .map_err(|error| {
            ApplicationError::Policy(format!("embedding receipt construction refused: {error:?}"))
        })
}
