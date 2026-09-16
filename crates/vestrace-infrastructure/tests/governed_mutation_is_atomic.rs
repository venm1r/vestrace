use std::sync::Arc;

use async_trait::async_trait;
use chrono::Duration;
use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::{
    AccessTokenMutation, ApplicationError, GovernedMutation, GovernedMutationApply,
    GovernedMutationRepository, IdempotencyRecord, OutboxMessage, RequestContext, UnitOfWork,
};
use vestrace_domain::{
    AccessTokenId, AuditEvent, PrincipalId, WorkspaceId, id::AuditEventId, identity::AccessToken,
    time::now,
};
use vestrace_infrastructure::{
    PgAccessTokenStore, PgGovernedMutationRepository, PgScopedTransaction, PgStore,
};

fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

async fn seed_context(pool: &PgPool, context: &RequestContext) {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("governed-{}", context.workspace_id))
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

fn token(context: &RequestContext) -> AccessToken {
    AccessToken {
        id: AccessTokenId::new(),
        workspace_id: context.workspace_id,
        principal_id: context.principal_id,
        token_hash: format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple()),
        label: "governed mutation".to_owned(),
        created_at: now(),
        expires_at: None,
        revoked_at: None,
        last_used_at: None,
    }
}

fn audit(context: &RequestContext, token: &AccessToken) -> AuditEvent {
    AuditEvent::new(
        AuditEventId::new(),
        context.workspace_id,
        context.principal_id,
        "access_token.created",
        "access_token",
        token.id.as_uuid(),
        serde_json::json!({"label": token.label}),
        now(),
    )
    .unwrap()
}

fn idempotency(context: &RequestContext) -> IdempotencyRecord {
    let created_at = now();
    IdempotencyRecord {
        idempotency_key: format!("governed-{}", Uuid::now_v7()),
        workspace_id: context.workspace_id,
        request_hash: format!("hash-{}", Uuid::now_v7()),
        response_payload: Some(serde_json::json!({"status":"created"})),
        status: "completed".to_owned(),
        created_at,
        expires_at: created_at + Duration::hours(1),
    }
}

#[derive(Clone)]
struct InsertToken {
    token: AccessToken,
    fail: bool,
}

#[async_trait]
impl GovernedMutationApply for InsertToken {
    async fn apply(
        &self,
        _context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<(), ApplicationError> {
        if self.fail {
            return Err(ApplicationError::Storage(
                "forced mutation failure".to_owned(),
            ));
        }
        let transaction = unit_of_work
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| {
                ApplicationError::Internal("expected PostgreSQL transaction".to_owned())
            })?;
        sqlx::query(
            "INSERT INTO access_tokens \
             (id, workspace_id, principal_id, token_hash, label, created_at, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.token.id.as_uuid())
        .bind(self.token.workspace_id.as_uuid())
        .bind(self.token.principal_id.as_uuid())
        .bind(&self.token.token_hash)
        .bind(&self.token.label)
        .bind(self.token.created_at)
        .bind(self.token.expires_at)
        .execute(transaction.connection())
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        Ok(())
    }
}

fn mutation<T: GovernedMutationApply>(
    context: RequestContext,
    token: AccessToken,
    apply: T,
    idempotency: Option<IdempotencyRecord>,
    outbox: Vec<OutboxMessage>,
) -> GovernedMutation<T> {
    GovernedMutation {
        context: context.clone(),
        audit: audit(&context, &token),
        idempotency,
        outbox,
        apply,
    }
}

async fn count(pool: &PgPool, table: &str, id: Uuid) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE id = $1"))
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn failed_mutation_leaves_no_audit_entry(pool: PgPool) {
    let context = context();
    seed_context(&pool, &context).await;
    let token = token(&context);
    let mutation = mutation(
        context,
        token.clone(),
        InsertToken { token, fail: true },
        None,
        Vec::new(),
    );
    let audit_id = mutation.audit.id.as_uuid();

    let repository = PgGovernedMutationRepository::new(PgStore::from_pool(pool.clone()));
    assert!(repository.commit(mutation).await.is_err());
    assert_eq!(count(&pool, "audit_events", audit_id).await, 0);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn failed_audit_write_rolls_back_the_mutation(pool: PgPool) {
    let context = context();
    seed_context(&pool, &context).await;
    let token = token(&context);
    let mutation = mutation(
        context.clone(),
        token.clone(),
        InsertToken {
            token: token.clone(),
            fail: false,
        },
        None,
        Vec::new(),
    );
    sqlx::query(
        "INSERT INTO audit_events \
         (id, workspace_id, principal_id, action, resource_type, resource_id, payload, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(mutation.audit.id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(context.principal_id.as_uuid())
    .bind("already.exists")
    .bind("access_token")
    .bind(Uuid::now_v7())
    .bind(serde_json::json!({}))
    .bind(now())
    .execute(&pool)
    .await
    .unwrap();
    let repository = PgGovernedMutationRepository::new(PgStore::from_pool(pool.clone()));
    assert!(repository.commit(mutation).await.is_err());
    assert_eq!(count(&pool, "access_tokens", token.id.as_uuid()).await, 0);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn failed_outbox_write_rolls_back_mutation_and_audit(pool: PgPool) {
    let context = context();
    seed_context(&pool, &context).await;
    let token = token(&context);
    let outbox = OutboxMessage::new(
        context.workspace_id,
        "access_token.created",
        serde_json::json!({}),
        now(),
    );
    sqlx::query(
        "INSERT INTO outbox (id, workspace_id, topic, payload, processed_at, created_at) \
         VALUES ($1, $2, $3, $4, NULL, $5)",
    )
    .bind(outbox.id.as_uuid())
    .bind(outbox.workspace_id.as_uuid())
    .bind(&outbox.topic)
    .bind(&outbox.payload)
    .bind(outbox.created_at)
    .execute(&pool)
    .await
    .unwrap();
    let mutation = mutation(
        context,
        token.clone(),
        InsertToken {
            token: token.clone(),
            fail: false,
        },
        None,
        vec![outbox],
    );
    let audit_id = mutation.audit.id.as_uuid();

    let repository = PgGovernedMutationRepository::new(PgStore::from_pool(pool.clone()));
    assert!(repository.commit(mutation).await.is_err());
    assert_eq!(count(&pool, "access_tokens", token.id.as_uuid()).await, 0);
    assert_eq!(count(&pool, "audit_events", audit_id).await, 0);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn failed_idempotency_write_rolls_back_mutation_and_audit(pool: PgPool) {
    let context = context();
    seed_context(&pool, &context).await;
    let token = token(&context);
    let idempotency = idempotency(&context);
    sqlx::query(
        "ALTER TABLE idempotency_keys \
         ADD CONSTRAINT idempotency_keys_test_forced_write_failure CHECK (FALSE) NOT VALID",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mutation = mutation(
        context,
        token.clone(),
        InsertToken {
            token: token.clone(),
            fail: false,
        },
        Some(idempotency),
        Vec::new(),
    );
    let audit_id = mutation.audit.id.as_uuid();

    let repository = PgGovernedMutationRepository::new(PgStore::from_pool(pool.clone()));
    assert!(repository.commit(mutation).await.is_err());
    assert_eq!(count(&pool, "access_tokens", token.id.as_uuid()).await, 0);
    assert_eq!(count(&pool, "audit_events", audit_id).await, 0);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn mutation_without_audit_is_refused_by_the_database(pool: PgPool) {
    let context = context();
    seed_context(&pool, &context).await;
    let mut transaction = pool.begin().await.unwrap();
    let mark_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO governed_mutation_audit_marks (id, workspace_id, audit_event_id, created_at) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(mark_id)
    .bind(context.workspace_id.as_uuid())
    .bind(Uuid::now_v7())
    .bind(now())
    .execute(&mut *transaction)
    .await
    .unwrap();
    let watermark: i64 = sqlx::query_scalar(
        "UPDATE installation_mutation_watermark \
         SET watermark = watermark + 1, advanced_at = NOW() \
         WHERE singleton \
         RETURNING watermark",
    )
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO installation_mutation_watermark_advances \
         (governed_mutation_mark_id, watermark, advanced_at) VALUES ($1, $2, $3)",
    )
    .bind(mark_id)
    .bind(watermark)
    .bind(now())
    .execute(&mut *transaction)
    .await
    .unwrap();

    let error = transaction
        .commit()
        .await
        .expect_err("a governed mutation marker without a real audit event must not commit");
    let database_error = error
        .as_database_error()
        .expect("the deferred audit foreign key must return a database error");
    assert_eq!(database_error.code().as_deref(), Some("23503"));
    assert_eq!(
        database_error.constraint(),
        Some("governed_mutation_audit_marks_audit_event_fkey"),
        "the audit-event foreign key, not the independently seeded watermark foreign key, must refuse the commit"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn successful_mutation_commits_all_four_together(pool: PgPool) {
    let context = context();
    seed_context(&pool, &context).await;
    let token = token(&context);
    let outbox = OutboxMessage::new(
        context.workspace_id,
        "access_token.created",
        serde_json::json!({}),
        now(),
    );
    let idempotency = idempotency(&context);
    let mutation = mutation(
        context,
        token.clone(),
        InsertToken {
            token: token.clone(),
            fail: false,
        },
        Some(idempotency.clone()),
        vec![outbox.clone()],
    );
    let audit_id = mutation.audit.id.as_uuid();

    let repository = PgGovernedMutationRepository::new(PgStore::from_pool(pool.clone()));
    repository.commit(mutation).await.unwrap();
    assert_eq!(count(&pool, "access_tokens", token.id.as_uuid()).await, 1);
    assert_eq!(count(&pool, "audit_events", audit_id).await, 1);
    assert_eq!(count(&pool, "outbox", outbox.id.as_uuid()).await, 1);
    let idempotency_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM idempotency_keys WHERE workspace_id = $1 AND idempotency_key = $2",
    )
    .bind(idempotency.workspace_id.as_uuid())
    .bind(&idempotency.idempotency_key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(idempotency_rows, 1);
    let governed_mutation_marks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM governed_mutation_audit_marks WHERE audit_event_id = $1",
    )
    .bind(audit_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(governed_mutation_marks, 1);
    let watermark_advances: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) \
         FROM installation_mutation_watermark_advances AS advance \
         JOIN governed_mutation_audit_marks AS mark \
           ON mark.id = advance.governed_mutation_mark_id \
         WHERE mark.audit_event_id = $1",
    )
    .bind(audit_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(watermark_advances, 1);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn access_token_creation_is_atomic_with_its_audit(pool: PgPool) {
    let context = context();
    seed_context(&pool, &context).await;
    let token = token(&context);
    let store = Arc::new(PgAccessTokenStore::new(PgStore::from_pool(pool.clone())));
    let mutation = mutation(
        context,
        token.clone(),
        AccessTokenMutation::new(store, token.clone()),
        None,
        Vec::new(),
    );
    let audit_id = mutation.audit.id.as_uuid();

    let repository = PgGovernedMutationRepository::new(PgStore::from_pool(pool.clone()));
    repository.commit(mutation).await.unwrap();
    assert_eq!(count(&pool, "access_tokens", token.id.as_uuid()).await, 1);
    assert_eq!(count(&pool, "audit_events", audit_id).await, 1);
}
