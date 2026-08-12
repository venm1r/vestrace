use chrono::{TimeZone, Utc};
use sqlx::PgPool;
use vestrace_application::{ApplicationError, ExternalEffectRepository};
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectPrecondition, EffectReversibility, EvidenceStrength,
    ExternalEffectIntent, ExternalEffectReceipt, IdempotencyProfile, ObservedEffectState,
    ReconciliationOutcome, reconcile_effect,
};
use vestrace_domain::{Capability, PrincipalId, RiskCategory, WorkspaceId};
use vestrace_infrastructure::{PgExternalEffectRepository, PgStore};

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn intent_for(workspace_id: WorkspaceId, execution_ref: &str) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        execution_ref,
        workspace_id,
        PrincipalId::new(),
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

fn intent() -> ExternalEffectIntent {
    intent_for(WorkspaceId::new(), "run-step-1")
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

#[sqlx::test(migrations = "../../migrations")]
async fn external_effect_repository_round_trips_unknown_and_reconciliation_evidence(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let intent = intent();
    let receipt = unknown_receipt(&intent);
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();

    repository.insert_intent(&intent).await.unwrap();
    repository.insert_intent(&intent).await.unwrap();
    repository.insert_receipt(&receipt).await.unwrap();
    repository.insert_receipt(&receipt).await.unwrap();
    repository
        .insert_reconciliation(&reconciliation)
        .await
        .unwrap();
    repository
        .insert_reconciliation(&reconciliation)
        .await
        .unwrap();

    assert_eq!(
        repository.find_intent(intent.id()).await.unwrap(),
        Some(intent)
    );
    assert_eq!(
        repository.find_receipt(receipt.id()).await.unwrap(),
        Some(receipt)
    );
    assert_eq!(
        repository
            .find_reconciliation(reconciliation.id())
            .await
            .unwrap(),
        Some(reconciliation)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_effect_repository_rejects_conflicting_immutable_intent_id(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let first = intent();
    let mut payload = serde_json::to_value(&first).unwrap();
    payload["adapter"] = serde_json::json!("other-adapter");
    let conflicting: ExternalEffectIntent = serde_json::from_value(payload).unwrap();

    repository.insert_intent(&first).await.unwrap();
    assert!(matches!(
        repository.insert_intent(&conflicting).await,
        Err(ApplicationError::Conflict(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_effect_repository_discovers_only_unreconciled_unknown_effects_in_workspace(
    pool: PgPool,
) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let workspace_id = WorkspaceId::new();
    let current = intent_for(workspace_id, "current");
    let other = intent_for(WorkspaceId::new(), "other");
    let current_receipt = unknown_receipt(&current);
    let other_receipt = unknown_receipt(&other);

    repository.insert_intent(&current).await.unwrap();
    repository.insert_receipt(&current_receipt).await.unwrap();
    repository.insert_intent(&other).await.unwrap();
    repository.insert_receipt(&other_receipt).await.unwrap();

    let candidates = repository
        .find_reconciliation_candidates(workspace_id)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].intent(), &current);
    assert_eq!(candidates[0].receipt(), &current_receipt);

    let reconciliation = reconcile_effect(
        &current,
        &current_receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(false),
            "external:not-found",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    repository
        .insert_reconciliation(&reconciliation)
        .await
        .unwrap();

    assert!(
        repository
            .find_reconciliation_candidates(workspace_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[test]
fn reconciliation_fixture_is_confirmed_before_persistence() {
    let intent = intent();
    let receipt = unknown_receipt(&intent);
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();

    assert_eq!(reconciliation.outcome(), ReconciliationOutcome::Confirmed);
}
