use vestrace_domain::run::{AgentRun, LegacyRunEventEnvelope};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateRunCommand {
    pub title: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunCommandResult {
    pub run: AgentRun,
    pub events: Vec<LegacyRunEventEnvelope>,
}
