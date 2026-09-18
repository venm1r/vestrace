use vestrace_domain::external_effects::{
    ExternalEffectIntent, ExternalEffectReceipt, ExternalReconciliation, ObservedEffectState,
    reconcile_effect,
};
use vestrace_domain::{DomainError, Timestamp};

use crate::{ApplicationError, RequestContext};

#[derive(Clone, Copy, Debug, Default)]
pub struct ExternalEffectReconciliationService;

impl ExternalEffectReconciliationService {
    pub fn new() -> Self {
        Self
    }

    pub fn reconcile<'a>(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
        receipt: impl Into<Option<&'a ExternalEffectReceipt>>,
        observations: Vec<ObservedEffectState>,
        reconciled_at: Timestamp,
    ) -> Result<ExternalReconciliation, ApplicationError> {
        let receipt = receipt.into();
        if intent.workspace_id() != context.workspace_id {
            return Err(DomainError::PolicyViolation(
                "external effect recovery workspace mismatch".into(),
            )
            .into());
        }
        if receipt.is_some_and(|receipt| !receipt.requires_reconciliation()) {
            return Err(DomainError::PolicyViolation(
                "external effect reconciliation requires an acknowledged or unknown receipt".into(),
            )
            .into());
        }
        reconcile_effect(intent, receipt, observations, reconciled_at)
            .map_err(ApplicationError::from)
    }
}
