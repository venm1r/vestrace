use chrono::{TimeZone, Utc};
use serde::Deserialize;
use vestrace_domain::id::{
    EvaluationId, EventId, LearningProjectionId, LearningProposalId, ModelExecutionAttemptId,
    ModelExecutionId, ModelId, PrincipalId, WorkspaceId,
};
use vestrace_domain::{
    EvaluationAuthority, EvaluationFact, EvaluationMetric, EvaluationResult, EvaluationTarget,
    EvaluatorKind, EvaluatorRef, EvidenceRef, LearnedProjection, LearningChange, LearningProposal,
    LearningProposalStatus, LearningTarget, ProjectionAuthority, ProjectionKind,
};

const L3_FIXTURE: &str = include_str!("fixtures/qualification/l3-cognition.json");

#[derive(Debug, Deserialize)]
struct L3FixtureManifest {
    schema_version: String,
    profile: String,
    release_gate: String,
    status: String,
    benchmark_suite_version: String,
    cases: Vec<L3FixtureCase>,
    known_limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct L3FixtureCase {
    id: String,
    status: String,
    mode: String,
    families: Vec<String>,
    requirements: Vec<String>,
    test: String,
    note: String,
}

fn fixed_at() -> vestrace_domain::Timestamp {
    Utc.with_ymd_and_hms(2026, 8, 11, 2, 0, 0).single().unwrap()
}

fn evaluation_fact(
    workspace_id: WorkspaceId,
    id: EvaluationId,
    result: EvaluationResult,
    metric_kind: &str,
    metric_value: f64,
) -> EvaluationFact {
    let attempt_id = ModelExecutionAttemptId::new();
    EvaluationFact::new(
        id,
        workspace_id,
        EvaluationTarget::ModelExecution {
            execution_id: ModelExecutionId::new(),
            attempt_id: Some(attempt_id),
            model_id: ModelId::new(),
            model_revision: "model-v7".to_owned(),
        },
        EvaluatorRef {
            kind: EvaluatorKind::Deterministic,
            model_id: None,
            revision: None,
            principal_id: None,
        },
        EvaluationMetric::new(metric_kind, metric_value, Some("score".to_owned())).unwrap(),
        result,
        vec![
            EvidenceRef::ModelExecutionRef { attempt_id },
            EvidenceRef::event(EventId::new()),
        ],
        EvaluationAuthority::Deterministic,
        "cognition-bench-policy-v1",
        fixed_at(),
    )
    .unwrap()
}

fn advisory_evaluation_fact(workspace_id: WorkspaceId, id: EvaluationId) -> EvaluationFact {
    let attempt_id = ModelExecutionAttemptId::new();
    EvaluationFact::new(
        id,
        workspace_id,
        EvaluationTarget::ModelExecution {
            execution_id: ModelExecutionId::new(),
            attempt_id: Some(attempt_id),
            model_id: ModelId::new(),
            model_revision: "model-v7".to_owned(),
        },
        EvaluatorRef {
            kind: EvaluatorKind::ModelJudge,
            model_id: Some(ModelId::new()),
            revision: Some("judge-v3".to_owned()),
            principal_id: None,
        },
        EvaluationMetric::new("quality.relevance", 0.01, Some("score".to_owned())).unwrap(),
        EvaluationResult::Fail,
        vec![EvidenceRef::ModelExecutionRef { attempt_id }],
        EvaluationAuthority::Advisory,
        "cognition-bench-policy-v1",
        fixed_at(),
    )
    .unwrap()
}

#[test]
fn l3_fixture_manifest_is_traceable_without_claiming_cognition_qualification() {
    let manifest: L3FixtureManifest = serde_json::from_str(L3_FIXTURE).unwrap();

    assert_eq!(manifest.schema_version, "vestrace.l3.cognition-fixture.v1");
    assert_eq!(manifest.profile, "COGNITION");
    assert_eq!(manifest.release_gate, "v0.3 Learn");
    assert_eq!(manifest.status, "evidence_fixture_only");
    assert_eq!(manifest.benchmark_suite_version, "cognition-bench-v1");
    assert!(!manifest.known_limitations.is_empty());
    assert!(manifest.cases.iter().any(|case| case.mode == "baseline"));
    assert!(manifest.cases.iter().any(|case| case.mode == "comparative"));

    for requirement_id in (1..=8).map(|number| format!("LRN-{number:03}")) {
        assert!(
            manifest
                .cases
                .iter()
                .any(|case| case.requirements.contains(&requirement_id)),
            "{requirement_id} has no L3 fixture mapping"
        );
    }

    for case in &manifest.cases {
        assert!(!case.id.is_empty());
        assert!(matches!(case.status.as_str(), "covered" | "blocked"));
        assert!(matches!(case.mode.as_str(), "baseline" | "comparative"));
        assert!(!case.families.is_empty());
        assert!(!case.requirements.is_empty());
        assert!(case.test.starts_with("l3_"));
        assert!(!case.note.is_empty());
    }
}

#[test]
fn l3_rebuild_is_deterministic_from_canonical_facts_and_preserves_measurement_refs() {
    let workspace_id = WorkspaceId::new();
    let first_fact = evaluation_fact(
        workspace_id,
        EvaluationId::new(),
        EvaluationResult::Pass,
        "quality.relevance",
        0.90,
    );
    let second_fact = evaluation_fact(
        workspace_id,
        EvaluationId::new(),
        EvaluationResult::Fail,
        "quality.relevance",
        0.40,
    );
    let advisory_fact = advisory_evaluation_fact(workspace_id, EvaluationId::new());
    let target = LearningTarget::Model {
        model_id: ModelId::new(),
    };
    let projection_id = LearningProjectionId::new();

    let first = LearnedProjection::rebuild_from_facts(
        projection_id,
        workspace_id,
        ProjectionKind::PerformanceSummary,
        target.clone(),
        "cognition-bench-v1",
        3,
        &[
            advisory_fact.clone(),
            second_fact.clone(),
            first_fact.clone(),
        ],
        fixed_at(),
    )
    .unwrap();
    let second = LearnedProjection::rebuild_from_facts(
        projection_id,
        workspace_id,
        ProjectionKind::PerformanceSummary,
        target,
        "cognition-bench-v1",
        3,
        &[
            first_fact.clone(),
            second_fact.clone(),
            advisory_fact.clone(),
        ],
        fixed_at(),
    )
    .unwrap();

    assert_eq!(first, second);
    assert_eq!(first.authority, ProjectionAuthority::Advisory);
    assert_eq!(first.source_evaluation_fact_ids.len(), 3);
    assert!(first.source_evaluation_fact_ids.contains(&first_fact.id));
    assert!(first.source_evaluation_fact_ids.contains(&second_fact.id));
    assert_eq!(first.content["fact_count"], 3);
    assert_eq!(first.content["result_counts"]["pass"], 1);
    assert_eq!(first.content["result_counts"]["fail"], 1);
    assert_eq!(first.content["aggregated_fact_count"], 2);
    assert_eq!(
        first.content["ignored_lower_authority_fact_ids"][0],
        advisory_fact.id.to_string()
    );
    assert_eq!(first.content["metric_kinds"][0], "quality.relevance");
    assert!(first.source_evidence_refs.contains(&EvidenceRef::event(
        match first_fact.evidence_refs[1] {
            EvidenceRef::EventRef { event_id } => event_id,
            _ => panic!("fixture fact must include an event reference"),
        }
    )));
}

#[test]
fn l3_authority_ordering_and_learning_boundary_are_explicit() {
    assert!(EvaluationAuthority::Deterministic.is_stronger_than(EvaluationAuthority::Advisory));
    assert!(EvaluationAuthority::HumanAuthorized.is_stronger_than(EvaluationAuthority::Advisory));
    assert!(!EvaluationAuthority::Advisory.is_stronger_than(EvaluationAuthority::Deterministic));

    let workspace_id = WorkspaceId::new();
    let fact = evaluation_fact(
        workspace_id,
        EvaluationId::new(),
        EvaluationResult::Pass,
        "quality.relevance",
        0.92,
    );
    let projection = LearnedProjection::rebuild_from_facts(
        LearningProjectionId::new(),
        workspace_id,
        ProjectionKind::Recommendation,
        LearningTarget::Model {
            model_id: ModelId::new(),
        },
        "cognition-bench-v1",
        1,
        std::slice::from_ref(&fact),
        fixed_at(),
    )
    .unwrap();
    assert_eq!(projection.authority, ProjectionAuthority::Advisory);

    let proposal = LearningProposal::new(
        LearningProposalId::new(),
        workspace_id,
        vec![projection.id],
        vec![fact.id],
        LearningTarget::Model {
            model_id: ModelId::new(),
        },
        LearningChange::ModelRouting {
            configuration: serde_json::json!({"strategy": "baseline"}),
        },
        1,
        "bounded benchmark recommendation",
        "cognition-bench-policy-v1",
        PrincipalId::new(),
        fixed_at(),
    )
    .unwrap();
    assert_eq!(proposal.status, LearningProposalStatus::Draft);
    let submitted = proposal.submit(fixed_at()).unwrap();
    assert_eq!(submitted.status, LearningProposalStatus::Submitted);
}

#[test]
fn l3_forgetting_property_keeps_raw_fact_identity_independent_of_projection_lifetime() {
    let workspace_id = WorkspaceId::new();
    let fact = evaluation_fact(
        workspace_id,
        EvaluationId::new(),
        EvaluationResult::Pass,
        "quality.relevance",
        0.88,
    );
    let raw_fact_id = fact.id;
    let projection = LearnedProjection::rebuild_from_facts(
        LearningProjectionId::new(),
        workspace_id,
        ProjectionKind::PerformanceSummary,
        LearningTarget::Model {
            model_id: ModelId::new(),
        },
        "cognition-bench-v1",
        1,
        std::slice::from_ref(&fact),
        fixed_at(),
    )
    .unwrap();

    drop(projection);
    assert_eq!(fact.id, raw_fact_id);
    assert_eq!(fact.workspace_id, workspace_id);
    assert!(!fact.evidence_refs.is_empty());
}
