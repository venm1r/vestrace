//! R1.2 run event-store migration contract.

#[sqlx::test(migrations = "./migrations")]
async fn run_events_expose_the_r1_envelope_columns(pool: sqlx::PgPool) {
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name
         FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'run_events'
           AND column_name IN (
               'event_version',
               'actor',
               'causation_id',
               'correlation_id',
               'occurred_at'
           )
         ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        columns,
        vec![
            "actor",
            "causation_id",
            "correlation_id",
            "event_version",
            "occurred_at",
        ]
    );
}
