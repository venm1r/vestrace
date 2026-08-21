use std::{borrow::Cow, future::Future};

use chrono::{TimeZone, Utc};
use sqlx::{PgPool, migrate::Migrator, postgres::PgPoolOptions};
use vestrace_application::{
    ApplicationError, ExternalEffectFaultSuiteEvidence, ExternalEffectRepository,
    FaultSuiteEvidenceRepository, RequestContext,
};
use vestrace_domain::external_effects::{
    DeliverySemantics, EffectFaultPoint, EffectLifecycleStatus, EffectPrecondition,
    EffectReversibility, EvidenceStrength, ExternalEffectIntent, ExternalEffectReceipt,
    FaultObservation, IdempotencyProfile, ObservedEffectState, ReconciliationOutcome,
    reconcile_effect,
};
use vestrace_domain::{Capability, PrincipalId, RiskCategory, WorkspaceId};
use vestrace_infrastructure::{PgExternalEffectRepository, PgStore};

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

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
         WHERE effect_id = $1 AND workspace_id = $2 ORDER BY recorded_at DESC, created_at DESC, id DESC LIMIT 1",
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
        .record_dispatch_started(&context, intent.id(), at(15))
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
         WHERE effect_id = $1 AND workspace_id = $2 ORDER BY recorded_at DESC, created_at DESC, id DESC LIMIT 1",
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
        .record_dispatch_started(&context, intent.id(), at(25))
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
        .record_dispatch_started(&stranger, effect.id(), at(15))
        .await
        .unwrap_err()
        .to_string();
    let missing = repository
        .record_dispatch_started(&stranger, vestrace_domain::ExternalEffectId::new(), at(15))
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
        first.record_dispatch_started(&first_context, effect_id, at(15)),
        second.record_dispatch_started(&second_context, effect_id, at(16)),
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
async fn a_lost_dispatch_without_a_receipt_enters_recovery_only_after_its_own_cutoff(pool: PgPool) {
    let repository = PgExternalEffectRepository::new(PgStore::from_pool(pool.clone()));
    let effect = intent();
    let context = context_for(effect.workspace_id());
    repository.insert_intent(&context, &effect).await.unwrap();
    repository
        .record_dispatch_started(&context, effect.id(), at(20))
        .await
        .unwrap();

    assert!(
        repository
            .find_reconciliation_candidates(&context, at(10_000), at(20))
            .await
            .unwrap()
            .is_empty()
    );
    let candidates = repository
        .find_reconciliation_candidates(&context, at(10_000), at(21))
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].intent(), &effect);
    assert_eq!(candidates[0].receipt(), None);

    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions \
             (effect_id, workspace_id, status, cause, cause_ref, recorded_at) \
         VALUES ($1, $2, 'prepared', 'intent_recorded', $3, $4)",
    )
    .bind(effect.id().as_uuid())
    .bind(effect.workspace_id().as_uuid())
    .bind(effect.id().to_string())
    .bind(at(22))
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        repository
            .find_reconciliation_candidates(&context, at(10_000), at(100))
            .await
            .unwrap()
            .is_empty(),
        "an older Dispatching row qualified after a newer lifecycle transition"
    );

    let reconciliation = reconcile_effect(
        &effect,
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
            .find_lifecycle_status(&context, effect.id())
            .await
            .unwrap(),
        Some(EffectLifecycleStatus::Reconciling)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_unknown_receipt_is_a_candidate_across_mixed_states_and_insertion_orders(
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
        .find_reconciliation_candidates(&context, at(100), at(100))
        .await
        .unwrap()
        .into_iter()
        .map(|candidate| candidate.receipt().unwrap().id().to_string())
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = vec![
        first_unknown.id().to_string(),
        second_unknown.id().to_string(),
    ];
    expected.sort();
    assert_eq!(actual, expected);
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
        .find_reconciliation_candidates(&context, at(100), at(100))
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
        .find_reconciliation_candidates(&context, at(10_000), at(10_000))
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
            .find_reconciliation_candidates(&context, at(10_000), at(10_000))
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
            .find_reconciliation_candidates(&context, at(30), at(30))
            .await
            .unwrap()
            .is_empty(),
        "an effect was asked about again in the same instant it was asked"
    );

    // Once the attempt is old enough, the effect is still an effect whose
    // outcome nobody knows.
    let candidates = repository
        .find_reconciliation_candidates(&context, at(90), at(90))
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
            .find_reconciliation_candidates(&context, at(10_000), at(10_000))
            .await
            .unwrap()
            .is_empty(),
        "a confirmed effect came back as a candidate, so the sweep would ask forever"
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
