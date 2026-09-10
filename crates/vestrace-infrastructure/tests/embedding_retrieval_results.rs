//! A retrieval attempt owns its fence, and a fence is cashed at most once.
//!
//! The property under test is that a stored answer can always be traced to the
//! exact generation it answered.  A generation that moves while an attempt is
//! in flight closes that attempt as changed rather than letting its answer
//! land, and only such a change may be followed by exactly one successor.

mod common;

use common::result_preparation_fixture::provision_result_behavior_database;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

async fn scoped(transaction: &mut Transaction<'_, Postgres>, workspace: Uuid) {
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut **transaction)
        .await
        .unwrap();
}

fn assert_refusal(error: sqlx::Error, expected_state: &str, expected_message: &str) {
    let database = error.as_database_error().expect("a database refusal");
    assert_eq!(
        database.code().as_deref(),
        Some(expected_state),
        "unexpected sqlstate; message was {:?}",
        database.message()
    );
    assert!(
        database.message().contains(expected_message),
        "expected refusal {expected_message:?}, got {:?}",
        database.message()
    );
}

/// Seeds the exact q1 structural evidence `vestrace_assert_canonical_embedding_space`
/// demands, for the shared delivery fixture's own world, then registers one
/// canonical space through the real guarded authority.
///
/// The evidence chain is not faked past its own guard: the registration still
/// goes through `vestrace_register_canonical_embedding_space`, which asserts
/// every join below.  What is seeded here is the durable evidence a real q1
/// qualification would have left behind.
async fn register_canonical_space(
    pool: &PgPool,
    runtime: &PgPool,
    fixture: &common::AcceptedJob,
) -> (Uuid, Uuid) {
    let workspace = fixture.context.workspace_id.as_uuid();
    let qualification_job: Uuid = sqlx::query_scalar(
        "SELECT qualification_job_id FROM model_qualification_revisions \
         WHERE workspace_id=$1 AND id=$2",
    )
    .bind(workspace)
    .bind(fixture.model_qualification_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let wire_model: String = sqlx::query_scalar(
        "SELECT wire_model_id FROM model_revisions WHERE workspace_id=$1 AND id=$2",
    )
    .bind(workspace)
    .bind(fixture.model_revision_id)
    .fetch_one(pool)
    .await
    .unwrap();

    let canonical_qualification = Uuid::now_v7();
    let probe_effect = Uuid::now_v7();
    let evidence_root = Uuid::now_v7();
    let evidence_check = Uuid::now_v7();
    let target_binding = Uuid::now_v7();

    // external_effect_intents predates the P03 guarded ownership and the
    // guarded owner holds no privilege on it, so it is written before the role
    // switch rather than under that role.
    sqlx::query(
        "INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) \
         VALUES($1,$2,'local','{}'::jsonb)",
    )
    .bind(probe_effect)
    .bind(workspace)
    .execute(pool)
    .await
    .expect("one probe external effect intent");

    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, workspace).await;
    // model_qualification_revisions is immutable P03 evidence, so the shared
    // fixture's 'embedding' capability cannot be widened in place.  A second
    // qualification revision over the same job and model states the
    // 'embeddings' request capability the canonical assertion reads, and the
    // transition targets that revision.
    sqlx::query(
        "INSERT INTO model_qualification_revisions(id,workspace_id,model_revision_id,         connection_revision_id,connection_qualification_revision_id,qualification_job_id,         capabilities,valid_until)          SELECT $1,workspace_id,model_revision_id,connection_revision_id,         connection_qualification_revision_id,qualification_job_id,         ARRAY['embedding','embeddings']::TEXT[],NOW()+INTERVAL '1 hour'          FROM model_qualification_revisions WHERE workspace_id=$2 AND id=$3",
    )
    .bind(canonical_qualification)
    .bind(workspace)
    .bind(fixture.model_qualification_id)
    .execute(&mut *owner)
    .await
    .expect("one further qualification revision stating the embeddings capability");
    sqlx::query(
        "INSERT INTO qualification_target_bindings(id,workspace_id,qualification_job_id,\
         connection_id,connection_revision_id,branch,no_auth_binding_revision_id,\
         embedding_model_revision_id) VALUES($1,$2,$3,$4,$5,'no_auth',$6,$7)",
    )
    .bind(target_binding)
    .bind(workspace)
    .bind(qualification_job)
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(fixture.no_auth_binding_id)
    .bind(fixture.model_revision_id)
    .execute(&mut *owner)
    .await
    .expect("one q1 target binding naming the embedding model revision");
    sqlx::query(
        "INSERT INTO model_request_evidence_roots(id,workspace_id,external_effect_id,\
         request_kind,binding_snapshot_id,qualification_target_binding_id,cause_kind,cause_id) \
         VALUES($1,$2,$3,'embeddings',NULL,$4,'qualification_probe',$5)",
    )
    .bind(evidence_root)
    .bind(workspace)
    .bind(probe_effect)
    .bind(target_binding)
    .bind(qualification_job)
    .execute(&mut *owner)
    .await
    .expect("one embeddings evidence root rooted at the q1 probe");
    sqlx::query(
        "INSERT INTO model_request_evidence_checks(id,workspace_id,evidence_root_id,status) \
         VALUES($1,$2,$3,'complete')",
    )
    .bind(evidence_check)
    .bind(workspace)
    .bind(evidence_root)
    .execute(&mut *owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO qualification_probe_results(id,workspace_id,qualification_job_id,\
         probe_ordinal,result,external_effect_id,model_request_evidence_id) \
         VALUES($1,$2,$3,'90','pass',$4,$5)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace)
    .bind(qualification_job)
    .bind(probe_effect)
    .bind(evidence_root)
    .execute(&mut *owner)
    .await
    .expect("the passing embeddings probe at ordinal 90");
    sqlx::query(
        "INSERT INTO provider_dispatch_causes(external_effect_id,workspace_id,\
         model_request_evidence_id,model_request_evidence_check_id,cause_kind,\
         qualification_job_id,qualification_target_binding_id,qualification_probe_ordinal) \
         VALUES($1,$2,$3,$4,'qualification_probe',$5,$6,'90')",
    )
    .bind(probe_effect)
    .bind(workspace)
    .bind(evidence_root)
    .bind(evidence_check)
    .bind(qualification_job)
    .bind(target_binding)
    .execute(&mut *owner)
    .await
    .expect("the dispatch cause binding the probe to its evidence");
    sqlx::query(
        "INSERT INTO qualification_q1_mre_sources(evidence_root_id,workspace_id,probe_ordinal,\
         message_layout,tool_choice,parallel_tool_calls,response_format,stream,stream_include_usage) \
         VALUES($1,$2,'90','plain_text','none',false,'none',false,false)",
    )
    .bind(evidence_root)
    .bind(workspace)
    .execute(&mut *owner)
    .await
    .expect("the q1 source describing the embeddings probe shape");
    owner.commit().await.unwrap();

    let shape = Uuid::now_v7();
    let registration = Uuid::now_v7();
    let mut governed = runtime.begin().await.unwrap();
    scoped(&mut governed, workspace).await;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_shape_revision($1,$2,1,'embeddings',false,ARRAY[]::TEXT[])",
    )
    .bind(shape)
    .bind(workspace)
    .fetch_one(&mut *governed)
    .await
    .expect("one embeddings request shape revision");
    let registered: Uuid = sqlx::query_scalar(
        "SELECT vestrace_register_canonical_embedding_space($1,$2,'activation-canonical',$3,$4,$5,$6,'float',768)",
    )
    .bind(registration)
    .bind(workspace)
    .bind(fixture.model_revision_id)
    .bind(canonical_qualification)
    .bind(shape)
    .bind(&wire_model)
    .fetch_one(&mut *governed)
    .await
    .expect("the real guarded authority must accept a fully evidenced canonical space");
    governed.commit().await.unwrap();

    // vestrace_register_canonical_embedding_space writes the registration
    // alone; the corpus state and generation guard that every result path
    // reads are seeded here so the canonical space behaves like a real one.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, workspace).await;
    sqlx::query(
        "INSERT INTO embedding_space_corpus_states(workspace_id,space_registration_id)          VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(registered)
    .execute(&mut *owner)
    .await
    .expect("one canonical corpus state");
    sqlx::query(
        "INSERT INTO embedding_index_generation_guards(workspace_id,space_registration_id)          VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(registered)
    .execute(&mut *owner)
    .await
    .expect("one canonical generation guard");
    owner.commit().await.unwrap();

    (registered, canonical_qualification)
}

struct Attempt {
    workspace: Uuid,
    space: Uuid,
    job: Uuid,
    generation: Uuid,
}

/// Publishes one Ready generation on the fixture's space and accepts one
/// `retrieval_query` job against it.
async fn attempt(pool: &PgPool, runtime: &PgPool) -> Attempt {
    let accepted = common::prepare_delivery_embedding_job(pool, runtime).await;
    let workspace = accepted.context.workspace_id.as_uuid();
    // A Ready generation may only exist on a canonical registration: 0197
    // refuses one on a legacy space until governed adoption has run, which is
    // exactly the LegacyAdoptionPending degradation this package models.
    let (space, _qualification) = register_canonical_space(pool, runtime, &accepted).await;

    // The canonical capture/publish pair, not the legacy opener: 0197 refuses a
    // Ready generation whose members are a legacy_upgrade representation.
    let generation = Uuid::now_v7();
    let mut governed = runtime.begin().await.unwrap();
    scoped(&mut governed, workspace).await;
    sqlx::query("SELECT vestrace_capture_embedding_generation($1,$2,$3,1::BIGINT)")
        .bind(generation)
        .bind(workspace)
        .bind(space)
        .execute(&mut *governed)
        .await
        .expect("one captured canonical generation");
    sqlx::query("SELECT vestrace_publish_embedding_generation($1,$2,$3,1::BIGINT)")
        .bind(workspace)
        .bind(space)
        .bind(generation)
        .execute(&mut *governed)
        .await
        .expect("the captured generation must publish Ready");
    governed.commit().await.unwrap();

    let job = accept_retrieval_job(pool, runtime, &accepted).await;
    Attempt {
        workspace,
        space,
        job,
        generation,
    }
}

/// Accepts one retrieval_query job over its own effect and evidence root.
async fn accept_retrieval_job(pool: &PgPool, runtime: &PgPool, base: &common::AcceptedJob) -> Uuid {
    let workspace = base.context.workspace_id.as_uuid();
    let effect = Uuid::now_v7();
    let evidence = Uuid::now_v7();
    let job = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) VALUES($1,$2,'local','{}'::jsonb)")
        .bind(effect)
        .bind(workspace)
        .execute(pool)
        .await
        .unwrap();
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, workspace).await;
    sqlx::query(
        "INSERT INTO model_request_evidence_roots\
         (id,workspace_id,external_effect_id,request_kind,binding_snapshot_id,cause_kind,cause_id) \
         VALUES($1,$2,$3,'embeddings',$4,'embedding_job',$5)",
    )
    .bind(evidence)
    .bind(workspace)
    .bind(effect)
    .bind(base.snapshot_id)
    .bind(job)
    .execute(&mut *owner)
    .await
    .unwrap();
    owner.commit().await.unwrap();

    let mut accept = runtime.begin().await.unwrap();
    scoped(&mut accept, workspace).await;
    let accepted: Uuid = sqlx::query_scalar(
        "SELECT vestrace_accept_embedding_job($1,$2,$3,'retrieval_query',$4,$5,$6,NULL,NULL::BIGINT)",
    )
    .bind(job)
    .bind(workspace)
    .bind(base.space_registration_id)
    .bind(base.snapshot_id)
    .bind(effect)
    .bind(evidence)
    .fetch_one(&mut *accept)
    .await
    .expect("the runtime role accepts one retrieval_query job");
    accept.commit().await.unwrap();
    assert_eq!(accepted, job);
    job
}

async fn accept_fence(runtime: &PgPool, a: &Attempt, request: Uuid) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, a.workspace).await;
    let result = sqlx::query_scalar(
        "SELECT vestrace_accept_embedding_retrieval_attempt($1,$2,$3,$4,NOW()+INTERVAL '30 seconds')",
    )
    .bind(a.workspace)
    .bind(a.job)
    .bind(request)
    .bind(a.space)
    .fetch_one(&mut *transaction)
    .await;
    finish(transaction, result).await
}

async fn finalize(
    runtime: &PgPool,
    a: &Attempt,
    fence: Uuid,
    references: &[(Uuid, Uuid, i64, f64)],
) -> Result<Uuid, sqlx::Error> {
    let memories: Vec<Uuid> = references.iter().map(|r| r.0).collect();
    let revisions: Vec<Uuid> = references.iter().map(|r| r.1).collect();
    let ranks: Vec<i64> = references.iter().map(|r| r.2).collect();
    let scores: Vec<f64> = references.iter().map(|r| r.3).collect();
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, a.workspace).await;
    let result = sqlx::query_scalar(
        "SELECT vestrace_finalize_embedding_retrieval_result($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(a.workspace)
    .bind(a.job)
    .bind(fence)
    .bind(memories)
    .bind(revisions)
    .bind(ranks)
    .bind(scores)
    .fetch_one(&mut *transaction)
    .await;
    finish(transaction, result).await
}

async fn observe_change(
    runtime: &PgPool,
    a: &Attempt,
    fence: Uuid,
    reason: &str,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, a.workspace).await;
    let result = sqlx::query_scalar(
        "SELECT vestrace_observe_embedding_retrieval_generation_change($1,$2,$3,$4)",
    )
    .bind(a.workspace)
    .bind(a.job)
    .bind(fence)
    .bind(reason)
    .fetch_one(&mut *transaction)
    .await;
    finish(transaction, result).await
}

async fn authorize_retry(
    runtime: &PgPool,
    a: &Attempt,
    successor: Uuid,
    successor_request: Uuid,
    key: &str,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, a.workspace).await;
    let result =
        sqlx::query_scalar("SELECT vestrace_authorize_embedding_retrieval_retry($1,$2,$3,$4,$5)")
            .bind(a.workspace)
            .bind(a.job)
            .bind(successor)
            .bind(successor_request)
            .bind(key)
            .fetch_one(&mut *transaction)
            .await;
    finish(transaction, result).await
}

async fn finish(
    transaction: Transaction<'_, Postgres>,
    result: Result<Uuid, sqlx::Error>,
) -> Result<Uuid, sqlx::Error> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

/// Moves the space on by capturing and publishing a second generation, which
/// is what a rebuild does underneath an in-flight attempt.  The guard cannot be
/// nudged by hand: its own validator demands a consistent Ready snapshot.
async fn advance_generation(pool: &PgPool, runtime: &PgPool, a: &Attempt) {
    let current: i64 = sqlx::query_scalar(
        "SELECT guard_version FROM embedding_index_generation_guards          WHERE workspace_id=$1 AND space_registration_id=$2",
    )
    .bind(a.workspace)
    .bind(a.space)
    .fetch_one(pool)
    .await
    .unwrap();
    let next = Uuid::now_v7();
    let mut governed = runtime.begin().await.unwrap();
    scoped(&mut governed, a.workspace).await;
    sqlx::query("SELECT vestrace_capture_embedding_generation($1,$2,$3,$4)")
        .bind(next)
        .bind(a.workspace)
        .bind(a.space)
        .bind(current)
        .execute(&mut *governed)
        .await
        .expect("a successor generation may be captured");
    sqlx::query("SELECT vestrace_publish_embedding_generation($1,$2,$3,$4)")
        .bind(a.workspace)
        .bind(a.space)
        .bind(next)
        .bind(current)
        .execute(&mut *governed)
        .await
        .expect("the successor generation must publish Ready");
    governed.commit().await.unwrap();
}

async fn counts(pool: &PgPool, a: &Attempt) -> (i64, i64) {
    let results: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_retrieval_results WHERE workspace_id=$1 AND job_id=$2",
    )
    .bind(a.workspace)
    .bind(a.job)
    .fetch_one(pool)
    .await
    .unwrap();
    let changes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_retrieval_generation_changes \
         WHERE workspace_id=$1 AND job_id=$2",
    )
    .bind(a.workspace)
    .bind(a.job)
    .fetch_one(pool)
    .await
    .unwrap();
    (results, changes)
}

async fn job_state(pool: &PgPool, a: &Attempt) -> String {
    sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
        .bind(a.workspace)
        .bind(a.job)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Query-first order: the attempt cashes its fence while the pinned generation
/// is still current, and the answer becomes terminal.  A guard that moves
/// afterwards does not retract it.
#[sqlx::test(migrations = false)]
async fn a_result_lands_while_its_pinned_generation_is_current(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7())
        .await
        .expect("one attempt is admitted against the current generation");

    // The fence pinned exactly the generation that was current, not merely
    // some generation: this is what makes a stored answer traceable.
    let pinned: Uuid = sqlx::query_scalar(
        "SELECT generation_id FROM embedding_retrieval_fences WHERE workspace_id=$1 AND id=$2",
    )
    .bind(a.workspace)
    .bind(fence)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pinned, a.generation);

    let first = (Uuid::now_v7(), Uuid::now_v7(), 0_i64, 0.75_f64);
    let second = (Uuid::now_v7(), Uuid::now_v7(), 1_i64, 0.25_f64);
    finalize(&runtime, &a, fence, &[first, second])
        .await
        .expect("the pinned generation is still current, so the answer is terminal");

    assert_eq!(counts(&pool, &a).await, (1, 0));
    assert_eq!(job_state(&pool, &a).await, "succeeded");

    // The references are stored in order, and carry nothing but references.
    let stored: Vec<(i32, Uuid, Uuid, i32, f64)> = sqlx::query_as(
        "SELECT r.ordinal,r.memory_id,r.revision_id,r.rank,r.score \
         FROM embedding_retrieval_result_references r \
         JOIN embedding_retrieval_results s ON s.workspace_id=r.workspace_id AND s.id=r.result_id \
         WHERE s.workspace_id=$1 AND s.job_id=$2 ORDER BY r.ordinal",
    )
    .bind(a.workspace)
    .bind(a.job)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        vec![
            (0, first.0, first.1, 0, 0.75),
            (1, second.0, second.1, 1, 0.25)
        ]
    );

    // Staling afterwards leaves the terminal answer exactly as it was.
    advance_generation(&pool, &runtime, &a).await;
    assert_eq!(counts(&pool, &a).await, (1, 0));
    runtime.close().await;
}

/// Change-first order: the guard moves while the attempt is in flight, so the
/// fence can no longer be cashed and no result row is written at all.
#[sqlx::test(migrations = false)]
async fn a_generation_that_moves_first_refuses_the_result_and_stores_nothing(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();

    advance_generation(&pool, &runtime, &a).await;

    let error = finalize(
        &runtime,
        &a,
        fence,
        &[(Uuid::now_v7(), Uuid::now_v7(), 0, 1.0)],
    )
    .await
    .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding retrieval result requires its exact pinned generation",
    );
    assert_eq!(counts(&pool, &a).await, (0, 0));

    observe_change(&runtime, &a, fence, "stale")
        .await
        .expect("the attempt closes as a confirmed generation change");
    assert_eq!(counts(&pool, &a).await, (0, 1));
    assert_eq!(job_state(&pool, &a).await, "failed_definite");
    runtime.close().await;
}

/// A job carries a result or a generation change, never both, in either order.
#[sqlx::test(migrations = false)]
async fn a_result_and_a_generation_change_exclude_each_other(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;

    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();
    finalize(
        &runtime,
        &a,
        fence,
        &[(Uuid::now_v7(), Uuid::now_v7(), 0, 0.5)],
    )
    .await
    .unwrap();
    let error = observe_change(&runtime, &a, fence, "stale")
        .await
        .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding retrieval attempt already produced a terminal result",
    );
    assert_eq!(counts(&pool, &a).await, (1, 0));

    let b = attempt(&pool, &runtime).await;
    let other = accept_fence(&runtime, &b, Uuid::now_v7()).await.unwrap();
    observe_change(&runtime, &b, other, "replaced")
        .await
        .unwrap();
    let error = finalize(
        &runtime,
        &b,
        other,
        &[(Uuid::now_v7(), Uuid::now_v7(), 0, 0.5)],
    )
    .await
    .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding retrieval attempt already closed as generation changed",
    );
    assert_eq!(counts(&pool, &b).await, (0, 1));
    runtime.close().await;
}

/// One fence per job and one logical request identity per fence.
#[sqlx::test(migrations = false)]
async fn a_job_takes_exactly_one_fence(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();
    let error = accept_fence(&runtime, &a, Uuid::now_v7())
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23505")
    );
    runtime.close().await;
}

/// One authorized successor: a replay under the same key returns it, and a
/// different key against the same predecessor conflicts.
#[sqlx::test(migrations = false)]
async fn one_confirmed_change_authorizes_exactly_one_successor(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();

    // No confirmed change yet.
    let successor = Uuid::now_v7();
    let error = authorize_retry(&runtime, &a, successor, Uuid::now_v7(), "key-1")
        .await
        .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding retrieval retry requires a confirmed generation change",
    );

    observe_change(&runtime, &a, fence, "corpus_changed")
        .await
        .unwrap();

    let request = Uuid::now_v7();
    let first = authorize_retry(&runtime, &a, successor, request, "key-1")
        .await
        .expect("a confirmed change authorizes one successor");
    assert_eq!(first, successor);

    // Same key replays the same successor.
    let replay = authorize_retry(&runtime, &a, successor, request, "key-1")
        .await
        .expect("the same key replays its own successor");
    assert_eq!(replay, successor);

    // A different key against the same predecessor conflicts.
    let error = authorize_retry(&runtime, &a, Uuid::now_v7(), Uuid::now_v7(), "key-2")
        .await
        .unwrap_err();
    assert_refusal(
        error,
        "23505",
        "embedding retrieval retry predecessor already has its successor",
    );

    let edges: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_retrieval_retry_edges WHERE workspace_id=$1",
    )
    .bind(a.workspace)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(edges, 1);
    runtime.close().await;
}

/// The runtime role reaches the retrieval authority only through its guarded
/// functions, and never writes these tables directly.
#[sqlx::test(migrations = false)]
async fn the_runtime_role_cannot_write_retrieval_tables_directly(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let rows: Vec<(String, String, bool, bool, bool, bool)> = sqlx::query_as(
        "SELECT relname,pg_get_userbyid(relowner),relrowsecurity,relforcerowsecurity, \
                has_table_privilege('vestrace',oid,'SELECT,REFERENCES'), \
                has_table_privilege('vestrace',oid,'INSERT') \
           FROM pg_class WHERE relname=ANY($1) ORDER BY relname",
    )
    .bind(vec![
        "embedding_retrieval_fences",
        "embedding_retrieval_generation_changes",
        "embedding_retrieval_result_references",
        "embedding_retrieval_results",
        "embedding_retrieval_retry_edges",
    ])
    .fetch_all(&runtime)
    .await
    .unwrap();
    assert_eq!(rows.len(), 5);
    for (relation, owner, rls, force, read, insert) in rows {
        assert_eq!(owner, "vestrace_guarded_owner", "{relation} owner");
        assert!(rls && force, "{relation} must force RLS");
        assert!(read, "{relation} runtime read/reference ACL");
        assert!(!insert, "{relation} must refuse runtime INSERT");
    }
    runtime.close().await;
}
