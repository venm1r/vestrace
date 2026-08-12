use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_application::{
    RunCheckpoint, RunRecoveryOperations, StartupRecoveryCandidate, StartupRecoveryOutcome,
    StartupRecoveryService,
};
use vestrace_domain::run::{AgentRun, NewAgentRun, RunExecutionMode, RunState, RunVersion};
use vestrace_domain::{
    AgentRunId, AgentRuntimeSnapshotId, RecoveryClassification, RecoveryTarget, WorkspaceId, now,
};

#[derive(Default)]
struct MockRecoveryOperations {
    rebuilt: Mutex<Vec<AgentRunId>>,
    rebuild_error: Mutex<Option<String>>,
}

#[async_trait]
impl RunRecoveryOperations for MockRecoveryOperations {
    async fn create_checkpoint(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
    ) -> Result<RunCheckpoint, ApplicationError> {
        Err(ApplicationError::Internal("unused".into()))
    }

    async fn validate_checkpoint(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
        _sequence: RunVersion,
    ) -> Result<RunCheckpoint, ApplicationError> {
        Err(ApplicationError::Internal("unused".into()))
    }

    async fn restore(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
    ) -> Result<RunState, ApplicationError> {
        Err(ApplicationError::Internal("unused".into()))
    }

    async fn rebuild_projection(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<AgentRun, ApplicationError> {
        self.rebuilt.lock().unwrap().push(run_id);
        if let Some(message) = self.rebuild_error.lock().unwrap().clone() {
            return Err(ApplicationError::Internal(message));
        }
        AgentRun::create(
            NewAgentRun {
                id: run_id,
                workspace_id: context.workspace_id,
                objective: "startup recovery".into(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                execution_mode: RunExecutionMode::Manual,
                parent: None,
                budget_snapshot_id: None,
                resource_usage_snapshot_id: None,
            },
            now(),
        )
        .map_err(ApplicationError::from)
    }
}

#[tokio::test]
async fn startup_recovery_rebuilds_only_safe_targets_and_reports_barriers() {
    let operations = Arc::new(MockRecoveryOperations::default());
    let service = StartupRecoveryService::new(operations.clone());
    let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());
    let resumable = AgentRunId::new();
    let unknown = AgentRunId::new();
    let human = AgentRunId::new();

    let report = service
        .run(
            &context,
            vec![
                StartupRecoveryCandidate::new(resumable, RecoveryTarget::RunningExecution),
                StartupRecoveryCandidate::new(unknown, RecoveryTarget::UnknownOutcome),
                StartupRecoveryCandidate::new(human, RecoveryTarget::DivergentHistory),
            ],
        )
        .await
        .unwrap();

    assert_eq!(operations.rebuilt.lock().unwrap().as_slice(), &[resumable]);
    assert_eq!(report.records().len(), 3);
    assert_eq!(
        report.records()[0].classification,
        RecoveryClassification::SafeToResume
    );
    assert_eq!(
        report.records()[0].outcome,
        StartupRecoveryOutcome::Restored
    );
    assert_eq!(
        report.records()[1].outcome,
        StartupRecoveryOutcome::ReconciliationRequired
    );
    assert_eq!(
        report.records()[2].outcome,
        StartupRecoveryOutcome::HumanReviewRequired
    );
}

#[tokio::test]
async fn startup_recovery_marks_safe_retry_as_retry_ready() {
    let operations = Arc::new(MockRecoveryOperations::default());
    let service = StartupRecoveryService::new(operations.clone());
    let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());
    let run_id = AgentRunId::new();

    let report = service
        .run(
            &context,
            vec![StartupRecoveryCandidate::new(
                run_id,
                RecoveryTarget::StaleLease,
            )],
        )
        .await
        .unwrap();

    assert_eq!(operations.rebuilt.lock().unwrap().as_slice(), &[run_id]);
    assert_eq!(
        report.records()[0].outcome,
        StartupRecoveryOutcome::RetryReady
    );
}

#[tokio::test]
async fn startup_recovery_rejects_duplicate_candidates_before_mutation() {
    let operations = Arc::new(MockRecoveryOperations::default());
    let service = StartupRecoveryService::new(operations.clone());
    let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());
    let run_id = AgentRunId::new();

    let error = service
        .run(
            &context,
            vec![
                StartupRecoveryCandidate::new(run_id, RecoveryTarget::RunningExecution),
                StartupRecoveryCandidate::new(run_id, RecoveryTarget::StaleLease),
            ],
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Domain(vestrace_domain::DomainError::InvalidArgument(message))
            if message.contains("duplicate startup recovery candidate")
    ));
    assert!(operations.rebuilt.lock().unwrap().is_empty());
}

#[tokio::test]
async fn startup_recovery_fails_closed_when_projection_rebuild_fails() {
    let operations = Arc::new(MockRecoveryOperations::default());
    *operations.rebuild_error.lock().unwrap() = Some("projection write failed".into());
    let service = StartupRecoveryService::new(operations.clone());
    let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());

    let error = service
        .run(
            &context,
            vec![StartupRecoveryCandidate::new(
                AgentRunId::new(),
                RecoveryTarget::RunningExecution,
            )],
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Internal(message) if message == "projection write failed"
    ));
}
