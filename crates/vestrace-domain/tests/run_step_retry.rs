//! Public contract for deterministic step-level retries.

use chrono::{Duration, TimeZone, Utc};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, PrincipalId, RunStepId, WorkspaceId},
    run::{
        RunActor, RunCommand, RunCommandEnvelope, RunEvent, RunStatus, RunWait, StallReason,
        StepFailure, StepFailureDisposition,
    },
};

#[test]
fn retry_commands_events_and_state_round_trip_through_json() {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    let run_id = AgentRunId::new();
    let step_id = RunStepId::new();
    let issued_at = Utc.with_ymd_and_hms(2026, 8, 5, 6, 0, 0).single().unwrap();
    let resume_at = issued_at + Duration::seconds(30);

    let commands = [
        RunCommand::StartStep {
            step_id,
            kind: "tool".to_owned(),
            label: Some("Retryable tool".to_owned()),
            max_attempts: 3,
        },
        RunCommand::FailStep {
            step_id,
            code: "provider_timeout".to_owned(),
            message: "provider timed out".to_owned(),
            disposition: StepFailureDisposition::Retry { resume_at },
        },
        RunCommand::ResumeStepRetry { step_id },
    ];

    for command in commands {
        let envelope = RunCommandEnvelope {
            command_id: OperationId::new(),
            idempotency_key: None,
            workspace_id,
            run_id,
            actor: RunActor::Principal(principal_id),
            expected_version: vestrace_domain::run::RunVersion::INITIAL,
            correlation_id: CorrelationId::new(),
            issued_at,
            command,
        };
        let json = serde_json::to_value(&envelope).unwrap();
        assert_eq!(
            serde_json::from_value::<RunCommandEnvelope>(json).unwrap(),
            envelope
        );
    }

    let failure = StepFailure {
        code: "provider_timeout".to_owned(),
        message: "provider timed out".to_owned(),
        failed_at: issued_at,
    };
    let wait = RunWait::Retry {
        step_id,
        kind: "tool".to_owned(),
        label: Some("Retryable tool".to_owned()),
        failed_attempt: 1,
        next_attempt: 2,
        max_attempts: 3,
        resume_at,
        last_failure: failure.clone(),
    };
    assert_eq!(
        serde_json::from_value::<RunWait>(serde_json::to_value(&wait).unwrap()).unwrap(),
        wait
    );

    let stalled = StallReason::RetryLimitExceeded {
        step_id,
        attempts: 3,
    };
    assert_eq!(
        serde_json::from_value::<StallReason>(serde_json::to_value(&stalled).unwrap()).unwrap(),
        stalled
    );
    assert!(RunStatus::WaitingForRetry.is_waiting());

    let events = [
        RunEvent::StepAttemptStarted {
            step_id,
            kind: "tool".to_owned(),
            label: None,
            attempt: 1,
            max_attempts: 3,
        },
        RunEvent::StepAttemptCompleted {
            step_id,
            attempt: 1,
            output_references: vec!["artifact://result".to_owned()],
        },
        RunEvent::StepAttemptFailed {
            step_id,
            attempt: 1,
            max_attempts: 3,
            code: failure.code.clone(),
            message: failure.message.clone(),
            retryable: true,
        },
        RunEvent::StepRetryScheduled {
            step_id,
            kind: "tool".to_owned(),
            label: None,
            failed_attempt: 1,
            next_attempt: 2,
            max_attempts: 3,
            resume_at,
            code: failure.code,
            message: failure.message,
        },
        RunEvent::StepRetryResumed {
            step_id,
            attempt: 2,
        },
    ];

    let expected_types = [
        "step.attempt_started",
        "step.attempt_completed",
        "step.attempt_failed",
        "step.retry_scheduled",
        "step.retry_resumed",
    ];
    for (event, expected_type) in events.into_iter().zip(expected_types) {
        assert_eq!(event.event_type(), expected_type);
        assert_eq!(event.event_version(), 1);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(serde_json::from_value::<RunEvent>(json).unwrap(), event);
    }
}
