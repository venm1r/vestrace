use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::external_effects::{
    ExternalEffectIntent, ExternalEffectReceipt, ExternalReconciliation,
};
use vestrace_domain::{
    DomainError, ExternalEffectId, ExternalEffectReceiptId, ExternalReconciliationId, Timestamp,
};

use crate::{ApplicationError, RequestContext};

/// Durable external-effect evidence.
///
/// # Why every method takes a context
///
/// It did not. `find_intent`, `find_receipt` and `find_reconciliation` took an
/// id and nothing else, and the adapter selected on that id alone across every
/// tenant — a receipt carries the external resource id and the provider's
/// response digest, so this was not a metadata leak.
///
/// A receipt and a reconciliation carry no workspace of their own: they belong
/// to an effect, and the effect belongs to a tenant. That is right in the
/// domain and unusable in storage, which is why migration 0144 gives their
/// tables the column and a composite foreign key that keeps it equal to the
/// intent's. The context is what supplies it — never the record, which would
/// make the check pass by construction.
#[async_trait]
pub trait ExternalEffectRepository: Send + Sync {
    async fn insert_intent(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
    ) -> Result<(), ApplicationError>;

    async fn find_intent(
        &self,
        context: &RequestContext,
        id: ExternalEffectId,
    ) -> Result<Option<ExternalEffectIntent>, ApplicationError>;

    async fn insert_receipt(
        &self,
        context: &RequestContext,
        receipt: &ExternalEffectReceipt,
    ) -> Result<(), ApplicationError>;

    async fn find_receipt(
        &self,
        context: &RequestContext,
        id: ExternalEffectReceiptId,
    ) -> Result<Option<ExternalEffectReceipt>, ApplicationError>;

    async fn insert_reconciliation(
        &self,
        context: &RequestContext,
        reconciliation: &ExternalReconciliation,
    ) -> Result<(), ApplicationError>;

    async fn find_reconciliation(
        &self,
        context: &RequestContext,
        id: ExternalReconciliationId,
    ) -> Result<Option<ExternalReconciliation>, ApplicationError>;

    /// Effects whose outcome nobody knows and which are worth asking about now.
    ///
    /// The workspace comes from the context alone. It used to be a separate
    /// argument, which let a caller pass one workspace and be scoped to
    /// another; there is no legitimate call that wants those to differ.
    ///
    /// `retry_unsettled_before` is the cutoff for asking again: a candidate that
    /// was already asked about, and gave an answer that settled nothing, comes
    /// back only once that attempt is older than this. Without it, "keep asking
    /// about what is still unknown" is a sweep that re-asks every provider on
    /// every tick. The cadence is the caller's to choose, so it is an argument
    /// rather than a constant buried in a query.
    async fn find_reconciliation_candidates(
        &self,
        context: &RequestContext,
        retry_unsettled_before: Timestamp,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError>;

    /// Settled outcomes whose run has not been told, oldest first.
    ///
    /// A settled reconciliation is a fact the run that asked for the effect does
    /// not have. It stays *owed* until something records that it was delivered,
    /// because the append to the run's stream and the insert of the
    /// reconciliation are different aggregates and cannot share a transaction:
    /// without the marker, an append that fails leaves an effect that is settled
    /// — and therefore out of the sweep — attached to a run that never learned.
    async fn find_undelivered_outcomes(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<UndeliveredOutcome>, ApplicationError>;

    /// Record that a settled outcome reached its run, or that there was no run
    /// to reach.
    async fn mark_outcome_delivered(
        &self,
        context: &RequestContext,
        reconciliation_id: ExternalReconciliationId,
        at: Timestamp,
    ) -> Result<(), ApplicationError>;
}

pub type SharedExternalEffectRepository = Arc<dyn ExternalEffectRepository>;

/// A settled outcome that its run has not been told about.
///
/// Carries the intent because that is the only thing that knows which run asked
/// — a reconciliation names an effect and a receipt, not what wanted them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndeliveredOutcome {
    pub reconciliation: ExternalReconciliation,
    pub intent: ExternalEffectIntent,
}

impl UndeliveredOutcome {
    /// The run owed this fact, when there is one.
    ///
    /// `None` for an operator acting directly, and for every effect created
    /// before an execution reference had to be a reference — those cannot be
    /// attributed to anything, which is a debt to nobody rather than a debt
    /// nobody has paid.
    pub fn run_id(&self) -> Option<vestrace_domain::id::AgentRunId> {
        self.intent.execution_run_id()
    }
}

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
