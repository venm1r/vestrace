use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::Timestamp;
use vestrace_domain::external_effects::{
    ExternalEffectIntent, ExternalEffectReceipt, ExternalReconciliation, ObservedEffectState,
};
use vestrace_domain::id::AgentRunId;

use crate::{
    ApplicationError, ExternalEffectReconciliationService, ExternalEffectRecoveryCandidate,
    RequestContext, SharedExternalEffectRepository,
};

#[async_trait]
pub trait ExternalEffectReadBackAdapter: Send + Sync {
    async fn observe(
        &self,
        intent: &ExternalEffectIntent,
        receipt: &ExternalEffectReceipt,
    ) -> Result<Vec<ObservedEffectState>, ApplicationError>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExternalEffectRecoveryReport {
    reconciliations: Vec<ExternalReconciliation>,
    /// The run each reconciliation belongs to, positionally.
    ///
    /// Carried here rather than looked up later because only the sweep holds the
    /// intent: a reconciliation names an effect and a receipt, not what asked
    /// for them.
    runs: Vec<Option<AgentRunId>>,
    unreachable: Vec<UnreconciledEffect>,
}

/// A candidate the sweep could not ask about.
///
/// Kept apart from a reconciliation on purpose: "we asked and could not tell"
/// is `ReconciliationOutcome::Inconclusive` and is a finding; "we could not ask"
/// is neither, and recording it as inconclusive would put a conclusion in the
/// record that nobody reached.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnreconciledEffect {
    pub effect_id: vestrace_domain::id::ExternalEffectId,
    pub reason: String,
}

impl ExternalEffectRecoveryReport {
    pub fn reconciliations(&self) -> &[ExternalReconciliation] {
        &self.reconciliations
    }

    /// The ones that ended the question: the effect happened, or it did not.
    pub fn settled(&self) -> impl Iterator<Item = &ExternalReconciliation> {
        self.reconciliations
            .iter()
            .filter(|item| item.outcome().is_settled())
    }

    /// The settled ones, each with the run that asked for the effect.
    ///
    /// `None` where the effect was an operator acting directly, or a workflow
    /// execution rather than a run. A settled outcome nobody can attribute is
    /// still worth reporting — it is just worth less, and saying which is which
    /// is the point.
    pub fn settled_with_runs(
        &self,
    ) -> impl Iterator<Item = (&ExternalReconciliation, Option<AgentRunId>)> {
        self.reconciliations
            .iter()
            .zip(self.runs.iter().copied())
            .filter(|(item, _)| item.outcome().is_settled())
    }

    /// The ones that recorded an attempt and answered nothing.
    ///
    /// Reported apart from [`Self::settled`] because a caller that treats them
    /// alike says "an unknown external effect outcome was settled" about an
    /// effect whose outcome is still unknown — which is what the worker did.
    pub fn unsettled(&self) -> impl Iterator<Item = &ExternalReconciliation> {
        self.reconciliations
            .iter()
            .filter(|item| !item.outcome().is_settled())
    }

    /// Candidates still waiting, with why.
    pub fn unreachable(&self) -> &[UnreconciledEffect] {
        &self.unreachable
    }
}

/// How long to leave an effect alone after asking about it and learning nothing.
///
/// The sweep runs on every worker tick. An effect the provider could not tell us
/// about is still an effect whose outcome nobody knows, so it has to come back —
/// but coming back immediately means re-asking every provider several times a
/// second, which is worse than not asking at all. A minute is short enough that
/// a transient provider problem clears within a few attempts, and long enough
/// that the sweep is not itself the load.
///
/// This lives here rather than in the worker because it is a property of
/// reconciliation, not of any particular loop that drives it.
pub const RECONCILIATION_RETRY_AFTER: chrono::Duration = chrono::Duration::minutes(1);

pub struct ExternalEffectRecoveryService {
    repository: SharedExternalEffectRepository,
    read_back: Arc<dyn ExternalEffectReadBackAdapter>,
    reconciliation: ExternalEffectReconciliationService,
}

impl ExternalEffectRecoveryService {
    pub fn new(
        repository: SharedExternalEffectRepository,
        read_back: Arc<dyn ExternalEffectReadBackAdapter>,
    ) -> Self {
        Self {
            repository,
            read_back,
            reconciliation: ExternalEffectReconciliationService::new(),
        }
    }

    pub async fn discover(
        &self,
        context: &RequestContext,
        retry_unsettled_before: Timestamp,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError> {
        self.repository
            .find_reconciliation_candidates(context, retry_unsettled_before)
            .await
    }

    pub async fn reconcile_candidate(
        &self,
        context: &RequestContext,
        candidate: &ExternalEffectRecoveryCandidate,
        reconciled_at: Timestamp,
    ) -> Result<ExternalReconciliation, ApplicationError> {
        let observations = self
            .read_back
            .observe(candidate.intent(), candidate.receipt())
            .await?;
        let reconciliation = self.reconciliation.reconcile(
            context,
            candidate.intent(),
            candidate.receipt(),
            observations,
            reconciled_at,
        )?;
        self.repository
            .insert_reconciliation(context, &reconciliation)
            .await?;
        Ok(reconciliation)
    }

    /// Reconcile every effect whose outcome nobody knows.
    ///
    /// # Why one failure does not end the sweep
    ///
    /// This propagated the first error, so a single unreachable endpoint left
    /// every other effect in the workspace unreconciled — including ones whose
    /// endpoints were answering. The same shape as a drain that stops at its
    /// first failed message: the failure is real and is not a reason to stop
    /// asking about everything else.
    ///
    /// A candidate that could not be asked is reported and left where it is. It
    /// stays a candidate, because nothing about it has been settled.
    ///
    /// So does one that *was* asked and gave an answer settling nothing — but
    /// only after `retry_unsettled_before`. Both are effects whose outcome
    /// nobody knows; the difference is that asking again immediately would just
    /// re-ask every provider on every tick.
    pub async fn run(
        &self,
        context: &RequestContext,
        reconciled_at: Timestamp,
        retry_unsettled_before: Timestamp,
    ) -> Result<ExternalEffectRecoveryReport, ApplicationError> {
        let candidates = self.discover(context, retry_unsettled_before).await?;
        let mut reconciliations = Vec::with_capacity(candidates.len());
        let mut runs = Vec::with_capacity(candidates.len());
        let mut unreachable = Vec::new();
        for candidate in &candidates {
            match self
                .reconcile_candidate(context, candidate, reconciled_at)
                .await
            {
                Ok(reconciliation) => {
                    reconciliations.push(reconciliation);
                    runs.push(candidate.intent().execution_run_id());
                }
                Err(error) => unreachable.push(UnreconciledEffect {
                    effect_id: candidate.intent().id(),
                    reason: error.to_string(),
                }),
            }
        }
        Ok(ExternalEffectRecoveryReport {
            reconciliations,
            runs,
            unreachable,
        })
    }

    /// One sweep at `at`, under the default retry cadence.
    ///
    /// What a driving loop should call: it should not have to know that "ask
    /// again about what is still unknown" needs a cutoff, only that there is
    /// one.
    pub async fn sweep(
        &self,
        context: &RequestContext,
        at: Timestamp,
    ) -> Result<ExternalEffectRecoveryReport, ApplicationError> {
        self.run(context, at, at - RECONCILIATION_RETRY_AFTER).await
    }
}
