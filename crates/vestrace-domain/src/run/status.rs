use crate::DomainError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunExecutionMode {
    Autopilot,
    Supervised,
    Manual,
}

impl RunExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Autopilot => "autopilot",
            Self::Supervised => "supervised",
            Self::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "autopilot" => Some(Self::Autopilot),
            "supervised" => Some(Self::Supervised),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Created,
    Preparing,
    Running,
    WaitingForInput,
    WaitingForApproval,
    WaitingForDependency,
    Paused,
    PausedPolicyChanged,
    Succeeded,
    SucceededWithWarnings,
    Partial,
    Failed,
    Cancelled,
    Expired,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Preparing => "preparing",
            Self::Running => "running",
            Self::WaitingForInput => "waiting_for_input",
            Self::WaitingForApproval => "waiting_for_approval",
            Self::WaitingForDependency => "waiting_for_dependency",
            Self::Paused => "paused",
            Self::PausedPolicyChanged => "paused_policy_changed",
            Self::Succeeded => "succeeded",
            Self::SucceededWithWarnings => "succeeded_with_warnings",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "created" => Some(Self::Created),
            "preparing" => Some(Self::Preparing),
            "running" => Some(Self::Running),
            "waiting_for_input" => Some(Self::WaitingForInput),
            "waiting_for_approval" => Some(Self::WaitingForApproval),
            "waiting_for_dependency" => Some(Self::WaitingForDependency),
            "paused" => Some(Self::Paused),
            "paused_policy_changed" => Some(Self::PausedPolicyChanged),
            "succeeded" => Some(Self::Succeeded),
            "succeeded_with_warnings" => Some(Self::SucceededWithWarnings),
            "partial" => Some(Self::Partial),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::SucceededWithWarnings
                | Self::Partial
                | Self::Failed
                | Self::Cancelled
                | Self::Expired
        )
    }

    pub fn is_active(self) -> bool {
        !self.is_terminal()
    }

    pub fn can_pause(self) -> bool {
        matches!(
            self,
            Self::Running
                | Self::WaitingForInput
                | Self::WaitingForApproval
                | Self::WaitingForDependency
        )
    }

    pub fn can_resume(self) -> bool {
        matches!(self, Self::Paused | Self::PausedPolicyChanged)
    }

    pub fn is_waiting(self) -> bool {
        matches!(
            self,
            Self::WaitingForInput | Self::WaitingForApproval | Self::WaitingForDependency
        )
    }

    pub fn transition_to(self, next: Self) -> Result<(), DomainError> {
        if self == next {
            return Ok(());
        }
        if self.is_terminal() {
            return Err(DomainError::InvalidArgument(format!(
                "cannot transition from terminal run status {self:?}"
            )));
        }
        let allowed: &[Self] = match self {
            Self::Created => &[Self::Preparing, Self::Cancelled],
            Self::Preparing => &[
                Self::Running,
                Self::WaitingForInput,
                Self::Failed,
                Self::Cancelled,
            ],
            Self::Running => &[
                Self::WaitingForInput,
                Self::WaitingForApproval,
                Self::WaitingForDependency,
                Self::Paused,
                Self::PausedPolicyChanged,
                Self::Succeeded,
                Self::SucceededWithWarnings,
                Self::Partial,
                Self::Failed,
                Self::Cancelled,
            ],
            Self::WaitingForInput => &[Self::Running, Self::Paused, Self::Cancelled, Self::Expired],
            Self::WaitingForApproval => {
                &[Self::Running, Self::Paused, Self::Cancelled, Self::Expired]
            }
            Self::WaitingForDependency => &[
                Self::Running,
                Self::Paused,
                Self::Failed,
                Self::Cancelled,
                Self::Expired,
            ],
            Self::Paused => &[Self::Running, Self::Cancelled, Self::Expired],
            Self::PausedPolicyChanged => &[Self::Running, Self::Cancelled, Self::Expired],
            Self::Succeeded
            | Self::SucceededWithWarnings
            | Self::Partial
            | Self::Failed
            | Self::Cancelled
            | Self::Expired => {
                return Err(DomainError::InvalidArgument(format!(
                    "cannot transition from terminal run status {self:?}"
                )));
            }
        };
        if allowed.contains(&next) {
            Ok(())
        } else {
            Err(DomainError::InvalidArgument(format!(
                "invalid run transition: {self:?} -> {next:?}"
            )))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStepStatus {
    Pending,
    Ready,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
    Unknown,
}

impl RunStepStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "ready" => Some(Self::Ready),
            "running" => Some(Self::Running),
            "waiting" => Some(Self::Waiting),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "skipped" => Some(Self::Skipped),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Skipped
        )
    }

    pub fn is_active(self) -> bool {
        !self.is_terminal() && self != Self::Unknown
    }

    pub fn transition_to(self, next: Self) -> Result<(), DomainError> {
        if self == next {
            return Ok(());
        }
        let allowed: &[Self] = match self {
            Self::Pending => &[Self::Ready, Self::Skipped, Self::Cancelled],
            Self::Ready => &[Self::Running, Self::Skipped, Self::Cancelled],
            Self::Running => &[
                Self::Waiting,
                Self::Succeeded,
                Self::Failed,
                Self::Cancelled,
                Self::Unknown,
            ],
            Self::Waiting => &[
                Self::Ready,
                Self::Running,
                Self::Failed,
                Self::Cancelled,
                Self::Unknown,
            ],
            Self::Unknown => &[
                Self::Waiting,
                Self::Succeeded,
                Self::Failed,
                Self::Cancelled,
            ],
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Skipped => {
                return Err(DomainError::InvalidArgument(format!(
                    "cannot transition from terminal step status {self:?}"
                )));
            }
        };
        if allowed.contains(&next) {
            Ok(())
        } else {
            Err(DomainError::InvalidArgument(format!(
                "invalid step transition: {self:?} -> {next:?}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_statuses_are_terminal() {
        assert!(RunStatus::Succeeded.is_terminal());
        assert!(RunStatus::SucceededWithWarnings.is_terminal());
        assert!(RunStatus::Partial.is_terminal());
        assert!(RunStatus::Failed.is_terminal());
        assert!(RunStatus::Cancelled.is_terminal());
        assert!(RunStatus::Expired.is_terminal());
    }

    #[test]
    fn non_terminal_statuses_are_active() {
        assert!(RunStatus::Created.is_active());
        assert!(RunStatus::Preparing.is_active());
        assert!(RunStatus::Running.is_active());
        assert!(RunStatus::WaitingForInput.is_active());
        assert!(RunStatus::WaitingForApproval.is_active());
        assert!(RunStatus::WaitingForDependency.is_active());
        assert!(RunStatus::Paused.is_active());
        assert!(RunStatus::PausedPolicyChanged.is_active());
    }

    #[test]
    fn created_can_prepare_or_cancel() {
        assert!(
            RunStatus::Created
                .transition_to(RunStatus::Preparing)
                .is_ok()
        );
        assert!(
            RunStatus::Created
                .transition_to(RunStatus::Cancelled)
                .is_ok()
        );
    }

    #[test]
    fn created_cannot_run() {
        assert!(
            RunStatus::Created
                .transition_to(RunStatus::Running)
                .is_err()
        );
    }

    #[test]
    fn preparing_can_run_wait_for_input_fail_or_cancel() {
        assert!(
            RunStatus::Preparing
                .transition_to(RunStatus::Running)
                .is_ok()
        );
        assert!(
            RunStatus::Preparing
                .transition_to(RunStatus::WaitingForInput)
                .is_ok()
        );
        assert!(
            RunStatus::Preparing
                .transition_to(RunStatus::Failed)
                .is_ok()
        );
        assert!(
            RunStatus::Preparing
                .transition_to(RunStatus::Cancelled)
                .is_ok()
        );
    }

    #[test]
    fn running_can_transition_to_all_specified() {
        for next in [
            RunStatus::WaitingForInput,
            RunStatus::WaitingForApproval,
            RunStatus::WaitingForDependency,
            RunStatus::Paused,
            RunStatus::PausedPolicyChanged,
            RunStatus::Succeeded,
            RunStatus::SucceededWithWarnings,
            RunStatus::Partial,
            RunStatus::Failed,
            RunStatus::Cancelled,
        ] {
            assert!(
                RunStatus::Running.transition_to(next).is_ok(),
                "Running -> {next:?} should be allowed"
            );
        }
    }

    #[test]
    fn running_cannot_prepare() {
        assert!(
            RunStatus::Running
                .transition_to(RunStatus::Preparing)
                .is_err()
        );
    }

    #[test]
    fn waiting_for_input_can_resume_pause_cancel_expire() {
        assert!(
            RunStatus::WaitingForInput
                .transition_to(RunStatus::Running)
                .is_ok()
        );
        assert!(
            RunStatus::WaitingForInput
                .transition_to(RunStatus::Paused)
                .is_ok()
        );
        assert!(
            RunStatus::WaitingForInput
                .transition_to(RunStatus::Cancelled)
                .is_ok()
        );
        assert!(
            RunStatus::WaitingForInput
                .transition_to(RunStatus::Expired)
                .is_ok()
        );
    }

    #[test]
    fn waiting_for_approval_can_resume_pause_cancel_expire() {
        assert!(
            RunStatus::WaitingForApproval
                .transition_to(RunStatus::Running)
                .is_ok()
        );
        assert!(
            RunStatus::WaitingForApproval
                .transition_to(RunStatus::Paused)
                .is_ok()
        );
        assert!(
            RunStatus::WaitingForApproval
                .transition_to(RunStatus::Cancelled)
                .is_ok()
        );
        assert!(
            RunStatus::WaitingForApproval
                .transition_to(RunStatus::Expired)
                .is_ok()
        );
    }

    #[test]
    fn waiting_for_dependency_can_resume_pause_fail_cancel_expire() {
        assert!(
            RunStatus::WaitingForDependency
                .transition_to(RunStatus::Running)
                .is_ok()
        );
        assert!(
            RunStatus::WaitingForDependency
                .transition_to(RunStatus::Paused)
                .is_ok()
        );
        assert!(
            RunStatus::WaitingForDependency
                .transition_to(RunStatus::Failed)
                .is_ok()
        );
        assert!(
            RunStatus::WaitingForDependency
                .transition_to(RunStatus::Cancelled)
                .is_ok()
        );
        assert!(
            RunStatus::WaitingForDependency
                .transition_to(RunStatus::Expired)
                .is_ok()
        );
    }

    #[test]
    fn paused_can_run_cancel_expire() {
        assert!(RunStatus::Paused.transition_to(RunStatus::Running).is_ok());
        assert!(
            RunStatus::Paused
                .transition_to(RunStatus::Cancelled)
                .is_ok()
        );
        assert!(RunStatus::Paused.transition_to(RunStatus::Expired).is_ok());
    }

    #[test]
    fn paused_policy_changed_can_run_cancel_expire() {
        assert!(
            RunStatus::PausedPolicyChanged
                .transition_to(RunStatus::Running)
                .is_ok()
        );
        assert!(
            RunStatus::PausedPolicyChanged
                .transition_to(RunStatus::Cancelled)
                .is_ok()
        );
        assert!(
            RunStatus::PausedPolicyChanged
                .transition_to(RunStatus::Expired)
                .is_ok()
        );
    }

    #[test]
    fn paused_cannot_prepare() {
        assert!(
            RunStatus::Paused
                .transition_to(RunStatus::Preparing)
                .is_err()
        );
    }

    #[test]
    fn terminal_cannot_transition() {
        for terminal in [
            RunStatus::Succeeded,
            RunStatus::SucceededWithWarnings,
            RunStatus::Partial,
            RunStatus::Failed,
            RunStatus::Cancelled,
            RunStatus::Expired,
        ] {
            assert!(terminal.transition_to(RunStatus::Running).is_err());
        }
    }

    #[test]
    fn same_status_is_noop() {
        assert!(RunStatus::Running.transition_to(RunStatus::Running).is_ok());
        assert!(RunStatus::Paused.transition_to(RunStatus::Paused).is_ok());
    }

    #[test]
    fn can_pause_only_from_active() {
        assert!(RunStatus::Running.can_pause());
        assert!(RunStatus::WaitingForInput.can_pause());
        assert!(RunStatus::WaitingForApproval.can_pause());
        assert!(RunStatus::WaitingForDependency.can_pause());
        assert!(!RunStatus::Created.can_pause());
        assert!(!RunStatus::Preparing.can_pause());
        assert!(!RunStatus::Paused.can_pause());
        assert!(!RunStatus::Succeeded.can_pause());
    }

    #[test]
    fn can_resume_only_from_paused() {
        assert!(RunStatus::Paused.can_resume());
        assert!(RunStatus::PausedPolicyChanged.can_resume());
        assert!(!RunStatus::Running.can_resume());
        assert!(!RunStatus::WaitingForInput.can_resume());
    }

    #[test]
    fn is_waiting_checks() {
        assert!(RunStatus::WaitingForInput.is_waiting());
        assert!(RunStatus::WaitingForApproval.is_waiting());
        assert!(RunStatus::WaitingForDependency.is_waiting());
        assert!(!RunStatus::Running.is_waiting());
        assert!(!RunStatus::Paused.is_waiting());
    }

    // Step status tests

    #[test]
    fn step_pending_can_ready_skip_or_cancel() {
        assert!(
            RunStepStatus::Pending
                .transition_to(RunStepStatus::Ready)
                .is_ok()
        );
        assert!(
            RunStepStatus::Pending
                .transition_to(RunStepStatus::Skipped)
                .is_ok()
        );
        assert!(
            RunStepStatus::Pending
                .transition_to(RunStepStatus::Cancelled)
                .is_ok()
        );
    }

    #[test]
    fn step_ready_can_run_skip_or_cancel() {
        assert!(
            RunStepStatus::Ready
                .transition_to(RunStepStatus::Running)
                .is_ok()
        );
        assert!(
            RunStepStatus::Ready
                .transition_to(RunStepStatus::Skipped)
                .is_ok()
        );
        assert!(
            RunStepStatus::Ready
                .transition_to(RunStepStatus::Cancelled)
                .is_ok()
        );
    }

    #[test]
    fn step_running_can_wait_succeed_fail_cancel_unknown() {
        for next in [
            RunStepStatus::Waiting,
            RunStepStatus::Succeeded,
            RunStepStatus::Failed,
            RunStepStatus::Cancelled,
            RunStepStatus::Unknown,
        ] {
            assert!(
                RunStepStatus::Running.transition_to(next).is_ok(),
                "Running -> {next:?} should be allowed"
            );
        }
    }

    #[test]
    fn step_waiting_can_ready_run_fail_cancel_unknown() {
        for next in [
            RunStepStatus::Ready,
            RunStepStatus::Running,
            RunStepStatus::Failed,
            RunStepStatus::Cancelled,
            RunStepStatus::Unknown,
        ] {
            assert!(
                RunStepStatus::Waiting.transition_to(next).is_ok(),
                "Waiting -> {next:?} should be allowed"
            );
        }
    }

    #[test]
    fn step_unknown_can_wait_succeed_fail_cancel() {
        for next in [
            RunStepStatus::Waiting,
            RunStepStatus::Succeeded,
            RunStepStatus::Failed,
            RunStepStatus::Cancelled,
        ] {
            assert!(
                RunStepStatus::Unknown.transition_to(next).is_ok(),
                "Unknown -> {next:?} should be allowed"
            );
        }
    }

    #[test]
    fn step_unknown_cannot_pending_or_ready_or_running() {
        assert!(
            RunStepStatus::Unknown
                .transition_to(RunStepStatus::Pending)
                .is_err()
        );
        assert!(
            RunStepStatus::Unknown
                .transition_to(RunStepStatus::Ready)
                .is_err()
        );
        assert!(
            RunStepStatus::Unknown
                .transition_to(RunStepStatus::Running)
                .is_err()
        );
    }

    #[test]
    fn step_terminal_cannot_transition() {
        for terminal in [
            RunStepStatus::Succeeded,
            RunStepStatus::Failed,
            RunStepStatus::Cancelled,
            RunStepStatus::Skipped,
        ] {
            assert!(terminal.transition_to(RunStepStatus::Running).is_err());
        }
    }

    #[test]
    fn step_same_status_is_noop() {
        assert!(
            RunStepStatus::Running
                .transition_to(RunStepStatus::Running)
                .is_ok()
        );
        assert!(
            RunStepStatus::Pending
                .transition_to(RunStepStatus::Pending)
                .is_ok()
        );
    }

    #[test]
    fn step_is_terminal() {
        assert!(RunStepStatus::Succeeded.is_terminal());
        assert!(RunStepStatus::Failed.is_terminal());
        assert!(RunStepStatus::Cancelled.is_terminal());
        assert!(RunStepStatus::Skipped.is_terminal());
        assert!(!RunStepStatus::Pending.is_terminal());
        assert!(!RunStepStatus::Running.is_terminal());
    }

    #[test]
    fn step_is_active() {
        assert!(RunStepStatus::Pending.is_active());
        assert!(RunStepStatus::Ready.is_active());
        assert!(RunStepStatus::Running.is_active());
        assert!(RunStepStatus::Waiting.is_active());
        assert!(!RunStepStatus::Unknown.is_active());
        assert!(!RunStepStatus::Succeeded.is_active());
    }
}
