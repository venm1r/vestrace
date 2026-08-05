use vestrace_infrastructure::PgStore;

#[sqlx::test]
async fn embedded_migrations_initialize_a_clean_database(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);

    store.migrate().await.unwrap();

    assert!(store.migrations_are_compatible().await.unwrap());
}

#[sqlx::test]
async fn embedded_migrations_are_idempotent(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);

    store.migrate().await.unwrap();
    store.migrate().await.unwrap();

    assert!(store.migrations_are_compatible().await.unwrap());
}
