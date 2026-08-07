use crate::DomainError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Queued,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Cancelled,
}

impl ExecutionStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    pub fn can_transition_to(&self, next: Self) -> bool {
        use ExecutionStatus::*;
        matches!(
            (self, next),
            (Queued, Running)
                | (Running, Waiting)
                | (Waiting, Running)
                | (Running, Succeeded)
                | (Running, Failed)
                | (Running, Cancelled)
                | (Waiting, Cancelled)
                | (Queued, Cancelled)
        )
    }

    pub fn transition_to(&self, next: Self) -> Result<(), DomainError> {
        if self.can_transition_to(next) {
            Ok(())
        } else {
            Err(DomainError::InvalidArgument(format!(
                "invalid execution transition: {:?} -> {:?}",
                self, next
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_to_running_allowed() {
        assert!(ExecutionStatus::Queued.can_transition_to(ExecutionStatus::Running));
    }

    #[test]
    fn running_to_waiting_allowed() {
        assert!(ExecutionStatus::Running.can_transition_to(ExecutionStatus::Waiting));
    }

    #[test]
    fn waiting_to_running_allowed() {
        assert!(ExecutionStatus::Waiting.can_transition_to(ExecutionStatus::Running));
    }

    #[test]
    fn running_to_succeeded_allowed() {
        assert!(ExecutionStatus::Running.can_transition_to(ExecutionStatus::Succeeded));
    }

    #[test]
    fn running_to_failed_allowed() {
        assert!(ExecutionStatus::Running.can_transition_to(ExecutionStatus::Failed));
    }

    #[test]
    fn running_to_cancelled_allowed() {
        assert!(ExecutionStatus::Running.can_transition_to(ExecutionStatus::Cancelled));
    }

    #[test]
    fn waiting_to_cancelled_allowed() {
        assert!(ExecutionStatus::Waiting.can_transition_to(ExecutionStatus::Cancelled));
    }

    #[test]
    fn queued_to_cancelled_allowed() {
        assert!(ExecutionStatus::Queued.can_transition_to(ExecutionStatus::Cancelled));
    }

    #[test]
    fn succeeded_to_running_rejected() {
        assert!(!ExecutionStatus::Succeeded.can_transition_to(ExecutionStatus::Running));
    }

    #[test]
    fn failed_to_running_rejected() {
        assert!(!ExecutionStatus::Failed.can_transition_to(ExecutionStatus::Running));
    }

    #[test]
    fn cancelled_to_running_rejected() {
        assert!(!ExecutionStatus::Cancelled.can_transition_to(ExecutionStatus::Running));
    }

    #[test]
    fn queued_to_succeeded_rejected() {
        assert!(!ExecutionStatus::Queued.can_transition_to(ExecutionStatus::Succeeded));
    }

    #[test]
    fn terminal_check() {
        assert!(ExecutionStatus::Succeeded.is_terminal());
        assert!(ExecutionStatus::Failed.is_terminal());
        assert!(ExecutionStatus::Cancelled.is_terminal());
        assert!(!ExecutionStatus::Running.is_terminal());
        assert!(!ExecutionStatus::Waiting.is_terminal());
        assert!(!ExecutionStatus::Queued.is_terminal());
    }

    #[test]
    fn transition_to_returns_error_for_invalid() {
        let result = ExecutionStatus::Succeeded.transition_to(ExecutionStatus::Running);
        assert!(result.is_err());
    }

    #[test]
    fn transition_to_ok_for_valid() {
        let result = ExecutionStatus::Queued.transition_to(ExecutionStatus::Running);
        assert!(result.is_ok());
    }
}
