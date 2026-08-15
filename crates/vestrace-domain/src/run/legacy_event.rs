use crate::{
    external_effects::{EvidenceStrength, ReconciliationOutcome},
    id::{
        AgentRunId, ApprovalRecordId, CorrelationId, ExternalEffectId, ExternalEffectReceiptId,
        OperationId, PrincipalId, RequestId, RunEventId, RunStepId, WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

use super::RunActor;
use super::RunVersion;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PendingRunEvent {
    pub event: LegacyRunEvent,
    pub occurred_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LegacyRunEventEnvelope {
    pub event_id: RunEventId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub sequence: RunVersion,
    pub event_type: String,
    pub event_version: u16,
    pub actor: RunActor,
    pub causation_id: OperationId,
    pub correlation_id: CorrelationId,
    pub payload: LegacyRunEvent,
    pub occurred_at: Timestamp,
    pub recorded_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LegacyRunEvent {
    Created {
        principal_id: PrincipalId,
        title: String,
    },
    Prepared,
    Started,
    StepStarted {
        step_id: RunStepId,
        kind: String,
        label: Option<String>,
    },
    StepSucceeded {
        step_id: RunStepId,
        output_references: Vec<String>,
    },
    StepFailed {
        step_id: RunStepId,
        code: String,
        message: String,
        retryable: bool,
    },
    WaitingForInput {
        request_id: RequestId,
        prompt: String,
    },
    WaitingForApproval {
        approval_id: ApprovalRecordId,
    },
    WaitingForDependency {
        dependency_run_id: AgentRunId,
    },
    /// An approval was granted for a run that was waiting for one.
    ///
    /// Distinct from [`Self::Resumed`] on purpose: an approval names who granted
    /// it, and collapsing the two would erase that from the canonical history.
    ApprovalGranted {
        approver_id: PrincipalId,
    },
    Resumed,
    Succeeded {
        summary: Option<String>,
    },
    SucceededWithWarnings {
        summary: String,
        warnings: Vec<String>,
    },
    PartialCompleted {
        summary: String,
        remaining_work: Vec<String>,
    },
    Failed {
        code: String,
        message: String,
        retryable: bool,
    },
    Cancelled {
        reason: Option<String>,
    },
    Paused,
    Expired,
    /// An external effect this run asked for turned out to have happened, or
    /// not.
    ///
    /// # Why this is in the run's history
    ///
    /// An effect that times out gets an `unknown` receipt, and the run carries
    /// on without knowing. Later — possibly after the run has finished — the
    /// reconciliation sweep asks the provider and finds out. Until this variant
    /// existed the answer was written to a table nobody joined against, and the
    /// run that asked for the effect was never told. `NotApplied` means the
    /// system dispatched something that did not happen, and that is a fact about
    /// the run.
    ///
    /// # Why it changes no state
    ///
    /// It is an observation, not a transition. A run that has already succeeded
    /// does not become failed because an effect it fired turned out not to land
    /// — whether it *should* is a policy question nobody has answered, and
    /// answering it by silently reopening finished runs would be the wrong way
    /// to ask. So this records what was learned and leaves the decision to a
    /// reader, which is the same shape as recording an unknown receipt in the
    /// first place.
    ///
    /// It is therefore valid in **every** status, including terminal ones. That
    /// is the point: the late answer is exactly the case this exists for.
    ExternalEffectSettled {
        effect_id: ExternalEffectId,
        receipt_id: ExternalEffectReceiptId,
        outcome: ReconciliationOutcome,
        evidence_strength: EvidenceStrength,
    },
}

impl LegacyRunEvent {
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Created { .. } => "run.created",
            Self::Prepared => "run.prepared",
            Self::Started => "run.started",
            Self::StepStarted { .. } => "step.started",
            Self::StepSucceeded { .. } => "step.succeeded",
            Self::StepFailed { .. } => "step.failed",
            Self::WaitingForInput { .. } => "run.waiting_for_input",
            Self::WaitingForApproval { .. } => "run.waiting_for_approval",
            Self::WaitingForDependency { .. } => "run.waiting_for_dependency",
            Self::ApprovalGranted { .. } => "run.approval_granted",
            Self::Resumed => "run.resumed",
            Self::Succeeded { .. } => "run.succeeded",
            Self::SucceededWithWarnings { .. } => "run.succeeded_with_warnings",
            Self::PartialCompleted { .. } => "run.partial_completed",
            Self::Failed { .. } => "run.failed",
            Self::Cancelled { .. } => "run.cancelled",
            Self::Paused => "run.paused",
            Self::Expired => "run.expired",
            Self::ExternalEffectSettled { .. } => "run.external_effect_settled",
        }
    }

    pub const fn event_version(&self) -> u16 {
        1
    }
}
