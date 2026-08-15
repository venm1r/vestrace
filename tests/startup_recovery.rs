use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_application::{
    RunCheckpoint, RunRecoveryOperations, StartupRecoveryCandidate, StartupRecoveryCandidateSource,
    StartupRecoveryOutcome, StartupRecoveryService,
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

#[derive(Default)]
struct MockCandidateSource {
    candidates: Mutex<Vec<StartupRecoveryCandidate>>,
    discovery_error: Mutex<Option<String>>,
}

#[async_trait]
impl StartupRecoveryCandidateSource for MockCandidateSource {
    async fn find_startup_recovery_candidates(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<StartupRecoveryCandidate>, ApplicationError> {
        if let Some(message) = self.discovery_error.lock().unwrap().clone() {
            return Err(ApplicationError::Storage(message));
        }
        Ok(self.candidates.lock().unwrap().clone())
    }
}

#[tokio::test]
async fn startup_recovery_processes_durably_discovered_candidates() {
    let operations = Arc::new(MockRecoveryOperations::default());
    let stale = AgentRunId::new();
    let unknown = AgentRunId::new();
    let source = Arc::new(MockCandidateSource::default());
    *source.candidates.lock().unwrap() = vec![
        StartupRecoveryCandidate::new(stale, RecoveryTarget::StaleLease),
        StartupRecoveryCandidate::new(unknown, RecoveryTarget::UnknownOutcome),
    ];
    let service = StartupRecoveryService::with_candidate_source(operations.clone(), source);
    let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());

    let report = service.run_discovered(&context).await.unwrap();

    assert_eq!(operations.rebuilt.lock().unwrap().as_slice(), &[stale]);
    assert_eq!(report.records().len(), 2);
    assert_eq!(
        report.records()[0].outcome,
        StartupRecoveryOutcome::RetryReady
    );
    assert_eq!(
        report.records()[1].outcome,
        StartupRecoveryOutcome::ReconciliationRequired
    );
}

#[tokio::test]
async fn startup_recovery_discovery_failure_does_not_become_an_empty_run() {
    let operations = Arc::new(MockRecoveryOperations::default());
    let source = Arc::new(MockCandidateSource::default());
    *source.discovery_error.lock().unwrap() = Some("candidate query failed".into());
    let service = StartupRecoveryService::with_candidate_source(operations.clone(), source);
    let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());

    let error = service.run_discovered(&context).await.unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Storage(message) if message == "candidate query failed"
    ));
    assert!(operations.rebuilt.lock().unwrap().is_empty());
}

#[tokio::test]
async fn startup_recovery_without_a_candidate_source_fails_closed() {
    let operations = Arc::new(MockRecoveryOperations::default());
    let service = StartupRecoveryService::new(operations.clone());
    let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());

    let error = service.run_discovered(&context).await.unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::InvalidConfiguration(message)
            if message.contains("startup recovery candidate source")
    ));
    assert!(operations.rebuilt.lock().unwrap().is_empty());
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
