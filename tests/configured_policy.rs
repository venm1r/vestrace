//! A configuration-driven policy engine for deployments that have no grant
//! store yet.
//!
//! This exists so an operator can open a console against a running deployment
//! before capability-grant persistence is implemented. It is deliberately not a
//! grant: it never claims `GrantMatched`, it cannot be revoked, and it is not
//! scoped to a subject.

use std::sync::Arc;

use vestrace_application::{
    ApplicationError, ConfiguredCapabilityPolicyEngine, PolicyDecisionEngine, RequestContext,
};
use vestrace_domain::{
    AuthorizationRequest, Capability, PolicyDecisionReason, PolicyDecisionResult, PrincipalId,
    RiskCategory, WorkspaceId,
};

fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

fn request(capability: Capability, risk: RiskCategory, path: &str) -> AuthorizationRequest {
    AuthorizationRequest::new(capability, "http.get".to_owned(), path, risk)
}

fn engine(
    capabilities: &[&str],
    ceiling: RiskCategory,
) -> Result<ConfiguredCapabilityPolicyEngine, ApplicationError> {
    ConfiguredCapabilityPolicyEngine::from_strings("console-config-v1", capabilities, ceiling)
}

#[tokio::test]
async fn configured_capability_is_allowed_at_any_resource_scope() {
    let engine = engine(&["memory.read", "execution.read"], RiskCategory::Medium).unwrap();
    let context = context();

    for path in ["/v1/runs", "/v1/runs/018f5b7e-3a2b-7c11-8a22-1234567890ab"] {
        let decision = engine
            .decide(
                &context,
                request(Capability::ExecutionRead, RiskCategory::Low, path),
            )
            .await
            .unwrap();

        assert_eq!(decision.result, PolicyDecisionResult::Allow);
        // Never `GrantMatched`: no grant was consulted, a configured allowance
        // was applied.
        assert_eq!(decision.reason, PolicyDecisionReason::ConfiguredAllowance);
        assert!(decision.matched_grant_id.is_none());
        assert_eq!(decision.policy_version, "console-config-v1");
    }
}

#[tokio::test]
async fn capability_outside_the_configured_set_is_denied() {
    let engine = engine(&["memory.read"], RiskCategory::Medium).unwrap();

    let decision = engine
        .decide(
            &context(),
            request(Capability::MemoryPurge, RiskCategory::Low, "/v1/memory"),
        )
        .await
        .unwrap();

    assert_eq!(decision.result, PolicyDecisionResult::Deny);
    assert_eq!(decision.reason, PolicyDecisionReason::CapabilityMismatch);
}

/// The risk ceiling is the one brake this engine keeps: a configured read
/// allowance must not silently authorize a critical operation.
#[tokio::test]
async fn risk_above_the_configured_ceiling_is_denied() {
    let engine = engine(&["execution.read"], RiskCategory::Low).unwrap();

    let decision = engine
        .decide(
            &context(),
            request(
                Capability::ExecutionRead,
                RiskCategory::Critical,
                "/v1/runs",
            ),
        )
        .await
        .unwrap();

    assert_eq!(decision.result, PolicyDecisionResult::Deny);
    assert_eq!(decision.reason, PolicyDecisionReason::RiskExceedsCeiling);
}

#[tokio::test]
async fn an_empty_capability_set_denies_everything() {
    let engine = engine(&[], RiskCategory::Critical).unwrap();

    let decision = engine
        .decide(
            &context(),
            request(Capability::MemoryRead, RiskCategory::Low, "/v1/memory"),
        )
        .await
        .unwrap();

    assert_eq!(decision.result, PolicyDecisionResult::Deny);
    assert_eq!(decision.reason, PolicyDecisionReason::DefaultDeny);
}

#[tokio::test]
async fn an_unknown_capability_name_is_rejected_at_construction() {
    let error = engine(&["memory.read", "not.a.capability"], RiskCategory::Low).unwrap_err();

    assert!(
        matches!(error, ApplicationError::Domain(_)),
        "unexpected error: {error:?}"
    );
}

#[tokio::test]
async fn a_blank_policy_version_is_rejected() {
    let error =
        ConfiguredCapabilityPolicyEngine::from_strings("   ", &["memory.read"], RiskCategory::Low)
            .unwrap_err();

    assert!(matches!(error, ApplicationError::InvalidConfiguration(_)));
}

#[tokio::test]
async fn engine_is_usable_as_a_shared_decision_engine() {
    let engine: Arc<dyn PolicyDecisionEngine> =
        Arc::new(engine(&["memory.read"], RiskCategory::Low).unwrap());

    let decision = engine
        .decide(
            &context(),
            request(Capability::MemoryRead, RiskCategory::Low, "/v1/memory"),
        )
        .await
        .unwrap();

    assert_eq!(decision.result, PolicyDecisionResult::Allow);
}

/// Deployment configuration arrives as a delimited string; folded YAML lists
/// leave whitespace around entries and empty trailing fragments.
#[tokio::test]
async fn capability_names_tolerate_configuration_whitespace() {
    let engine = ConfiguredCapabilityPolicyEngine::from_strings(
        "console-config-v1",
        &["memory.read", " execution.read", "", "  "],
        RiskCategory::Medium,
    )
    .unwrap();

    let decision = engine
        .decide(
            &context(),
            request(Capability::ExecutionRead, RiskCategory::Low, "/v1/runs"),
        )
        .await
        .unwrap();

    assert_eq!(decision.result, PolicyDecisionResult::Allow);
}
