use vestrace_application::{
    CapabilityRestorationDecision, CapabilityRestorationPolicy, CapabilityRestorationService,
    RestorationEvidence, RestorationStage,
};
use vestrace_domain::security::Capability;
use vestrace_domain::trust::TrustState;

fn policy() -> CapabilityRestorationPolicy {
    CapabilityRestorationPolicy::new([
        (Capability::AuditRead, RestorationStage::DiagnosticsReadOnly),
        (
            Capability::MemoryWrite,
            RestorationStage::InternalDeterministicWrites,
        ),
        (Capability::MemoryPurge, RestorationStage::SemanticMutation),
        (
            Capability::ProviderWrite,
            RestorationStage::ReversibleExternalEffects,
        ),
        (
            Capability::WorkspaceAdmin,
            RestorationStage::IrreversibleExternalEffects,
        ),
    ])
    .unwrap()
}

fn evidence(qualified: bool, revalidated: bool) -> RestorationEvidence {
    RestorationEvidence::new(
        qualified,
        revalidated,
        vec![
            "evidence:qualification".into(),
            "evidence:revalidation".into(),
        ],
    )
}

#[test]
fn untrusted_scope_keeps_only_diagnostics_capability_available() {
    let policy = policy();
    let diagnostics = CapabilityRestorationService::evaluate(
        TrustState::Untrusted,
        Capability::AuditRead,
        &policy,
        &evidence(false, false),
    );
    let mutation = CapabilityRestorationService::evaluate(
        TrustState::Untrusted,
        Capability::MemoryWrite,
        &policy,
        &evidence(true, true),
    );

    assert!(diagnostics.is_allowed());
    assert!(!mutation.is_allowed());
}

#[test]
fn revalidating_scope_does_not_restore_writes_even_with_qualification_evidence() {
    let decision = CapabilityRestorationService::evaluate(
        TrustState::Revalidating,
        Capability::MemoryWrite,
        &policy(),
        &evidence(true, true),
    );

    assert!(!decision.is_allowed());
}

#[test]
fn degraded_trust_stops_before_semantic_and_external_effects() {
    let policy = policy();
    let deterministic = CapabilityRestorationService::evaluate(
        TrustState::DegradedTrust,
        Capability::MemoryWrite,
        &policy,
        &evidence(true, true),
    );
    let semantic = CapabilityRestorationService::evaluate(
        TrustState::DegradedTrust,
        Capability::MemoryPurge,
        &policy,
        &evidence(true, true),
    );

    assert!(deterministic.is_allowed());
    assert!(!semantic.is_allowed());
}

#[test]
fn trusted_scope_requires_qualification_and_revalidation_for_risky_steps() {
    let policy = policy();
    let blocked = CapabilityRestorationService::evaluate(
        TrustState::Trusted,
        Capability::WorkspaceAdmin,
        &policy,
        &evidence(true, false),
    );
    let restored = CapabilityRestorationService::evaluate(
        TrustState::Trusted,
        Capability::WorkspaceAdmin,
        &policy,
        &evidence(true, true),
    );

    assert!(!blocked.is_allowed());
    assert!(restored.is_allowed());
}

#[test]
fn undeclared_capability_is_denied_without_a_default_fallback() {
    let decision = CapabilityRestorationService::evaluate(
        TrustState::Trusted,
        Capability::MemoryRead,
        &policy(),
        &evidence(true, true),
    );

    assert!(matches!(
        decision,
        CapabilityRestorationDecision::Blocked { .. }
    ));
}

#[test]
fn qualified_restoration_requires_non_empty_evidence_references() {
    let decision = CapabilityRestorationService::evaluate(
        TrustState::Trusted,
        Capability::MemoryWrite,
        &policy(),
        &RestorationEvidence::new(true, true, Vec::new()),
    );

    assert!(!decision.is_allowed());
}

#[test]
fn restoration_policy_rejects_duplicate_capabilities() {
    let error = CapabilityRestorationPolicy::new([
        (Capability::AuditRead, RestorationStage::DiagnosticsReadOnly),
        (
            Capability::AuditRead,
            RestorationStage::InternalDeterministicWrites,
        ),
    ])
    .expect_err("duplicate capability policy must fail closed");

    assert!(error.to_string().contains("duplicate"));
}
