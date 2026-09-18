use std::{sync::Arc, time::Duration};

use sqlx::PgPool;
use tokio::sync::{Barrier, oneshot};
use uuid::Uuid;
use vestrace_application::{InstallationMutationPermit, PermitMode, RequestContext};
use vestrace_domain::{PrincipalId, WorkspaceId, time::now};
use vestrace_infrastructure::{PgInstallationMutationPermit, PgStore};

fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

async fn seed_context(pool: &PgPool, context: &RequestContext) {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("permit-{}", context.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(context.principal_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(format!("principal-{}", context.principal_id))
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_audit(pool: &PgPool, context: &RequestContext) -> Uuid {
    let audit_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO audit_events \
         (id, workspace_id, principal_id, action, resource_type, resource_id, payload, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(audit_id)
    .bind(context.workspace_id.as_uuid())
    .bind(context.principal_id.as_uuid())
    .bind("permit.test")
    .bind("installation")
    .bind(Uuid::now_v7())
    .bind(serde_json::json!({}))
    .bind(now())
    .execute(pool)
    .await
    .unwrap();
    audit_id
}

async fn advance_watermark(pool: &PgPool, context: &RequestContext, audit_id: Uuid) -> i64 {
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.workspace_id")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.principal_id")
        .bind(context.principal_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let advance = sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_record_governed_mutation_audit_mark_and_advance($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(context.workspace_id.as_uuid())
    .bind(audit_id)
    .bind(now())
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    advance
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn exclusive_excludes_shared(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    let permit = PgInstallationMutationPermit::new(store);
    let context = context();
    let exclusive = permit
        .acquire(PermitMode::Exclusive, &context)
        .await
        .unwrap();

    let (acquired, mut acquired_result) = oneshot::channel();
    let concurrent = permit.clone();
    let concurrent_context = context.clone();
    let waiter = tokio::spawn(async move {
        let handle = concurrent
            .acquire(PermitMode::Shared, &concurrent_context)
            .await
            .unwrap();
        acquired.send(()).unwrap();
        handle.rollback().await.unwrap();
    });

    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut acquired_result)
            .await
            .is_err(),
        "a shared permit acquired while an exclusive permit was live",
    );
    exclusive.rollback().await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), &mut acquired_result)
        .await
        .expect("the shared permit must proceed after exclusive rollback")
        .expect("the waiter must report acquisition");
    waiter.await.unwrap();
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn shared_permits_are_concurrent(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    let permit = PgInstallationMutationPermit::new(store);
    let context = context();
    let first = permit.acquire(PermitMode::Shared, &context).await.unwrap();

    let second = tokio::time::timeout(
        Duration::from_secs(1),
        permit.acquire(PermitMode::Shared, &context),
    )
    .await
    .expect("a second shared permit must not wait for the first")
    .unwrap();

    second.rollback().await.unwrap();
    first.rollback().await.unwrap();
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn crashed_holder_releases_its_permit(pool: PgPool) {
    let store = PgStore::from_pool(pool);
    let permit = PgInstallationMutationPermit::new(store);
    let context = context();
    let exclusive = permit
        .acquire(PermitMode::Exclusive, &context)
        .await
        .unwrap();

    drop(exclusive);

    let shared = tokio::time::timeout(
        Duration::from_secs(1),
        permit.acquire(PermitMode::Shared, &context),
    )
    .await
    .expect("dropping the holder must release its transaction-scoped permit")
    .unwrap();
    shared.rollback().await.unwrap();
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn mutation_without_watermark_advance_is_refused(pool: PgPool) {
    let context = context();
    seed_context(&pool, &context).await;
    let audit_id = seed_audit(&pool, &context).await;
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO governed_mutation_audit_marks (id, workspace_id, audit_event_id, created_at) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(context.workspace_id.as_uuid())
    .bind(audit_id)
    .bind(now())
    .execute(&mut *transaction)
    .await
    .unwrap();

    let error = transaction
        .commit()
        .await
        .expect_err("a governed mutation marker without a watermark advance committed");
    let database_error = error
        .as_database_error()
        .expect("the deferred watermark foreign key must return a database error");
    assert_eq!(database_error.code().as_deref(), Some("23503"));
    assert_eq!(
        database_error.constraint(),
        Some("governed_mutation_audit_marks_watermark_advance_fkey"),
        "the watermark-advance foreign key must refuse the commit"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn watermark_is_monotonic_under_concurrency(pool: PgPool) {
    let first_context = context();
    let second_context = context();
    seed_context(&pool, &first_context).await;
    seed_context(&pool, &second_context).await;
    let first_audit = seed_audit(&pool, &first_context).await;
    let second_audit = seed_audit(&pool, &second_context).await;
    let barrier = Arc::new(Barrier::new(3));

    let first = {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            advance_watermark(&pool, &first_context, first_audit).await
        })
    };
    let second = {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            advance_watermark(&pool, &second_context, second_audit).await
        })
    };

    barrier.wait().await;
    let mut advances = vec![first.await.unwrap(), second.await.unwrap()];
    advances.sort_unstable();
    assert_eq!(advances, vec![1, 2]);
    let current: i64 =
        sqlx::query_scalar("SELECT watermark FROM installation_mutation_watermark WHERE singleton")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(current, 2);
}
