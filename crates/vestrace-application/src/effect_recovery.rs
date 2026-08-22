use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::Timestamp;
use vestrace_domain::external_effects::{
    ExternalEffectAdapterDescriptor, ExternalEffectIntent, ExternalEffectReceipt,
    ExternalReconciliation, ObservedEffectState,
};
use vestrace_domain::id::AgentRunId;

use crate::{
    ApplicationError, ExternalEffectReconciliationService, ExternalEffectRecoveryCandidate,
    LostDispatchAdoption, RequestContext, SharedExternalEffectRepository,
};

#[async_trait]
pub trait ExternalEffectReadBackAdapter: Send + Sync {
    async fn observe(
        &self,
        intent: &ExternalEffectIntent,
        receipt: Option<&ExternalEffectReceipt>,
    ) -> Result<Vec<ObservedEffectState>, ApplicationError>;
}

/// Why one candidate could not complete its recovery attempt.
///
/// Failures before a usable provider observation stay distinct from failures
/// while interpreting or persisting that observation. Only the former are
/// failed recovery attempts: folding a storage failure into that record would
/// make an internal write failure look like evidence that the provider could
/// not be asked.
#[derive(Debug, thiserror::Error)]
pub enum ExternalEffectRecoveryError {
    #[error("no external effect read-back route is registered for adapter `{adapter}`")]
    MissingReadBackRoute { adapter: String },
    #[error("external effect read-back route for adapter `{adapter}` does not support read-back")]
    ReadBackUnsupported { adapter: String },
    #[error(transparent)]
    ReadBack(ApplicationError),
    #[error(transparent)]
    Reconciliation(ApplicationError),
    #[error(transparent)]
    Application(#[from] ApplicationError),
}

/// The read-back endpoints recovery is allowed to ask, keyed by persisted
/// adapter identity.
///
/// Recovery used to hold one endpoint and therefore treated configuration
/// order as evidence about which external system owned an effect. Keeping the
/// name beside the endpoint makes an absent route explicit and refuses the
/// equally unsafe ambiguity of two endpoints claiming the same name.
pub struct ExternalEffectReadBackRegistry {
    adapters: BTreeMap<String, ExternalEffectReadBackRoute>,
}

struct ExternalEffectReadBackRoute {
    descriptor: ExternalEffectAdapterDescriptor,
    adapter: Arc<dyn ExternalEffectReadBackAdapter>,
}

impl ExternalEffectReadBackRegistry {
    pub fn new(
        adapters: impl IntoIterator<
            Item = (
                String,
                ExternalEffectAdapterDescriptor,
                Arc<dyn ExternalEffectReadBackAdapter>,
            ),
        >,
    ) -> Result<Self, ApplicationError> {
        let mut registry = Self {
            adapters: BTreeMap::new(),
        };
        for (name, descriptor, adapter) in adapters {
            if registry.adapters.contains_key(&name) {
                return Err(ApplicationError::InvalidConfiguration(format!(
                    "external effect read-back adapter name `{name}` is registered more than once"
                )));
            }
            if descriptor.name() != name {
                return Err(ApplicationError::InvalidConfiguration(format!(
                    "external effect read-back route name `{name}` does not match descriptor `{}`",
                    descriptor.name()
                )));
            }
            registry.adapters.insert(
                name,
                ExternalEffectReadBackRoute {
                    descriptor,
                    adapter,
                },
            );
        }
        Ok(registry)
    }

    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.adapters.keys().map(String::as_str)
    }

    fn is_actionable(&self, adapter: &str) -> bool {
        self.adapters
            .get(adapter)
            .is_some_and(|route| route.descriptor.supports_read_back())
    }

    fn resolve(
        &self,
        adapter: &str,
    ) -> Result<&Arc<dyn ExternalEffectReadBackAdapter>, ExternalEffectRecoveryError> {
        let route = self.adapters.get(adapter).ok_or_else(|| {
            ExternalEffectRecoveryError::MissingReadBackRoute {
                adapter: adapter.to_owned(),
            }
        })?;
        if !route.descriptor.supports_read_back() {
            return Err(ExternalEffectRecoveryError::ReadBackUnsupported {
                adapter: adapter.to_owned(),
            });
        }
        Ok(&route.adapter)
    }
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
    saturated: bool,
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

    /// Whether discovery spent the whole candidate budget for this pass.
    ///
    /// A full batch does not prove more work remains, but a partial batch proves
    /// the sweep did not stop because of its budget. Keeping that distinction in
    /// the report lets a driver expose persistent backlog pressure.
    pub const fn saturated(&self) -> bool {
        self.saturated
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

/// How long a failed recovery attempt yields its place in the bounded sweep.
///
/// A shorter interval notices a transiently unreachable provider sooner; a
/// longer one lets fewer permanently unroutable effects consume provider-work
/// budget. One minute matches the existing reconciliation cadence while the
/// separate constant and query argument keep those two policies independent.
pub const FAILED_RECOVERY_ATTEMPT_RETRY_AFTER: chrono::Duration = chrono::Duration::minutes(1);

/// Maximum provider read-backs one workspace performs in a reconciliation pass.
///
/// This bounds a tick's worst-case duration and provider load at the cost of
/// taking more ticks to drain a backlog. Eight is deliberately smaller than the
/// delivery batch: these operations are sequential network calls, while outcome
/// delivery is local persistence. The constants stay separate because those
/// costs need to remain independently tunable.
pub const RECONCILIATION_BATCH: u32 = 8;

/// Fallback dispatch allowance for an adapter that declares no timeout.
///
/// This is added to the transition's recorded time and **persisted** as that
/// dispatch's deadline only when the adapter cannot state a duration of its
/// own. Every replica then compares a stored promise rather than applying a
/// threshold of its own. It was once `DISPATCH_CONSIDERED_LOST_AFTER`
/// and recovery subtracted it from `now` to guess whether a call had been
/// abandoned — a guess a second replica had no reason to share, and one that
/// could declare a live call lost inside the adapter's own timeout. The
/// duration is unchanged; its use is now explicitly the exceptional fallback
/// rather than the normal dispatch contract.
pub const DEFAULT_DISPATCH_ALLOWANCE: chrono::Duration = chrono::Duration::minutes(5);

pub struct ExternalEffectRecoveryService {
    repository: SharedExternalEffectRepository,
    read_backs: ExternalEffectReadBackRegistry,
    reconciliation: ExternalEffectReconciliationService,
}

impl ExternalEffectRecoveryService {
    pub fn new(
        repository: SharedExternalEffectRepository,
        read_backs: ExternalEffectReadBackRegistry,
    ) -> Self {
        Self {
            repository,
            read_backs,
            reconciliation: ExternalEffectReconciliationService::new(),
        }
    }

    pub async fn discover(
        &self,
        context: &RequestContext,
        retry_unsettled_before: Timestamp,
        retry_failed_before: Timestamp,
        dispatch_expired_before: Timestamp,
        limit: u32,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError> {
        Ok(self
            .discover_raw(
                context,
                retry_unsettled_before,
                retry_failed_before,
                dispatch_expired_before,
                limit,
            )
            .await?
            .into_iter()
            .filter(|candidate| self.read_backs.is_actionable(candidate.intent().adapter()))
            .collect())
    }

    /// The persisted work set. `run` deliberately retains unsupported and
    /// missing routes here so its existing failed-attempt record yields their
    /// bounded-sweep place; public discovery exposes only candidates a caller
    /// could actually ask right now.
    async fn discover_raw(
        &self,
        context: &RequestContext,
        retry_unsettled_before: Timestamp,
        retry_failed_before: Timestamp,
        dispatch_expired_before: Timestamp,
        limit: u32,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError> {
        self.repository
            .find_reconciliation_candidates(
                context,
                retry_unsettled_before,
                retry_failed_before,
                dispatch_expired_before,
                limit,
            )
            .await
    }

    pub async fn reconcile_candidate(
        &self,
        context: &RequestContext,
        candidate: &ExternalEffectRecoveryCandidate,
        reconciled_at: Timestamp,
    ) -> Result<Option<ExternalReconciliation>, ExternalEffectRecoveryError> {
        if let Some(lost_dispatch) = candidate.lost_dispatch() {
            if lost_dispatch.needs_adoption()
                && self
                    .repository
                    .adopt_lost_dispatch(
                        context,
                        candidate.intent().id(),
                        lost_dispatch.dispatch_transition_id(),
                        reconciled_at,
                    )
                    .await?
                    == LostDispatchAdoption::AlreadyAdopted
            {
                // Another sweeper owns this attempt. Reporting it as a provider
                // failure would be false, and asking would duplicate read-back.
                return Ok(None);
            }
        }
        let read_back = self.read_backs.resolve(candidate.intent().adapter())?;
        let observations = read_back
            .observe(candidate.intent(), candidate.receipt())
            .await
            .map_err(ExternalEffectRecoveryError::ReadBack)?;
        let obtained_observation = !observations.is_empty();
        let reconciliation = self
            .reconciliation
            .reconcile(
                context,
                candidate.intent(),
                candidate.receipt(),
                observations,
                reconciled_at,
            )
            .map_err(|error| {
                if obtained_observation {
                    ExternalEffectRecoveryError::Reconciliation(error)
                } else {
                    // An adapter returning no observation is not evidence about
                    // the outside world and cannot produce a reconciliation.
                    // Treat it like a read-back failure so it yields its place
                    // in the bounded sweep without inventing an inconclusive row.
                    ExternalEffectRecoveryError::ReadBack(error)
                }
            })?;
        self.repository
            .insert_reconciliation(context, &reconciliation)
            .await?;
        Ok(Some(reconciliation))
    }

    /// Reconcile the oldest eligible effects, up to `limit` candidates.
    ///
    /// The report is saturated when discovery fills that budget, which means
    /// another bounded pass may still have work to do.
    ///
    /// # Why one failure does not end the sweep
    ///
    /// This propagated the first error, so a single unreachable endpoint left
    /// every other effect in the workspace unreconciled — including ones whose
    /// endpoints were answering. The same shape as a drain that stops at its
    /// first failed message: the failure is real and is not a reason to stop
    /// asking about everything else.
    ///
    /// A candidate that could not be asked is recorded before it is reported.
    /// It stays unsettled, but yields its place in the bounded sweep until the
    /// failed-attempt cutoff passes.
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
        retry_failed_before: Timestamp,
        dispatch_expired_before: Timestamp,
        limit: u32,
    ) -> Result<ExternalEffectRecoveryReport, ApplicationError> {
        let candidates = self
            .discover_raw(
                context,
                retry_unsettled_before,
                retry_failed_before,
                dispatch_expired_before,
                limit,
            )
            .await?;
        let saturated = candidates.len() == limit as usize;
        let mut reconciliations = Vec::with_capacity(candidates.len());
        let mut runs = Vec::with_capacity(candidates.len());
        let mut unreachable = Vec::new();
        for candidate in &candidates {
            match self
                .reconcile_candidate(context, candidate, reconciled_at)
                .await
            {
                Ok(Some(reconciliation)) => {
                    reconciliations.push(reconciliation);
                    runs.push(candidate.intent().execution_run_id());
                }
                Ok(None) => {}
                Err(ExternalEffectRecoveryError::Application(error)) => return Err(error),
                Err(ExternalEffectRecoveryError::Reconciliation(error)) => {
                    unreachable.push(UnreconciledEffect {
                        effect_id: candidate.intent().id(),
                        reason: error.to_string(),
                    });
                }
                Err(error) => {
                    let reason = error.to_string();
                    self.repository
                        .record_failed_recovery_attempt(
                            context,
                            candidate.intent().id(),
                            reconciled_at,
                            &reason,
                        )
                        .await?;
                    unreachable.push(UnreconciledEffect {
                        effect_id: candidate.intent().id(),
                        reason,
                    });
                }
            }
        }
        Ok(ExternalEffectRecoveryReport {
            reconciliations,
            runs,
            unreachable,
            saturated,
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
        self.run(
            context,
            at,
            at - RECONCILIATION_RETRY_AFTER,
            at - FAILED_RECOVERY_ATTEMPT_RETRY_AFTER,
            at,
            RECONCILIATION_BATCH,
        )
        .await
    }
}
