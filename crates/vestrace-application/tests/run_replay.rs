use vestrace_application::run::replay_run;
use vestrace_domain::{
    id::{AgentRuntimeSnapshotId, CorrelationId},
    run::{
        ResumeCursor, RunActorRef, RunEvent, RunEventPayload, RunExecutionMode, RunStatus,
        RunVersion,
    },
    time::Timestamp,
};

fn now() -> Timestamp {
    use chrono::TimeZone;
    chrono::Utc
        .with_ymd_and_hms(2026, 8, 8, 12, 0, 0)
        .single()
        .unwrap()
}

fn make_event(
    version: RunVersion,
    run_id: vestrace_domain::id::AgentRunId,
    workspace_id: vestrace_domain::id::WorkspaceId,
    payload: RunEventPayload,
) -> RunEvent {
    RunEvent::new(
        run_id,
        workspace_id,
        version,
        ResumeCursor::from_version(version),
        RunActorRef::System,
        payload,
        CorrelationId::new(),
        None,
        now(),
    )
    .unwrap()
}

#[test]
fn replay_reconstructs_projection() {
    let run_id = vestrace_domain::id::AgentRunId::new();
    let ws = vestrace_domain::id::WorkspaceId::new();

    let events = vec![
        make_event(
            RunVersion::INITIAL,
            run_id,
            ws,
            RunEventPayload::RunCreated {
                objective: "Summarize changes".into(),
                execution_mode: RunExecutionMode::Autopilot,
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                parent: None,
            },
        ),
        make_event(
            RunVersion::new(2).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Created,
                to: RunStatus::Preparing,
                result: None,
            },
        ),
        make_event(
            RunVersion::new(3).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Preparing,
                to: RunStatus::Running,
                result: None,
            },
        ),
        make_event(
            RunVersion::new(4).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Running,
                to: RunStatus::WaitingForInput,
                result: None,
            },
        ),
    ];

    let projection = replay_run(&events).unwrap();
    assert_eq!(projection.status, RunStatus::WaitingForInput);
    assert_eq!(projection.version, RunVersion::new(4).unwrap());
    assert_eq!(projection.objective, "Summarize changes");
}

#[test]
fn empty_stream_rejected() {
    let result = replay_run(&[]);
    assert!(result.is_err());
}

#[test]
fn first_event_must_be_run_created() {
    let run_id = vestrace_domain::id::AgentRunId::new();
    let ws = vestrace_domain::id::WorkspaceId::new();

    let event = make_event(
        RunVersion::new(2).unwrap(),
        run_id,
        ws,
        RunEventPayload::RunStatusChanged {
            from: RunStatus::Created,
            to: RunStatus::Preparing,
            result: None,
        },
    );

    let result = replay_run(&[event]);
    assert!(result.is_err());
}

#[test]
fn non_contiguous_versions_rejected() {
    let run_id = vestrace_domain::id::AgentRunId::new();
    let ws = vestrace_domain::id::WorkspaceId::new();

    let created = make_event(
        RunVersion::INITIAL,
        run_id,
        ws,
        RunEventPayload::RunCreated {
            objective: "Test".into(),
            execution_mode: RunExecutionMode::Autopilot,
            coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
            parent: None,
        },
    );

    let skip = make_event(
        RunVersion::new(3).unwrap(),
        run_id,
        ws,
        RunEventPayload::RunStatusChanged {
            from: RunStatus::Created,
            to: RunStatus::Preparing,
            result: None,
        },
    );

    let result = replay_run(&[created, skip]);
    assert!(result.is_err());
}

#[test]
fn mixed_run_ids_rejected() {
    let run_id_1 = vestrace_domain::id::AgentRunId::new();
    let run_id_2 = vestrace_domain::id::AgentRunId::new();
    let ws = vestrace_domain::id::WorkspaceId::new();

    let created = make_event(
        RunVersion::INITIAL,
        run_id_1,
        ws,
        RunEventPayload::RunCreated {
            objective: "Test".into(),
            execution_mode: RunExecutionMode::Autopilot,
            coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
            parent: None,
        },
    );

    let mut wrong = make_event(
        RunVersion::new(2).unwrap(),
        run_id_1,
        ws,
        RunEventPayload::RunStatusChanged {
            from: RunStatus::Created,
            to: RunStatus::Preparing,
            result: None,
        },
    );
    wrong.run_id = run_id_2;

    let result = replay_run(&[created, wrong]);
    assert!(result.is_err());
}

#[test]
fn terminal_result_recorded() {
    let run_id = vestrace_domain::id::AgentRunId::new();
    let ws = vestrace_domain::id::WorkspaceId::new();

    let events = vec![
        make_event(
            RunVersion::INITIAL,
            run_id,
            ws,
            RunEventPayload::RunCreated {
                objective: "Test".into(),
                execution_mode: RunExecutionMode::Autopilot,
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                parent: None,
            },
        ),
        make_event(
            RunVersion::new(2).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Created,
                to: RunStatus::Preparing,
                result: None,
            },
        ),
        make_event(
            RunVersion::new(3).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Preparing,
                to: RunStatus::Running,
                result: None,
            },
        ),
        make_event(
            RunVersion::new(4).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Running,
                to: RunStatus::Succeeded,
                result: Some(vestrace_domain::run::RunTerminalResult::Succeeded {
                    summary: Some("Done".into()),
                }),
            },
        ),
    ];

    let projection = replay_run(&events).unwrap();
    assert_eq!(projection.status, RunStatus::Succeeded);
    assert!(projection.result.is_some());
    assert!(projection.finished_at.is_some());
}

#[test]
fn event_sequence_must_equal_version() {
    let run_id = vestrace_domain::id::AgentRunId::new();
    let ws = vestrace_domain::id::WorkspaceId::new();

    let result = RunEvent::new(
        run_id,
        ws,
        RunVersion::new(3).unwrap(),
        ResumeCursor::from_version(RunVersion::new(2).unwrap()),
        RunActorRef::System,
        RunEventPayload::RunStatusChanged {
            from: RunStatus::Preparing,
            to: RunStatus::Running,
            result: None,
        },
        CorrelationId::new(),
        None,
        now(),
    );
    assert!(result.is_err());
}
