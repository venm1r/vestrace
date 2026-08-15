use std::sync::Arc;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use sqlx::types::Uuid;
use tokio::sync::Mutex;
use vestrace_application::{
    ApplicationError, ExternalEffectReadBackAdapter, ExternalEffectRecoveryCandidate,
    ExternalEffectRecoveryService, ExternalEffectRepository, RequestContext,
};
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectPrecondition, EffectReversibility, EvidenceStrength,
    ExternalEffectIntent, ExternalEffectReceipt, ExternalReconciliation, IdempotencyProfile,
    ObservedEffectState, ReconciliationOutcome,
};
use vestrace_domain::{
    Capability, ExternalEffectId, ExternalEffectReceiptId, ExternalReconciliationId, PrincipalId,
    RiskCategory, WorkspaceId,
};

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

/// A distinct run for a fixture effect to belong to.
///
/// These were labels — `"current"`, `"other"` — which is what `execution_ref`
/// accepted before it had to be a reference anything could follow.
fn run_ref() -> String {
    format!("run://{}", vestrace_domain::id::AgentRunId::new())
}

fn intent(
    workspace_id: WorkspaceId,
    actor_id: PrincipalId,
    execution_ref: &str,
) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        execution_ref,
        workspace_id,
        actor_id,
        "webhook-v1",
        "send",
        "https://alpha.effects.test/hook",
        "sha256:arguments",
        "deliver notification",
        vec![EffectPrecondition::new("resource-version", "v1").unwrap()],
        "sha256:preconditions-v1",
        RiskCategory::Medium,
        EffectReversibility::Compensatable,
        IdempotencyProfile::ProviderKey,
        DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        Some("budget:reservation-1"),
        Some("policy:decision-1"),
        at(10),
    )
    .unwrap()
}

fn unknown_receipt(intent: &ExternalEffectIntent) -> ExternalEffectReceipt {
    ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        at(20),
        vec!["evidence:timeout".into()],
    )
    .unwrap()
}

#[derive(Default)]
struct MemoryEffectRepository {
    candidates: Mutex<Vec<ExternalEffectRecoveryCandidate>>,
    reconciled: Mutex<Vec<ExternalReconciliation>>,
    reconciled_keys: Mutex<Vec<(ExternalEffectId, ExternalEffectReceiptId)>>,
}

impl MemoryEffectRepository {
    async fn seed(&self, candidate: ExternalEffectRecoveryCandidate) {
        self.candidates.lock().await.push(candidate);
    }

    async fn reconciled_count(&self) -> usize {
        self.reconciled.lock().await.len()
    }

    async fn mark_reconciled(&self, candidate: &ExternalEffectRecoveryCandidate) {
        self.reconciled_keys
            .lock()
            .await
            .push((candidate.intent().id(), candidate.receipt().id()));
    }
}

#[async_trait]
impl ExternalEffectRepository for MemoryEffectRepository {
    async fn insert_intent(
        &self,
        _context: &RequestContext,
        _intent: &ExternalEffectIntent,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn find_intent(
        &self,
        _context: &RequestContext,
        _id: ExternalEffectId,
    ) -> Result<Option<ExternalEffectIntent>, ApplicationError> {
        Ok(None)
    }

    async fn insert_receipt(
        &self,
        _context: &RequestContext,
        _receipt: &ExternalEffectReceipt,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn find_receipt(
        &self,
        _context: &RequestContext,
        _id: ExternalEffectReceiptId,
    ) -> Result<Option<ExternalEffectReceipt>, ApplicationError> {
        Ok(None)
    }

    async fn insert_reconciliation(
        &self,
        _context: &RequestContext,
        reconciliation: &ExternalReconciliation,
    ) -> Result<(), ApplicationError> {
        self.reconciled.lock().await.push(reconciliation.clone());
        let payload = serde_json::to_value(reconciliation).unwrap();
        self.reconciled_keys.lock().await.push((
            ExternalEffectId::from_uuid(
                Uuid::parse_str(payload["effect_id"].as_str().unwrap()).unwrap(),
            ),
            ExternalEffectReceiptId::from_uuid(
                Uuid::parse_str(payload["receipt_id"].as_str().unwrap()).unwrap(),
            ),
        ));
        Ok(())
    }

    async fn find_reconciliation(
        &self,
        _context: &RequestContext,
        _id: ExternalReconciliationId,
    ) -> Result<Option<ExternalReconciliation>, ApplicationError> {
        Ok(None)
    }

    /// The delivery debt is not what this fixture is about.
    ///
    /// Returning nothing owed is honest here rather than lazy: this file tests
    /// the sweep, and a fake that invented debts would make the sweep's tests
    /// depend on delivery semantics they do not exercise. Delivery has its own
    /// test against real storage.
    async fn find_undelivered_outcomes(
        &self,
        _context: &RequestContext,
        _limit: u32,
    ) -> Result<Vec<vestrace_application::UndeliveredOutcome>, ApplicationError> {
        Ok(Vec::new())
    }

    async fn mark_outcome_delivered(
        &self,
        _context: &RequestContext,
        _reconciliation_id: ExternalReconciliationId,
        _at: vestrace_domain::Timestamp,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    /// Mirrors the stored query: an effect leaves the sweep when something
    /// **settled** it, not merely when somebody asked about it.
    async fn find_reconciliation_candidates(
        &self,
        context: &RequestContext,
        retry_unsettled_before: vestrace_domain::Timestamp,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError> {
        let workspace_id = context.workspace_id;
        let reconciled = self.reconciled.lock().await;
        let reconciled_keys = self.reconciled_keys.lock().await;
        Ok(self
            .candidates
            .lock()
            .await
            .iter()
            .filter(|candidate| candidate.intent().workspace_id() == workspace_id)
            .filter(|candidate| {
                let latest = reconciled
                    .iter()
                    .filter(|item| {
                        let payload = serde_json::to_value(item).unwrap();
                        payload["effect_id"].as_str().is_some_and(|value| {
                            value == candidate.intent().id().as_uuid().to_string()
                        }) && payload["receipt_id"].as_str().is_some_and(|value| {
                            value == candidate.receipt().id().as_uuid().to_string()
                        })
                    })
                    .max_by_key(|item| item.reconciled_at());
                let still_open = match latest {
                    None => true,
                    Some(item) => {
                        !item.outcome().is_settled()
                            && item.reconciled_at() < retry_unsettled_before
                    }
                };
                still_open
                    && !reconciled_keys
                        .contains(&(candidate.intent().id(), candidate.receipt().id()))
            })
            .cloned()
            .collect())
    }
}

struct MemoryReadBack {
    calls: Arc<Mutex<usize>>,
    observations: Vec<ObservedEffectState>,
}

#[async_trait]
impl ExternalEffectReadBackAdapter for MemoryReadBack {
    async fn observe(
        &self,
        _intent: &ExternalEffectIntent,
        _receipt: &ExternalEffectReceipt,
    ) -> Result<Vec<ObservedEffectState>, ApplicationError> {
        *self.calls.lock().await += 1;
        Ok(self.observations.clone())
    }
}

#[tokio::test]
async fn discovery_is_workspace_scoped_and_excludes_already_reconciled_records() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = Arc::new(MemoryEffectRepository::default());
    let current = intent(workspace_id, actor_id, &run_ref());
    let resolved = intent(workspace_id, actor_id, &run_ref());
    let other = intent(WorkspaceId::new(), actor_id, &run_ref());

    repository
        .seed(
            ExternalEffectRecoveryCandidate::new(current.clone(), unknown_receipt(&current))
                .unwrap(),
        )
        .await;
    let resolved_candidate =
        ExternalEffectRecoveryCandidate::new(resolved.clone(), unknown_receipt(&resolved)).unwrap();
    repository.seed(resolved_candidate.clone()).await;
    repository.mark_reconciled(&resolved_candidate).await;
    repository
        .seed(ExternalEffectRecoveryCandidate::new(other.clone(), unknown_receipt(&other)).unwrap())
        .await;

    let read_back = Arc::new(MemoryReadBack {
        calls: Arc::new(Mutex::new(0)),
        observations: vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:resolved",
            vec!["evidence:provider".into()],
        )],
    });
    let service = ExternalEffectRecoveryService::new(repository.clone(), read_back);
    let context = RequestContext::new(workspace_id, actor_id);

    let report = service.run(&context, at(30), at(30)).await.unwrap();
    assert_eq!(report.reconciliations().len(), 1);
    assert_eq!(
        report.reconciliations()[0].outcome(),
        ReconciliationOutcome::Confirmed
    );
    assert_eq!(repository.reconciled_count().await, 1);

    let second_report = service.run(&context, at(31), at(31)).await.unwrap();
    assert!(second_report.reconciliations().is_empty());
}

/// A read-back that observed nothing settles nothing, and says so.
///
/// # What changed here, and why
///
/// This asserted that the whole sweep returned `Err`. The property it exists to
/// protect is that **no reconciliation is persisted from zero observations** —
/// "we asked and learned nothing" must not be written down as a conclusion —
/// and that still holds. What changed is the blast radius: one candidate whose
/// endpoint says nothing used to abort the sweep, leaving every other unknown
/// effect in the workspace unreconciled, including ones whose endpoints were
/// answering. That is the same shape as a drain that stops at its first failed
/// message.
///
/// The candidate is now reported by identity and reason, and stays a candidate.
#[tokio::test]
async fn read_back_without_observations_settles_nothing_and_reports_it() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = Arc::new(MemoryEffectRepository::default());
    let effect = intent(workspace_id, actor_id, &run_ref());
    repository
        .seed(
            ExternalEffectRecoveryCandidate::new(effect.clone(), unknown_receipt(&effect)).unwrap(),
        )
        .await;

    let read_back = Arc::new(MemoryReadBack {
        calls: Arc::new(Mutex::new(0)),
        observations: Vec::new(),
    });
    let service = ExternalEffectRecoveryService::new(repository.clone(), read_back);
    let context = RequestContext::new(workspace_id, actor_id);

    let report = service.run(&context, at(30), at(30)).await.unwrap();

    // Nothing was concluded, and nothing was written.
    assert!(report.reconciliations().is_empty());
    assert_eq!(repository.reconciled_count().await, 0);

    // And the failure is not swallowed: it names the effect and why.
    assert_eq!(report.unreachable().len(), 1);
    assert_eq!(report.unreachable()[0].effect_id, effect.id());
    assert!(
        report.unreachable()[0]
            .reason
            .contains("at least one observation"),
        "{}",
        report.unreachable()[0].reason
    );
}
