use serde_json::json;
use vestrace_domain::{
    AgentRevisionId, EvaluationId, EvidenceRef, LearnedProjection, LearningChange,
    LearningProjectionId, LearningProposal, LearningProposalStatus, LearningTarget, PrincipalId,
    ProjectionAuthority, ProjectionGenerator, ProjectionKind, WorkflowRevisionId, WorkspaceId, now,
};

#[test]
fn learned_projection_is_advisory_and_keeps_raw_fact_provenance() {
    let workspace_id = WorkspaceId::new();
    let fact_id = EvaluationId::new();
    let projection = LearnedProjection::new(
        LearningProjectionId::new(),
        workspace_id,
        ProjectionKind::PerformanceSummary,
        LearningTarget::AgentRevision {
            revision_id: AgentRevisionId::new(),
        },
        ProjectionGenerator::Deterministic {
            algorithm_version: "bench-v1".to_owned(),
        },
        1,
        vec![fact_id],
        vec![EvidenceRef::EvaluationRef {
            evaluation_id: fact_id,
        }],
        json!({"quality": 0.91}),
        now(),
    )
    .unwrap();

    assert_eq!(projection.authority, ProjectionAuthority::Advisory);
    assert_eq!(projection.source_evaluation_fact_ids, vec![fact_id]);
    assert!(
        projection
            .source_evidence_refs
            .contains(&EvidenceRef::EvaluationRef {
                evaluation_id: fact_id
            })
    );
}

#[test]
fn proposal_is_versioned_and_submission_never_applies_a_change() {
    let proposal = LearningProposal::new(
        vestrace_domain::LearningProposalId::new(),
        WorkspaceId::new(),
        vec![LearningProjectionId::new()],
        vec![EvaluationId::new()],
        LearningTarget::WorkflowRevision {
            revision_id: WorkflowRevisionId::new(),
        },
        LearningChange::WorkflowDefinition {
            definition: json!({"steps": ["review"]}),
        },
        3,
        "repeated deterministic failures suggest a safer workflow ordering",
        "policy-learning-v1",
        PrincipalId::new(),
        now(),
    )
    .unwrap();

    assert_eq!(proposal.status, LearningProposalStatus::Draft);
    let submitted = proposal.submit(now()).unwrap();
    assert_eq!(submitted.status, LearningProposalStatus::Submitted);
    assert_eq!(submitted.expected_target_revision, 3);
    assert!(
        !serde_json::to_string(&submitted)
            .unwrap()
            .contains("applied")
    );
}

#[test]
fn proposal_rejects_empty_provenance_and_invalid_revision() {
    let result = LearningProposal::new(
        vestrace_domain::LearningProposalId::new(),
        WorkspaceId::new(),
        Vec::new(),
        Vec::new(),
        LearningTarget::WorkflowRevision {
            revision_id: WorkflowRevisionId::new(),
        },
        LearningChange::WorkflowDefinition {
            definition: json!({"steps": []}),
        },
        0,
        "missing provenance",
        "policy-learning-v1",
        PrincipalId::new(),
        now(),
    );

    assert!(result.is_err());
}

#[test]
fn learning_target_has_no_capability_or_permission_variant() {
    let target = LearningTarget::AgentRevision {
        revision_id: AgentRevisionId::new(),
    };
    let encoded = serde_json::to_value(target).unwrap();
    assert_eq!(encoded["kind"], "agent_revision");
    assert!(encoded.get("capability").is_none());
    assert!(encoded.get("permission").is_none());
}

#[test]
fn proposal_change_rejects_capability_payloads_even_inside_asset_configuration() {
    let result = LearningProposal::new(
        vestrace_domain::LearningProposalId::new(),
        WorkspaceId::new(),
        vec![LearningProjectionId::new()],
        vec![EvaluationId::new()],
        LearningTarget::SkillRevision {
            revision_id: vestrace_domain::SkillRevisionId::new(),
        },
        LearningChange::SkillImplementation {
            implementation: json!({"nested": {"allow_capability_grants": ["admin"]}}),
        },
        1,
        "must remain a proposal",
        "policy-learning-v1",
        PrincipalId::new(),
        now(),
    );

    assert!(result.is_err());
}

#[test]
fn projection_rejects_evidence_fact_ids_not_declared_as_sources() {
    let result = LearnedProjection::new(
        LearningProjectionId::new(),
        WorkspaceId::new(),
        ProjectionKind::PerformanceSummary,
        LearningTarget::AgentRevision {
            revision_id: AgentRevisionId::new(),
        },
        ProjectionGenerator::Deterministic {
            algorithm_version: "bench-v1".to_owned(),
        },
        1,
        vec![EvaluationId::new()],
        vec![EvidenceRef::EvaluationRef {
            evaluation_id: EvaluationId::new(),
        }],
        json!({"quality": 0.9}),
        now(),
    );

    assert!(result.is_err());
}
