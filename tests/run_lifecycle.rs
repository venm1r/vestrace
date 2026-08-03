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
    let principal_id = PrincipalId::new();
    let at = now();

    let run = AgentRun::new(run_id, ws_id, principal_id, "Test Agent Run", at);
    assert_eq!(run.status, RunStatus::Created);
    assert_eq!(run.version, RunVersion::INITIAL);
}
