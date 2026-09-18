use vestrace_domain::id::AgentRuntimeSnapshotId;
use vestrace_domain::run::{AgentRun, RunExecutionMode, RunState, RunVersion};

/// Derives the query projection from the canonical event-reduced run state.
pub fn project_run(state: &RunState) -> AgentRun {
    AgentRun {
        id: state.id,
        workspace_id: state.workspace_id,
        objective: state.title.clone(),
        coordinator_snapshot_id: AgentRuntimeSnapshotId::from_uuid(state.principal_id.as_uuid()),
        active_plan_revision_id: None,
        execution_mode: RunExecutionMode::Autopilot,
        status: state.status,
        current_step_id: state.active_step.as_ref().map(|s| s.id),
        checkpoint_id: None,
        parent: None,
        root_run_id: state.id,
        budget_snapshot_id: None,
        resource_usage_snapshot_id: None,
        version: state.version,
        result: None,
        created_at: state.created_at,
        updated_at: state.updated_at,
        finished_at: state.finished_at,
    }
}

pub fn project_version(state: &RunState) -> RunVersion {
    state.version
}
