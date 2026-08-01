use vestrace_domain::{
    id::*, models::runtime::*, now,
};

#[test]
fn test_attempt_status_creation() {
    let attempt = ModelExecutionAttempt {
        id: ModelExecutionAttemptId::new(),
        model_execution_id: ModelExecutionId::new(),
        workspace_id: WorkspaceId::new(),
        attempt_number: 1,
        provider_id: ProviderId::new(),
        status: AttemptStatus::Success,
        error_message: None,
        created_at: now(),
    };

    assert_eq!(attempt.attempt_number, 1);
    assert_eq!(attempt.status, AttemptStatus::Success);
}
