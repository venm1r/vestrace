use chrono::{Duration, Utc};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use vestrace_application::{
    BudgetPolicyEngine, GrantPolicyEngine, PolicyDecisionEngine, RequestContext,
};
use vestrace_domain::id::{CapabilityGrantId, PrincipalId, WorkspaceId};
use vestrace_domain::{
    AuthorizationRequest, BudgetConstraint, Capability, CapabilityGrant, CapabilityGrantSpec,
    DelegatedCapability, DelegationContract, GrantCondition, HierarchicalBudget, RiskCategory,
};

const G3_FIXTURE: &str = include_str!("fixtures/qualification/g3-governance.json");

#[derive(Debug, Deserialize)]
struct G3FixtureManifest {
    schema_version: String,
    profile: String,
    release_gate: String,
    status: String,
    cases: Vec<G3FixtureCase>,
    known_limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct G3FixtureCase {
    id: String,
    status: String,
    requirements: Vec<String>,
    test: String,
    note: String,
}

fn at() -> vestrace_domain::Timestamp {
    Utc::now()
}

// Explicit test fixture constructor: keep every grant field visible at each call site.
#[allow(clippy::too_many_arguments)]
fn grant_spec(
    workspace_id: WorkspaceId,
    subject_id: PrincipalId,
    issuer_id: PrincipalId,
    capability: Capability,
    operation: &str,
    resource_scope: &str,
    valid_from: vestrace_domain::Timestamp,
    valid_until: Option<vestrace_domain::Timestamp>,
    budget: Option<u64>,
    risk_ceiling: RiskCategory,
) -> CapabilityGrantSpec {
    CapabilityGrantSpec {
        id: CapabilityGrantId::new(),
        workspace_id,
        subject_id,
        issuer_id,
        capability,
        operation: operation.to_owned(),
        resource_scope: resource_scope.to_owned(),
        valid_from,
        valid_until,
        budget: budget.map(BudgetConstraint::new),
        risk_ceiling,
        conditions: Vec::<GrantCondition>::new(),
    }
}

fn parent_and_permission() -> (
    CapabilityGrant,
    CapabilityGrant,
    WorkspaceId,
    PrincipalId,
    vestrace_domain::Timestamp,
) {
    let workspace_id = WorkspaceId::new();
    let parent_subject = PrincipalId::new();
    let root_issuer = PrincipalId::new();
    let now = at();
    let valid_from = now - Duration::minutes(1);
    let valid_until = Some(now + Duration::hours(1));
    let parent = CapabilityGrant::issue(
        grant_spec(
            workspace_id,
            parent_subject,
            root_issuer,
            Capability::MemoryWrite,
            "memory.write",
            "memory://project-1",
            valid_from,
            valid_until,
            Some(100),
            RiskCategory::High,
        ),
        now,
    )
    .unwrap();
    let permission = CapabilityGrant::issue(
        grant_spec(
            workspace_id,
            parent_subject,
            root_issuer,
            Capability::CapabilityDelegate,
            "capability.delegate",
            "memory://project-1",
            valid_from,
            valid_until,
            Some(100),
            RiskCategory::High,
        ),
        now,
    )
    .unwrap();
    (parent, permission, workspace_id, parent_subject, now)
}

fn child_spec(
    workspace_id: WorkspaceId,
    issuer_id: PrincipalId,
    now: vestrace_domain::Timestamp,
) -> CapabilityGrantSpec {
    grant_spec(
        workspace_id,
        PrincipalId::new(),
        issuer_id,
        Capability::MemoryWrite,
        "memory.write",
        "memory://project-1/items/one",
        now,
        Some(now + Duration::minutes(30)),
        Some(30),
        RiskCategory::Medium,
    )
}

fn contract() -> DelegationContract {
    DelegationContract::new(
        Capability::MemoryWrite,
        "memory://project-1",
        1,
        Some(3_600),
        RiskCategory::High,
        30,
    )
    .unwrap()
}

#[test]
fn g3_fixture_manifest_is_traceable_without_claiming_durable_qualification() {
    let manifest: G3FixtureManifest = serde_json::from_str(G3_FIXTURE).unwrap();

    assert_eq!(manifest.schema_version, "vestrace.g3.governance-fixture.v1");
    assert_eq!(manifest.profile, "GOVERNANCE");
    assert_eq!(manifest.release_gate, "v0.4 Govern");
    assert_eq!(manifest.status, "evidence_fixture_only");
    assert!(!manifest.known_limitations.is_empty());

    for requirement_id in ["CAP-005", "CAP-006", "CAP-007", "CAP-013"] {
        assert!(
            manifest
                .cases
                .iter()
                .any(|case| case.requirements.iter().any(|id| id == requirement_id)),
            "{requirement_id} has no G3 fixture mapping"
        );
    }
    for case in &manifest.cases {
        assert!(!case.id.is_empty());
        assert_eq!(case.status, "covered");
        assert!(case.test.starts_with("g3_"));
        assert!(!case.note.is_empty());
    }
}

#[test]
fn g3_valid_delegation_is_attenuated_and_reserves_a_budget_subset() {
    let (parent, permission, workspace_id, parent_subject, now) = parent_and_permission();
    let mut budget = HierarchicalBudget::new(100);
    let delegated = DelegatedCapability::from_root(
        &parent,
        &permission,
        child_spec(workspace_id, parent_subject, now),
        &contract(),
        &mut budget,
        now,
    )
    .unwrap();

    assert_eq!(delegated.parent_grant_id(), parent.id);
    assert_eq!(delegated.delegation_grant_id(), permission.id);
    assert_eq!(delegated.depth(), 1);
    assert_eq!(delegated.grant().capability, Capability::MemoryWrite);
    assert_eq!(delegated.grant().risk_ceiling, RiskCategory::Medium);
    assert_eq!(delegated.grant().budget.unwrap().max_units, 30);
    assert_eq!(budget.remaining(parent.id), Some(70));
    assert_eq!(budget.remaining(delegated.grant().id), Some(30));
}

#[test]
fn g3_broader_child_authority_is_denied() {
    let (parent, permission, workspace_id, parent_subject, now) = parent_and_permission();

    let cases = [
        {
            let mut spec = child_spec(workspace_id, parent_subject, now);
            spec.capability = Capability::MemoryPurge;
            spec.operation = "memory.purge".to_owned();
            spec
        },
        {
            let mut spec = child_spec(workspace_id, parent_subject, now);
            spec.resource_scope = "memory://project-1-other".to_owned();
            spec
        },
        {
            let mut spec = child_spec(workspace_id, parent_subject, now);
            spec.risk_ceiling = RiskCategory::Critical;
            spec
        },
        {
            let mut spec = child_spec(workspace_id, parent_subject, now);
            spec.valid_until = Some(now + Duration::hours(2));
            spec
        },
        {
            let mut spec = child_spec(workspace_id, parent_subject, now);
            spec.budget = Some(BudgetConstraint::new(31));
            spec
        },
    ];

    for spec in cases {
        let mut budget = HierarchicalBudget::new(100);
        assert!(
            DelegatedCapability::from_root(
                &parent,
                &permission,
                spec,
                &contract(),
                &mut budget,
                now,
            )
            .is_err()
        );
    }
}

#[test]
fn g3_missing_permission_and_self_delegation_are_denied() {
    let (parent, permission, workspace_id, parent_subject, now) = parent_and_permission();
    let invalid_permission = CapabilityGrant::issue(
        grant_spec(
            workspace_id,
            parent_subject,
            PrincipalId::new(),
            Capability::MemoryWrite,
            "memory.write",
            "memory://project-1",
            now - Duration::minutes(1),
            Some(now + Duration::hours(1)),
            Some(100),
            RiskCategory::High,
        ),
        now,
    )
    .unwrap();

    let mut budget = HierarchicalBudget::new(100);
    assert!(
        DelegatedCapability::from_root(
            &parent,
            &invalid_permission,
            child_spec(workspace_id, parent_subject, now),
            &contract(),
            &mut budget,
            now,
        )
        .is_err()
    );

    let mut self_spec = child_spec(workspace_id, parent_subject, now);
    self_spec.subject_id = parent_subject;
    let mut budget = HierarchicalBudget::new(100);
    assert!(
        DelegatedCapability::from_root(
            &parent,
            &permission,
            self_spec,
            &contract(),
            &mut budget,
            now,
        )
        .is_err()
    );
}

#[test]
fn g3_delegation_preserves_parent_conditions_and_has_no_partial_root_on_invalid_child() {
    let (mut parent, permission, workspace_id, parent_subject, now) = parent_and_permission();
    parent
        .conditions
        .push(GrantCondition::new("environment", "production").unwrap());
    let mut budget = HierarchicalBudget::new(100);
    assert!(
        DelegatedCapability::from_root(
            &parent,
            &permission,
            child_spec(workspace_id, parent_subject, now),
            &contract(),
            &mut budget,
            now,
        )
        .is_err()
    );
    assert!(budget.reservations.is_empty());

    let mut invalid_child = child_spec(workspace_id, parent_subject, now);
    invalid_child.conditions = vec![GrantCondition {
        name: " ".to_owned(),
        expected_value: "production".to_owned(),
    }];
    let mut budget = HierarchicalBudget::new(100);
    assert!(
        DelegatedCapability::from_root(
            &parent,
            &permission,
            invalid_child,
            &contract(),
            &mut budget,
            now,
        )
        .is_err()
    );
    assert!(budget.reservations.is_empty());
}

#[test]
fn g3_depth_is_bounded_even_when_a_contract_requests_more() {
    let (parent, permission, workspace_id, parent_subject, now) = parent_and_permission();
    assert!(
        DelegationContract::new(
            Capability::MemoryWrite,
            "memory://project-1",
            9,
            None,
            RiskCategory::High,
            30,
        )
        .is_err()
    );

    let mut budget = HierarchicalBudget::new(100);
    let first = DelegatedCapability::from_root(
        &parent,
        &permission,
        child_spec(workspace_id, parent_subject, now),
        &DelegationContract::new(
            Capability::MemoryWrite,
            "memory://project-1",
            2,
            None,
            RiskCategory::High,
            30,
        )
        .unwrap(),
        &mut budget,
        now,
    )
    .unwrap();
    let second_permission = CapabilityGrant::issue(
        grant_spec(
            workspace_id,
            first.grant().subject_id,
            parent_subject,
            Capability::CapabilityDelegate,
            "capability.delegate",
            "memory://project-1/items/one",
            now - Duration::minutes(1),
            Some(now + Duration::hours(1)),
            Some(30),
            RiskCategory::High,
        ),
        now,
    )
    .unwrap();
    let mut next_spec = child_spec(workspace_id, first.grant().subject_id, now);
    next_spec.subject_id = PrincipalId::new();
    let depth_one_contract = DelegationContract::new(
        Capability::MemoryWrite,
        "memory://project-1/items/one",
        1,
        None,
        RiskCategory::High,
        30,
    )
    .unwrap();
    assert!(
        first
            .delegate(
                &second_permission,
                next_spec,
                &depth_one_contract,
                &mut budget,
                now,
            )
            .is_err()
    );
}

#[test]
fn g3_child_validity_cannot_outlive_the_delegation_permission() {
    let (parent, mut permission, workspace_id, parent_subject, now) = parent_and_permission();
    permission.valid_until = Some(now + Duration::minutes(10));
    let mut budget = HierarchicalBudget::new(100);
    assert!(
        DelegatedCapability::from_root(
            &parent,
            &permission,
            child_spec(workspace_id, parent_subject, now),
            &contract(),
            &mut budget,
            now,
        )
        .is_err()
    );
    assert!(budget.reservations.is_empty());
}

#[test]
fn g3_hierarchical_budget_charges_cannot_overspend_or_bypass_parent_allocation() {
    let root = CapabilityGrantId::new();
    let child = CapabilityGrantId::new();
    let sibling = CapabilityGrantId::new();
    let mut budget = HierarchicalBudget::new(100);
    budget.register_root(root, 100).unwrap();
    budget.reserve_child(root, child, 60).unwrap();

    budget.charge(child, 40).unwrap();
    assert_eq!(budget.remaining(child), Some(20));
    assert!(budget.charge(child, 21).is_err());
    assert!(budget.reserve_child(root, sibling, 41).is_err());
    assert_eq!(budget.remaining(root), Some(40));
}

#[tokio::test]
async fn g3_child_grant_uses_the_existing_policy_decision_boundary() {
    let (parent, permission, workspace_id, parent_subject, now) = parent_and_permission();
    let mut budget = HierarchicalBudget::new(100);
    let delegated = DelegatedCapability::from_root(
        &parent,
        &permission,
        child_spec(workspace_id, parent_subject, now),
        &contract(),
        &mut budget,
        now,
    )
    .unwrap();
    let engine = GrantPolicyEngine::new("g3-policy-v1", [delegated.grant().clone()]).unwrap();
    let context = RequestContext::new(workspace_id, delegated.grant().subject_id);
    let request = AuthorizationRequest::new(
        Capability::MemoryWrite,
        "memory.write",
        "memory://project-1/items/one",
        RiskCategory::Low,
    )
    .with_budget_units(10);

    let decision = engine.decide(&context, request).await.unwrap();
    assert!(decision.is_allowed());
    assert_eq!(decision.matched_grant_id, Some(delegated.grant().id));
}

#[tokio::test]
async fn g3_budget_policy_boundary_charges_and_denies_after_child_allocation() {
    let (parent, permission, workspace_id, parent_subject, now) = parent_and_permission();
    let mut budget = HierarchicalBudget::new(100);
    let delegated = DelegatedCapability::from_root(
        &parent,
        &permission,
        child_spec(workspace_id, parent_subject, now),
        &contract(),
        &mut budget,
        now,
    )
    .unwrap();
    let shared_budget = Arc::new(Mutex::new(budget));
    let engine = BudgetPolicyEngine::new(
        "g3-budget-policy-v1",
        [delegated.grant().clone()],
        shared_budget.clone(),
    )
    .unwrap();
    let context = RequestContext::new(workspace_id, delegated.grant().subject_id);
    let request = || {
        AuthorizationRequest::new(
            Capability::MemoryWrite,
            "memory.write",
            "memory://project-1/items/one",
            RiskCategory::Low,
        )
        .with_budget_units(20)
    };

    assert!(
        engine
            .decide(&context, request())
            .await
            .unwrap()
            .is_allowed()
    );
    let exhausted = engine.decide(&context, request()).await.unwrap();
    assert!(!exhausted.is_allowed());
    assert_eq!(
        exhausted.reason,
        vestrace_domain::PolicyDecisionReason::BudgetExceeded
    );
    assert_eq!(
        shared_budget
            .lock()
            .unwrap()
            .remaining(delegated.grant().id),
        Some(10)
    );
}
