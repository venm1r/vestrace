use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::external_effects::{
    ExternalEffectIntent, ExternalEffectReceipt, ExternalReconciliation,
};
use vestrace_domain::{ExternalEffectId, ExternalEffectReceiptId, ExternalReconciliationId};

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
}

pub type SharedExternalEffectRepository = Arc<dyn ExternalEffectRepository>;
