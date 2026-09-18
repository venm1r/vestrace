use vestrace_domain::{id::*, now, planning::*};

#[test]
fn test_execution_plan_creation() {
    let plan_id = ExecutionPlanId::new();
    let run_id = AgentRunId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let plan = ExecutionPlan {
        id: plan_id,
        workspace_id: ws_id,
        run_id,
        title: "Test Execution Plan".into(),
        mode: PlanningMode::Direct,
        created_at: at,
    };

    let revision = ExecutionPlanRevision {
        id: ExecutionPlanRevisionId::new(),
        plan_id: plan.id,
        workspace_id: ws_id,
        revision_number: 1,
        content_hash: "sha256:fakehash".into(),
        steps: vec![],
        created_at: at,
    };

    assert_eq!(plan.mode, PlanningMode::Direct);
    assert_eq!(revision.revision_number, 1);
}
