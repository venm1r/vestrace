use chrono::{TimeZone, Utc};
use serde_json::json;
use vestrace_domain::{
    EvaluationAuthority, EvaluationFact, EvaluationMetric, EvaluationResult, EvaluationTarget,
    EvaluatorKind, EvaluatorRef, EvidenceRef,
    id::{EvaluationId, EventId, ModelExecutionAttemptId, ModelExecutionId, ModelId, WorkspaceId},
};
use vestrace_http::api::evaluations::CreateEvaluationFactRequest;

fn fixed_at() -> vestrace_domain::Timestamp {
    Utc.with_ymd_and_hms(2026, 8, 11, 1, 0, 0).single().unwrap()
}

fn valid_fact() -> EvaluationFact {
    let workspace_id = WorkspaceId::new();
    let execution_id = ModelExecutionId::new();
    let attempt_id = ModelExecutionAttemptId::new();

    EvaluationFact::new(
        EvaluationId::new(),
        workspace_id,
        EvaluationTarget::ModelExecution {
            execution_id,
            attempt_id: Some(attempt_id),
            model_id: ModelId::new(),
            model_revision: "model-v7".to_owned(),
        },
        EvaluatorRef {
            kind: EvaluatorKind::ModelJudge,
            model_id: Some(ModelId::new()),
            revision: Some("judge-model-v3".to_owned()),
            principal_id: None,
        },
        EvaluationMetric::new("quality.relevance", 0.92, Some("score".to_owned())).unwrap(),
        EvaluationResult::Pass,
        vec![
            EvidenceRef::ModelExecutionRef { attempt_id },
            EvidenceRef::event(EventId::new()),
        ],
        EvaluationAuthority::Advisory,
        "policy-evaluation-v2",
        fixed_at(),
    )
    .unwrap()
}

#[test]
fn l1_evaluation_fact_v2_binds_exact_target_evaluator_metric_and_evidence() {
    let fact = valid_fact();

    assert!(matches!(
        fact.target,
        EvaluationTarget::ModelExecution {
            attempt_id: Some(_),
            ..
        }
    ));
    assert_eq!(fact.evaluator.kind, EvaluatorKind::ModelJudge);
    assert_eq!(fact.evaluator.revision.as_deref(), Some("judge-model-v3"));
    assert_eq!(fact.metric.kind, "quality.relevance");
    assert_eq!(fact.metric.value, 0.92);
    assert_eq!(fact.result, EvaluationResult::Pass);
    assert_eq!(fact.policy_version, "policy-evaluation-v2");
    assert_eq!(fact.evidence_refs.len(), 2);
}

#[test]
fn l1_evaluation_fact_serialization_keeps_typed_refs_and_does_not_become_a_learning_projection() {
    let fact = valid_fact();
    let encoded = serde_json::to_value(&fact).unwrap();

    assert_eq!(encoded["evaluator"]["kind"], "model_judge");
    assert_eq!(encoded["metric"]["kind"], "quality.relevance");
    assert_eq!(encoded["authority"], "advisory");
    assert_eq!(encoded["policy_version"], "policy-evaluation-v2");
    assert!(encoded["evidence_refs"].as_array().unwrap().len() >= 2);
    assert!(encoded.get("learned_projection").is_none());
}

#[test]
fn l1_evaluation_fact_validation_rejects_missing_evidence_bad_metrics_and_incomplete_model_judge() {
    let fact = valid_fact();
    let no_evidence = EvaluationFact::new(
        fact.id,
        fact.workspace_id,
        fact.target.clone(),
        fact.evaluator.clone(),
        fact.metric.clone(),
        fact.result,
        Vec::new(),
        fact.authority,
        fact.policy_version.clone(),
        fact.created_at,
    );
    assert!(no_evidence.is_err());

    assert!(EvaluationMetric::new("quality", f64::NAN, None).is_err());
    assert!(EvaluationMetric::new(" ", 0.5, None).is_err());

    let incomplete_evaluator = EvaluatorRef {
        kind: EvaluatorKind::ModelJudge,
        model_id: Some(ModelId::new()),
        revision: None,
        principal_id: None,
    };
    assert!(
        EvaluationFact::new(
            EvaluationId::new(),
            WorkspaceId::new(),
            EvaluationTarget::ToolInvocation {
                invocation_id: vestrace_domain::id::ToolInvocationId::new(),
                tool_definition_id: vestrace_domain::id::ToolDefinitionId::new(),
                tool_revision: "tool-v1".to_owned(),
            },
            incomplete_evaluator,
            EvaluationMetric::new("quality", 0.5, None).unwrap(),
            EvaluationResult::Inconclusive,
            vec![EvidenceRef::ExternalReference {
                uri: "fixture://evaluation".to_owned(),
                label: Some("fixture".to_owned()),
                integrity_hash: None,
            }],
            EvaluationAuthority::Advisory,
            "policy-v1",
            fixed_at(),
        )
        .is_err()
    );
}

#[test]
fn l1_evaluation_fact_http_shape_can_round_trip_as_a_create_payload() {
    let fact = valid_fact();
    let payload = json!({
        "target": fact.target,
        "evaluator": fact.evaluator,
        "metric": fact.metric,
        "result": fact.result,
        "evidence_refs": fact.evidence_refs,
        "authority": fact.authority,
        "policy_version": fact.policy_version,
    });

    assert_eq!(payload["result"], "pass");
    assert_eq!(payload["authority"], "advisory");
    assert!(payload["evidence_refs"].is_array());
    assert_eq!(payload["target"]["kind"], "model_execution");
    assert!(payload["target"]["attempt_id"].is_string());
    let request: CreateEvaluationFactRequest = serde_json::from_value(payload).unwrap();
    assert_eq!(request.policy_version, "policy-evaluation-v2");
    assert_eq!(request.evidence_refs.len(), 2);
}
