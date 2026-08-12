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
        "endpoint:alpha",
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
    async fn insert_intent(&self, _intent: &ExternalEffectIntent) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn find_intent(
        &self,
        _id: ExternalEffectId,
    ) -> Result<Option<ExternalEffectIntent>, ApplicationError> {
        Ok(None)
    }

    async fn insert_receipt(
        &self,
        _receipt: &ExternalEffectReceipt,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn find_receipt(
        &self,
        _id: ExternalEffectReceiptId,
    ) -> Result<Option<ExternalEffectReceipt>, ApplicationError> {
        Ok(None)
    }

    async fn insert_reconciliation(
        &self,
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
        _id: ExternalReconciliationId,
    ) -> Result<Option<ExternalReconciliation>, ApplicationError> {
        Ok(None)
    }

    async fn find_reconciliation_candidates(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ExternalEffectRecoveryCandidate>, ApplicationError> {
        let reconciled = self.reconciled.lock().await;
        let reconciled_keys = self.reconciled_keys.lock().await;
        Ok(self
            .candidates
            .lock()
            .await
            .iter()
            .filter(|candidate| candidate.intent().workspace_id() == workspace_id)
            .filter(|candidate| {
                !reconciled.iter().any(|item| {
                    let payload = serde_json::to_value(item).unwrap();
                    payload["effect_id"]
                        .as_str()
                        .is_some_and(|value| value == candidate.intent().id().as_uuid().to_string())
                        && payload["receipt_id"].as_str().is_some_and(|value| {
                            value == candidate.receipt().id().as_uuid().to_string()
                        })
                }) && !reconciled_keys
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
    let current = intent(workspace_id, actor_id, "current");
    let resolved = intent(workspace_id, actor_id, "resolved");
    let other = intent(WorkspaceId::new(), actor_id, "other");

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

    let report = service.run(&context, at(30)).await.unwrap();
    assert_eq!(report.reconciliations().len(), 1);
    assert_eq!(
        report.reconciliations()[0].outcome(),
        ReconciliationOutcome::Confirmed
    );
    assert_eq!(repository.reconciled_count().await, 1);

    let second_report = service.run(&context, at(31)).await.unwrap();
    assert!(second_report.reconciliations().is_empty());
}

#[tokio::test]
async fn read_back_without_observations_fails_closed_before_persistence() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let repository = Arc::new(MemoryEffectRepository::default());
    let effect = intent(workspace_id, actor_id, "empty-read-back");
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

    let error = service.run(&context, at(30)).await.unwrap_err();
    assert!(matches!(
        error,
        ApplicationError::Domain(vestrace_domain::DomainError::InvalidArgument(message))
            if message.contains("at least one observation")
    ));
    assert_eq!(repository.reconciled_count().await, 0);
}
