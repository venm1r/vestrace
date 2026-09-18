use std::collections::HashMap;

use chrono::{Duration, TimeZone, Utc};
use vestrace_domain::{
    Capability, HealthFindingId, HealthScope, HealthState, InvariantDefinition, InvariantRegistry,
    RepairRisk, Repairability,
    conformance::gate::{UnderstandGate, UnderstandGateFailure, UnderstandMilestone},
    health::{
        FindingDisposition, HealthProjectionState, HealthSeverity, RepairBudget, RepairBudgetError,
        RepairExecutionError, RepairExecutionResult, RepairOperatorCommand, RepairOperatorPhase,
        RepairPlan, Reversibility, VerificationResult, VerificationRun, assess_recurrence,
    },
};

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn definition(id: &str, dependencies: Vec<&str>) -> InvariantDefinition {
    InvariantDefinition::new(
        id,
        "v1",
        "derived state matches the authoritative source",
        "rebuild the projection from the event log",
        HealthSeverity::Error,
        Repairability::Auto,
        dependencies.into_iter().map(str::to_owned).collect(),
    )
    .unwrap()
}

fn finding(definition: &InvariantDefinition, fingerprint: &str) -> vestrace_domain::HealthFinding {
    vestrace_domain::HealthFinding::new(
        definition,
        HealthScope::workspace(vestrace_domain::WorkspaceId::new()),
        fingerprint,
        HealthState::Unhealthy,
        vec!["evidence:health:1".to_owned()],
        at(10),
    )
    .unwrap()
}

#[test]
fn h1_findings_keep_occurrences_separate_and_projection_follows_dependencies() {
    let root = definition("health.root", vec!["health.index"]);
    let unrelated = definition("health.unrelated", vec![]);
    let registry = InvariantRegistry::new([root.clone(), unrelated.clone()]).unwrap();
    assert!(registry.get("health.root").is_some());
    assert_eq!(registry.definitions().count(), 2);
    let root_finding = finding(&root, "root-fp");
    let mut index_finding = finding(&definition("health.index", vec![]), "index-fp");
    let unrelated_finding = finding(&unrelated, "unrelated-fp");

    let first = index_finding
        .record_occurrence(
            HealthState::Unknown,
            vec!["evidence:index:unknown".into()],
            at(20),
        )
        .unwrap();
    let second = index_finding
        .record_occurrence(
            HealthState::Unhealthy,
            vec!["evidence:index:bad".into()],
            at(30),
        )
        .unwrap();
    assert_ne!(first.id(), second.id());
    assert_eq!(first.finding_id(), index_finding.id());
    assert_eq!(index_finding.occurrence_count(), 2);

    let mut dependencies = HashMap::new();
    dependencies.insert("health.root".to_owned(), vec!["health.index".to_owned()]);
    let projection = vestrace_domain::health::project_health(
        "health.root",
        &[root_finding, index_finding, unrelated_finding],
        &dependencies,
    );
    assert_eq!(projection.state(), HealthProjectionState::Unhealthy);

    let unknown = vestrace_domain::health::project_health("health.missing", &[], &HashMap::new());
    assert_eq!(unknown.state(), HealthProjectionState::Unknown);
}

#[test]
fn h2_stale_or_unauthorized_plans_never_start_and_verification_controls_closure() {
    let definition = definition("health.repairable", vec![]);
    let finding = finding(&definition, "source-v1");
    let plan = RepairPlan::new(
        vec![finding.id()],
        "source-v1",
        finding.scope().clone(),
        vec!["source-v1".into()],
        vec!["rebuild-derived-index".into()],
        vec!["source-v2".into()],
        vec!["check-derived-index".into()],
        RepairRisk::Low,
        Reversibility::Restartable,
        Capability::MemoryWrite,
        at(40),
        Some(at(400)),
    )
    .unwrap();

    let denied = plan.start_execution(
        &vestrace_domain::health::RepairAuthorization::deny(
            Capability::MemoryWrite,
            "policy-v1",
            "evidence:denied",
        ),
        "source-v1",
        at(50),
    );
    assert!(matches!(denied, Err(RepairExecutionError::Unauthorized)));

    let stale = plan.start_execution(
        &vestrace_domain::health::RepairAuthorization::allow(
            Capability::MemoryWrite,
            "policy-v1",
            "evidence:allowed",
        ),
        "source-v2",
        at(50),
    );
    assert!(matches!(stale, Err(RepairExecutionError::StalePlan)));

    let execution = plan
        .start_execution(
            &vestrace_domain::health::RepairAuthorization::allow(
                Capability::MemoryWrite,
                "policy-v1",
                "evidence:allowed",
            ),
            "source-v1",
            at(50),
        )
        .unwrap()
        .complete(RepairExecutionResult::Succeeded, at(60))
        .expect("an execution completes once");
    assert_eq!(execution.result(), Some(RepairExecutionResult::Succeeded));

    // A verdict cannot be rewritten after the fact: an execution record is
    // evidence about a change to production state, and one whose result can be
    // replaced is evidence of nothing.
    assert!(
        execution
            .clone()
            .complete(RepairExecutionResult::Failed, at(70))
            .is_err()
    );
    assert_eq!(
        finding.lifecycle_status(),
        vestrace_domain::FindingLifecycleStatus::Open
    );

    let verification = VerificationRun::new(
        plan.id(),
        vec![finding.id()],
        VerificationResult::Passed,
        vec!["check-derived-index".into()],
        vec!["evidence:verified".into()],
        at(70),
    )
    .unwrap();
    let resolved = finding.apply_verification(&verification).unwrap();
    assert_eq!(
        resolved.lifecycle_status(),
        vestrace_domain::FindingLifecycleStatus::Resolved
    );
}

#[test]
fn h3_budget_flapping_and_dispositions_are_bounded_and_auditable() {
    let mut budget = RepairBudget::new(2, Duration::hours(1), Duration::minutes(5));
    assert!(budget.reserve(at(100)).is_ok());
    assert!(matches!(
        budget.reserve(at(101)),
        Err(RepairBudgetError::CooldownActive { .. })
    ));
    assert!(budget.reserve(at(500)).is_ok());
    assert!(matches!(
        budget.reserve(at(1_000)),
        Err(RepairBudgetError::AttemptBudgetExceeded)
    ));

    let definition = definition("health.flapping", vec![]);
    let mut finding = finding(&definition, "flap-fp");
    let mut occurrences = Vec::new();
    for (index, state) in [
        HealthState::Unhealthy,
        HealthState::Healthy,
        HealthState::Unhealthy,
        HealthState::Healthy,
    ]
    .into_iter()
    .enumerate()
    {
        occurrences.push(
            finding
                .record_occurrence(
                    state,
                    vec![format!("evidence:flap:{index}")],
                    at(700 + index as i64),
                )
                .unwrap(),
        );
    }
    finding
        .record_occurrence(
            HealthState::Unhealthy,
            vec!["evidence:flap:final".into()],
            at(704),
        )
        .unwrap();
    let recurrence = assess_recurrence(&occurrences, 3);
    assert!(recurrence.is_recurrent());
    assert!(recurrence.is_flapping());

    finding
        .set_disposition(FindingDisposition::accepted_risk(
            vestrace_domain::PrincipalId::new(),
            "known provider lag",
            Some(at(2_000)),
            "health-policy-v1",
            "audit:accepted-risk:1",
        ))
        .unwrap();
    assert_eq!(
        finding.lifecycle_status(),
        vestrace_domain::FindingLifecycleStatus::AcceptedRisk
    );
    assert_eq!(finding.observed_state(), HealthState::Unhealthy);
    assert!(finding.disposition().is_some());
}

#[test]
fn h4_operator_contract_keeps_inspect_plan_and_repair_distinct() {
    let plan_id = vestrace_domain::RepairPlanId::new();
    let inspect = RepairOperatorCommand::inspect();
    assert_eq!(inspect.phase(), RepairOperatorPhase::Inspect);
    assert!(inspect.is_read_only());

    let plan = RepairOperatorCommand::plan(vec![HealthFindingId::new()]);
    assert_eq!(plan.phase(), RepairOperatorPhase::Plan);
    assert!(plan.authorization().is_none());

    let repair = RepairOperatorCommand::repair(
        plan_id,
        "source-v1",
        vestrace_domain::health::RepairAuthorization::allow(
            Capability::MemoryWrite,
            "policy-v1",
            "evidence:repair",
        ),
    )
    .unwrap();
    assert_eq!(repair.phase(), RepairOperatorPhase::Repair);
    assert!(!repair.is_read_only());
    assert!(repair.plan_id().is_some());
}

#[test]
fn h5_understand_gate_requires_all_hlt_must_evidence() {
    let evidence = UnderstandGate::required_requirements()
        .into_iter()
        .map(|requirement_id| {
            vestrace_domain::conformance::gate::HardGateEvidence::pass(
                requirement_id,
                format!("evidence:h5:{requirement_id}"),
                None,
                vestrace_domain::conformance::gate::EvidenceOrigin::LocalExecutable,
            )
        })
        .collect::<Vec<_>>();
    let decision = UnderstandGate::evaluate(&evidence);
    assert!(decision.is_passed());
    assert_eq!(decision.milestone(), UnderstandMilestone::Understand);
    assert_eq!(UnderstandGate::required_requirements().len(), 19);

    let missing = UnderstandGate::required_requirements()
        .into_iter()
        .filter(|id| id.number != 9)
        .map(|requirement_id| {
            vestrace_domain::conformance::gate::HardGateEvidence::pass(
                requirement_id,
                format!("evidence:h5:{requirement_id}"),
                None,
                vestrace_domain::conformance::gate::EvidenceOrigin::LocalExecutable,
            )
        })
        .collect::<Vec<_>>();
    let decision = UnderstandGate::evaluate(&missing);
    assert!(!decision.is_passed());
    assert!(
        decision
            .failures()
            .contains(&UnderstandGateFailure::MissingEvidence {
                requirement_id: vestrace_domain::conformance::RequirementId::new(
                    vestrace_domain::conformance::RequirementFamily::Hlt,
                    9,
                ),
            })
    );
}
