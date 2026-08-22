use std::{
    borrow::Cow,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use sqlx::{PgPool, migrate::Migrator, postgres::PgPoolOptions};
use vestrace_application::{
    ApplicationError, ExternalEffectFaultSuiteEvidence, ExternalEffectReadBackAdapter,
    ExternalEffectReadBackRegistry, ExternalEffectRecoveryService, ExternalEffectRepository,
    FaultSuiteEvidenceRepository, RECONCILIATION_BATCH, RequestContext,
};
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectFaultPoint, EffectLifecycleStatus, EffectPrecondition,
    EffectReversibility, EvidenceStrength, ExternalEffectIntent, ExternalEffectReceipt,
    FaultObservation, IdempotencyProfile, ObservedEffectState, ReconciliationOutcome,
    reconcile_effect,
};
use vestrace_domain::{Capability, PrincipalId, RiskCategory, WorkerId, WorkspaceId};
use vestrace_infrastructure::{PgExternalEffectRepository, PgStore};

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

#[derive(Debug, sqlx::FromRow)]
struct LegacyDispatchTransitionRow {
    id: uuid::Uuid,
    effect_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    status: String,
    cause: String,
    cause_ref: String,
    recorded_at: chrono::DateTime<Utc>,
    created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct MigratedDispatchTransitionRow {
    id: uuid::Uuid,
    effect_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    status: String,
    cause: String,
    cause_ref: String,
    recorded_at: chrono::DateTime<Utc>,
    created_at: chrono::DateTime<Utc>,
    dispatch_owner: Option<String>,
    dispatch_expires_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct LegacyOrdinalTransitionRow {
    id: uuid::Uuid,
    effect_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    status: String,
    cause: String,
    cause_ref: String,
    recorded_at: chrono::DateTime<Utc>,
    created_at: chrono::DateTime<Utc>,
    dispatch_owner: Option<String>,
    dispatch_expires_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct MigratedOrdinalTransitionRow {
    id: uuid::Uuid,
    effect_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    status: String,
    cause: String,
    cause_ref: String,
    recorded_at: chrono::DateTime<Utc>,
    created_at: chrono::DateTime<Utc>,
    dispatch_owner: Option<String>,
    dispatch_expires_at: Option<chrono::DateTime<Utc>>,
    ordinal: i64,
}

struct CountingReadBack {
    calls: Arc<AtomicUsize>,
}

struct FailingReadBack;

struct FailOnceReadBack {
    calls: Arc<AtomicUsize>,
}

fn read_back_descriptor(
    name: &str,
) -> vestrace_domain::external_effects::ExternalEffectAdapterDescriptor {
    vestrace_domain::external_effects::ExternalEffectAdapterDescriptor::new(
        name,
        None,
        DeliverySemantics::AtLeastOnce,
        IdempotencyProfile::ProviderKey,
        EffectReversibility::Compensatable,
        vestrace_domain::external_effects::DryRunMode::Unsupported,
        true,
        true,
        Capability::ExportRead,
    )
    .unwrap()
}

#[async_trait]
impl ExternalEffectReadBackAdapter for CountingReadBack {
    async fn observe(
        &self,
        _intent: &ExternalEffectIntent,
        _receipt: Option<&ExternalEffectReceipt>,
    ) -> Result<Vec<ObservedEffectState>, ApplicationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )])
    }
}

#[async_trait]
impl ExternalEffectReadBackAdapter for FailingReadBack {
    async fn observe(
        &self,
        _intent: &ExternalEffectIntent,
        _receipt: Option<&ExternalEffectReceipt>,
    ) -> Result<Vec<ObservedEffectState>, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "provider read-back failed".into(),
        ))
    }
}

#[async_trait]
impl ExternalEffectReadBackAdapter for FailOnceReadBack {
    async fn observe(
        &self,
        _intent: &ExternalEffectIntent,
        _receipt: Option<&ExternalEffectReceipt>,
    ) -> Result<Vec<ObservedEffectState>, ApplicationError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(ApplicationError::Unavailable(
                "provider read-back failed".into(),
            ));
        }
        Ok(vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:recovered",
            vec!["evidence:provider-lookup".into()],
        )])
    }
}

#[derive(Clone)]
struct LifecycleRuntimeRole {
    name: String,
}

impl LifecycleRuntimeRole {
    fn quoted(&self) -> String {
        format!("\"{}\"", self.name.replace('"', "\"\""))
    }
}

async fn with_lifecycle_runtime_role<T, F, Fut>(admin_pool: &PgPool, test: F) -> T
where
    T: Send + 'static,
    F: FnOnce(PgPool, LifecycleRuntimeRole) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let role = LifecycleRuntimeRole {
        name: format!("vestrace_lifecycle_{}", uuid::Uuid::now_v7().simple()),
    };
    let password = uuid::Uuid::now_v7().simple().to_string();
    let quoted = role.quoted();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(admin_pool)
        .await
        .unwrap();
    let quoted_database = format!("\"{}\"", database.replace('"', "\"\""));
    sqlx::query(&format!(
        "CREATE ROLE {quoted} LOGIN PASSWORD '{password}' \
         NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS"
    ))
    .execute(admin_pool)
    .await
    .unwrap();

    // From this point on the role is cluster-global state. Every failure is
    // captured until cleanup has attempted all of its statements.
    let execution = exercise_lifecycle_runtime_role(
        admin_pool,
        role.clone(),
        &password,
        &quoted_database,
        test,
    )
    .await;
    let cleanup_failures =
        cleanup_lifecycle_runtime_role(admin_pool, &role, &quoted_database).await;

    match execution {
        Err(setup_failure) => panic!(
            "restricted lifecycle runtime-role setup failed: {setup_failure}; \
             cleanup failures after attempting every step: {cleanup_failures:?}"
        ),
        Ok(Err(join_error)) if join_error.is_panic() => {
            if !cleanup_failures.is_empty() {
                eprintln!(
                    "restricted lifecycle runtime-role cleanup failures after attempting every \
                     step: {cleanup_failures:?}"
                );
            }
            std::panic::resume_unwind(join_error.into_panic())
        }
        Ok(Err(join_error)) => panic!(
            "runtime role test task was cancelled: {join_error}; cleanup failures after \
             attempting every step: {cleanup_failures:?}"
        ),
        Ok(Ok(value)) if cleanup_failures.is_empty() => value,
        Ok(Ok(_)) => panic!(
            "restricted lifecycle runtime-role cleanup failed after attempting every step: \
             {cleanup_failures:?}"
        ),
    }
}

async fn exercise_lifecycle_runtime_role<T, F, Fut>(
    admin_pool: &PgPool,
    role: LifecycleRuntimeRole,
    password: &str,
    quoted_database: &str,
    test: F,
) -> Result<Result<T, tokio::task::JoinError>, String>
where
    T: Send + 'static,
    F: FnOnce(PgPool, LifecycleRuntimeRole) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let quoted = role.quoted();
    sqlx::query(&format!(
        "GRANT CONNECT ON DATABASE {quoted_database} TO {quoted}"
    ))
    .execute(admin_pool)
    .await
    .map_err(|error| format!("grant CONNECT failed: {error}"))?;
    sqlx::query(&format!("GRANT USAGE ON SCHEMA public TO {quoted}"))
        .execute(admin_pool)
        .await
        .map_err(|error| format!("grant schema USAGE failed: {error}"))?;
    sqlx::query(&format!(
        "GRANT SELECT ON TABLE external_effect_lifecycle_transitions TO {quoted}"
    ))
    .execute(admin_pool)
    .await
    .map_err(|error| format!("grant lifecycle SELECT failed: {error}"))?;

    let options = admin_pool
        .connect_options()
        .as_ref()
        .clone()
        .username(&role.name)
        .password(password);
    let runtime_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| format!("runtime-role pool connection failed: {error}"))?;
    let outcome = tokio::spawn(test(runtime_pool.clone(), role)).await;
    runtime_pool.close().await;
    Ok(outcome)
}

async fn cleanup_lifecycle_runtime_role(
    admin_pool: &PgPool,
    role: &LifecycleRuntimeRole,
    quoted_database: &str,
) -> Vec<String> {
    let quoted = role.quoted();
    let statements = [
        (
            "revoke lifecycle table privileges",
            format!(
                "REVOKE ALL PRIVILEGES ON TABLE external_effect_lifecycle_transitions FROM {quoted}"
            ),
        ),
        (
            "revoke schema usage",
            format!("REVOKE USAGE ON SCHEMA public FROM {quoted}"),
        ),
        (
            "revoke database connect",
            format!("REVOKE CONNECT ON DATABASE {quoted_database} FROM {quoted}"),
        ),
        ("drop role", format!("DROP ROLE {quoted}")),
    ];
    let mut failures = Vec::new();
    for (step, statement) in statements {
        if let Err(error) = sqlx::query(&statement).execute(admin_pool).await {
            failures.push(format!("{step}: {error}"));
        }
    }
    failures
}

fn at(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

/// A distinct run for a fixture effect to belong to.
///
/// These were labels — `"current"`, `"other"` — which is what `execution_ref`
/// accepted before it had to be a reference anything could follow.
fn run_ref() -> String {
    format!("run://{}", vestrace_domain::id::AgentRunId::new())
}

fn intent_for(workspace_id: WorkspaceId, execution_ref: &str) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        execution_ref,
        workspace_id,
        PrincipalId::new(),
        "webhook-v1",
        "send",
        "https://alpha.effects.test/hook",
        "sha256:arguments",
        "deliver notification",
        vec![EffectPrecondition::new("resource-version", "v1").unwrap()],
        "sha256:preconditions-v1",
        RiskCategory::Medium,
        EffectReversibility::Compensatable,
        IdempotencyProfile::ProviderKey,
        DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        Some("budget:reservation-1"),
        Some("policy:decision-1"),
        at(10),
    )
    .unwrap()
}

fn intent() -> ExternalEffectIntent {
    intent_for(
        WorkspaceId::new(),
        "run://01900000-0000-7000-8000-000000000001",
    )
}

/// The workspace an adapter call is scoped to.
///
/// Every method on this port takes one now. It used to take none, and read by
/// id alone across every tenant.
fn context_for(workspace_id: WorkspaceId) -> RequestContext {
    RequestContext::new(workspace_id, PrincipalId::new())
}

fn unknown_receipt(intent: &ExternalEffectIntent) -> ExternalEffectReceipt {
    ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        at(20),
        vec!["evidence:timeout".into()],
    )
    .unwrap()
}

fn receipt_with_status(
    intent: &ExternalEffectIntent,
    status: EffectLifecycleStatus,
    recorded_at: chrono::DateTime<chrono::Utc>,
) -> ExternalEffectReceipt {
    let receipt = ExternalEffectReceipt::synthetic_unknown(
        intent.id(),
        intent.adapter(),
        recorded_at,
        vec![format!("evidence:{status:?}")],
    )
    .unwrap();
    let mut payload = serde_json::to_value(receipt).unwrap();
    payload["outcome_status"] = serde_json::to_value(status).unwrap();
    payload["response_class"] = serde_json::json!(format!("{status:?}"));
    serde_json::from_value(payload).unwrap()
}

#[sqlx::test]
async fn lifecycle_migration_preserves_preexisting_evidence_without_inventing_a_status(
    pool: PgPool,
) {
    let migrations_before_lifecycle = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version < 153)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    migrations_before_lifecycle.run(&pool).await.unwrap();

    let intent = intent();
    let receipt = unknown_receipt(&intent);
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(intent.workspace_id().to_string())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_intents (id, workspace_id, adapter, payload) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(intent.id().as_uuid())
    .bind(intent.workspace_id().as_uuid())
    .bind(intent.adapter())
    .bind(serde_json::to_value(&intent).unwrap())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_receipts \
             (id, effect_id, workspace_id, outcome_status, payload) \
         VALUES ($1, $2, $3, 'unknown', $4)",
    )
    .bind(receipt.id().as_uuid())
    .bind(intent.id().as_uuid())
    .bind(intent.workspace_id().as_uuid())
    .bind(serde_json::to_value(&receipt).unwrap())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_reconciliations \
             (id, effect_id, receipt_id, workspace_id, outcome, evidence_strength, \
              reconciled_at, payload) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(reconciliation.id().as_uuid())
    .bind(intent.id().as_uuid())
    .bind(receipt.id().as_uuid())
    .bind(intent.workspace_id().as_uuid())
    .bind("confirmed")
    .bind("provider_idempotency_lookup")
    .bind(reconciliation.reconciled_at())
    .bind(serde_json::to_value(&reconciliation).unwrap())
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();

    let mut inspect_before = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(intent.workspace_id().to_string())
        .execute(&mut *inspect_before)
        .await
        .unwrap();
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_build_object( \
             'intent', (SELECT to_jsonb(i) FROM external_effect_intents i WHERE id = $1), \
             'receipt', (SELECT to_jsonb(r) FROM external_effect_receipts r WHERE id = $2), \
             'reconciliation', (SELECT to_jsonb(x) FROM external_reconciliations x WHERE id = $3) \
         )",
    )
    .bind(intent.id().as_uuid())
    .bind(receipt.id().as_uuid())
    .bind(reconciliation.id().as_uuid())
    .fetch_one(&mut *inspect_before)
    .await
    .unwrap();
    inspect_before.commit().await.unwrap();

    MIGRATOR.run(&pool).await.unwrap();

    let mut inspect_after = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(intent.workspace_id().to_string())
        .execute(&mut *inspect_after)
        .await
        .unwrap();
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_build_object( \
             'intent', (SELECT to_jsonb(i) FROM external_effect_intents i WHERE id = $1), \
             'receipt', (SELECT to_jsonb(r) FROM external_effect_receipts r WHERE id = $2), \
             'reconciliation', (SELECT to_jsonb(x) FROM external_reconciliations x WHERE id = $3) \
         )",
    )
    .bind(intent.id().as_uuid())
    .bind(receipt.id().as_uuid())
    .bind(reconciliation.id().as_uuid())
    .fetch_one(&mut *inspect_after)
    .await
    .unwrap();
    assert_eq!(after, before, "0153 modified pre-existing evidence");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM external_effect_lifecycle_transitions")
            .fetch_one(&mut *inspect_after)
            .await
            .unwrap(),
        0,
        "0153 backfilled lifecycle assertions nobody recorded"
    );
    inspect_after.commit().await.unwrap();

    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    assert_eq!(
        repository
            .find_lifecycle_status(&context_for(intent.workspace_id()), intent.id())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repository
            .find_receipt_by_effect(&context_for(intent.workspace_id()), intent.id())
            .await
            .unwrap(),
        Some(receipt),
        "an effect id could not reach its receipt persisted before 0153"
    );
}

#[sqlx::test]
async fn dispatch_deadline_migration_preserves_0153_rows_and_enforces_only_new_evidence(
    pool: PgPool,
) {
    let migrations_through_lifecycle = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 153)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    migrations_through_lifecycle.run(&pool).await.unwrap();

    let effect = intent();
    let legacy_transition_id = uuid::Uuid::now_v7();
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(effect.workspace_id().to_string())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_intents (id, workspace_id, adapter, payload) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(effect.id().as_uuid())
    .bind(effect.workspace_id().as_uuid())
    .bind(effect.adapter())
    .bind(serde_json::to_value(&effect).unwrap())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions \
             (id, effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
         VALUES ($1, $2, $3, 'dispatching', 'dispatch_started', $4, $5)",
    )
    .bind(legacy_transition_id)
    .bind(effect.id().as_uuid())
    .bind(effect.workspace_id().as_uuid())
    .bind(effect.id().to_string())
    .bind(at(20))
    .execute(&mut *transaction)
    .await
    .unwrap();
    let before = sqlx::query_as::<_, LegacyDispatchTransitionRow>(
        "SELECT id, effect_id, workspace_id, status, cause, cause_ref, recorded_at, created_at \
             FROM external_effect_lifecycle_transitions WHERE id = $1",
    )
    .bind(legacy_transition_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();

    MIGRATOR.run(&pool).await.unwrap();

    let after = sqlx::query_as::<_, MigratedDispatchTransitionRow>(
        "SELECT id, effect_id, workspace_id, status, cause, cause_ref, recorded_at, created_at, \
                dispatch_owner, dispatch_expires_at \
         FROM external_effect_lifecycle_transitions WHERE id = $1",
    )
    .bind(legacy_transition_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.effect_id, before.effect_id);
    assert_eq!(after.workspace_id, before.workspace_id);
    assert_eq!(after.status, before.status);
    assert_eq!(after.cause, before.cause);
    assert_eq!(after.cause_ref, before.cause_ref);
    assert_eq!(after.recorded_at, before.recorded_at);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(
        (after.dispatch_owner, after.dispatch_expires_at),
        (None, None),
        "0154 invented ownership evidence for a 0153-era dispatch"
    );
    assert!(
        !sqlx::query_scalar::<_, bool>(
            "SELECT convalidated FROM pg_constraint \
             WHERE conname = 'external_effect_lifecycle_dispatch_ownership_qualified'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "the compatibility CHECK was validated against 0153-era rows"
    );

    for (status, owner, expires_at) in [
        ("dispatching", None, None),
        ("dispatching", Some("worker-a"), None),
        ("dispatching", None, Some(at(30))),
        ("prepared", Some("worker-a"), None),
        ("prepared", None, Some(at(30))),
        ("prepared", Some("worker-a"), Some(at(30))),
    ] {
        let mut invalid = pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
            .bind(effect.workspace_id().to_string())
            .execute(&mut *invalid)
            .await
            .unwrap();
        let cause = if status == "dispatching" {
            "dispatch_started"
        } else {
            "intent_recorded"
        };
        let inserted = sqlx::query(
            "INSERT INTO external_effect_lifecycle_transitions \
                 (effect_id, workspace_id, status, cause, cause_ref, recorded_at, \
                  dispatch_owner, dispatch_expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(effect.id().as_uuid())
        .bind(effect.workspace_id().as_uuid())
        .bind(status)
        .bind(cause)
        .bind(uuid::Uuid::now_v7().to_string())
        .bind(at(25))
        .bind(owner)
        .bind(expires_at)
        .execute(&mut *invalid)
        .await;
        assert!(
            inserted.is_err(),
            "a new {status} transition accepted owner={owner:?}, expires_at={expires_at:?}"
        );
        invalid.rollback().await.unwrap();
    }

    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let context = context_for(effect.workspace_id());
    for cutoff in [at(0), at(20), at(10_000)] {
        assert!(
            repository
                .find_reconciliation_candidates(&context, at(10_000), at(10_000), cutoff, u32::MAX,)
                .await
                .unwrap()
                .is_empty(),
            "a 0153-era dispatch with no deadline was swept at {cutoff}"
        );
    }
    assert_eq!(
        repository
            .count_deadline_less_dispatching_transitions(&context)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        repository
            .count_deadline_less_dispatching_transitions(&context_for(WorkspaceId::new()))
            .await
            .unwrap(),
        0,
        "the exemption count crossed a workspace boundary"
    );
}

#[sqlx::test]
async fn lifecycle_ordinal_migration_orders_distinct_and_tied_rows_preserves_fields_and_advances_sequence(
    pool: PgPool,
) {
    let migrations_through_dispatch_deadlines = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 154)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    migrations_through_dispatch_deadlines
        .run(&pool)
        .await
        .unwrap();

    let effect = intent();
    let dispatch_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let receipt_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
    let prepared_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap();
    let distinct_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000004").unwrap();
    let receipt_ref = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000005").unwrap();
    let tied_created_at = at(100);
    let distinct_created_at = at(90);
    let mut stored_order = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(effect.workspace_id().to_string())
        .execute(&mut *stored_order)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_intents (id, workspace_id, adapter, payload) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(effect.id().as_uuid())
    .bind(effect.workspace_id().as_uuid())
    .bind(effect.adapter())
    .bind(serde_json::to_value(&effect).unwrap())
    .execute(&mut *stored_order)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions \
             (id, effect_id, workspace_id, status, cause, cause_ref, recorded_at, created_at, \
              dispatch_owner, dispatch_expires_at) \
         VALUES \
             ($1, $4, $5, 'prepared', 'intent_recorded', $6, $7, $10, NULL, NULL), \
             ($2, $4, $5, 'dispatching', 'dispatch_started', $6, $8, $10, $11, $12), \
             ($3, $4, $5, 'unknown', 'receipt_recorded', $13, $9, $10, NULL, NULL), \
             ($14, $4, $5, 'prepared', 'intent_recorded', $6, $15, $16, NULL, NULL)",
    )
    // The distinct timestamp row is inserted last but must sort first. Among
    // the three tied rows, VALUES order is prepared, dispatch, receipt while
    // fixed-id order is dispatch, receipt, prepared.
    .bind(prepared_id)
    .bind(dispatch_id)
    .bind(receipt_id)
    .bind(effect.id().as_uuid())
    .bind(effect.workspace_id().as_uuid())
    .bind(effect.id().to_string())
    .bind(at(10))
    .bind(at(30))
    .bind(at(20))
    .bind(tied_created_at)
    .bind("worker-fixed")
    .bind(at(40))
    .bind(receipt_ref.to_string())
    .bind(distinct_id)
    .bind(at(5))
    .bind(distinct_created_at)
    .execute(&mut *stored_order)
    .await
    .unwrap();
    stored_order.commit().await.unwrap();

    let before: Vec<LegacyOrdinalTransitionRow> = sqlx::query_as(
        "SELECT id, effect_id, workspace_id, status, cause, cause_ref, recorded_at, created_at, \
                dispatch_owner, dispatch_expires_at \
         FROM external_effect_lifecycle_transitions ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(before.len(), 4);
    assert_eq!(
        before
            .iter()
            .filter(|row| row.created_at == tied_created_at)
            .count(),
        3
    );
    assert_eq!(
        before
            .iter()
            .filter(|row| row.created_at == distinct_created_at)
            .count(),
        1
    );

    MIGRATOR.run(&pool).await.unwrap();

    let after: Vec<MigratedOrdinalTransitionRow> = sqlx::query_as(
        "SELECT id, effect_id, workspace_id, status, cause, cause_ref, recorded_at, created_at, \
                dispatch_owner, dispatch_expires_at, ordinal \
         FROM external_effect_lifecycle_transitions ORDER BY ordinal",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        after.iter().map(|row| row.ordinal).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        after.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![distinct_id, dispatch_id, receipt_id, prepared_id],
        "legacy rows did not use created_at first and the fixed-id fallback for ties"
    );
    let before_by_id = before
        .iter()
        .map(|row| (row.id, row))
        .collect::<std::collections::HashMap<_, _>>();
    for after in &after {
        let before = before_by_id
            .get(&after.id)
            .expect("migration introduced an unknown transition id");
        assert_eq!(before.id, after.id);
        assert_eq!(before.effect_id, after.effect_id);
        assert_eq!(before.workspace_id, after.workspace_id);
        assert_eq!(before.status, after.status);
        assert_eq!(before.cause, after.cause);
        assert_eq!(before.cause_ref, after.cause_ref);
        assert_eq!(before.recorded_at, after.recorded_at);
        assert_eq!(before.created_at, after.created_at);
        assert_eq!(before.dispatch_owner, after.dispatch_owner);
        assert_eq!(before.dispatch_expires_at, after.dispatch_expires_at);
    }

    let backfill_max = after
        .iter()
        .map(|row| row.ordinal)
        .max()
        .expect("backfill produced no ordinals");
    let post_migration_ordinal: i64 = sqlx::query_scalar(
        "INSERT INTO external_effect_lifecycle_transitions \
             (effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
         VALUES ($1, $2, 'prepared', 'intent_recorded', $3, $4) \
         RETURNING ordinal",
    )
    .bind(effect.id().as_uuid())
    .bind(effect.workspace_id().as_uuid())
    .bind(effect.id().to_string())
    .bind(at(200))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        post_migration_ordinal > backfill_max,
        "the ordinal sequence did not advance beyond the legacy backfill"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_effect_repository_round_trips_unknown_and_reconciliation_evidence(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let intent = intent();
    let receipt = unknown_receipt(&intent);
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();

    let context = context_for(intent.workspace_id());

    repository.insert_intent(&context, &intent).await.unwrap();
    repository.insert_intent(&context, &intent).await.unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();
    repository
        .insert_reconciliation(&context, &reconciliation)
        .await
        .unwrap();
    repository
        .insert_reconciliation(&context, &reconciliation)
        .await
        .unwrap();

    assert_eq!(
        repository.find_intent(&context, intent.id()).await.unwrap(),
        Some(intent)
    );
    assert_eq!(
        repository
            .find_receipt(&context, receipt.id())
            .await
            .unwrap(),
        Some(receipt)
    );
    assert_eq!(
        repository
            .find_reconciliation(&context, reconciliation.id())
            .await
            .unwrap(),
        Some(reconciliation)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn repository_operations_append_qualified_lifecycle_evidence_without_replay_regression(
    pool: PgPool,
) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool.clone()));
    let intent = intent();
    let context = context_for(intent.workspace_id());

    repository.insert_intent(&context, &intent).await.unwrap();
    let prepared: (String, String, String) = sqlx::query_as(
        "SELECT status, cause, cause_ref FROM external_effect_lifecycle_transitions \
         WHERE effect_id = $1 AND workspace_id = $2 ORDER BY ordinal DESC LIMIT 1",
    )
    .bind(intent.id().as_uuid())
    .bind(intent.workspace_id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        prepared,
        (
            "prepared".into(),
            "intent_recorded".into(),
            intent.id().to_string()
        )
    );

    repository
        .record_dispatch_started(&context, intent.id(), WorkerId::new(), at(315), at(15))
        .await
        .unwrap();
    repository.insert_intent(&context, &intent).await.unwrap();
    assert_eq!(
        repository
            .find_lifecycle_status(&context, intent.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Dispatching),
        "replaying the immutable intent must not reset its lifecycle"
    );

    let receipt = unknown_receipt(&intent);
    repository.insert_receipt(&context, &receipt).await.unwrap();
    let receipt_transition: (String, String, String) = sqlx::query_as(
        "SELECT status, cause, cause_ref FROM external_effect_lifecycle_transitions \
         WHERE effect_id = $1 AND workspace_id = $2 ORDER BY ordinal DESC LIMIT 1",
    )
    .bind(intent.id().as_uuid())
    .bind(intent.workspace_id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        receipt_transition,
        (
            "unknown".into(),
            "receipt_recorded".into(),
            receipt.id().to_string()
        )
    );

    repository
        .record_dispatch_started(&context, intent.id(), WorkerId::new(), at(325), at(25))
        .await
        .unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_lifecycle_transitions WHERE effect_id = $1 AND workspace_id = $2"
        )
        .bind(intent.id().as_uuid())
        .bind(intent.workspace_id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        4,
        "a receipt replay appended a lifecycle transition"
    );
    assert_eq!(
        repository
            .find_lifecycle_status(&context, intent.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Dispatching),
        "an older receipt replay regressed a later lifecycle transition"
    );
    let mut conflicting_payload = serde_json::to_value(&receipt).unwrap();
    conflicting_payload["response_class"] = serde_json::json!("different");
    let conflicting: ExternalEffectReceipt = serde_json::from_value(conflicting_payload).unwrap();
    assert!(matches!(
        repository.insert_receipt(&context, &conflicting).await,
        Err(ApplicationError::Conflict(_))
    ));
    assert_eq!(
        repository
            .find_receipt_by_effect(&context, intent.id())
            .await
            .unwrap(),
        Some(receipt),
        "an effect id did not reach its persisted receipt"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn missing_and_foreign_dispatch_start_fail_without_disclosing_which(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool.clone()));
    let effect = intent();
    let owner = context_for(effect.workspace_id());
    repository.insert_intent(&owner, &effect).await.unwrap();

    let stranger = context_for(WorkspaceId::new());
    let foreign = repository
        .record_dispatch_started(&stranger, effect.id(), WorkerId::new(), at(315), at(15))
        .await
        .unwrap_err()
        .to_string();
    let missing = repository
        .record_dispatch_started(
            &stranger,
            vestrace_domain::ExternalEffectId::new(),
            WorkerId::new(),
            at(315),
            at(15),
        )
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(foreign, missing);
    assert_eq!(
        repository
            .find_lifecycle_status(&stranger, effect.id())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_lifecycle_transitions WHERE effect_id = $1"
        )
        .bind(effect.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1,
        "a rejected dispatch start appended evidence"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn restricted_runtime_role_sees_own_lifecycle_and_not_a_foreign_transition(pool: PgPool) {
    let admin_repository = PgExternalEffectRepository::new(PgStore::from_pool(pool.clone()));
    let own = intent();
    let foreign = intent();
    let own_context = context_for(own.workspace_id());
    let foreign_context = context_for(foreign.workspace_id());
    admin_repository
        .insert_intent(&own_context, &own)
        .await
        .unwrap();
    admin_repository
        .insert_intent(&foreign_context, &foreign)
        .await
        .unwrap();

    let own_id = own.id();
    let foreign_id = foreign.id();
    with_lifecycle_runtime_role(&pool, move |runtime_pool, role| async move {
        let flags: (bool, bool) = sqlx::query_as(
            "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
        )
        .fetch_one(&runtime_pool)
        .await
        .unwrap();
        assert_eq!(flags, (false, false), "{} can bypass RLS", role.name);

        let repository = PgExternalEffectRepository::new(PgStore::from_pool(runtime_pool));
        assert_eq!(
            repository
                .find_lifecycle_status(&own_context, own_id)
                .await
                .unwrap(),
            Some(EffectLifecycleStatus::Prepared)
        );
        assert_eq!(
            repository
                .find_lifecycle_status(&own_context, foreign_id)
                .await
                .unwrap(),
            None
        );
    })
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_lifecycle_writers_append_without_losing_a_transition(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool.clone()));
    let effect = intent();
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();

    let first = repository.clone();
    let second = repository.clone();
    let first_context = context.clone();
    let second_context = context.clone();
    let effect_id = effect.id();
    let (first_result, second_result) = tokio::join!(
        first.record_dispatch_started(&first_context, effect_id, WorkerId::new(), at(315), at(15),),
        second.record_dispatch_started(
            &second_context,
            effect_id,
            WorkerId::new(),
            at(316),
            at(16),
        ),
    );
    first_result.unwrap();
    second_result.unwrap();

    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_lifecycle_transitions \
             WHERE effect_id = $1 AND workspace_id = $2"
        )
        .bind(effect.id().as_uuid())
        .bind(effect.workspace_id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        3
    );
    assert_eq!(
        repository
            .find_lifecycle_status(&context, effect.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Dispatching)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_concurrent_recovery_candidates_adopt_and_reconcile_exactly_once(pool: PgPool) {
    let repository = Arc::new(PgExternalEffectRepository::new(PgStore::from_pool(
        pool.clone(),
    )));
    let effect = intent();
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    let dispatch_transition_id = repository
        .record_dispatch_started(&context, effect.id(), WorkerId::new(), at(21), at(20))
        .await
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let first = ExternalEffectRecoveryService::new(
        repository.clone(),
        ExternalEffectReadBackRegistry::new([(
            "webhook-v1".to_owned(),
            read_back_descriptor("webhook-v1"),
            Arc::new(CountingReadBack {
                calls: Arc::clone(&calls),
            }) as Arc<dyn ExternalEffectReadBackAdapter>,
        )])
        .unwrap(),
    );
    let second = ExternalEffectRecoveryService::new(
        repository.clone(),
        ExternalEffectReadBackRegistry::new([(
            "webhook-v1".to_owned(),
            read_back_descriptor("webhook-v1"),
            Arc::new(CountingReadBack {
                calls: Arc::clone(&calls),
            }) as Arc<dyn ExternalEffectReadBackAdapter>,
        )])
        .unwrap(),
    );

    // Both sweepers hold the same pre-adoption candidate. The unique
    // dispatch-lost key, not a sequential re-query, decides which may ask.
    let (first_candidates, second_candidates) = tokio::join!(
        first.discover(&context, at(10), at(10), at(22), RECONCILIATION_BATCH),
        second.discover(&context, at(10), at(10), at(22), RECONCILIATION_BATCH),
    );
    let first_candidate = first_candidates.unwrap().pop().unwrap();
    let second_candidate = second_candidates.unwrap().pop().unwrap();
    assert_eq!(
        first_candidate.dispatch_transition_id(),
        Some(dispatch_transition_id)
    );
    assert_eq!(
        second_candidate.dispatch_transition_id(),
        Some(dispatch_transition_id)
    );

    let (first_result, second_result) = tokio::join!(
        first.reconcile_candidate(&context, &first_candidate, at(30)),
        second.reconcile_candidate(&context, &second_candidate, at(30)),
    );
    let reconciliations = [first_result.unwrap(), second_result.unwrap()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    assert_eq!(reconciliations.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_lifecycle_transitions \
             WHERE workspace_id = $1 AND effect_id = $2 AND cause = 'dispatch_lost'"
        )
        .bind(effect.workspace_id().as_uuid())
        .bind(effect.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_reconciliations \
             WHERE workspace_id = $1 AND effect_id = $2"
        )
        .bind(effect.workspace_id().as_uuid())
        .bind(effect.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn missing_route_after_adoption_backs_off_without_duplicate_adoption_evidence(pool: PgPool) {
    let repository = Arc::new(PgExternalEffectRepository::new(PgStore::from_pool(
        pool.clone(),
    )));
    let effect = intent();
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    repository
        .record_dispatch_started(&context, effect.id(), WorkerId::new(), at(21), at(20))
        .await
        .unwrap();
    let service = ExternalEffectRecoveryService::new(
        repository.clone(),
        ExternalEffectReadBackRegistry::new([]).unwrap(),
    );

    let first = service.sweep(&context, at(30)).await.unwrap();
    let second = service.sweep(&context, at(31)).await.unwrap();
    let retry = service.sweep(&context, at(91)).await.unwrap();

    assert_eq!(first.unreachable().len(), 1);
    assert!(second.unreachable().is_empty());
    assert_eq!(retry.unreachable().len(), 1);
    assert_eq!(first.unreachable()[0].effect_id, effect.id());
    assert_eq!(retry.unreachable()[0].effect_id, effect.id());
    assert!(first.reconciliations().is_empty());
    assert!(second.reconciliations().is_empty());
    assert_eq!(
        repository
            .find_lifecycle_status(&context, effect.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Unknown)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_lifecycle_transitions \
             WHERE workspace_id = $1 AND effect_id = $2 AND cause = 'dispatch_lost'"
        )
        .bind(effect.workspace_id().as_uuid())
        .bind(effect.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_recovery_attempts \
             WHERE workspace_id = $1 AND effect_id = $2"
        )
        .bind(effect.workspace_id().as_uuid())
        .bind(effect.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_reconciliations \
             WHERE workspace_id = $1 AND effect_id = $2"
        )
        .bind(effect.workspace_id().as_uuid())
        .bind(effect.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_failure_after_adoption_backs_off_without_duplicate_adoption_evidence(
    pool: PgPool,
) {
    let repository = Arc::new(PgExternalEffectRepository::new(PgStore::from_pool(
        pool.clone(),
    )));
    let effect = intent();
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    repository
        .record_dispatch_started(&context, effect.id(), WorkerId::new(), at(21), at(20))
        .await
        .unwrap();
    let service = ExternalEffectRecoveryService::new(
        repository.clone(),
        ExternalEffectReadBackRegistry::new([(
            "webhook-v1".to_owned(),
            read_back_descriptor("webhook-v1"),
            Arc::new(FailingReadBack) as Arc<dyn ExternalEffectReadBackAdapter>,
        )])
        .unwrap(),
    );

    let first = service.sweep(&context, at(30)).await.unwrap();
    let second = service.sweep(&context, at(31)).await.unwrap();
    let retry = service.sweep(&context, at(91)).await.unwrap();

    assert_eq!(first.unreachable().len(), 1);
    assert!(second.unreachable().is_empty());
    assert_eq!(retry.unreachable().len(), 1);
    assert_eq!(first.unreachable()[0].effect_id, effect.id());
    assert_eq!(retry.unreachable()[0].effect_id, effect.id());
    assert!(first.reconciliations().is_empty());
    assert!(second.reconciliations().is_empty());
    assert_eq!(
        repository
            .find_lifecycle_status(&context, effect.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Unknown)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_lifecycle_transitions \
             WHERE workspace_id = $1 AND effect_id = $2 AND cause = 'dispatch_lost'"
        )
        .bind(effect.workspace_id().as_uuid())
        .bind(effect.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_recovery_attempts \
             WHERE workspace_id = $1 AND effect_id = $2"
        )
        .bind(effect.workspace_id().as_uuid())
        .bind(effect.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_reconciliations \
             WHERE workspace_id = $1 AND effect_id = $2"
        )
        .bind(effect.workspace_id().as_uuid())
        .bind(effect.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn receipt_inserted_after_adoption_wins_over_recorded_time_and_retires_candidate(
    pool: PgPool,
) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let effect = intent();
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    let dispatch_transition_id = repository
        .record_dispatch_started(&context, effect.id(), WorkerId::new(), at(21), at(20))
        .await
        .unwrap();
    repository
        .adopt_lost_dispatch(&context, effect.id(), dispatch_transition_id, at(30))
        .await
        .unwrap();

    let receipt = receipt_with_status(&effect, EffectLifecycleStatus::Acknowledged, at(15));
    repository.insert_receipt(&context, &receipt).await.unwrap();

    assert_eq!(
        repository
            .find_lifecycle_status(&context, effect.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Acknowledged),
        "recorded_at incorrectly outranked a real receipt"
    );
    let candidates = repository
        .find_reconciliation_candidates(&context, at(100), at(100), at(100), u32::MAX)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].receipt(), Some(&receipt));
}

#[sqlx::test(migrations = "../../migrations")]
async fn lower_ordinal_receipt_still_outranks_a_later_dispatch_lost_guess(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool.clone()));
    let effect = intent();
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    let dispatch_transition_id = repository
        .record_dispatch_started(&context, effect.id(), WorkerId::new(), at(21), at(20))
        .await
        .unwrap();
    let receipt = receipt_with_status(&effect, EffectLifecycleStatus::Acknowledged, at(15));

    // Allocate the receipt transition's ordinal first but keep it invisible.
    // Adoption then allocates and commits a higher ordinal before this receipt
    // commits, reproducing the adverse BIGSERIAL/visibility ordering.
    let mut late_receipt = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(effect.workspace_id().to_string())
        .execute(&mut *late_receipt)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_receipts \
             (id, effect_id, workspace_id, outcome_status, payload) \
         VALUES ($1, $2, $3, 'acknowledged', $4)",
    )
    .bind(receipt.id().as_uuid())
    .bind(effect.id().as_uuid())
    .bind(effect.workspace_id().as_uuid())
    .bind(serde_json::to_value(&receipt).unwrap())
    .execute(&mut *late_receipt)
    .await
    .unwrap();
    let receipt_ordinal: i64 = sqlx::query_scalar(
        "INSERT INTO external_effect_lifecycle_transitions \
             (effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
         VALUES ($1, $2, 'acknowledged', 'receipt_recorded', $3, $4) \
         RETURNING ordinal",
    )
    .bind(effect.id().as_uuid())
    .bind(effect.workspace_id().as_uuid())
    .bind(receipt.id().to_string())
    .bind(receipt.recorded_at())
    .fetch_one(&mut *late_receipt)
    .await
    .unwrap();

    repository
        .adopt_lost_dispatch(&context, effect.id(), dispatch_transition_id, at(30))
        .await
        .unwrap();
    let adoption_ordinal: i64 = sqlx::query_scalar(
        "SELECT ordinal FROM external_effect_lifecycle_transitions \
         WHERE workspace_id = $1 AND effect_id = $2 AND cause = 'dispatch_lost'",
    )
    .bind(effect.workspace_id().as_uuid())
    .bind(effect.id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(receipt_ordinal < adoption_ordinal);
    late_receipt.commit().await.unwrap();

    assert_eq!(
        repository
            .find_lifecycle_status(&context, effect.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Acknowledged)
    );
    // This asserted the candidate set was empty until §34 stopped an
    // acknowledged receipt from counting as a settled outcome. The effect is a
    // candidate again — but through the *receipt* branch, which is what this
    // test is about: the late real receipt retired the lost-dispatch guess, so
    // the candidate carries a receipt rather than arriving from the
    // receipt-less path the adoption had put it on.
    let candidates = repository
        .find_reconciliation_candidates(&context, at(100), at(100), at(100), u32::MAX)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].receipt().is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn equal_age_dispatches_are_selected_by_deadline_and_keep_their_distinct_owners(
    pool: PgPool,
) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool.clone()));
    let workspace_id = WorkspaceId::new();
    let context = context_for(workspace_id);
    let expired = intent_for(workspace_id, &run_ref());
    let live = intent_for(workspace_id, &run_ref());
    let expired_owner = WorkerId::new();
    let live_owner = WorkerId::new();
    for effect in [&expired, &live] {
        repository.insert_intent(&context, effect).await.unwrap();
    }
    repository
        .record_dispatch_started(&context, expired.id(), expired_owner, at(19), at(20))
        .await
        .unwrap();
    repository
        .record_dispatch_started(&context, live.id(), live_owner, at(21), at(20))
        .await
        .unwrap();

    let candidates = repository
        .find_reconciliation_candidates(&context, at(10_000), at(10_000), at(20), u32::MAX)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].intent(), &expired);
    assert_eq!(candidates[0].receipt(), None);

    let mut stored: Vec<(uuid::Uuid, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT effect_id, dispatch_owner, dispatch_expires_at \
         FROM external_effect_lifecycle_transitions \
         WHERE workspace_id = $1 AND status = 'dispatching' ORDER BY effect_id",
    )
    .bind(workspace_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    stored.sort_by_key(|row| row.0);
    let mut expected = vec![
        (expired.id().as_uuid(), expired_owner.to_string(), at(19)),
        (live.id().as_uuid(), live_owner.to_string(), at(21)),
    ];
    expected.sort_by_key(|row| row.0);
    assert_eq!(stored, expected);

    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions \
             (effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
         VALUES ($1, $2, 'prepared', 'intent_recorded', $3, $4)",
    )
    .bind(expired.id().as_uuid())
    .bind(expired.workspace_id().as_uuid())
    .bind(expired.id().to_string())
    .bind(at(5))
    .execute(&pool)
    .await
    .unwrap();
    let later_candidates = repository
        .find_reconciliation_candidates(&context, at(10_000), at(10_000), at(100), u32::MAX)
        .await
        .unwrap();
    assert_eq!(
        later_candidates.len(),
        1,
        "the newer lifecycle transition did not retire only its own dispatch"
    );
    assert_eq!(
        later_candidates[0].intent(),
        &live,
        "a later ordinal with adverse lower recorded_at did not retire the older dispatch"
    );

    let reconciliation = reconcile_effect(
        &expired,
        None,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    assert_eq!(reconciliation.receipt_id(), None);
    repository
        .insert_reconciliation(&context, &reconciliation)
        .await
        .unwrap();
    assert_eq!(
        repository
            .find_lifecycle_status(&context, expired.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Reconciling)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_unsettled_receipt_is_a_candidate_across_mixed_states_and_insertion_orders(
    pool: PgPool,
) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let workspace_id = WorkspaceId::new();
    let context = context_for(workspace_id);
    let first_effect = intent_for(workspace_id, &run_ref());
    let second_effect = intent_for(workspace_id, &run_ref());
    for effect in [&first_effect, &second_effect] {
        repository.insert_intent(&context, effect).await.unwrap();
    }
    let first_unknown = receipt_with_status(&first_effect, EffectLifecycleStatus::Unknown, at(20));
    let first_acknowledged =
        receipt_with_status(&first_effect, EffectLifecycleStatus::Acknowledged, at(30));
    repository
        .insert_receipt(&context, &first_unknown)
        .await
        .unwrap();
    repository
        .insert_receipt(&context, &first_acknowledged)
        .await
        .unwrap();
    let second_acknowledged =
        receipt_with_status(&second_effect, EffectLifecycleStatus::Acknowledged, at(30));
    let second_unknown =
        receipt_with_status(&second_effect, EffectLifecycleStatus::Unknown, at(20));
    repository
        .insert_receipt(&context, &second_acknowledged)
        .await
        .unwrap();
    repository
        .insert_receipt(&context, &second_unknown)
        .await
        .unwrap();

    let mut actual = repository
        .find_reconciliation_candidates(&context, at(100), at(100), at(100), u32::MAX)
        .await
        .unwrap()
        .into_iter()
        .map(|candidate| candidate.receipt().unwrap().id().to_string())
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = vec![
        first_unknown.id().to_string(),
        first_acknowledged.id().to_string(),
        second_acknowledged.id().to_string(),
        second_unknown.id().to_string(),
    ];
    expected.sort();
    assert_eq!(actual, expected);
}

/// Mutant caught: widening only the in-memory double or only the service leaves
/// acknowledged evidence permanently outside the PostgreSQL sweep, despite the
/// provider having merely accepted the request rather than confirmed its effect.
#[sqlx::test(migrations = "../../migrations")]
async fn acknowledged_receipts_follow_the_existing_unsettled_and_settled_candidate_rules(
    pool: PgPool,
) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let effect = intent();
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    let receipt = receipt_with_status(&effect, EffectLifecycleStatus::Acknowledged, at(20));
    repository.insert_receipt(&context, &receipt).await.unwrap();

    assert_eq!(
        repository
            .find_reconciliation_candidates(&context, at(100), at(100), at(100), u32::MAX)
            .await
            .unwrap()
            .iter()
            .map(|candidate| candidate.receipt())
            .collect::<Vec<_>>(),
        vec![Some(&receipt)]
    );

    let inconclusive = reconcile_effect(
        &effect,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            None,
            "external:inconclusive",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    repository
        .insert_reconciliation(&context, &inconclusive)
        .await
        .unwrap();
    assert!(
        repository
            .find_reconciliation_candidates(&context, at(30), at(100), at(100), u32::MAX)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repository
            .find_reconciliation_candidates(&context, at(31), at(100), at(100), u32::MAX)
            .await
            .unwrap()
            .len(),
        1
    );

    let settled = reconcile_effect(
        &effect,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:confirmed",
            vec!["evidence:provider-lookup".into()],
        )],
        at(32),
    )
    .unwrap();
    repository
        .insert_reconciliation(&context, &settled)
        .await
        .unwrap();
    assert!(
        repository
            .find_reconciliation_candidates(&context, at(100), at(100), at(100), u32::MAX)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn multiple_unknown_receipts_for_one_effect_are_distinct_candidates(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let effect = intent();
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    let first = receipt_with_status(&effect, EffectLifecycleStatus::Unknown, at(20));
    let second = receipt_with_status(&effect, EffectLifecycleStatus::Unknown, at(30));
    repository.insert_receipt(&context, &first).await.unwrap();
    repository.insert_receipt(&context, &second).await.unwrap();

    let mut actual = repository
        .find_reconciliation_candidates(&context, at(100), at(100), at(100), u32::MAX)
        .await
        .unwrap()
        .into_iter()
        .map(|candidate| candidate.receipt().unwrap().id().to_string())
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = vec![first.id().to_string(), second.id().to_string()];
    expected.sort();
    assert_eq!(actual, expected);
}

#[sqlx::test(migrations = "../../migrations")]
async fn bounded_recovery_sweeps_take_the_oldest_batch_then_drain_the_backlog(pool: PgPool) {
    let repository = Arc::new(PgExternalEffectRepository::new(PgStore::from_pool(
        pool.clone(),
    )));
    let workspace_id = WorkspaceId::new();
    let context = context_for(workspace_id);
    let mut effects = Vec::new();
    for candidate_at in [at(30), at(10), at(20)] {
        let effect = intent_for(workspace_id, &run_ref());
        let receipt = unknown_receipt(&effect);
        repository.insert_intent(&context, &effect).await.unwrap();
        repository.insert_receipt(&context, &receipt).await.unwrap();
        sqlx::query(
            "UPDATE external_effect_receipts SET created_at = $3 \
             WHERE workspace_id = $1 AND id = $2",
        )
        .bind(workspace_id.as_uuid())
        .bind(receipt.id().as_uuid())
        .bind(candidate_at)
        .execute(&pool)
        .await
        .unwrap();
        effects.push((candidate_at, effect));
    }
    effects.sort_by_key(|(candidate_at, _)| *candidate_at);

    let service = ExternalEffectRecoveryService::new(
        repository,
        ExternalEffectReadBackRegistry::new([(
            "webhook-v1".to_owned(),
            read_back_descriptor("webhook-v1"),
            Arc::new(CountingReadBack {
                calls: Arc::new(AtomicUsize::new(0)),
            }) as Arc<dyn ExternalEffectReadBackAdapter>,
        )])
        .unwrap(),
    );

    let first = service
        .run(&context, at(100), at(100), at(100), at(100), 2)
        .await
        .unwrap();
    let second = service
        .run(&context, at(101), at(101), at(101), at(101), 2)
        .await
        .unwrap();

    assert_eq!(first.reconciliations().len(), 2);
    assert_eq!(first.reconciliations()[0].effect_id(), effects[0].1.id());
    assert_eq!(first.reconciliations()[1].effect_id(), effects[1].1.id());
    assert!(first.saturated());
    assert_eq!(second.reconciliations().len(), 1);
    assert_eq!(second.reconciliations()[0].effect_id(), effects[2].1.id());
    assert!(!second.saturated());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_recovery_attempt_is_durable_not_a_reconciliation_and_backs_off(pool: PgPool) {
    let repository = Arc::new(PgExternalEffectRepository::new(PgStore::from_pool(
        pool.clone(),
    )));
    let effect = intent();
    let receipt = unknown_receipt(&effect);
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();
    let service = ExternalEffectRecoveryService::new(
        repository,
        ExternalEffectReadBackRegistry::new([]).unwrap(),
    );

    let first = service
        .run(
            &context,
            at(30),
            at(30),
            at(0),
            at(30),
            RECONCILIATION_BATCH,
        )
        .await
        .unwrap();
    let second = service
        .run(
            &context,
            at(31),
            at(31),
            at(0),
            at(31),
            RECONCILIATION_BATCH,
        )
        .await
        .unwrap();

    assert_eq!(first.unreachable().len(), 1);
    assert_eq!(first.unreachable()[0].effect_id, effect.id());
    assert!(second.unreachable().is_empty());
    assert!(second.reconciliations().is_empty());
    let stored: (uuid::Uuid, uuid::Uuid, chrono::DateTime<Utc>, String) = sqlx::query_as(
        "SELECT effect_id, workspace_id, attempted_at, failure_reason \
         FROM external_effect_recovery_attempts \
         WHERE effect_id = $1 AND workspace_id = $2",
    )
    .bind(effect.id().as_uuid())
    .bind(effect.workspace_id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored.0, effect.id().as_uuid());
    assert_eq!(stored.1, effect.workspace_id().as_uuid());
    assert_eq!(stored.2, at(30));
    assert!(stored.3.contains("webhook-v1"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_reconciliations \
             WHERE effect_id = $1 AND workspace_id = $2",
        )
        .bind(effect.id().as_uuid())
        .bind(effect.workspace_id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );

    let after_cutoff = service
        .run(
            &context,
            at(91),
            at(91),
            at(31),
            at(91),
            RECONCILIATION_BATCH,
        )
        .await
        .unwrap();
    assert_eq!(after_cutoff.unreachable().len(), 1);
    assert_eq!(after_cutoff.unreachable()[0].effect_id, effect.id());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_provider_failure_then_success_reconciles_once_without_another_failed_attempt_delay(
    pool: PgPool,
) {
    let repository = Arc::new(PgExternalEffectRepository::new(PgStore::from_pool(
        pool.clone(),
    )));
    let effect = intent();
    let receipt = unknown_receipt(&effect);
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let service = ExternalEffectRecoveryService::new(
        repository,
        ExternalEffectReadBackRegistry::new([(
            "webhook-v1".to_owned(),
            read_back_descriptor("webhook-v1"),
            Arc::new(FailOnceReadBack {
                calls: Arc::clone(&calls),
            }) as Arc<dyn ExternalEffectReadBackAdapter>,
        )])
        .unwrap(),
    );

    let failed = service.sweep(&context, at(100)).await.unwrap();
    let backing_off = service.sweep(&context, at(101)).await.unwrap();
    let recovered = service.sweep(&context, at(161)).await.unwrap();
    let settled = service.sweep(&context, at(162)).await.unwrap();

    assert_eq!(failed.unreachable().len(), 1);
    assert!(backing_off.unreachable().is_empty());
    assert!(backing_off.reconciliations().is_empty());
    assert_eq!(recovered.reconciliations().len(), 1);
    assert_eq!(recovered.reconciliations()[0].effect_id(), effect.id());
    assert!(settled.unreachable().is_empty());
    assert!(settled.reconciliations().is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_recovery_attempts \
             WHERE effect_id = $1 AND workspace_id = $2",
        )
        .bind(effect.id().as_uuid())
        .bind(effect.workspace_id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_reconciliations \
             WHERE effect_id = $1 AND workspace_id = $2",
        )
        .bind(effect.id().as_uuid())
        .bind(effect.workspace_id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn receipt_lookup_follows_latest_receipt_transition_not_adverse_recorded_time(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let effect = intent();
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    let recorded_later_but_inserted_first =
        receipt_with_status(&effect, EffectLifecycleStatus::Unknown, at(30));
    let recorded_earlier_but_inserted_last =
        receipt_with_status(&effect, EffectLifecycleStatus::Acknowledged, at(20));
    repository
        .insert_receipt(&context, &recorded_later_but_inserted_first)
        .await
        .unwrap();
    repository
        .insert_receipt(&context, &recorded_earlier_but_inserted_last)
        .await
        .unwrap();

    assert_eq!(
        repository
            .find_receipt_by_effect(&context, effect.id())
            .await
            .unwrap(),
        Some(recorded_earlier_but_inserted_last),
        "receipt lookup followed recorded time instead of the latest receipt transition ordinal"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn lifecycle_status_storage_names_exactly_match_the_database_check(pool: PgPool) {
    let mut stored = sqlx::query_scalar::<_, Option<Vec<String>>>(
        "SELECT array_agg(matches[1] ORDER BY matches[1])
         FROM pg_constraint constraint_row
         CROSS JOIN LATERAL regexp_matches(
             pg_get_constraintdef(constraint_row.oid),
             $regex$'([^']+)'$regex$,
             'g'
         ) matches
         WHERE constraint_row.conname = 'external_effect_lifecycle_status_known'",
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .expect("the lifecycle status CHECK exists");
    stored.dedup();
    let mut domain = EffectLifecycleStatus::all_names()
        .iter()
        .map(|status| (*status).to_owned())
        .collect::<Vec<_>>();
    domain.sort();
    assert_eq!(stored, domain);
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_effect_evidence_is_not_readable_from_another_workspace(pool: PgPool) {
    // The three reads took an id and nothing else, so any tenant's id returned
    // that tenant's row — and a receipt carries the external resource id and
    // the provider's response digest, which is the effect itself rather than
    // metadata about it.
    //
    // This runs as a superuser, like every other `sqlx::test` here, so the
    // policy is bypassed and only the query's own predicate is under test. That
    // is deliberate: it is the half that has to hold when a caller reaches the
    // database through some future path that does not scope.
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let intent = intent();
    let receipt = unknown_receipt(&intent);
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();

    let owner = context_for(intent.workspace_id());
    repository.insert_intent(&owner, &intent).await.unwrap();
    repository.insert_receipt(&owner, &receipt).await.unwrap();
    repository
        .insert_reconciliation(&owner, &reconciliation)
        .await
        .unwrap();

    let stranger = context_for(WorkspaceId::new());
    assert_eq!(
        repository
            .find_intent(&stranger, intent.id())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repository
            .find_receipt(&stranger, receipt.id())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repository
            .find_reconciliation(&stranger, reconciliation.id())
            .await
            .unwrap(),
        None
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_receipt_cannot_be_filed_against_another_workspaces_effect(pool: PgPool) {
    // A receipt carries no workspace of its own — it belongs to an effect, and
    // the effect belongs to a tenant. So the adapter binds the workspace from
    // the request context, and nothing it holds could detect a mismatch.
    //
    // The composite foreign key onto `(id, workspace_id)` of the intent is what
    // makes that safe: the pair has to exist. This assertion holds under a
    // superuser too, because a foreign key is not a policy.
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let intent = intent();
    let receipt = unknown_receipt(&intent);

    let owner = context_for(intent.workspace_id());
    repository.insert_intent(&owner, &intent).await.unwrap();

    let stranger = context_for(WorkspaceId::new());
    assert!(
        repository
            .insert_receipt(&stranger, &receipt)
            .await
            .is_err(),
        "a receipt was filed against an effect in another workspace"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_recovery_attempt_cannot_be_filed_against_another_workspaces_effect(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool.clone()));
    let effect = intent();
    let owner = context_for(effect.workspace_id());
    repository.insert_intent(&owner, &effect).await.unwrap();
    let stranger = context_for(WorkspaceId::new());

    assert!(
        repository
            .record_failed_recovery_attempt(
                &stranger,
                effect.id(),
                at(30),
                "provider read-back failed",
            )
            .await
            .is_err(),
        "a failed attempt was filed through a foreign request context"
    );
    assert!(
        sqlx::query(
            "INSERT INTO external_effect_recovery_attempts \
                 (effect_id, workspace_id, attempted_at, failure_reason) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(effect.id().as_uuid())
        .bind(stranger.workspace_id.as_uuid())
        .bind(at(30))
        .bind("provider read-back failed")
        .execute(&pool)
        .await
        .is_err(),
        "the composite intent foreign key accepted a foreign workspace"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_recovery_attempts WHERE effect_id = $1"
        )
        .bind(effect.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_effect_repository_rejects_conflicting_immutable_intent_id(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let first = intent();
    let mut payload = serde_json::to_value(&first).unwrap();
    payload["adapter"] = serde_json::json!("other-adapter");
    let conflicting: ExternalEffectIntent = serde_json::from_value(payload).unwrap();
    let context = context_for(first.workspace_id());

    repository.insert_intent(&context, &first).await.unwrap();
    assert!(matches!(
        repository.insert_intent(&context, &conflicting).await,
        Err(ApplicationError::Conflict(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn external_effect_repository_discovers_only_unreconciled_unknown_effects_in_workspace(
    pool: PgPool,
) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let workspace_id = WorkspaceId::new();
    let current = intent_for(workspace_id, &run_ref());
    let other = intent_for(WorkspaceId::new(), &run_ref());
    let current_receipt = unknown_receipt(&current);
    let other_receipt = unknown_receipt(&other);

    let context = context_for(workspace_id);
    let other_context = context_for(other.workspace_id());

    repository.insert_intent(&context, &current).await.unwrap();
    repository
        .insert_receipt(&context, &current_receipt)
        .await
        .unwrap();
    repository
        .insert_intent(&other_context, &other)
        .await
        .unwrap();
    repository
        .insert_receipt(&other_context, &other_receipt)
        .await
        .unwrap();

    let candidates = repository
        .find_reconciliation_candidates(&context, at(10_000), at(10_000), at(10_000), u32::MAX)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].intent(), &current);
    assert_eq!(candidates[0].receipt(), Some(&current_receipt));

    let reconciliation = reconcile_effect(
        &current,
        &current_receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(false),
            "external:not-found",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    repository
        .insert_reconciliation(&context, &reconciliation)
        .await
        .unwrap();

    assert!(
        repository
            .find_reconciliation_candidates(&context, at(10_000), at(10_000), at(10_000), u32::MAX,)
            .await
            .unwrap()
            .is_empty()
    );
}

/// An answer retires an effect from the sweep. A non-answer must not.
///
/// The discovery query dropped an effect as soon as *any* reconciliation row
/// existed for it, so "we asked the provider and it could not tell us" retired
/// the effect permanently: the receipt stayed `unknown` and nothing ever asked
/// again. This is the difference between a settled outcome and a recorded
/// attempt.
#[sqlx::test(migrations = "../../migrations")]
async fn an_inconclusive_answer_does_not_retire_an_effect_from_the_sweep(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let workspace_id = WorkspaceId::new();
    let effect = intent_for(workspace_id, &run_ref());
    let receipt = unknown_receipt(&effect);
    let context = context_for(workspace_id);

    repository.insert_intent(&context, &effect).await.unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();

    let inconclusive = reconcile_effect(
        &effect,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            // The provider answered and could not say.
            None,
            "external:indeterminate",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    assert_eq!(inconclusive.outcome(), ReconciliationOutcome::Inconclusive);
    repository
        .insert_reconciliation(&context, &inconclusive)
        .await
        .unwrap();

    // Immediately afterwards there is nothing to gain by asking again.
    assert!(
        repository
            .find_reconciliation_candidates(&context, at(30), at(10_000), at(30), u32::MAX)
            .await
            .unwrap()
            .is_empty(),
        "an effect was asked about again in the same instant it was asked, or the failed-attempt \
         cutoff incorrectly enabled an inconclusive retry"
    );

    // Once the attempt is old enough, the effect is still an effect whose
    // outcome nobody knows.
    let candidates = repository
        .find_reconciliation_candidates(&context, at(90), at(0), at(90), u32::MAX)
        .await
        .unwrap();
    assert_eq!(
        candidates.len(),
        1,
        "an inconclusive answer retired the effect from the sweep, so its outcome stays unknown \
         forever and nothing ever asks again"
    );
    assert_eq!(candidates[0].intent(), &effect);

    // An answer does retire it, however old the attempt is.
    let settled = reconcile_effect(
        &effect,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )],
        at(100),
    )
    .unwrap();
    repository
        .insert_reconciliation(&context, &settled)
        .await
        .unwrap();

    assert!(
        repository
            .find_reconciliation_candidates(&context, at(10_000), at(10_000), at(10_000), u32::MAX,)
            .await
            .unwrap()
            .is_empty(),
        "a confirmed effect came back as a candidate, so the sweep would ask forever"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_equal_time_inconclusive_reconciliation_does_not_clear_a_failed_attempt(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool));
    let workspace_id = WorkspaceId::new();
    let effect = intent_for(workspace_id, &run_ref());
    let receipt = unknown_receipt(&effect);
    let context = context_for(workspace_id);
    repository.insert_intent(&context, &effect).await.unwrap();
    repository.insert_receipt(&context, &receipt).await.unwrap();
    let inconclusive = reconcile_effect(
        &effect,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            None,
            "external:indeterminate",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();
    repository
        .insert_reconciliation(&context, &inconclusive)
        .await
        .unwrap();
    repository
        .record_failed_recovery_attempt(&context, effect.id(), at(30), "route unavailable")
        .await
        .unwrap();

    assert!(
        repository
            .find_reconciliation_candidates(&context, at(31), at(0), at(31), u32::MAX)
            .await
            .unwrap()
            .is_empty(),
        "an older reconciliation with the same timestamp cleared the later failed attempt"
    );
}

#[test]
fn reconciliation_fixture_is_confirmed_before_persistence() {
    let intent = intent();
    let receipt = unknown_receipt(&intent);
    let reconciliation = reconcile_effect(
        &intent,
        &receipt,
        vec![ObservedEffectState::new(
            EvidenceStrength::ProviderIdempotencyLookup,
            Some(true),
            "external:delivered",
            vec!["evidence:provider-lookup".into()],
        )],
        at(30),
    )
    .unwrap();

    assert_eq!(reconciliation.outcome(), ReconciliationOutcome::Confirmed);
}

#[sqlx::test(migrations = "../../migrations")]
async fn fault_suite_evidence_repository_round_trips_and_is_idempotent(pool: PgPool) {
    let repository =
        vestrace_infrastructure::PgFaultSuiteEvidenceRepository::new(PgStore::from_pool(pool));
    let evidence = fault_evidence(true);

    repository.insert(&evidence).await.unwrap();
    repository.insert(&evidence).await.unwrap();

    assert_eq!(
        repository.find_by_id(evidence.id()).await.unwrap(),
        Some(evidence)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn fault_suite_evidence_repository_rejects_conflicting_immutable_id(pool: PgPool) {
    let repository =
        vestrace_infrastructure::PgFaultSuiteEvidenceRepository::new(PgStore::from_pool(pool));
    let first = fault_evidence(true);
    let mut payload = serde_json::to_value(&first).unwrap();
    payload["target_digest"] = serde_json::json!("sha256:other-target");
    let conflicting: ExternalEffectFaultSuiteEvidence = serde_json::from_value(payload).unwrap();

    repository.insert(&first).await.unwrap();
    assert!(matches!(
        repository.insert(&conflicting).await,
        Err(ApplicationError::Conflict(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn fault_suite_evidence_repository_preserves_failed_evidence(pool: PgPool) {
    let repository =
        vestrace_infrastructure::PgFaultSuiteEvidenceRepository::new(PgStore::from_pool(pool));
    let evidence = fault_evidence(false);

    repository.insert(&evidence).await.unwrap();

    let stored = repository.find_by_id(evidence.id()).await.unwrap().unwrap();
    assert!(!stored.is_passed());
    assert!(!stored.failures().is_empty());
}

fn fault_evidence(passed: bool) -> ExternalEffectFaultSuiteEvidence {
    let observations = EffectFaultPoint::required_points()
        .into_iter()
        .map(FaultObservation::expected)
        .collect::<Vec<_>>();
    serde_json::from_value::<ExternalEffectFaultSuiteEvidence>(serde_json::json!({
        "id": uuid::Uuid::now_v7(),
        "target_digest": "sha256:target",
        "observations": observations.into_iter().map(|observation| serde_json::json!({
            "point": observation.point,
            "status": observation.status,
            "retry_attempted": observation.retry_attempted,
            "reconciliation_started": observation.reconciliation_started,
            "receipt_persisted": observation.receipt_persisted
        })).collect::<Vec<_>>(),
        "passed": passed,
        "failures": if passed {
            Vec::<String>::new()
        } else {
            vec!["unsafe retry".to_owned()]
        },
        "created_at": at(40)
    }))
    .unwrap()
}
