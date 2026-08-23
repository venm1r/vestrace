use sqlx::{PgPool, Row};
use uuid::Uuid;
use vestrace_application::{
    ModelDataPolicyDecisionRecord, ModelDataPolicyDecisionRepository, ModelDataPolicyMode,
};
use vestrace_domain::id::{AgentRunId, RunStepId};
use vestrace_domain::{DataDestination, Sensitivity, now};
use vestrace_infrastructure::{PgModelDataPolicyDecisionRepository, PgStore};

fn decision() -> ModelDataPolicyDecisionRecord {
    ModelDataPolicyDecisionRecord {
        id: Uuid::now_v7(),
        run_id: AgentRunId::new(),
        step_id: RunStepId::new(),
        destination: DataDestination::RemoteProvider,
        classification: Sensitivity::Confidential,
        allowed: false,
        reason: "destination is not allowed; destination RemoteProvider".into(),
        policy_version: "data-policy-v7".into(),
        mode: ModelDataPolicyMode::Observe,
        decided_at: now(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn model_data_policy_decisions_preserve_the_boundary_facts(pool: PgPool) {
    let repository = PgModelDataPolicyDecisionRepository::new(PgStore::from_pool(pool.clone()));
    let decision = decision();

    repository.record(&decision).await.unwrap();

    let row = sqlx::query(
        "SELECT run_id, step_id, destination, classification, verdict, reason, policy_version, mode, decided_at
         FROM model_data_policy_decisions
         WHERE id = $1",
    )
    .bind(decision.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<Uuid, _>("run_id"), decision.run_id.as_uuid());
    assert_eq!(row.get::<Uuid, _>("step_id"), decision.step_id.as_uuid());
    assert_eq!(row.get::<String, _>("destination"), "remote_provider");
    assert_eq!(row.get::<String, _>("classification"), "confidential");
    assert_eq!(row.get::<String, _>("verdict"), "denied");
    assert_eq!(row.get::<String, _>("reason"), decision.reason);
    assert_eq!(row.get::<String, _>("policy_version"), "data-policy-v7");
    assert_eq!(row.get::<String, _>("mode"), "observe");
    let stored_decided_at = row.get::<chrono::DateTime<chrono::Utc>, _>("decided_at");
    // Exact equality is wrong here: PostgreSQL TIMESTAMPTZ rounds to
    // microseconds while chrono retains nanoseconds. A difference within one
    // microsecond is the column's storage precision; anything larger means the
    // persisted model_data_policy_decisions.decided_at fact changed.
    let decided_at_difference =
        (stored_decided_at.timestamp_micros() - decision.decided_at.timestamp_micros()).abs();
    assert!(
        decided_at_difference <= 1,
        "model_data_policy_decisions.decided_at differs by {decided_at_difference} microseconds: stored {stored_decided_at:?}, expected {:?}",
        decision.decided_at
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn model_data_policy_decisions_cannot_be_edited_or_deleted(pool: PgPool) {
    let repository = PgModelDataPolicyDecisionRepository::new(PgStore::from_pool(pool.clone()));
    let decision = decision();
    repository.record(&decision).await.unwrap();

    let update =
        sqlx::query("UPDATE model_data_policy_decisions SET verdict = 'allowed' WHERE id = $1")
            .bind(decision.id)
            .execute(&pool)
            .await
            .unwrap_err();
    assert!(
        update.as_database_error().is_some_and(|error| error
            .message()
            .contains("model data-policy decisions are append-only")),
        "unexpected UPDATE error: {update}"
    );

    let delete = sqlx::query("DELETE FROM model_data_policy_decisions WHERE id = $1")
        .bind(decision.id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        delete.as_database_error().is_some_and(|error| error
            .message()
            .contains("model data-policy decisions are append-only")),
        "unexpected DELETE error: {delete}"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM model_data_policy_decisions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
