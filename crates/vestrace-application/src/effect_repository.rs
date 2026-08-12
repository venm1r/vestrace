use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::external_effects::{
    ExternalEffectIntent, ExternalEffectReceipt, ExternalReconciliation,
};
use vestrace_domain::{
    DomainError, ExternalEffectId, ExternalEffectReceiptId, ExternalReconciliationId, WorkspaceId,
};

use crate::ApplicationError;

#[async_trait]
pub trait ExternalEffectRepository: Send + Sync {
    async fn insert_intent(&self, intent: &ExternalEffectIntent) -> Result<(), ApplicationError>;

    async fn find_intent(
        &self,
        id: ExternalEffectId,
    ) -> Result<Option<ExternalEffectIntent>, ApplicationError>;

    async fn insert_receipt(&self, receipt: &ExternalEffectReceipt)
    -> Result<(), ApplicationError>;

    async fn find_receipt(
        &self,
        id: ExternalEffectReceiptId,
    ) -> Result<Option<ExternalEffectReceipt>, ApplicationError>;

    async fn insert_reconciliation(
        &self,
        reconciliation: &ExternalReconciliation,
    ) -> Result<(), ApplicationError>;

    async fn find_reconciliation(
        &self,
        id: ExternalReconciliationId,
    ) -> Result<Option<ExternalReconciliation>, ApplicationError>;

    async fn find_reconciliation_candidates(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError>;
}

pub type SharedExternalEffectRepository = Arc<dyn ExternalEffectRepository>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalEffectRecoveryCandidate {
    intent: ExternalEffectIntent,
    receipt: ExternalEffectReceipt,
}

impl ExternalEffectRecoveryCandidate {
    pub fn new(
        intent: ExternalEffectIntent,
        receipt: ExternalEffectReceipt,
    ) -> Result<Self, ApplicationError> {
        if receipt.effect_id() != intent.id() {
            return Err(DomainError::InvalidArgument(
                "external effect recovery receipt does not belong to intent".into(),
            )
            .into());
        }
        if !receipt.requires_reconciliation() {
            return Err(DomainError::PolicyViolation(
                "external effect recovery requires an UNKNOWN receipt".into(),
            )
            .into());
        }
        Ok(Self { intent, receipt })
    }

    pub fn intent(&self) -> &ExternalEffectIntent {
        &self.intent
    }

    pub fn receipt(&self) -> &ExternalEffectReceipt {
        &self.receipt
    }
}
