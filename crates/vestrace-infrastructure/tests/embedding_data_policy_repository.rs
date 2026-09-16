use sqlx::{PgPool, Row};
use uuid::Uuid;
use vestrace_application::{
    EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
    EmbeddingDataPolicyMode, EmbeddingPurpose,
};
use vestrace_domain::{DataDestination, Sensitivity, now};
use vestrace_infrastructure::{PgEmbeddingDataPolicyDecisionRepository, PgStore};

fn decision(attempt: u32) -> EmbeddingDataPolicyDecisionRecord {
    EmbeddingDataPolicyDecisionRecord {
        id: Uuid::now_v7(),
        purpose: EmbeddingPurpose::Delivery,
        causal_reference_id: Uuid::now_v7(),
        delivery_attempt: Some(attempt),
        batch_ordinal: None,
        destination: DataDestination::RemoteProvider,
        classification: Sensitivity::Confidential,
        classification_labels: vec!["internal".into()],
        unclassified_count: 0,
        input_count: 1,
        classification_allowed: true,
        destination_allowed: false,
        allowed: false,
        reason: "data destination check refused RemoteProvider".into(),
        policy_version: "embedding-policy-v1".into(),
        mode: EmbeddingDataPolicyMode::Observe,
        decided_at: now(),
    }
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn embedding_decisions_preserve_purpose_cause_and_both_policy_checks(pool: PgPool) {
    let repository = PgEmbeddingDataPolicyDecisionRepository::new(PgStore::from_pool(pool.clone()));
    let first = decision(1);
    let mut second = decision(2);
    second.causal_reference_id = first.causal_reference_id;

    repository.record(&first).await.unwrap();
    repository.record(&second).await.unwrap();

    let rows = sqlx::query(
        "SELECT purpose, causal_reference_id, delivery_attempt, batch_ordinal,
                destination, classification, classification_labels,
                unclassified_count, input_count, classification_allowed,
                destination_allowed, verdict, reason, policy_version, mode
         FROM embedding_data_policy_decisions
         WHERE causal_reference_id = $1
         ORDER BY delivery_attempt",
    )
    .bind(first.causal_reference_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<String, _>("purpose"), "delivery");
    assert_eq!(rows[0].get::<i32, _>("delivery_attempt"), 1);
    assert_eq!(rows[1].get::<i32, _>("delivery_attempt"), 2);
    assert_eq!(rows[0].get::<Option<i32>, _>("batch_ordinal"), None);
    assert_eq!(rows[0].get::<String, _>("destination"), "remote_provider");
    assert_eq!(rows[0].get::<String, _>("classification"), "confidential");
    assert_eq!(
        rows[0].get::<Vec<String>, _>("classification_labels"),
        vec!["internal"]
    );
    assert_eq!(rows[0].get::<i32, _>("unclassified_count"), 0);
    assert_eq!(rows[0].get::<i32, _>("input_count"), 1);
    assert!(rows[0].get::<bool, _>("classification_allowed"));
    assert!(!rows[0].get::<bool, _>("destination_allowed"));
    assert_eq!(rows[0].get::<String, _>("verdict"), "denied");
    assert_eq!(rows[0].get::<String, _>("mode"), "observe");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn embedding_decisions_are_append_only_and_not_tenant_owned(pool: PgPool) {
    let repository = PgEmbeddingDataPolicyDecisionRepository::new(PgStore::from_pool(pool.clone()));
    let decision = decision(1);
    repository.record(&decision).await.unwrap();

    let workspace_column: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM information_schema.columns
             WHERE table_schema = 'public'
               AND table_name = 'embedding_data_policy_decisions'
               AND column_name = 'workspace_id'
         )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!workspace_column);

    let update =
        sqlx::query("UPDATE embedding_data_policy_decisions SET verdict = 'allowed' WHERE id = $1")
            .bind(decision.id)
            .execute(&pool)
            .await
            .unwrap_err();
    assert!(
        update.as_database_error().is_some_and(|error| error
            .message()
            .contains("embedding data-policy decisions are append-only")),
        "unexpected UPDATE error: {update}"
    );

    let delete = sqlx::query("DELETE FROM embedding_data_policy_decisions WHERE id = $1")
        .bind(decision.id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        delete.as_database_error().is_some_and(|error| error
            .message()
            .contains("embedding data-policy decisions are append-only")),
        "unexpected DELETE error: {delete}"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn purpose_rejects_a_causal_shape_another_path_could_mislabel(pool: PgPool) {
    let error = sqlx::query(
        "INSERT INTO embedding_data_policy_decisions (
             id, purpose, causal_reference_id, delivery_attempt, batch_ordinal,
             destination, classification, classification_labels,
             unclassified_count, input_count, classification_allowed,
             destination_allowed, verdict, reason, policy_version, mode, decided_at
         ) VALUES (
             $1, 'dimension_probe', $2, 3, NULL, 'local_model', 'internal',
             ARRAY[]::TEXT[], 1, 1, true, true, 'allowed', 'bad cause shape',
             'embedding-policy-v1', 'enforce', now()
         )",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&pool)
    .await
    .unwrap_err();

    let database_error = error
        .as_database_error()
        .expect("database constraint error");
    assert_eq!(
        database_error.constraint(),
        Some("embedding_data_policy_cause_matches_purpose")
    );
}
