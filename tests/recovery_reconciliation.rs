use chrono::{TimeZone, Utc};
use vestrace_application::{ApplicationError, ExternalEffectReconciliationService, RequestContext};
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectPrecondition, EffectReversibility, EvidenceStrength,
    ExternalEffectIntent, ExternalEffectReceipt, IdempotencyProfile, ObservedEffectState,
    ReconciliationOutcome,
};
use vestrace_domain::{Capability, PrincipalId, RiskCategory, WorkspaceId};

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn intent(workspace_id: WorkspaceId, actor_id: PrincipalId) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        "run-step-1",
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
