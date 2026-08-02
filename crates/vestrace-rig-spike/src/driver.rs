use crate::contracts::{SpikeLoopCommand, SpikeLoopEffect, SpikeModelLoopPort};

pub struct RigSpikeDriver {
    current_turn: u32,
    max_turns: u32,
}

impl RigSpikeDriver {
    pub fn new() -> Self {
        Self {
            current_turn: 0,
            max_turns: 5,
        }
    }
}

impl SpikeModelLoopPort for RigSpikeDriver {
    fn apply(&mut self, command: SpikeLoopCommand) -> Result<SpikeLoopEffect, String> {
        match command {
            SpikeLoopCommand::Start(start) => {
                self.max_turns = start.maximum_model_turns;
                self.current_turn = 1;
                Ok(SpikeLoopEffect::InvokeModel(format!(
                    "Model invocation turn 1 for prompt: {}",
                    start.prompt
                )))
            }
            SpikeLoopCommand::SupplyModelResult(res) => {
                if self.current_turn >= self.max_turns {
                    Ok(SpikeLoopEffect::Completed(res))
                } else {
                    self.current_turn += 1;
                    Ok(SpikeLoopEffect::Completed(format!("Final answer: {}", res)))
                }
            }
        }
    }
}
