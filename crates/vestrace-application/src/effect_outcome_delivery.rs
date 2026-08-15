//! Telling the run what happened to the effect it asked for.
//!
//! # The gap this closes
//!
//! An effect that times out gets an `unknown` receipt and the run carries on
//! without knowing. The reconciliation sweep later asks the provider and finds
//! out — and wrote the answer to a table nobody joined against. `NotApplied`
//! means the system dispatched something that did not happen, and no run, no
//! surface and no operator was told.
//!
//! Three slices in a row ended by naming this and deferring it. This is it.
//!
//! # Why a debt rather than a write
//!
//! Appending to the run's event stream and inserting the reconciliation touch
//! two different aggregates and cannot share a transaction. Either order loses
//! something: insert-then-append and a failed append leaves an effect that is
//! settled — so out of the sweep — attached to a run that never learned;
//! append-then-insert and a crash between them duplicates the run event.
//!
//! So the reconciliation carries `notified_at`, and a settled outcome stays
//! *owed* until something records that it was delivered. A failed append leaves
//! the debt outstanding and the next pass retries it. A crash after appending
//! but before marking re-appends — at-least-once, the same guarantee the outbox
//! gives, and the effect id in the event lets a reader collapse duplicates.

use std::sync::Arc;

use vestrace_domain::id::{CorrelationId, OperationId, RunEventId};
use vestrace_domain::run::{LegacyRunEvent, LegacyRunEventEnvelope, RunActor, RunVersion};
use vestrace_domain::DomainError;
use vestrace_domain::{Timestamp, now};

use crate::{
    ApplicationError, RequestContext, SharedExternalEffectRepository,
    runs::{SharedRunEventStore, SharedRunRecoveryStore},
};

/// How many debts one pass settles per workspace.
const DELIVERY_BATCH: u32 = 32;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OutcomeDeliveryReport {
    /// Outcomes appended to a run's history.
    pub delivered: usize,
    /// Outcomes belonging to no run — an operator acting directly, or an effect
    /// created before an execution reference had to be a reference.
    ///
    /// Counted apart from `delivered` because nothing was told: the debt is
    /// closed because there is nobody to pay it, which is a different fact.
    pub unattributable: usize,
    /// Outcomes whose run could not be told this pass, and are still owed.
    pub deferred: usize,
}

impl OutcomeDeliveryReport {
    pub fn did_work(&self) -> bool {
        self.delivered > 0 || self.unattributable > 0
    }
}

/// Delivers settled external-effect outcomes into the history of the run that
/// asked for them.
pub struct EffectOutcomeDeliveryService {
    effects: SharedExternalEffectRepository,
    runs: SharedRunEventStore,
    /// Where the stream head comes from. `RunEventStore` can only load a whole
    /// stream, and reading every event of a long run to learn its last version
    /// is the wrong cost for appending one fact.
    heads: SharedRunRecoveryStore,
}

impl EffectOutcomeDeliveryService {
    pub fn new(
        effects: SharedExternalEffectRepository,
        runs: SharedRunEventStore,
        heads: SharedRunRecoveryStore,
    ) -> Self {
        Self {
            effects,
            runs,
            heads,
        }
    }

    pub async fn deliver_once(
        &self,
        context: &RequestContext,
        at: Timestamp,
    ) -> Result<OutcomeDeliveryReport, ApplicationError> {
        let owed = self
            .effects
            .find_undelivered_outcomes(context, DELIVERY_BATCH)
            .await?;

        let mut report = OutcomeDeliveryReport::default();
        for outcome in owed {
            let Some(run_id) = outcome.run_id() else {
                // Nobody to tell. Closing the debt is not pretending it was
                // paid — `unattributable` says exactly what happened, and
                // leaving it open would retry it on every pass forever.
                self.effects
                    .mark_outcome_delivered(context, outcome.reconciliation.id(), at)
                    .await?;
                report.unattributable += 1;
                continue;
            };

            match self.append_to_run(context, run_id, &outcome, at).await {
                Ok(()) => {
                    self.effects
                        .mark_outcome_delivered(context, outcome.reconciliation.id(), at)
                        .await?;
                    report.delivered += 1;
                }
                // A run that has moved since the head was read, a run that does
                // not exist, a storage failure. The debt stays owed and the next
                // pass tries again; one run's problem does not stop the others,
                // which is the same rule the sweep and the outbox drain follow.
                Err(_) => report.deferred += 1,
            }
        }
        Ok(report)
    }

    async fn append_to_run(
        &self,
        context: &RequestContext,
        run_id: vestrace_domain::id::AgentRunId,
        outcome: &crate::UndeliveredOutcome,
        at: Timestamp,
    ) -> Result<(), ApplicationError> {
        let head = self
            .heads
            .load_stream_head(context, run_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::from(DomainError::NotFound(format!(
                    "run {run_id} has no event stream to record an effect outcome in"
                )))
            })?;

        let sequence = RunVersion::new(head.value() + 1)?;
        let payload = LegacyRunEvent::ExternalEffectSettled {
            effect_id: outcome.reconciliation.effect_id(),
            receipt_id: outcome.reconciliation.receipt_id(),
            outcome: outcome.reconciliation.outcome(),
            evidence_strength: outcome.reconciliation.evidence_strength(),
        };
        let envelope = LegacyRunEventEnvelope {
            event_id: RunEventId::new(),
            workspace_id: context.workspace_id,
            run_id,
            sequence,
            event_type: payload.event_type().to_owned(),
            event_version: payload.event_version(),
            actor: RunActor::System {
                component: "effect-outcome-delivery".to_owned(),
            },
            causation_id: OperationId::new(),
            correlation_id: CorrelationId::from_uuid(
                outcome.reconciliation.effect_id().as_uuid(),
            ),
            payload,
            // When the observation was made, not when it was filed. The two
            // differ by however long the run was unreachable.
            occurred_at: outcome.reconciliation.reconciled_at(),
            recorded_at: at,
        };

        self.runs
            .append(context, run_id, head, &[envelope])
            .await
            .map(|_| ())
    }
}

/// Deliver for one workspace, at the current time.
pub async fn deliver_effect_outcomes(
    service: &Arc<EffectOutcomeDeliveryService>,
    context: &RequestContext,
) -> Result<OutcomeDeliveryReport, ApplicationError> {
    service.deliver_once(context, now()).await
}
