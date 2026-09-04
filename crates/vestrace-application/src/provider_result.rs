//! Transactional retained provider-result authority.

use async_trait::async_trait;
use std::sync::Arc;
use vestrace_domain::{
    AgentRunId, ArtifactId, ArtifactRevisionId, ContentMaterialId, ExternalEffectId,
    ExternalEffectReceiptId, IntentNonce, MaterialKeyBindingReceipt, MaterialKeyCreationIntentId,
    MaterialKeyId, ModelExecutionId, PreparedMaterialAttachmentId, RunStepId, SizeClass,
    WorkItemId,
};

use crate::{
    ApplicationError, EffectiveChatEvidence, EffectiveChatFinishReason, EffectiveChatResult,
    ProviderDispatchAuthority, ProviderUsage, RequestContext, UnitOfWork,
};

pub const PROVIDER_RESULT_CONFLICT: &str = "PROVIDER_RESULT_CONFLICT";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderResultIdentities {
    pub material_intent_id: MaterialKeyCreationIntentId,
    pub content_material_id: ContentMaterialId,
    pub material_key_id: MaterialKeyId,
    pub intent_nonce: IntentNonce,
    pub prepared_attachment_id: PreparedMaterialAttachmentId,
    pub receipt_id: ExternalEffectReceiptId,
    pub artifact_id: ArtifactId,
    pub artifact_revision_id: ArtifactRevisionId,
    pub model_execution_id: ModelExecutionId,
    pub advance_work_item_id: WorkItemId,
}

/// Safe, non-content result facts stored beside the definite provider receipt.
///
/// The guarded two-argument receipt witness consumes this typed sidecar from
/// the already-inserted receipt payload. Keeping it separate from retained
/// content preserves the frozen witness signature and makes restart recovery
/// independent of an in-memory adapter result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderResultReceiptEvidence {
    advance_work_item_id: WorkItemId,
    finish_reason: &'static str,
    usage_known: bool,
    prompt_tokens: Option<i32>,
    completion_tokens: Option<i32>,
}

impl ProviderResultReceiptEvidence {
    pub fn new(
        advance_work_item_id: WorkItemId,
        evidence: EffectiveChatEvidence,
        usage: &ProviderUsage,
    ) -> Result<Self, ApplicationError> {
        let finish_reason = match evidence {
            EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop) => "stop",
            EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Length) => "length",
            EffectiveChatEvidence::Completed(EffectiveChatFinishReason::ToolCalls) => "tool_calls",
            EffectiveChatEvidence::Completed(EffectiveChatFinishReason::ContentFilter) => {
                "content_filter"
            }
        };
        let (usage_known, prompt_tokens, completion_tokens) = match usage {
            ProviderUsage::Known {
                prompt_tokens,
                completion_tokens,
            } => (
                true,
                Some(i32::try_from(*prompt_tokens).map_err(|_| {
                    ApplicationError::Policy("provider usage exceeds storage bounds".into())
                })?),
                Some(i32::try_from(*completion_tokens).map_err(|_| {
                    ApplicationError::Policy("provider usage exceeds storage bounds".into())
                })?),
            ),
            ProviderUsage::Unknown => (false, None, None),
        };
        Ok(Self {
            advance_work_item_id,
            finish_reason,
            usage_known,
            prompt_tokens,
            completion_tokens,
        })
    }

    pub const fn advance_work_item_id(self) -> WorkItemId {
        self.advance_work_item_id
    }

    pub const fn finish_reason(self) -> &'static str {
        self.finish_reason
    }

    pub const fn usage_known(self) -> bool {
        self.usage_known
    }

    pub const fn prompt_tokens(self) -> Option<i32> {
        self.prompt_tokens
    }

    pub const fn completion_tokens(self) -> Option<i32> {
        self.completion_tokens
    }
}

pub struct PrepareProviderResult {
    pub context: RequestContext,
    pub effect_id: ExternalEffectId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub identities: ProviderResultIdentities,
    pub result: EffectiveChatResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedProviderResult {
    pub preparation_id: uuid::Uuid,
    pub effect_id: ExternalEffectId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub identities: ProviderResultIdentities,
    pub size_class: SizeClass,
    pub evidence: crate::EffectiveChatEvidence,
    pub usage: ProviderUsage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizeProviderResult {
    pub prepared: PreparedProviderResult,
    pub binding_receipt: MaterialKeyBindingReceipt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderResultPublication {
    pub publication_id: uuid::Uuid,
    pub preparation_id: uuid::Uuid,
    pub artifact_id: ArtifactId,
    pub artifact_revision_id: ArtifactRevisionId,
    pub content_material_id: ContentMaterialId,
    pub model_execution_id: ModelExecutionId,
    pub size_class: SizeClass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderResultFaultPoint {
    BeforeReserve,
    AfterReserve,
    AfterVaultCreate,
    AfterVaultReceipt,
    AfterEncryption,
    AfterResultPrepared,
    AfterReceipt,
    AfterWitness,
    AfterBind,
    AfterRuntimeEnvelopes,
    AfterLivePromotion,
    BeforeRunSuccess,
}

pub trait ProviderResultFaultInjector: Send + Sync {
    fn check(&self, point: ProviderResultFaultPoint) -> Result<(), ApplicationError>;
}

pub struct NoProviderResultFaults;

impl ProviderResultFaultInjector for NoProviderResultFaults {
    fn check(&self, _point: ProviderResultFaultPoint) -> Result<(), ApplicationError> {
        Ok(())
    }
}

#[async_trait]
pub trait ProviderResultRepository: Send + Sync {
    async fn prepare(
        &self,
        request: PrepareProviderResult,
    ) -> Result<PreparedProviderResult, ApplicationError>;

    /// Persists a successful provider result under the original dispatch
    /// authority.  The implementation must prepare the retained result,
    /// record and witness its acknowledged receipt, and release the exact
    /// admission lease in one transaction.  Publication is intentionally a
    /// separate finalizer-owned operation.
    async fn prepare_after_dispatch(
        &self,
        request: PrepareProviderResult,
        authority: &ProviderDispatchAuthority,
    ) -> Result<PreparedProviderResult, ApplicationError>;

    async fn recover_result_prepared(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
    ) -> Result<PreparedProviderResult, ApplicationError>;

    async fn finalize(
        &self,
        context: &RequestContext,
        request: FinalizeProviderResult,
    ) -> Result<ProviderResultPublication, ApplicationError>;

    async fn finalize_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        request: FinalizeProviderResult,
    ) -> Result<ProviderResultPublication, ApplicationError>;
}

pub struct ProviderResultFinalizer<R> {
    repository: Arc<R>,
}

impl<R> ProviderResultFinalizer<R>
where
    R: ProviderResultRepository,
{
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    pub async fn prepare(
        &self,
        request: PrepareProviderResult,
    ) -> Result<PreparedProviderResult, ApplicationError> {
        self.repository.prepare(request).await
    }

    pub async fn prepare_after_dispatch(
        &self,
        request: PrepareProviderResult,
        authority: &ProviderDispatchAuthority,
    ) -> Result<PreparedProviderResult, ApplicationError> {
        self.repository
            .prepare_after_dispatch(request, authority)
            .await
    }

    pub async fn recover_result_prepared(
        &self,
        context: &RequestContext,
        effect_id: ExternalEffectId,
    ) -> Result<PreparedProviderResult, ApplicationError> {
        self.repository
            .recover_result_prepared(context, effect_id)
            .await
    }

    pub async fn finalize(
        &self,
        context: &RequestContext,
        request: FinalizeProviderResult,
    ) -> Result<ProviderResultPublication, ApplicationError> {
        self.repository.finalize(context, request).await
    }
}
