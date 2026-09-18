use vestrace_domain::{id::*, now, run::*};

#[test]
fn test_agent_run_version_increments() {
    let version = RunVersion::INITIAL;
    assert_eq!(version.value(), 1);

    let next_version = version.next().unwrap();
    assert_eq!(next_version.value(), 2);
}

#[test]
fn test_agent_run_initialization() {
    let run_id = AgentRunId::new();
    let ws_id = WorkspaceId::new();
    let at = now();

    let run = AgentRun::create(
        NewAgentRun {
            id: run_id,
            workspace_id: ws_id,
            objective: "Test Agent Run".into(),
            coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
            execution_mode: RunExecutionMode::Autopilot,
            parent: None,
            budget_snapshot_id: None,
            resource_usage_snapshot_id: None,
        },
        at,
    )
    .unwrap();
    assert_eq!(run.status, RunStatus::Created);
    assert_eq!(run.version, RunVersion::INITIAL);
}
