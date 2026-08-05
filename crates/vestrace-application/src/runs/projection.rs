use vestrace_domain::run::{AgentRun, RunState};

/// Derives the query projection from the canonical event-reduced run state.
pub fn project_run(state: &RunState) -> AgentRun {
    AgentRun {
        id: state.id,
        workspace_id: state.workspace_id,
        principal_id: state.principal_id,
        title: state.title.clone(),
        status: state.status,
        version: state.version,
        created_at: state.created_at,
        updated_at: state.updated_at,
    }
}
