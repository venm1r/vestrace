use chrono::{TimeZone, Utc};
use sqlx::PgPool;
use vestrace_application::{
    ApplicationError, ExternalEffectFaultSuiteEvidence, ExternalEffectRepository,
    FaultSuiteEvidenceRepository, RequestContext,
};
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectFaultPoint, EffectPrecondition, EffectReversibility, EvidenceStrength,
    ExternalEffectIntent, ExternalEffectReceipt, FaultObservation, IdempotencyProfile,
    ObservedEffectState, ReconciliationOutcome, reconcile_effect,
};
use vestrace_domain::{Capability, PrincipalId, RiskCategory, WorkspaceId};
use vestrace_infrastructure::{PgExternalEffectRepository, PgStore};

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

fn intent_for(workspace_id: WorkspaceId, execution_ref: &str) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        execution_ref,
        workspace_id,
        PrincipalId::new(),
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

fn intent() -> ExternalEffectIntent {
    intent_for(WorkspaceId::new(), "run://01900000-0000-7000-8000-000000000001")
}

/// The workspace an adapter call is scoped to.
///
/// Every method on this port takes one now. It used to take none, and read by
/// id alone across every tenant.
fn context_for(workspace_id: WorkspaceId) -> RequestContext {
    RequestContext::new(workspace_id, PrincipalId::new())
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

    let context = context_for(intent.workspace_id());

    repository.insert_intent(&context, &intent).await.unwrap();
    repository.insert_intent(&context, &intent).await.unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();
    repository
        .insert_reconciliation(&context, &reconciliation)
        .await
        .unwrap();
    repository
        .insert_reconciliation(&context, &reconciliation)
        .await
        .unwrap();

    assert_eq!(
        repository.find_intent(&context, intent.id()).await.unwrap(),
        Some(intent)
    );
    assert_eq!(
        repository
            .find_receipt(&context, receipt.id())
            .await
            .unwrap(),
        Some(receipt)
    );
    assert_eq!(
        repository
            .find_reconciliation(&context, reconciliation.id())
            .await
            .unwrap(),
        Some(reconciliation)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_effect_evidence_is_not_readable_from_another_workspace(pool: PgPool) {
    // The three reads took an id and nothing else, so any tenant's id returned
    // that tenant's row — and a receipt carries the external resource id and
    // the provider's response digest, which is the effect itself rather than
    // metadata about it.
    //
    // This runs as a superuser, like every other `sqlx::test` here, so the
    // policy is bypassed and only the query's own predicate is under test. That
    // is deliberate: it is the half that has to hold when a caller reaches the
    // database through some future path that does not scope.
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

    let owner = context_for(intent.workspace_id());
    repository.insert_intent(&owner, &intent).await.unwrap();
    repository.insert_receipt(&owner, &receipt).await.unwrap();
    repository
        .insert_reconciliation(&owner, &reconciliation)
        .await
        .unwrap();

    let stranger = context_for(WorkspaceId::new());
    assert_eq!(
        repository
            .find_intent(&stranger, intent.id())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repository
            .find_receipt(&stranger, receipt.id())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repository
            .find_reconciliation(&stranger, reconciliation.id())
            .await
            .unwrap(),
        None
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_receipt_cannot_be_filed_against_another_workspaces_effect(pool: PgPool) {
    // A receipt carries no workspace of its own — it belongs to an effect, and
    // the effect belongs to a tenant. So the adapter binds the workspace from
    // the request context, and nothing it holds could detect a mismatch.
    //
    // The composite foreign key onto `(id, workspace_id)` of the intent is what
    // makes that safe: the pair has to exist. This assertion holds under a
    // superuser too, because a foreign key is not a policy.
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let intent = intent();
    let receipt = unknown_receipt(&intent);

    let owner = context_for(intent.workspace_id());
    repository.insert_intent(&owner, &intent).await.unwrap();

    let stranger = context_for(WorkspaceId::new());
    assert!(
        repository
            .insert_receipt(&stranger, &receipt)
            .await
            .is_err(),
        "a receipt was filed against an effect in another workspace"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_effect_repository_rejects_conflicting_immutable_intent_id(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let first = intent();
    let mut payload = serde_json::to_value(&first).unwrap();
    payload["adapter"] = serde_json::json!("other-adapter");
    let conflicting: ExternalEffectIntent = serde_json::from_value(payload).unwrap();
    let context = context_for(first.workspace_id());

    repository.insert_intent(&context, &first).await.unwrap();
    assert!(matches!(
        repository.insert_intent(&context, &conflicting).await,
        Err(ApplicationError::Conflict(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_effect_repository_discovers_only_unreconciled_unknown_effects_in_workspace(
    pool: PgPool,
) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let workspace_id = WorkspaceId::new();
    let current = intent_for(workspace_id, &run_ref());
    let other = intent_for(WorkspaceId::new(), &run_ref());
    let current_receipt = unknown_receipt(&current);
    let other_receipt = unknown_receipt(&other);

    let context = context_for(workspace_id);
    let other_context = context_for(other.workspace_id());

    repository.insert_intent(&context, &current).await.unwrap();
    repository
        .insert_receipt(&context, &current_receipt)
        .await
        .unwrap();
    repository
        .insert_intent(&other_context, &other)
        .await
        .unwrap();
    repository
        .insert_receipt(&other_context, &other_receipt)
        .await
        .unwrap();

    let candidates = repository
        .find_reconciliation_candidates(&context, at(10_000))
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
        .insert_reconciliation(&context, &reconciliation)
        .await
        .unwrap();

    assert!(
        repository
            .find_reconciliation_candidates(&context, at(10_000))
            .await
            .unwrap()
            .is_empty()
    );
}

/// An answer retires an effect from the sweep. A non-answer must not.
///
/// The discovery query dropped an effect as soon as *any* reconciliation row
/// existed for it, so "we asked the provider and it could not tell us" retired
/// the effect permanently: the receipt stayed `unknown` and nothing ever asked
/// again. This is the difference between a settled outcome and a recorded
/// attempt.
#[sqlx::test(migrations = "../../migrations")]
async fn an_inconclusive_answer_does_not_retire_an_effect_from_the_sweep(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let workspace_id = WorkspaceId::new();
    let effect = intent_for(workspace_id, &run_ref());
    let receipt = unknown_receipt(&effect);
    let context = context_for(workspace_id);

    repository.insert_intent(&context, &effect).await.unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();

    let inconclusive = reconcile_effect(
        &effect,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            // The provider answered and could not say.
            None,
            "external:indeterminate",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    assert_eq!(inconclusive.outcome(), ReconciliationOutcome::Inconclusive);
    repository
        .insert_reconciliation(&context, &inconclusive)
        .await
        .unwrap();

    // Immediately afterwards there is nothing to gain by asking again.
    assert!(
        repository
            .find_reconciliation_candidates(&context, at(30))
            .await
            .unwrap()
            .is_empty(),
        "an effect was asked about again in the same instant it was asked"
    );

    // Once the attempt is old enough, the effect is still an effect whose
    // outcome nobody knows.
    let candidates = repository
        .find_reconciliation_candidates(&context, at(90))
        .await
        .unwrap();
    assert_eq!(
        candidates.len(),
        1,
        "an inconclusive answer retired the effect from the sweep, so its outcome stays unknown \
         forever and nothing ever asks again"
    );
    assert_eq!(candidates[0].intent(), &effect);

    // An answer does retire it, however old the attempt is.
    let settled = reconcile_effect(
        &effect,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )],
        at(100),
    )
    .unwrap();
    repository
        .insert_reconciliation(&context, &settled)
        .await
        .unwrap();

    assert!(
        repository
            .find_reconciliation_candidates(&context, at(10_000))
            .await
            .unwrap()
            .is_empty(),
        "a confirmed effect came back as a candidate, so the sweep would ask forever"
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

#[sqlx::test(migrations = "../../migrations")]
async fn fault_suite_evidence_repository_round_trips_and_is_idempotent(pool: PgPool) {
    let repository =
        vestrace_infrastructure::PgFaultSuiteEvidenceRepository::new(PgStore::from_pool(pool));
    let evidence = fault_evidence(true);

    repository.insert(&evidence).await.unwrap();
    repository.insert(&evidence).await.unwrap();

    assert_eq!(
        repository.find_by_id(evidence.id()).await.unwrap(),
        Some(evidence)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn fault_suite_evidence_repository_rejects_conflicting_immutable_id(pool: PgPool) {
    let repository =
        vestrace_infrastructure::PgFaultSuiteEvidenceRepository::new(PgStore::from_pool(pool));
    let first = fault_evidence(true);
    let mut payload = serde_json::to_value(&first).unwrap();
    payload["target_digest"] = serde_json::json!("sha256:other-target");
    let conflicting: ExternalEffectFaultSuiteEvidence = serde_json::from_value(payload).unwrap();

    repository.insert(&first).await.unwrap();
    assert!(matches!(
        repository.insert(&conflicting).await,
        Err(ApplicationError::Conflict(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn fault_suite_evidence_repository_preserves_failed_evidence(pool: PgPool) {
    let repository =
        vestrace_infrastructure::PgFaultSuiteEvidenceRepository::new(PgStore::from_pool(pool));
    let evidence = fault_evidence(false);

    repository.insert(&evidence).await.unwrap();

    let stored = repository.find_by_id(evidence.id()).await.unwrap().unwrap();
    assert!(!stored.is_passed());
    assert!(!stored.failures().is_empty());
}

fn fault_evidence(passed: bool) -> ExternalEffectFaultSuiteEvidence {
    let observations = EffectFaultPoint::required_points()
        .into_iter()
        .map(FaultObservation::expected)
        .collect::<Vec<_>>();
    serde_json::from_value::<ExternalEffectFaultSuiteEvidence>(serde_json::json!({
        "id": uuid::Uuid::now_v7(),
        "target_digest": "sha256:target",
        "observations": observations.into_iter().map(|observation| serde_json::json!({
            "point": observation.point,
            "status": observation.status,
            "retry_attempted": observation.retry_attempted,
            "reconciliation_started": observation.reconciliation_started,
            "receipt_persisted": observation.receipt_persisted
        })).collect::<Vec<_>>(),
        "passed": passed,
        "failures": if passed {
            Vec::<String>::new()
        } else {
            vec!["unsafe retry".to_owned()]
        },
        "created_at": at(40)
    }))
    .unwrap()
}
