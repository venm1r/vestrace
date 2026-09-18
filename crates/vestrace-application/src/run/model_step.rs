//! Executing a run step through the one governed provider attempt.
//!
//! Before this existed, [`super::handlers::ExecuteStepHandler`] moved a step to
//! `Running` and immediately to `Succeeded` without calling anything: every run
//! reported success having done no work. Then a provider executor existed but
//! took its prompt from `AgentRun.objective` and its route from `config.model`,
//! so the thing that was called was chosen by process configuration rather than
//! by anything the Run had agreed to.
//!
//! # What replaced it
//!
//! The step's input is durable encrypted material, its route is the Run's
//! pinned `ModelBindingSnapshot`, and both are rediscovered here rather than
//! carried in the work item. This executor therefore has no routing authority
//! of its own: it cannot name a Connection, a revision, a model, a credential
//! or a base URL, and there is no configuration it could read to acquire one.
//!
//! # Scope
//!
//! Single-shot: one reconstructed request, one adapter call, one published
//! result. That is a real execution and it is not an agent loop; the
//! distinction is stated here so no caller infers the latter.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use vestrace_domain::{
    AdapterDispatchResult, AgentRunId, ArtifactId, ArtifactRevisionId, AuditEventId,
    ConnectionKind, ContentMaterialId, DryRunMode, EffectAuthorization, ExternalEffectAdapter,
    ExternalEffectAdapterDescriptor, ExternalEffectIntent, ExternalEffectReceiptId, IntentNonce,
    MaterialKeyBindingReceipt, MaterialKeyCreationIntentId, MaterialKeyId, ModelExecutionId,
    PolicyDecisionId, PreparedMaterialAttachmentId, RunStepId, WorkItemId, WorkerId, id::OutboxId,
};

use crate::provider_dispatch::{
    ProviderDispatchAuthority, ProviderDispatchCause, ProviderDispatchOutcome,
    ProviderDispatchRequest, RunStepAttemptRecovery, SharedProviderDispatchRepository,
};
use crate::provider_result::{
    FinalizeProviderResult, PrepareProviderResult, PreparedProviderResult,
    ProviderResultIdentities, ProviderResultRepository,
};
use crate::providers::{EffectiveModelRequest, EffectiveModelResponse, ProviderError};
use crate::{ApplicationError, ConnectionAuth, IdempotencyRecord, OutboxMessage, RequestContext};

/// How long one admitted Run-step dispatch may stay live before recovery may
/// call it lost.
///
/// Not configurable: the deadline is enforced by the database from its own
/// clock, and a per-deployment value would let one worker hold a slot the
/// admission function is entitled to reclaim.
pub const RUN_STEP_DISPATCH_TTL_SECONDS: u16 = 60;

/// Which Run step to execute. Nothing else.
///
/// This carried the Run's objective until the input became governed material.
/// It does not any more, and it must not again: a prompt travelling in a work
/// item is a prompt that never passed the data-policy boundary, and a title is
/// public display metadata that was never meant to reach a provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepModelRequest {
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
}

/// What one governed attempt did.
///
/// Closed and content-free by construction. The provider's output reaches
/// durable storage as encrypted material through the result finalizer and
/// never travels back through this value: an outcome that carried the
/// completion would put model output into a run event, where it could never be
/// purged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepModelOutcome {
    /// The result finalizer already advanced the Run step. The caller must not
    /// complete the step a second time.
    Published,
    /// Policy refused before any adapter call. The decision is durable.
    Denied { authorization_id: PolicyDecisionId },
    /// This step could not be dispatched now, and nothing was called. Either
    /// another owner holds a live dispatch, or admission is saturated and said
    /// when to come back. The step is not finished either way, so the caller
    /// must come back rather than drop it.
    Conflict { retry_after_seconds: Option<u32> },
    /// A dispatch left the process and its outcome cannot be established. The
    /// original effect was adopted as `Unknown`; the adapter was not called
    /// again and must not be.
    RecoveredUnknown,
}

#[async_trait]
pub trait StepModelExecutor: Send + Sync {
    async fn execute(
        &self,
        context: &RequestContext,
        request: StepModelRequest,
    ) -> Result<StepModelOutcome, ApplicationError>;
}

pub type SharedStepModelExecutor = Arc<dyn StepModelExecutor>;

/// The one seam a governed Run step may reach a network through.
///
/// It takes the destination by value from [`ProviderDispatchOutcome::Prepared`]
/// and cannot look one up: `kind` and `runtime_base_url` come from the pinned
/// revision, and `auth` is the single owned credential the dispatch
/// transaction consumed its lease to produce. An implementation that resolved
/// any of them itself would be routing.
#[async_trait]
pub trait GovernedModelAdapter: Send + Sync {
    async fn execute(
        &self,
        kind: ConnectionKind,
        runtime_base_url: &str,
        auth: ConnectionAuth,
        request: EffectiveModelRequest,
    ) -> Result<EffectiveModelResponse, ProviderError>;
}

/// Executes one durable Run-step attempt and publishes its result.
///
/// Every dependency is an authority that already existed: the dispatch
/// repository owns admission and the reconstructed request, the result
/// repository owns encryption and publication, and the adapter owns exactly
/// one network call. This type owns only their order.
pub struct GovernedProviderStepExecutor<R> {
    dispatch: SharedProviderDispatchRepository,
    results: Arc<R>,
    adapter: Arc<dyn GovernedModelAdapter>,
    worker_id: WorkerId,
}

impl<R> GovernedProviderStepExecutor<R>
where
    R: ProviderResultRepository,
{
    pub fn new(
        dispatch: SharedProviderDispatchRepository,
        results: Arc<R>,
        adapter: Arc<dyn GovernedModelAdapter>,
        worker_id: WorkerId,
    ) -> Self {
        Self {
            dispatch,
            results,
            adapter,
            worker_id,
        }
    }

    /// Publish an already-prepared result.
    ///
    /// Reached both by the ordinary path and by a restart that found the
    /// attempt in `ResultPrepared`. The provider is not called on either: the
    /// content is already durable, and the only thing left is to bind it and
    /// advance the Run.
    async fn publish(
        &self,
        context: &RequestContext,
        prepared: PreparedProviderResult,
    ) -> Result<StepModelOutcome, ApplicationError> {
        self.results
            .finalize(
                context,
                FinalizeProviderResult {
                    prepared,
                    binding_receipt: MaterialKeyBindingReceipt::new(),
                },
            )
            .await?;
        Ok(StepModelOutcome::Published)
    }
}

/// Fresh identities for one result.
///
/// Allocated by the caller rather than by the database so that the atomic
/// finalizer can require the exact tuple it was given: identities chosen
/// inside the transaction could not be named by a predicate that runs before
/// it.
fn result_identities() -> ProviderResultIdentities {
    ProviderResultIdentities {
        material_intent_id: MaterialKeyCreationIntentId::new(),
        content_material_id: ContentMaterialId::new(),
        material_key_id: MaterialKeyId::new(),
        intent_nonce: IntentNonce::new(),
        prepared_attachment_id: PreparedMaterialAttachmentId::new(),
        receipt_id: ExternalEffectReceiptId::new(),
        artifact_id: ArtifactId::new(),
        artifact_revision_id: ArtifactRevisionId::new(),
        model_execution_id: ModelExecutionId::new(),
        advance_work_item_id: WorkItemId::new(),
    }
}

/// Map a provider failure onto the application's vocabulary.
///
/// Rate limiting and timeouts are `Unavailable` — the run may succeed on a
/// later attempt — while a malformed response is `Internal`, because retrying
/// an adapter that cannot read the provider's replies will not help.
///
/// A rejected credential is `InvalidConfiguration`: also not worth retrying,
/// but it names something the operator owns and can fix, which `Internal` does
/// not.
fn provider_failure(error: ProviderError) -> ApplicationError {
    match error {
        ProviderError::Timeout | ProviderError::RateLimited | ProviderError::Unavailable(_) => {
            ApplicationError::Unavailable(error.to_string())
        }
        ProviderError::CredentialRejected(message) => {
            ApplicationError::InvalidConfiguration(format!(
                "the model provider rejected the pinned credential ({message}); \
                 rotate the credential bound to this Connection revision"
            ))
        }
        ProviderError::InvalidResponse(message) => ApplicationError::Internal(format!(
            "model provider returned an unusable response: {message}"
        )),
    }
}

/// The closed response class recorded on a non-success receipt.
///
/// A timeout is `unknown` rather than `failed`: the request may well have been
/// performed, and a receipt that called it a failure would licence a second
/// call for an effect that already happened.
fn non_success_result(error: &ProviderError) -> AdapterDispatchResult {
    match error {
        ProviderError::Timeout => {
            AdapterDispatchResult::unknown("provider_timeout", vec!["run_step_dispatch".to_owned()])
        }
        ProviderError::RateLimited => AdapterDispatchResult::failed(
            "provider_rate_limited",
            vec!["run_step_dispatch".to_owned()],
        ),
        ProviderError::Unavailable(_) => AdapterDispatchResult::failed(
            "provider_unavailable",
            vec!["run_step_dispatch".to_owned()],
        ),
        ProviderError::CredentialRejected(_) => AdapterDispatchResult::failed(
            "provider_credential_rejected",
            vec!["run_step_dispatch".to_owned()],
        ),
        ProviderError::InvalidResponse(_) => AdapterDispatchResult::failed(
            "provider_invalid_response",
            vec!["run_step_dispatch".to_owned()],
        ),
    }
}

/// Replays one already-observed adapter outcome so the shared completion
/// authority can build its receipt.
///
/// The call has happened by the time this exists; the domain still requires an
/// adapter to author a receipt, so this one reports what was seen and performs
/// nothing.
struct ObservedProviderAttempt {
    descriptor: ExternalEffectAdapterDescriptor,
    result: AdapterDispatchResult,
}

impl ObservedProviderAttempt {
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

impl ExternalEffectAdapter for ObservedProviderAttempt {
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

fn observed_receipt(
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
    let adapter = ObservedProviderAttempt::new(intent, result)?;
    authorized
        .dispatch(&adapter, intent.precondition_digest(), Utc::now())
        .map_err(|error| {
            ApplicationError::Policy(format!("Run-step receipt construction refused: {error:?}"))
        })
}

#[async_trait]
impl<R> StepModelExecutor for GovernedProviderStepExecutor<R>
where
    R: ProviderResultRepository + 'static,
{
    async fn execute(
        &self,
        context: &RequestContext,
        request: StepModelRequest,
    ) -> Result<StepModelOutcome, ApplicationError> {
        let StepModelRequest { run_id, step_id } = request;

        // Classification comes first and touches no adapter. A restart that
        // called the provider before asking where its previous attempt got to
        // is exactly the duplicate this ordering exists to prevent.
        let recovery = self
            .dispatch
            .recover_run_step_attempt(context, run_id, step_id, Utc::now())
            .await?;
        match recovery {
            RunStepAttemptRecovery::Published => return Ok(StepModelOutcome::Published),
            RunStepAttemptRecovery::AdoptedUnknown | RunStepAttemptRecovery::AlreadyUnknown => {
                return Ok(StepModelOutcome::RecoveredUnknown);
            }
            // Another owner's dispatch is still inside its deadline. It is not
            // ours to call, and it is not ours to fail.
            RunStepAttemptRecovery::AwaitDispatchDeadline => {
                // The owner's deadline is enforced by the database from its own
                // clock, so there is no local interval to quote here.
                return Ok(StepModelOutcome::Conflict {
                    retry_after_seconds: None,
                });
            }
            RunStepAttemptRecovery::ResumeResultPrepared { effect_id } => {
                let prepared = self
                    .results
                    .recover_result_prepared(context, effect_id)
                    .await?;
                return self.publish(context, prepared).await;
            }
            RunStepAttemptRecovery::ResumeReserved | RunStepAttemptRecovery::ResumeAdmitted => {}
        }

        let plan = self
            .dispatch
            .load_run_step_dispatch_plan(context, run_id, step_id)
            .await?;
        // Dispatch takes its effect from the intent and everything downstream
        // takes it from the attempt. If those ever disagreed, one effect would
        // be admitted and a different one completed, so the executor proves
        // they are the same rather than assuming its loader did.
        if plan.intent.id() != plan.attempt.external_effect_id
            || plan.attempt.run_id != run_id
            || plan.attempt.step_id != step_id
        {
            return Err(ApplicationError::Conflict(
                "the Run-step dispatch plan does not describe this step's attempt".to_owned(),
            ));
        }
        let effect_id = plan.attempt.external_effect_id;
        let now = Utc::now();
        let audit = vestrace_domain::AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "provider.dispatch.prepared",
            "external_effect",
            effect_id.as_uuid(),
            serde_json::json!({
                "phase": "pre_dispatch",
                "cause": "run_step",
                "run_id": run_id.as_uuid(),
                "step_id": step_id.as_uuid(),
            }),
            now,
        )
        .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        let outcome = self
            .dispatch
            .prepare_dispatch(ProviderDispatchRequest {
                context: context.clone(),
                intent: plan.intent.clone(),
                model_request_evidence_id: plan.attempt.model_request_evidence_id,
                connection_id: plan.connection_id,
                connection_revision_id: plan.connection_revision_id,
                cause: ProviderDispatchCause::RunStep {
                    run_id,
                    step_id,
                    snapshot_id: plan.attempt.model_binding_snapshot_id,
                },
                admission_id: uuid::Uuid::now_v7(),
                wait_id: uuid::Uuid::now_v7(),
                concurrency_lease_id: uuid::Uuid::now_v7(),
                dispatch_ttl_seconds: RUN_STEP_DISPATCH_TTL_SECONDS,
                worker_id: self.worker_id,
                credential: plan.credential,
                audit,
                idempotency: Some(IdempotencyRecord {
                    idempotency_key: format!("provider-dispatch:{effect_id}"),
                    workspace_id: context.workspace_id,
                    request_hash: "safe-provider-dispatch-tuple-v1".to_owned(),
                    response_payload: None,
                    status: "completed".to_owned(),
                    created_at: now,
                    expires_at: now + chrono::Duration::hours(1),
                }),
                outbox: vec![OutboxMessage {
                    id: OutboxId::new(),
                    workspace_id: context.workspace_id,
                    topic: "provider.dispatch.prepared".to_owned(),
                    payload: serde_json::json!({ "effect_id": effect_id.as_uuid() }),
                    created_at: now,
                    attempts: 0,
                }],
            })
            .await?;

        let (authority, target, effective_request, auth) = match outcome {
            ProviderDispatchOutcome::Prepared {
                authority,
                target,
                request,
                auth,
                ..
            } => (*authority, target, request, auth),
            ProviderDispatchOutcome::Denied { authorization_id } => {
                return Ok(StepModelOutcome::Denied { authorization_id });
            }
            // Admission is saturated or throttled. It said when to come back,
            // and that is the whole content of the answer.
            ProviderDispatchOutcome::Conflict {
                retry_after_seconds,
                ..
            } => {
                return Ok(StepModelOutcome::Conflict {
                    retry_after_seconds,
                });
            }
        };

        // The one call. Everything before it was a transaction and everything
        // after it is a transaction; nothing here may be retried in place,
        // because a second call would be a second effect.
        let response = self
            .adapter
            .execute(
                target.kind,
                &target.runtime_base_url,
                auth,
                effective_request,
            )
            .await;

        let result = match response {
            Ok(EffectiveModelResponse::ChatCompletions(result)) => result,
            Ok(_) => {
                // The reconstructed request and the adapter disagree about what
                // was asked. That is a definite failure of this attempt, and it
                // is recorded as one before the error is returned.
                self.dispatch
                    .complete_post_network(
                        context,
                        crate::provider_dispatch::ProviderPostNetworkCompletion {
                            authority: authority.clone(),
                            receipt: observed_receipt(
                                &plan.intent,
                                &authority,
                                AdapterDispatchResult::failed(
                                    "provider_response_shape_mismatch",
                                    vec!["run_step_dispatch".to_owned()],
                                ),
                            )?,
                            throttle: None,
                        },
                    )
                    .await?;
                return Err(ApplicationError::Internal(
                    "the governed adapter answered a Run step with a non-chat response".to_owned(),
                ));
            }
            Err(error) => {
                // Recorded before returning, so a failing provider is visible
                // in the effect's own history rather than only in logs. A
                // failure to record must not mask the original error.
                let receipt =
                    observed_receipt(&plan.intent, &authority, non_success_result(&error))?;
                if let Err(completion_error) = self
                    .dispatch
                    .complete_post_network(
                        context,
                        crate::provider_dispatch::ProviderPostNetworkCompletion {
                            authority: authority.clone(),
                            receipt,
                            throttle: None,
                        },
                    )
                    .await
                {
                    tracing::error!(
                        error = %completion_error,
                        "a Run-step provider attempt failed and its completion could not be recorded"
                    );
                }
                return Err(provider_failure(error));
            }
        };

        let prepared = self
            .results
            .prepare_after_dispatch(
                PrepareProviderResult {
                    context: context.clone(),
                    effect_id,
                    run_id,
                    step_id,
                    identities: result_identities(),
                    result,
                },
                &authority,
            )
            .await?;
        self.publish(context, prepared).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retryable_provider_failure_is_reported_as_unavailable() {
        assert!(matches!(
            provider_failure(ProviderError::RateLimited),
            ApplicationError::Unavailable(_)
        ));
        assert!(matches!(
            provider_failure(ProviderError::Timeout),
            ApplicationError::Unavailable(_)
        ));
    }

    #[test]
    fn a_rejected_credential_names_what_the_operator_must_replace() {
        // Reported as configuration rather than an internal fault: this is the
        // one provider failure the operator can act on, and the message has to
        // say so, or a live run fails with an error that reads like a bug.
        let error = provider_failure(ProviderError::CredentialRejected(
            "provider returned 403 Forbidden".into(),
        ));
        assert!(matches!(error, ApplicationError::InvalidConfiguration(_)));
        let rendered = error.to_string();
        assert!(rendered.contains("403"));
        assert!(rendered.contains("Connection revision"));
    }

    #[test]
    fn an_unusable_response_is_not_reported_as_retryable() {
        // Retrying an adapter that cannot read the provider's replies produces
        // the same unusable reply again.
        assert!(matches!(
            provider_failure(ProviderError::InvalidResponse("no content".into())),
            ApplicationError::Internal(_)
        ));
    }

    #[test]
    fn a_timeout_is_recorded_as_unknown_rather_than_failed() {
        // The request may well have been performed. Calling that a failure
        // would licence a second call for an effect that already happened.
        let rendered = format!("{:?}", non_success_result(&ProviderError::Timeout));
        assert!(rendered.contains("Unknown"), "{rendered}");
        let rendered = format!("{:?}", non_success_result(&ProviderError::RateLimited));
        assert!(rendered.contains("Failed"), "{rendered}");
    }

    #[test]
    fn the_outcome_carries_no_provider_content() {
        // The outcome travels back to a handler that writes run events; model
        // output in such an event could never be purged.
        let rendered = format!("{:?}", StepModelOutcome::Published);
        assert_eq!(rendered, "Published");
        let rendered = format!(
            "{:?}",
            StepModelOutcome::Conflict {
                retry_after_seconds: Some(7)
            }
        );
        assert!(rendered.contains('7'), "{rendered}");
    }

    #[test]
    fn the_request_carries_only_the_step_identity() {
        // A prompt in a work item is a prompt that never passed the data
        // boundary. The type is what stops one being put back.
        let rendered = format!(
            "{:?}",
            StepModelRequest {
                run_id: AgentRunId::new(),
                step_id: RunStepId::new(),
            }
        );
        assert!(rendered.contains("run_id"));
        assert!(rendered.contains("step_id"));
        assert!(!rendered.contains("objective"));
    }
}
