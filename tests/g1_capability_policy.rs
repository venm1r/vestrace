use chrono::{Duration, TimeZone, Utc};
use serde::Deserialize;
use vestrace_application::{
    DenyAllPolicyEngine, GrantPolicyEngine, PolicyDecisionEngine, RequestContext,
};
use vestrace_domain::id::{CapabilityGrantId, PolicyDecisionId, PrincipalId, WorkspaceId};
use vestrace_domain::{
    AuthorizationRequest, BudgetConstraint, Capability, CapabilityGrant, CapabilityGrantSpec,
    GrantCondition, PolicyDecisionReason, PolicyDecisionResult, RiskCategory,
    evaluate_capability_grants,
};

const G1_FIXTURE: &str = include_str!("fixtures/qualification/g1-governance.json");

#[derive(Debug, Deserialize)]
struct G1FixtureManifest {
    schema_version: String,
    profile: String,
    release_gate: String,
    status: String,
    cases: Vec<G1FixtureCase>,
    known_limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct G1FixtureCase {
    id: String,
    status: String,
    requirements: Vec<String>,
    test: String,
    note: String,
}

fn fixed_at() -> vestrace_domain::Timestamp {
    Utc.with_ymd_and_hms(2026, 8, 11, 3, 0, 0).single().unwrap()
}

fn valid_spec(workspace_id: WorkspaceId, subject_id: PrincipalId) -> CapabilityGrantSpec {
    let at = fixed_at();
    CapabilityGrantSpec {
        id: CapabilityGrantId::new(),
        workspace_id,
        subject_id,
        issuer_id: PrincipalId::new(),
        capability: Capability::MemoryWrite,
        operation: "memory.write".to_owned(),
        resource_scope: "memory://project-1".to_owned(),
        valid_from: at,
        valid_until: Some(at + Duration::hours(1)),
        budget: Some(BudgetConstraint::new(10)),
        risk_ceiling: RiskCategory::Medium,
        conditions: vec![GrantCondition::new("environment", "production").unwrap()],
    }
}

fn matching_request() -> AuthorizationRequest {
    AuthorizationRequest::new(
        Capability::MemoryWrite,
        "memory.write",
        "memory://project-1",
        RiskCategory::Low,
    )
    .with_budget_units(4)
    .with_condition(GrantCondition::new("environment", "production").unwrap())
}

#[test]
fn g1_fixture_manifest_is_traceable_without_claiming_governance_qualification() {
    let manifest: G1FixtureManifest = serde_json::from_str(G1_FIXTURE).unwrap();

    assert_eq!(manifest.schema_version, "vestrace.g1.governance-fixture.v1");
    assert_eq!(manifest.profile, "GOVERNANCE");
    assert_eq!(manifest.release_gate, "v0.4 Govern");
    assert_eq!(manifest.status, "evidence_fixture_only");
    assert!(!manifest.known_limitations.is_empty());

    for requirement_id in [
        "CAP-001", "CAP-002", "CAP-003", "CAP-004", "CAP-008", "CAP-009", "CAP-011",
    ] {
        assert!(
            manifest
                .cases
                .iter()
                .any(|case| case.requirements.iter().any(|id| id == requirement_id)),
            "{requirement_id} has no G1 fixture mapping"
        );
    }

    for case in &manifest.cases {
        assert!(!case.id.is_empty());
        assert!(matches!(case.status.as_str(), "covered" | "blocked"));
        assert!(case.test.starts_with("g1_"));
        assert!(!case.note.is_empty());
    }
}

#[test]
fn g1_grant_rejects_blank_selectors_conditions_and_inverted_validity() {
    let workspace_id = WorkspaceId::new();
    let subject_id = PrincipalId::new();

    let mut blank_operation = valid_spec(workspace_id, subject_id);
    blank_operation.operation = " ".to_owned();
    assert!(CapabilityGrant::issue(blank_operation, fixed_at()).is_err());

    let mut blank_resource = valid_spec(workspace_id, subject_id);
    blank_resource.resource_scope = "".to_owned();
    assert!(CapabilityGrant::issue(blank_resource, fixed_at()).is_err());

    let mut inverted_validity = valid_spec(workspace_id, subject_id);
    inverted_validity.valid_until = Some(fixed_at() - Duration::seconds(1));
    assert!(CapabilityGrant::issue(inverted_validity, fixed_at()).is_err());

    assert!(GrantCondition::new(" ", "production").is_err());
    assert!(GrantCondition::new("environment", " ").is_err());
}

#[test]
fn g1_matching_grant_allows_and_records_exact_policy_input() {
    let workspace_id = WorkspaceId::new();
    let subject_id = PrincipalId::new();
    let grant = CapabilityGrant::issue(valid_spec(workspace_id, subject_id), fixed_at()).unwrap();
    let request = matching_request();

    let decision = evaluate_capability_grants(
        PolicyDecisionId::new(),
        workspace_id,
        subject_id,
        "g1-policy-v1",
        &request,
        &[grant.clone()],
        fixed_at(),
    )
    .unwrap();

    assert_eq!(decision.result, PolicyDecisionResult::Allow);
    assert!(decision.is_allowed());
    assert_eq!(decision.policy_version, "g1-policy-v1");
    assert_eq!(decision.matched_grant_id, Some(grant.id));
    assert_eq!(decision.input_state.workspace_id, workspace_id);
    assert_eq!(decision.input_state.subject_id, subject_id);
    assert_eq!(decision.input_state.operation, "memory.write");
    assert_eq!(decision.input_state.resource_scope, "memory://project-1");
    assert_eq!(decision.input_state.requested_budget_units, 4);
    assert_eq!(decision.input_state.context_risk, RiskCategory::Low);
    assert_eq!(decision.input_state.effective_risk, RiskCategory::Low);
}

#[test]
fn g1_policy_defaults_to_deny_for_every_non_matching_constraint() {
    let workspace_id = WorkspaceId::new();
    let subject_id = PrincipalId::new();
    let grant = CapabilityGrant::issue(valid_spec(workspace_id, subject_id), fixed_at()).unwrap();

    let cases = [
        (
            "wrong operation",
            matching_request().with_operation("memory.purge"),
            PolicyDecisionReason::OperationMismatch,
        ),
        (
            "wrong resource",
            matching_request().with_resource_scope("memory://other"),
            PolicyDecisionReason::ResourceMismatch,
        ),
        (
            "wrong capability",
            AuthorizationRequest::new(
                Capability::MemoryPurge,
                "memory.write",
                "memory://project-1",
                RiskCategory::Low,
            )
            .with_budget_units(4)
            .with_condition(GrantCondition::new("environment", "production").unwrap()),
            PolicyDecisionReason::CapabilityMismatch,
        ),
        (
            "unsatisfied condition",
            AuthorizationRequest::new(
                Capability::MemoryWrite,
                "memory.write",
                "memory://project-1",
                RiskCategory::Low,
            )
            .with_budget_units(4),
            PolicyDecisionReason::ConditionNotSatisfied,
        ),
        (
            "over budget",
            matching_request().with_budget_units(11),
            PolicyDecisionReason::BudgetExceeded,
        ),
        (
            "context elevated risk",
            matching_request().with_context_risk(RiskCategory::High),
            PolicyDecisionReason::RiskExceedsCeiling,
        ),
    ];

    for (label, request, expected_reason) in cases {
        let decision = evaluate_capability_grants(
            PolicyDecisionId::new(),
            workspace_id,
            subject_id,
            "g1-policy-v1",
            &request,
            &[grant.clone()],
            fixed_at(),
        )
        .unwrap();

        assert_eq!(decision.result, PolicyDecisionResult::Deny, "{label}");
        assert!(!decision.is_allowed(), "{label}");
        assert_eq!(decision.reason, expected_reason, "{label}");
        assert_eq!(decision.policy_version, "g1-policy-v1", "{label}");
    }
}

#[test]
fn g1_expired_and_revoked_grants_are_rejected_fail_closed() {
    let workspace_id = WorkspaceId::new();
    let subject_id = PrincipalId::new();
    let at = fixed_at();
    let spec = valid_spec(workspace_id, subject_id);
    let grant = CapabilityGrant::issue(spec, at).unwrap();
    let request = matching_request();
    assert!(grant.is_active_at(at));
    assert!(!grant.is_active_at(at + Duration::hours(1)));

    let expired = evaluate_capability_grants(
        PolicyDecisionId::new(),
        workspace_id,
        subject_id,
        "g1-policy-v1",
        &request,
        &[grant.clone()],
        at + Duration::hours(1),
    )
    .unwrap();
    assert_eq!(expired.result, PolicyDecisionResult::Deny);
    assert_eq!(expired.reason, PolicyDecisionReason::Expired);

    let mut revoked_grant = grant;
    revoked_grant.revoke(at + Duration::minutes(1)).unwrap();
    let revoked = evaluate_capability_grants(
        PolicyDecisionId::new(),
        workspace_id,
        subject_id,
        "g1-policy-v1",
        &request,
        &[revoked_grant],
        at + Duration::minutes(2),
    )
    .unwrap();
    assert_eq!(revoked.result, PolicyDecisionResult::Deny);
    assert_eq!(revoked.reason, PolicyDecisionReason::Revoked);
}

#[test]
fn g1_not_yet_valid_grants_are_rejected_fail_closed() {
    let workspace_id = WorkspaceId::new();
    let subject_id = PrincipalId::new();
    let at = fixed_at();
    let grant = CapabilityGrant::issue(valid_spec(workspace_id, subject_id), at).unwrap();

    let decision = evaluate_capability_grants(
        PolicyDecisionId::new(),
        workspace_id,
        subject_id,
        "g1-policy-v1",
        &matching_request(),
        &[grant],
        at - Duration::seconds(1),
    )
    .unwrap();

    assert_eq!(decision.result, PolicyDecisionResult::Deny);
    assert_eq!(decision.reason, PolicyDecisionReason::NotYetValid);
}

#[test]
fn g1_workspace_and_subject_mismatches_cannot_use_a_grant() {
    let workspace_id = WorkspaceId::new();
    let subject_id = PrincipalId::new();
    let grant = CapabilityGrant::issue(valid_spec(workspace_id, subject_id), fixed_at()).unwrap();
    let request = matching_request();

    let wrong_workspace = evaluate_capability_grants(
        PolicyDecisionId::new(),
        WorkspaceId::new(),
        subject_id,
        "g1-policy-v1",
        &request,
        &[grant.clone()],
        fixed_at(),
    )
    .unwrap();
    assert_eq!(
        wrong_workspace.reason,
        PolicyDecisionReason::WorkspaceMismatch
    );
    assert!(!wrong_workspace.is_allowed());

    let wrong_subject = evaluate_capability_grants(
        PolicyDecisionId::new(),
        workspace_id,
        PrincipalId::new(),
        "g1-policy-v1",
        &request,
        &[grant],
        fixed_at(),
    )
    .unwrap();
    assert_eq!(wrong_subject.reason, PolicyDecisionReason::SubjectMismatch);
    assert!(!wrong_subject.is_allowed());
}

#[tokio::test]
async fn g1_application_grant_policy_engine_binds_context_identity() {
    let workspace_id = WorkspaceId::new();
    let subject_id = PrincipalId::new();
    let now = Utc::now();
    let mut spec = valid_spec(workspace_id, subject_id);
    spec.valid_from = now - Duration::minutes(1);
    spec.valid_until = Some(now + Duration::minutes(1));
    let grant = CapabilityGrant::issue(spec, now).unwrap();
    let engine = GrantPolicyEngine::new("g1-policy-v1", vec![grant]).unwrap();
    let context = RequestContext::new(workspace_id, subject_id);

    let decision = engine.decide(&context, matching_request()).await.unwrap();

    assert!(decision.is_allowed());
    assert_eq!(decision.input_state.workspace_id, workspace_id);
    assert_eq!(decision.input_state.subject_id, subject_id);
}

#[tokio::test]
async fn g1_deny_all_policy_engine_is_explicitly_fail_closed() {
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let decision = DenyAllPolicyEngine
        .decide(&context, matching_request())
        .await
        .unwrap();

    assert_eq!(decision.result, PolicyDecisionResult::Deny);
    assert_eq!(decision.reason, PolicyDecisionReason::DefaultDeny);
    assert!(!decision.is_allowed());
}
