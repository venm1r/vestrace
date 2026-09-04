//! Recovery after a lost embedding dispatch.
//!
//! Spec line 245: "Loss after `Dispatching` remains deadline-gated; one recovery
//! winner leaves the effect `Unknown` and finalizes the job
//! `InconclusiveUnknown`. Neither workers nor recovery automatically retry a job
//! after `Dispatching`."
//!
//! The property that matters is that recovery *decides what already happened*
//! and never makes something happen again. This suite proves that against the
//! database rather than against a counter in a double: a second provider call
//! would need a second effect and a second admission, and those are rows.

use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::{
    AcceptEmbeddingJob, ApplicationError, EmbeddingJobAttemptRecovery, EmbeddingJobRepository,
    IdempotencyRecord, ProviderDispatchRepository, RequestContext,
};
use vestrace_domain::{
    AuditEvent, EmbeddingJobId, EmbeddingSpaceId, ModelRequestEvidenceId, PrincipalId, WorkspaceId,
    embedding::EmbeddingJobKind, id::AuditEventId,
};
use vestrace_infrastructure::{PgEmbeddingJobRepository, PgStore};

#[path = "common/mod.rs"]
mod common;

use common::*;

/// Everything a second provider call would have to leave behind.
///
/// One effect, one admission, one concurrency lease, one `dispatching`
/// transition. Recovery may add an `unknown` outcome; it may not add any of
/// these.
async fn dispatch_footprint(pool: &PgPool, fixture: &AcceptedJob) -> (i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM provider_concurrency_leases WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
              WHERE effect_id=$2 AND status='dispatching')",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Brings one job to a committed `Dispatching` with no receipt, which is the
/// state a worker that died mid-call leaves behind.
async fn dispatched(pool: &PgPool, runtime: &PgPool) -> AcceptedJob {
    let fixture = accept_embedding_job(pool, runtime).await;
    make_dispatchable(pool, runtime, &fixture).await;
    dispatching_repository(runtime, None)
        .prepare_dispatch(embedding_dispatch_request(&fixture))
        .await
        .expect("the fixture must reach a committed Dispatching");
    fixture
}

fn acknowledgement_command(
    fixture: &AcceptedJob,
    successor_job_id: EmbeddingJobId,
    successor_mre_id: ModelRequestEvidenceId,
    expected_predecessor_version: u64,
    idempotency_key: &str,
) -> AcceptEmbeddingJob {
    let at = Utc::now();
    AcceptEmbeddingJob {
        job_id: successor_job_id,
        space_registration_id: EmbeddingSpaceId::from_uuid(fixture.space_registration_id),
        kind: EmbeddingJobKind::Delivery,
        model_binding_snapshot_id: fixture.snapshot_id,
        intent: embedding_intent(fixture, successor_job_id),
        model_request_evidence_id: successor_mre_id,
        retries_unknown_embedding_job_id: Some(fixture.job_id),
        expected_predecessor_version: Some(expected_predecessor_version),
        idempotency: Some(IdempotencyRecord {
            idempotency_key: idempotency_key.to_owned(),
            workspace_id: fixture.context.workspace_id,
            request_hash: format!(
                "{}:{}:{}:{}:{}:{}",
                fixture.job_id,
                expected_predecessor_version,
                successor_job_id,
                successor_mre_id,
                fixture.space_registration_id,
                fixture.snapshot_id,
            ),
            response_payload: None,
            status: "completed".to_owned(),
            created_at: at,
            expires_at: at + Duration::hours(24),
        }),
        outbox: Vec::new(),
        audit: AuditEvent::new(
            AuditEventId::new(),
            fixture.context.workspace_id,
            fixture.context.principal_id,
            "embedding.job.unknown_acknowledged",
            "embedding_job",
            successor_job_id.as_uuid(),
            serde_json::json!({"predecessor_embedding_job_id": fixture.job_id}),
            at,
        )
        .unwrap(),
    }
}

async fn recover_unknown(runtime: &PgPool, fixture: &AcceptedJob) {
    let recovery = dispatch_repository(runtime)
        .recover_embedding_job_attempt(
            &fixture.context,
            fixture.job_id,
            Utc::now() + Duration::hours(1),
        )
        .await
        .unwrap();
    assert_eq!(recovery, EmbeddingJobAttemptRecovery::AdoptedUnknown);
}

/// A second successor identity races through the partial unique index. It is a
/// policy refusal, not an infrastructure failure: the operator may learn that
/// one successor already exists but must not see a storage-shaped 500.
#[sqlx::test(migrations = "../../migrations")]
async fn a_second_successor_identity_is_a_semantic_acceptance_refusal(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    recover_unknown(&runtime, &fixture).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime));

    repository
        .accept_governed(
            fixture.context.clone(),
            acknowledgement_command(
                &fixture,
                EmbeddingJobId::new(),
                ModelRequestEvidenceId::new(),
                2,
                "first",
            ),
        )
        .await
        .unwrap();
    let error = repository
        .accept_governed(
            fixture.context.clone(),
            acknowledgement_command(
                &fixture,
                EmbeddingJobId::new(),
                ModelRequestEvidenceId::new(),
                2,
                "second",
            ),
        )
        .await
        .expect_err("the one-successor index must refuse a second identity");

    assert!(
        matches!(
            error,
            ApplicationError::Policy(ref message)
                if message == "EMBEDDING_JOB_ACCEPTANCE_REFUSED"
        ),
        "the partial unique index must retain its policy meaning: {error:?}"
    );
}

/// A duplicate-charge acknowledgement is a successor of the same embedding
/// operation, not a way to reinterpret a recovered Delivery as another job
/// kind. The guarded predecessor lookup must reject the mismatch before the
/// intent it was paired with can survive the transaction.
#[sqlx::test(migrations = "../../migrations")]
async fn a_successor_with_a_kind_different_from_its_predecessor_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    recover_unknown(&runtime, &fixture).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime));
    let before: (i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut command = acknowledgement_command(
        &fixture,
        EmbeddingJobId::new(),
        ModelRequestEvidenceId::new(),
        2,
        "kind-mismatch",
    );
    command.kind = EmbeddingJobKind::RetrievalQuery;

    let result = repository
        .accept_governed(fixture.context.clone(), command)
        .await;
    assert!(
        matches!(
            result,
            Err(ApplicationError::Policy(ref message))
                if message == "EMBEDDING_JOB_ACCEPTANCE_REFUSED"
        ),
        "a successor must keep its predecessor's kind: {result:?}"
    );
    let after: (i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after, before,
        "a refused kind mismatch leaves no job or effect"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn acknowledgement_creates_one_fresh_successor_and_preserves_the_unknown_head(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    recover_unknown(&runtime, &fixture).await;
    let before: (String, i64, Uuid) =
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let successor_id = EmbeddingJobId::new();
    let successor_mre_id = ModelRequestEvidenceId::new();
    let command = acknowledgement_command(&fixture, successor_id, successor_mre_id, 2, "ack");
    let successor_effect_id = command.intent.id().as_uuid();

    PgEmbeddingJobRepository::new(PgStore::from_pool(runtime))
        .accept_governed(fixture.context.clone(), command)
        .await
        .unwrap();

    let successor: (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT retries_unknown_embedding_job_id, external_effect_id, model_request_evidence_id \
         FROM embedding_jobs WHERE id=$1",
    )
    .bind(successor_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(successor.0, fixture.job_id.as_uuid());
    assert_eq!(successor.1, successor_effect_id);
    assert_eq!(successor.2, successor_mre_id.as_uuid());
    assert_ne!(successor.1, before.2, "the successor owns a fresh effect");
    let after: (String, i64, Uuid) =
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        after, before,
        "acknowledgement never mutates the predecessor"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_identical_acknowledgement_replay_is_refused_without_another_job_or_effect(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    recover_unknown(&runtime, &fixture).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime));
    let successor_id = EmbeddingJobId::new();
    let successor_mre_id = ModelRequestEvidenceId::new();
    repository
        .accept_governed(
            fixture.context.clone(),
            acknowledgement_command(&fixture, successor_id, successor_mre_id, 2, "replay"),
        )
        .await
        .unwrap();

    let replay = repository
        .accept_governed(
            fixture.context.clone(),
            acknowledgement_command(&fixture, successor_id, successor_mre_id, 2, "replay"),
        )
        .await
        .expect_err("a fresh effect id makes the matching request a guarded refusal");
    assert!(matches!(
        replay,
        ApplicationError::Policy(ref message) if message == "EMBEDDING_JOB_ACCEPTANCE_REFUSED"
    ));
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
            (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        counts,
        (2, 2),
        "both calls leave one predecessor and one successor only"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn acknowledgement_refuses_nonterminal_heads_without_creating_a_successor_or_effect(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let requested = accept_embedding_job(&pool, &runtime).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    let before_requested: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
                (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(requested.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        repository
            .accept_governed(
                requested.context.clone(),
                acknowledgement_command(
                    &requested,
                    EmbeddingJobId::new(),
                    ModelRequestEvidenceId::new(),
                    1,
                    "requested"
                ),
            )
            .await
            .is_err()
    );
    let after_requested: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
                (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(requested.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_requested, before_requested);

    let dispatching = dispatched(&pool, &runtime).await;
    let before_dispatching: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
                (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(dispatching.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        repository
            .accept_governed(
                dispatching.context.clone(),
                acknowledgement_command(
                    &dispatching,
                    EmbeddingJobId::new(),
                    ModelRequestEvidenceId::new(),
                    1,
                    "dispatching"
                ),
            )
            .await
            .is_err()
    );
    let after_dispatching: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1), \
                (SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1)",
    )
    .bind(dispatching.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_dispatching, before_dispatching);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_stale_version_cannot_converge_on_an_existing_successor(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    recover_unknown(&runtime, &fixture).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    let successor_id = EmbeddingJobId::new();
    let successor_mre_id = ModelRequestEvidenceId::new();
    let command = acknowledgement_command(&fixture, successor_id, successor_mre_id, 2, "current");
    let successor_effect_id = command.intent.id().as_uuid();
    repository
        .accept_governed(fixture.context.clone(), command)
        .await
        .unwrap();
    let before: ((String, i64, Uuid), (String, i64, Uuid)) = (
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(successor_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
    );

    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let stale = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_accept_embedding_job($1,$2,$3,'delivery',$4,$5,$6,$7,$8)",
    )
    .bind(successor_id.as_uuid())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.space_registration_id)
    .bind(fixture.snapshot_id)
    .bind(successor_effect_id)
    .bind(successor_mre_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind(3_i64)
    .fetch_one(&mut *transaction)
    .await;
    assert_eq!(
        stale
            .unwrap_err()
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );
    transaction.rollback().await.unwrap();
    let after: ((String, i64, Uuid), (String, i64, Uuid)) = (
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
        sqlx::query_as("SELECT state, version, external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(successor_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
    );
    assert_eq!(after, before);
}

/// Before the deadline there is nothing to decide.
///
/// Line 245 makes loss after `Dispatching` deadline-gated. A worker that
/// adopted an in-flight dispatch the moment it noticed one would be racing the
/// provider it cannot see.
#[sqlx::test(migrations = "../../migrations")]
async fn recovery_waits_for_the_dispatch_deadline(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;

    let recovery = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, Utc::now())
        .await
        .expect("a live dispatch classifies rather than refusing");

    assert_eq!(recovery, EmbeddingJobAttemptRecovery::AwaitDispatchDeadline);
    let state: String = sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE id=$1")
        .bind(fixture.job_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        state, "requested",
        "waiting for a deadline must not finalize the job"
    );
}

/// Past the deadline the effect becomes `Unknown` and the job terminal, and
/// nothing else is created.
///
/// The recovery time is passed in rather than waited for: the executor's
/// time-dependent branches cannot both be driven end to end without waiting out
/// the dispatch TTL, which P03's qualification established. What is proved here
/// against the database is the deadline branch; the pre-deadline branch above is
/// proved the same way, and neither is proved by sleeping.
#[sqlx::test(migrations = "../../migrations")]
async fn past_the_deadline_one_winner_adopts_and_finalizes(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    let before = dispatch_footprint(&pool, &fixture).await;
    assert_eq!(before, (1, 1, 1, 1), "one dispatch leaves one of each");

    let recovery = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(
            &fixture.context,
            fixture.job_id,
            Utc::now() + Duration::hours(1),
        )
        .await
        .expect("a dispatch past its deadline is adoptable");
    assert_eq!(recovery, EmbeddingJobAttemptRecovery::AdoptedUnknown);

    let (state, version): (String, i64) =
        sqlx::query_as("SELECT state, version FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (state.as_str(), version),
        ("inconclusive_unknown", 2),
        "the job is terminal and its version advanced"
    );
    let unknown: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
          WHERE effect_id=$1 AND status='unknown'",
    )
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unknown, 1, "the effect carries exactly one unknown outcome");

    // The decisive assertion. A second provider call would need a second effect
    // and a second admission; recovery created neither.
    assert_eq!(
        dispatch_footprint(&pool, &fixture).await,
        before,
        "recovery created a new dispatch footprint"
    );
}

/// A crash between the effect outcome and the job outcome is recovered by
/// running recovery again, and by nothing else.
///
/// Line 245 requires the job to be finalized from the immutable effect outcome
/// without another adapter call. The finalizer is idempotent, so the second run
/// reports `AlreadyUnknown` and leaves the version where the first run put it.
#[sqlx::test(migrations = "../../migrations")]
async fn a_second_recovery_adds_nothing(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    let repository = dispatch_repository(&runtime);
    let recovered_at = Utc::now() + Duration::hours(1);

    repository
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, recovered_at)
        .await
        .unwrap();
    let after_first = dispatch_footprint(&pool, &fixture).await;

    let second = repository
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, recovered_at)
        .await
        .expect("a terminal job still classifies");
    assert_eq!(second, EmbeddingJobAttemptRecovery::AlreadyUnknown);

    let (version, unknowns): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT version FROM embedding_jobs WHERE id=$1), \
                (SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
                  WHERE effect_id=$2 AND status='unknown')",
    )
    .bind(fixture.job_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (version, unknowns),
        (2, 1),
        "a second recovery must append no second outcome"
    );
    assert_eq!(dispatch_footprint(&pool, &fixture).await, after_first);
}

/// No scheduler advances a terminal ambiguity head.
///
/// Line 257 makes the authorized acknowledgement the sole successor path, so
/// rediscovering the plan of an `InconclusiveUnknown` job must not present it as
/// something to dispatch again.
#[sqlx::test(migrations = "../../migrations")]
async fn a_terminal_unknown_job_is_never_presented_as_dispatchable(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    let repository = dispatch_repository(&runtime);
    repository
        .recover_embedding_job_attempt(
            &fixture.context,
            fixture.job_id,
            Utc::now() + Duration::hours(1),
        )
        .await
        .unwrap();

    let footprint = dispatch_footprint(&pool, &fixture).await;
    let second_dispatch = dispatching_repository(&runtime, None)
        .prepare_dispatch(embedding_dispatch_request(&fixture))
        .await;
    assert!(
        second_dispatch.is_err(),
        "a terminal unknown job must not dispatch again"
    );
    assert_eq!(
        dispatch_footprint(&pool, &fixture).await,
        footprint,
        "a refused second dispatch must leave no new footprint"
    );
}

/// Another workspace cannot recover this job into a terminal state.
#[sqlx::test(migrations = "../../migrations")]
async fn recovery_is_workspace_bound(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatched(&pool, &runtime).await;
    let intruder = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(intruder.workspace_id.as_uuid())
        .bind(format!("intruder-{}", Uuid::now_v7()))
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        dispatch_repository(&runtime)
            .recover_embedding_job_attempt(
                &intruder,
                fixture.job_id,
                Utc::now() + Duration::hours(1)
            )
            .await
            .is_err()
    );
    let state: String = sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE id=$1")
        .bind(fixture.job_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "requested");
}
