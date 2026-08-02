use vestrace_domain::id::{AgentRunId, RunStepId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpikeLoopStart {
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub prompt: String,
    pub maximum_model_turns: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub enum SpikeLoopCommand {
    Start(SpikeLoopStart),
    SupplyModelResult(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub enum SpikeLoopEffect {
    InvokeModel(String),
    Completed(String),
    Failed(String),
}

pub trait SpikeModelLoopPort {
    fn apply(&mut self, command: SpikeLoopCommand) -> Result<SpikeLoopEffect, String>;
}
