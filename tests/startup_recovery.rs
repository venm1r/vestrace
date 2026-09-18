use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_application::{
    RecoveryQualificationEvidence, RecoveryQualificationEvidenceRepository, RunCheckpoint,
    RunRecoveryOperations, StartupRecoveryCandidate, StartupRecoveryCandidateSource,
    StartupRecoveryOutcome, StartupRecoveryRecord, StartupRecoveryService,
};
use vestrace_domain::run::{AgentRun, NewAgentRun, RunExecutionMode, RunState, RunVersion};
use vestrace_domain::{
    AgentRunId, AgentRuntimeSnapshotId, RecoveryAction, RecoveryClassification, RecoveryTarget,
    WorkspaceId, evaluate_recovery_qualification, now,
};

#[derive(Default)]
struct MockRecoveryOperations {
    rebuilt: Mutex<Vec<AgentRunId>>,
    rebuild_error: Mutex<Option<String>>,
    rebuild_error_for: Mutex<Option<AgentRunId>>,
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
            let fail_for = *self.rebuild_error_for.lock().unwrap();
            if fail_for.is_none() || fail_for == Some(run_id) {
                return Err(ApplicationError::Internal(message));
            }
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

#[derive(Default)]
struct RecordingRecoveryQualificationEvidenceRepository {
    observations: Mutex<Vec<RecoveryQualificationEvidence>>,
    insert_error: Mutex<Option<String>>,
    stall_insert: Mutex<bool>,
}

#[async_trait]
impl RecoveryQualificationEvidenceRepository for RecordingRecoveryQualificationEvidenceRepository {
    async fn insert(
        &self,
        observation: &RecoveryQualificationEvidence,
    ) -> Result<(), ApplicationError> {
        if *self.stall_insert.lock().unwrap() {
            std::future::pending().await
        }
        if let Some(message) = self.insert_error.lock().unwrap().clone() {
            return Err(ApplicationError::Storage(message));
        }
        self.observations.lock().unwrap().push(observation.clone());
        Ok(())
    }

    async fn list(&self) -> Result<Vec<RecoveryQualificationEvidence>, ApplicationError> {
        Ok(self.observations.lock().unwrap().clone())
    }
}

fn recording_evidence_repository() -> Arc<RecordingRecoveryQualificationEvidenceRepository> {
    Arc::new(RecordingRecoveryQualificationEvidenceRepository::default())
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
    let service = StartupRecoveryService::with_candidate_source(
        operations.clone(),
        source,
        recording_evidence_repository(),
    );
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
    let service = StartupRecoveryService::with_candidate_source(
        operations.clone(),
        source,
        recording_evidence_repository(),
    );
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
    let service = StartupRecoveryService::new(operations.clone(), recording_evidence_repository());
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
    let evidence = recording_evidence_repository();
    let service = StartupRecoveryService::new(operations.clone(), evidence.clone());
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
    let observations = evidence.observations.lock().unwrap();
    assert_eq!(observations.len(), 3);
    assert_eq!(observations[0].run_id(), resumable);
    assert_eq!(observations[0].target(), RecoveryTarget::RunningExecution);
    assert_eq!(
        observations[0].classification(),
        RecoveryClassification::SafeToResume
    );
    assert_eq!(observations[0].action(), RecoveryAction::Resume);
}

#[tokio::test]
async fn aborted_outcome_is_recorded_as_abort_even_when_classification_says_resume() {
    let evidence = recording_evidence_repository();
    let record = StartupRecoveryRecord {
        run_id: AgentRunId::new(),
        target: RecoveryTarget::RunningExecution,
        classification: RecoveryClassification::SafeToResume,
        outcome: StartupRecoveryOutcome::Aborted,
    };
    let observation = RecoveryQualificationEvidence::from_recovery_record(&record, now());

    evidence.insert(&observation).await.unwrap();

    let stored = evidence.list().await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].action(), RecoveryAction::Abort);
    let decision = evaluate_recovery_qualification(&[stored[0].to_observation()]);
    assert!(
        decision
            .failures()
            .contains(&"recovery target RunningExecution has unsafe action".to_owned()),
        "unexpected failures: {:?}",
        decision.failures()
    );
}

#[test]
fn every_recorded_action_comes_from_the_recovery_outcome() {
    let cases = [
        (StartupRecoveryOutcome::Restored, RecoveryAction::Resume),
        (StartupRecoveryOutcome::RetryReady, RecoveryAction::Retry),
        (
            StartupRecoveryOutcome::ReconciliationRequired,
            RecoveryAction::Reconcile,
        ),
        (StartupRecoveryOutcome::Aborted, RecoveryAction::Abort),
        (
            StartupRecoveryOutcome::HumanReviewRequired,
            RecoveryAction::HumanReview,
        ),
    ];

    for (outcome, expected_action) in cases {
        let evidence = RecoveryQualificationEvidence::from_recovery_record(
            &StartupRecoveryRecord {
                run_id: AgentRunId::new(),
                target: RecoveryTarget::RunningExecution,
                classification: RecoveryClassification::SafeToResume,
                outcome,
            },
            now(),
        );

        assert_eq!(evidence.action(), expected_action, "outcome: {outcome:?}");
    }
}

#[tokio::test]
async fn evidence_failure_does_not_prevent_recovery_actions_for_later_candidates() {
    let operations = Arc::new(MockRecoveryOperations::default());
    let evidence = recording_evidence_repository();
    *evidence.insert_error.lock().unwrap() = Some("observation write failed".into());
    let service = StartupRecoveryService::new(operations.clone(), evidence);
    let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());
    let first = AgentRunId::new();
    let second = AgentRunId::new();

    let error = service
        .run(
            &context,
            vec![
                StartupRecoveryCandidate::new(first, RecoveryTarget::RunningExecution),
                StartupRecoveryCandidate::new(second, RecoveryTarget::StaleLease),
            ],
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Storage(message) if message == "observation write failed"
    ));
    assert_eq!(
        operations.rebuilt.lock().unwrap().as_slice(),
        &[first, second],
        "recording must not decide which recovery actions run"
    );
}

#[tokio::test]
async fn stalled_evidence_write_does_not_prevent_recovery_actions_for_later_candidates() {
    let operations = Arc::new(MockRecoveryOperations::default());
    let evidence = recording_evidence_repository();
    *evidence.stall_insert.lock().unwrap() = true;
    let service = StartupRecoveryService::new(operations.clone(), evidence)
        .with_evidence_write_timeout(std::time::Duration::from_millis(10));
    let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());
    let first = AgentRunId::new();
    let second = AgentRunId::new();

    let error = service
        .run(
            &context,
            vec![
                StartupRecoveryCandidate::new(first, RecoveryTarget::RunningExecution),
                StartupRecoveryCandidate::new(second, RecoveryTarget::StaleLease),
            ],
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Storage(message)
            if message == "recovery qualification evidence write timed out"
    ));
    assert_eq!(
        operations.rebuilt.lock().unwrap().as_slice(),
        &[first, second],
        "stalled recording must not decide which recovery actions run"
    );
}

#[tokio::test]
async fn startup_recovery_marks_safe_retry_as_retry_ready() {
    let operations = Arc::new(MockRecoveryOperations::default());
    let service = StartupRecoveryService::new(operations.clone(), recording_evidence_repository());
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
    let service = StartupRecoveryService::new(operations.clone(), recording_evidence_repository());
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
    let service = StartupRecoveryService::new(operations.clone(), recording_evidence_repository());
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

#[tokio::test]
async fn a_later_recovery_failure_does_not_erase_an_earlier_recovery_observation() {
    let operations = Arc::new(MockRecoveryOperations::default());
    let first = AgentRunId::new();
    let second = AgentRunId::new();
    *operations.rebuild_error.lock().unwrap() = Some("second projection write failed".into());
    *operations.rebuild_error_for.lock().unwrap() = Some(second);
    let evidence = recording_evidence_repository();
    let service = StartupRecoveryService::new(operations, evidence.clone());
    let context = RequestContext::new(WorkspaceId::new(), vestrace_domain::PrincipalId::new());

    let error = service
        .run(
            &context,
            vec![
                StartupRecoveryCandidate::new(first, RecoveryTarget::RunningExecution),
                StartupRecoveryCandidate::new(second, RecoveryTarget::StaleLease),
            ],
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Internal(message) if message == "second projection write failed"
    ));
    let observations = evidence.list().await.unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].run_id(), first);
    assert_eq!(observations[0].action(), RecoveryAction::Resume);
}
