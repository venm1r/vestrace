//! Executing a run step by invoking a model.
//!
//! Before this existed, [`super::handlers::ExecuteStepHandler`] moved a step to
//! `Running` and immediately to `Succeeded` without calling anything: every run
//! reported success having done no work. This is the path that makes the step
//! actually happen.
//!
//! # Scope
//!
//! Single-shot: one prompt, one completion, no tool use and no multi-turn loop.
//! The prompt is the run's objective. That is a real execution, and it is not an
//! agent loop; the distinction is stated here so no caller infers the latter.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use vestrace_domain::id::{AgentRunId, ArtifactId, ModelExecutionId, ModelId, RunStepId};
use vestrace_domain::trust::{DataClassification, evaluate_model_boundary};

use crate::artifacts::{ArtifactContent, SharedArtifactRepository};
use crate::models::{ModelExecutionRecord, SharedModelExecutionRepository, SharedModelRepository};
use crate::providers::{GenerationRequest, ProviderError, SharedTextGenerationProviderFactory};
use crate::{
    ApplicationError, ModelDataPolicyDecisionRecord, ModelDataPolicyMode, ModelDataPolicySettings,
    RequestContext, SharedModelDataPolicyDecisionRepository,
};

/// Which model a step's invocation uses.
///
/// Configuration rather than inference: a system that guesses which model to
/// bill and which to record is a system whose cost accounting cannot be
/// reconciled afterwards.
///
/// The model is named, not identified by row id, because `models` is
/// workspace-scoped: one configured UUID could only ever be correct for one
/// workspace. The row is looked up per workspace at execution time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepModelSettings {
    /// The provider's name for the model, sent on the wire and matched against
    /// `models.model_name` in the executing workspace.
    pub model_name: String,
    pub max_tokens: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepModelRequest {
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    /// The prompt. Currently the run's objective.
    pub objective: String,
}

/// What the invocation produced, as references rather than content.
///
/// The completion text is stored as an artifact and named here by digest. The
/// outcome deliberately does not carry the text: it travels into a run event,
/// and an append-only event holding model output could never be purged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepModelOutcome {
    pub artifact_id: ArtifactId,
    pub content_hash: String,
    pub model_execution_id: ModelExecutionId,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub latency_ms: u32,
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

/// Invokes a model, stores the completion, and records the accounting.
pub struct ProviderStepModelExecutor {
    providers: SharedTextGenerationProviderFactory,
    models: SharedModelRepository,
    artifacts: SharedArtifactRepository,
    executions: SharedModelExecutionRepository,
    data_policy_decisions: SharedModelDataPolicyDecisionRepository,
    settings: StepModelSettings,
    data_policy: ModelDataPolicySettings,
}

impl ProviderStepModelExecutor {
    pub fn new(
        providers: SharedTextGenerationProviderFactory,
        models: SharedModelRepository,
        artifacts: SharedArtifactRepository,
        executions: SharedModelExecutionRepository,
        data_policy_decisions: SharedModelDataPolicyDecisionRepository,
        settings: StepModelSettings,
        data_policy: ModelDataPolicySettings,
    ) -> Self {
        Self {
            providers,
            models,
            artifacts,
            executions,
            data_policy_decisions,
            settings,
            data_policy,
        }
    }

    /// Find the workspace's row for the configured model.
    ///
    /// An absent row is a configuration error, not something to invent: the
    /// accounting record has a foreign key to `models`, so a fabricated id
    /// would either fail at insert or attribute the cost to another model.
    async fn model_id(&self, context: &RequestContext) -> Result<ModelId, ApplicationError> {
        let models = self.models.list(context).await?;
        models
            .into_iter()
            .find(|model| model.model_name == self.settings.model_name)
            .map(|model| model.id)
            .ok_or_else(|| {
                ApplicationError::InvalidConfiguration(format!(
                    "no model named {:?} is registered in this workspace",
                    self.settings.model_name
                ))
            })
    }

    /// Record consumption even when the call failed.
    ///
    /// A failed call still costs time, and often still costs tokens. Recording
    /// only successes would make the ledger disagree with the invoice.
    async fn record(
        &self,
        context: &RequestContext,
        model_id: ModelId,
        status: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
        latency_ms: u32,
    ) -> Result<ModelExecutionId, ApplicationError> {
        let record = ModelExecutionRecord {
            id: ModelExecutionId::new(),
            workspace_id: context.workspace_id,
            model_id,
            prompt_tokens,
            completion_tokens,
            latency_ms,
            status: status.to_string(),
            created_at: vestrace_domain::time::now(),
        };
        self.executions.record(context, &record).await?;
        Ok(record.id)
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
                "the model provider rejected the configured credential ({message}); \
                 replace the workspace secret holding the provider API key"
            ))
        }
        ProviderError::InvalidResponse(message) => ApplicationError::Internal(format!(
            "model provider returned an unusable response: {message}"
        )),
    }
}

#[async_trait]
impl StepModelExecutor for ProviderStepModelExecutor {
    async fn execute(
        &self,
        context: &RequestContext,
        request: StepModelRequest,
    ) -> Result<StepModelOutcome, ApplicationError> {
        // Both resolved before the clock starts: a missing model row or an
        // unresolvable credential is a configuration failure, and charging its
        // latency to the provider would misattribute the delay.
        let model_id = self.model_id(context).await?;
        let provider = self.providers.provider_for(context).await?;

        let classification = DataClassification::source(
            self.data_policy.classification,
            "run-objective-channel",
            "deployment configuration policy.data.classification",
        )?;
        let destination = provider.egress.destination();
        // Required capabilities are refused during configuration loading: the
        // worker context does not carry the originating subject, so `false` is
        // the only honest value at this boundary and is unreachable for a
        // configured required capability.
        let decision = evaluate_model_boundary(
            &self.data_policy.policy,
            &classification,
            destination,
            false,
        );
        let reason = if decision.is_allowed() {
            decision.reason().to_owned()
        } else {
            format!("{}; destination {destination:?}", decision.reason())
        };
        let record = ModelDataPolicyDecisionRecord {
            id: uuid::Uuid::now_v7(),
            run_id: request.run_id,
            step_id: request.step_id,
            destination,
            classification: self.data_policy.classification,
            allowed: decision.is_allowed(),
            reason,
            policy_version: decision.policy_version().to_owned(),
            mode: self.data_policy.mode,
            decided_at: vestrace_domain::time::now(),
        };
        // This commit is the disclosure boundary. If it fails, the provider is
        // not called: a crash or storage fault must never leave an unrecorded
        // disclosure behind.
        self.data_policy_decisions.record(&record).await?;
        if !record.allowed && self.data_policy.mode == ModelDataPolicyMode::Enforce {
            return Err(ApplicationError::Policy(format!(
                "model data policy denied destination {destination:?}: {}",
                decision.reason()
            )));
        }

        let started = Instant::now();
        let generation = provider
            .provider
            .generate(GenerationRequest {
                model: self.settings.model_name.clone(),
                prompt: request.objective,
                max_tokens: self.settings.max_tokens,
            })
            .await;

        let latency_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

        let generation = match generation {
            Ok(generation) => generation,
            Err(error) => {
                // Recorded before returning, so a failing provider is visible
                // in the ledger rather than only in logs. A failure to record
                // must not mask the original error.
                if let Err(record_error) = self
                    .record(context, model_id, "failed", 0, 0, latency_ms)
                    .await
                {
                    tracing::error!(
                        error = %record_error,
                        "model execution failed and the failure could not be recorded"
                    );
                }
                return Err(provider_failure(error));
            }
        };

        let content = ArtifactContent {
            media_type: "text/plain; charset=utf-8".to_string(),
            bytes: generation.content.into_bytes(),
        };
        let stored = self
            .artifacts
            .store(
                context,
                &format!(
                    "run-{}-step-{}",
                    request.run_id.as_uuid(),
                    request.step_id.as_uuid()
                ),
                &content,
            )
            .await?;

        let model_execution_id = self
            .record(
                context,
                model_id,
                "succeeded",
                generation.prompt_tokens,
                generation.completion_tokens,
                latency_ms,
            )
            .await?;

        Ok(StepModelOutcome {
            artifact_id: stored.artifact_id,
            content_hash: stored.content_hash,
            model_execution_id,
            prompt_tokens: generation.prompt_tokens,
            completion_tokens: generation.completion_tokens,
            latency_ms,
        })
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
        assert!(rendered.contains("workspace secret"));
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
    fn the_outcome_carries_references_and_not_the_completion_text() {
        // The outcome travels into an append-only run event; model output in
        // such an event could never be purged.
        let outcome = StepModelOutcome {
            artifact_id: ArtifactId::new(),
            content_hash: "a".repeat(64),
            model_execution_id: ModelExecutionId::new(),
            prompt_tokens: 1,
            completion_tokens: 2,
            latency_ms: 3,
        };
        let rendered = format!("{outcome:?}");
        assert!(rendered.contains("content_hash"));
        assert!(!rendered.contains("content:"));
    }
}
