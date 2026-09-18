use chrono::{TimeZone, Utc};
use vestrace_application::{ApplicationError, ExternalEffectReconciliationService, RequestContext};
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectLifecycleStatus, EffectPrecondition, EffectReversibility,
    EvidenceStrength, ExternalEffectIntent, ExternalEffectReceipt, IdempotencyProfile,
    ObservedEffectState, ReconciliationOutcome,
};
use vestrace_domain::{Capability, PrincipalId, RiskCategory, WorkspaceId};

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn intent(workspace_id: WorkspaceId, actor_id: PrincipalId) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        "run://01900000-0000-7000-8000-000000000001",
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

#[test]
fn reconciliation_service_returns_explicit_confirmed_outcome_for_unknown_effect() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let intent = intent(workspace_id, actor_id);
    let receipt = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        at(20),
        vec!["evidence:timeout".into()],
    )
    .unwrap();
    let context = RequestContext::new(workspace_id, actor_id);

    let reconciliation = ExternalEffectReconciliationService::new()
        .reconcile(
            &context,
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
    assert_eq!(
        reconciliation.evidence_strength(),
        EvidenceStrength::ProviderIdempotencyLookup
    );
}

/// Mutant caught: treating a transport acknowledgement as a settled provider
/// result leaves it outside reconciliation, so no later read-back can establish
/// whether the requested business effect happened.
#[test]
fn reconciliation_service_accepts_acknowledged_receipts_without_calling_them_confirmed() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let intent = intent(workspace_id, actor_id);
    let unknown = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        at(20),
        vec!["evidence:acknowledgement".into()],
    )
    .unwrap();
    let mut payload = serde_json::to_value(unknown).unwrap();
    payload["outcome_status"] = serde_json::to_value(EffectLifecycleStatus::Acknowledged).unwrap();
    let receipt: ExternalEffectReceipt = serde_json::from_value(payload).unwrap();
    let context = RequestContext::new(workspace_id, actor_id);

    let reconciliation = ExternalEffectReconciliationService::new()
        .reconcile(
            &context,
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

    assert!(receipt.requires_reconciliation());
    assert!(!receipt.is_business_confirmation());
    assert_eq!(reconciliation.outcome(), ReconciliationOutcome::Confirmed);
}

/// Mutant caught: permitting a provider-declared failed receipt into read-back
/// treats a rejection as an open business outcome and can create false evidence.
#[test]
fn reconciliation_service_refuses_failed_receipts() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let intent = intent(workspace_id, actor_id);
    let unknown = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        at(20),
        vec!["evidence:failed".into()],
    )
    .unwrap();
    let mut payload = serde_json::to_value(unknown).unwrap();
    payload["outcome_status"] = serde_json::to_value(EffectLifecycleStatus::Failed).unwrap();
    let receipt: ExternalEffectReceipt = serde_json::from_value(payload).unwrap();

    let error = ExternalEffectReconciliationService::new()
        .reconcile(
            &RequestContext::new(workspace_id, actor_id),
            &intent,
            &receipt,
            vec![ObservedEffectState::new(
                EvidenceStrength::ProviderIdempotencyLookup,
                Some(true),
                "external:must-not-settle",
                vec!["evidence:provider".into()],
            )],
            at(30),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ApplicationError::Domain(vestrace_domain::DomainError::PolicyViolation(message))
            if message.contains("acknowledged or unknown")
    ));
}

#[test]
fn reconciliation_service_rejects_cross_workspace_recovery() {
    let intent = intent(WorkspaceId::new(), PrincipalId::new());
    let receipt = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        at(20),
        vec!["evidence:timeout".into()],
    )
    .unwrap();
    let context = RequestContext::new(WorkspaceId::new(), intent.actor_id());

    let error = ExternalEffectReconciliationService::new()
        .reconcile(&context, &intent, &receipt, vec![], at(30))
        .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Domain(vestrace_domain::DomainError::PolicyViolation(message))
            if message.contains("workspace")
    ));
}

#[test]
fn reconciliation_service_requires_evidence_and_does_not_treat_unknown_as_retryable() {
    let workspace_id = WorkspaceId::new();
    let actor_id = PrincipalId::new();
    let intent = intent(workspace_id, actor_id);
    let receipt = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        at(20),
        vec!["evidence:timeout".into()],
    )
    .unwrap();
    let context = RequestContext::new(workspace_id, actor_id);

    let error = ExternalEffectReconciliationService::new()
        .reconcile(&context, &intent, &receipt, vec![], at(30))
        .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Domain(vestrace_domain::DomainError::InvalidArgument(message))
            if message.contains("at least one observation")
    ));
    assert_eq!(
        receipt.retry_decision(),
        vestrace_domain::external_effects::RetryDecision::DeniedUnknown
    );
}
