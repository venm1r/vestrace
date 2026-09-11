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

// --- What migration 0205 opened, and the two reads the worker makes through
// the fence it opened them for.

/// Claims one bounded batch of dispatch work as the runtime role.
async fn claim_dispatch(runtime: &PgPool, workspace: Uuid, owner: &str) -> Vec<Uuid> {
    let mut governed = runtime.begin().await.unwrap();
    scoped(&mut governed, workspace).await;
    let rows: Vec<(Uuid,)> =
        sqlx::query_as("SELECT job_id FROM vestrace_claim_embedding_work($1,'dispatch',$2,16)")
            .bind(workspace)
            .bind(owner)
            .fetch_all(&mut *governed)
            .await
            .expect("a dispatch claim");
    governed.commit().await.unwrap();
    rows.into_iter().map(|row| row.0).collect()
}

/// A retrieval query is claimable exactly when it has a live fence.
///
/// Before the fence there is no pinned generation, so dispatching would embed a
/// query against a corpus nothing had agreed on; after the deadline nobody is
/// waiting, so the provider call would be paid for an answer with no reader.
#[sqlx::test(migrations = false)]
async fn only_a_live_fence_makes_a_retrieval_query_claimable(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;

    let unfenced = claim_dispatch(&runtime, a.workspace, "worker:unfenced").await;
    assert!(
        !unfenced.contains(&a.job),
        "a retrieval query with no fence pins no generation and must not be claimable"
    );

    accept_fence(&runtime, &a, Uuid::now_v7())
        .await
        .expect("the fence");
    let fenced = claim_dispatch(&runtime, a.workspace, "worker:fenced").await;
    assert!(
        fenced.contains(&a.job),
        "a fenced retrieval query is exactly the work a worker holding a local index must take"
    );

    // Past its deadline it stops being claimable, and the claim is not merely
    // hidden by the one just taken: a fresh owner sees the same refusal.
    // Both tables are guarded, so the clock is moved as the guarded owner. No
    // product path expires a fence in place; this is a test reaching past the
    // authority on purpose, to observe what the claim does on the other side of
    // a deadline it cannot otherwise wait for.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, a.workspace).await;
    sqlx::query(
        "UPDATE embedding_retrieval_fences SET deadline=now()-INTERVAL '1 second'           WHERE workspace_id=$1 AND job_id=$2",
    )
    .bind(a.workspace)
    .bind(a.job)
    .execute(&mut *owner)
    .await
    .expect("an expired fence");
    sqlx::query("DELETE FROM embedding_job_work_claims WHERE workspace_id=$1 AND job_id=$2")
        .bind(a.workspace)
        .bind(a.job)
        .execute(&mut *owner)
        .await
        .expect("the claim just taken is cleared so a fresh owner may try");
    owner.commit().await.unwrap();
    let expired = claim_dispatch(&runtime, a.workspace, "worker:expired").await;
    assert!(
        !expired.contains(&a.job),
        "past its deadline nobody is waiting for the answer"
    );
    runtime.close().await;
}

/// The worker reads back exactly the generation the fence pinned, in the shape
/// the local registry keys an index by.
#[sqlx::test(migrations = false)]
async fn the_worker_reads_back_the_generation_the_fence_pinned(pool: PgPool) {
    use vestrace_application::embedding::EmbeddingRetrievalRepository;

    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7())
        .await
        .expect("the fence");

    let context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(a.workspace),
        vestrace_domain::PrincipalId::new(),
    );
    let repository = vestrace_infrastructure::postgres::PgEmbeddingRetrievalRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    );
    let (read_fence, snapshot) = repository
        .pinned_attempt(&context, vestrace_domain::EmbeddingJobId::from_uuid(a.job))
        .await
        .expect("the pinned attempt of an accepted retrieval query");

    assert_eq!(read_fence, fence, "the fence a job owns is the one it took");
    assert_eq!(
        snapshot.generation_id.as_uuid(),
        a.generation,
        "the snapshot names the generation the fence pinned"
    );
    assert_eq!(snapshot.workspace_id.as_uuid(), a.workspace);
    assert!(
        snapshot.space.is_canonical(),
        "only a canonical space can carry a Ready generation to answer from"
    );
    // Every number comes from the generation itself, so a snapshot assembled
    // from two sources cannot name a pair that never existed together.
    let stored: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT generation_epoch, captured_guard_version, corpus_revision, \
                built_through_projection_ordinal, member_count \
           FROM embedding_corpus_generations WHERE workspace_id=$1 AND id=$2",
    )
    .bind(a.workspace)
    .bind(a.generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(i64::try_from(snapshot.generation_epoch).unwrap(), stored.0);
    assert_eq!(i64::try_from(snapshot.guard_version).unwrap(), stored.1);
    assert_eq!(i64::try_from(snapshot.corpus_revision).unwrap(), stored.2);
    assert_eq!(
        i64::try_from(snapshot.built_through_projection_ordinal).unwrap(),
        stored.3
    );
    assert_eq!(i64::try_from(snapshot.member_count).unwrap(), stored.4);
    snapshot
        .validate()
        .expect("what the registry is keyed by must be a valid canonical snapshot");
    runtime.close().await;
}

// --- The whole path, once: a question asked against a pinned generation and an
// answer read back from the durable result.
//
// Everything before this proves a piece. This proves they are the same path.
// The one piece it does not rebuild is the local index, because an index is a
// cache and `embedding_index_builds` proves how one is built; what is under
// test here is what retrieval does with a generation, a query and an index that
// already exists.

const E2E_MODEL: &str = "text-embedding-nomic-embed-text-v1.5";
const E2E_OUTPUT_COUNT: usize = 2;

#[derive(Default)]
struct E2eAdapter {
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl vestrace_application::run::GovernedModelAdapter for E2eAdapter {
    async fn execute(
        &self,
        _kind: vestrace_domain::ConnectionKind,
        _runtime_base_url: &str,
        _auth: vestrace_application::ConnectionAuth,
        request: vestrace_application::EffectiveModelRequest,
    ) -> Result<vestrace_application::EffectiveModelResponse, vestrace_application::ProviderError>
    {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(matches!(
            request,
            vestrace_application::EffectiveModelRequest::Embeddings(_)
        ));
        let data = (0..E2E_OUTPUT_COUNT)
            .map(|ordinal| {
                vestrace_application::GovernedEmbeddingVector::from_provider_components(
                    ordinal,
                    vec![1.0; 768],
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(vestrace_application::EffectiveModelResponse::Embeddings(
            vestrace_application::GovernedEmbeddingsResponse::new(
                E2E_MODEL,
                E2E_MODEL.into(),
                data,
                E2E_OUTPUT_COUNT,
            )?,
        ))
    }
}

/// One active memory whose current content becomes a governed source material.
///
/// This is the link the whole answer hangs on. A projection names a material,
/// a material's intent names what owns it, and only a `memory_revision` owner
/// lets a hit be reported as a memory at all -- so a fixture that seeded any
/// other owner would prove the search and nothing about the answer.
async fn e2e_memory_source(
    pool: &PgPool,
    runtime: &PgPool,
    accepted: &common::AcceptedJob,
) -> (Uuid, Uuid, Uuid) {
    let workspace = accepted.context.workspace_id;
    let memory = Uuid::now_v7();
    let revision = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO memories(id,workspace_id,kind,status,state_revision) \
         VALUES($1,$2,'fact','candidate',1)",
    )
    .bind(memory)
    .bind(workspace.as_uuid())
    .execute(pool)
    .await
    .expect("memory");
    sqlx::query(
        "INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,\
         confidence,importance) VALUES($1,$2,$3,1,'the answer a query should find',1.0,0.5)",
    )
    .bind(revision)
    .bind(memory)
    .bind(workspace.as_uuid())
    .execute(pool)
    .await
    .expect("revision");
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision)
        .bind(memory)
        .execute(pool)
        .await
        .expect("active revision");

    let vault_fixture = common::result_preparation_fixture::OutputVaultFixture::new();
    let materializer = vestrace_infrastructure::postgres::PgGovernedContentMaterializer::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
        std::sync::Arc::new(vault_fixture.vault(workspace)),
        std::sync::Arc::new(vestrace_infrastructure::crypto::ContentMaterialCodec::new()),
    );
    let source = materializer
        .materialize_revision(&accepted.context, revision)
        .await
        .expect("materialization must not fail")
        .expect("an intact revision is not a blocker");
    (memory, revision, source.material_id)
}

/// Executes the fixture's delivery job with the memory-owned material among its
/// sources, so the projections it publishes depend on a memory revision.
async fn e2e_publish_projections(
    pool: &PgPool,
    runtime: &PgPool,
    accepted: &common::AcceptedJob,
    memory_source: Uuid,
) {
    use common::result_preparation_fixture::{
        DeliveryPolicyCase, OutputVaultFixture, acceptance_command, attach_source_to_evidence,
        live_source, outputs, reconcile_output_receipts, record_delivery_policy,
    };
    use vestrace_application::EmbeddingOutputKeyRepository;

    let sources = [
        vestrace_domain::ContentMaterialId::from_uuid(memory_source),
        live_source(runtime, accepted).await,
    ];
    common::make_dispatchable_with_policy(pool, runtime, accepted, true).await;
    for (ordinal, source) in sources.into_iter().enumerate() {
        attach_source_to_evidence(pool, accepted, source, 8 + ordinal as i64).await;
    }
    let output_set = outputs();
    let receipt_id = Uuid::now_v7();
    vestrace_infrastructure::postgres::PgEmbeddingOutputKeyRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    )
    .accept_delivery_outputs(
        &accepted.context,
        acceptance_command(accepted, receipt_id, output_set.clone()),
    )
    .await
    .expect("the result chain must accept the delivery job");
    let vault = OutputVaultFixture::new();
    reconcile_output_receipts(runtime, accepted, &output_set, &vault).await;
    record_delivery_policy(runtime, receipt_id, DeliveryPolicyCase::ExactAllowed).await;

    let store = vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone());
    let dispatch = std::sync::Arc::new(common::dispatching_repository(runtime, None));
    let output_vault = std::sync::Arc::new(vault.vault(accepted.context.workspace_id));
    let preparation = std::sync::Arc::new(
        vestrace_application::EmbeddingResultPreparationService::new(
            std::sync::Arc::new(
                vestrace_infrastructure::postgres::PgEmbeddingResultRepository::new(
                    store.clone(),
                    dispatch.clone(),
                ),
            ),
            output_vault.clone(),
            std::sync::Arc::new(vestrace_infrastructure::crypto::ContentMaterialCodec::new()),
        ),
    );
    let finalization = std::sync::Arc::new(
        vestrace_application::EmbeddingResultFinalizationService::new(
            std::sync::Arc::new(
                vestrace_infrastructure::postgres::PgEmbeddingResultFinalizationRepository::new(
                    store,
                ),
            ),
            output_vault,
            std::sync::Arc::new(
                vestrace_infrastructure::postgres::EmbeddingOutputHmacCommitter::new(),
            ),
        ),
    );
    let adapter = std::sync::Arc::new(E2eAdapter::default());
    let outcome = vestrace_application::embedding::EmbeddingExecutor::new(
        dispatch,
        preparation,
        finalization,
        adapter.clone(),
        vestrace_domain::WorkerId::new(),
    )
    .execute(&accepted.context, accepted.job_id)
    .await
    .expect("the delivery job must finalize");
    assert_eq!(
        outcome,
        vestrace_application::embedding::EmbeddingExecutionOutcome::Succeeded
    );
    assert_eq!(adapter.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// A published projection resolves to the memory revision it was computed from.
///
/// This is the link every retrieval answer hangs on and the one that fails
/// silently. A projection names the material it came from, that material's
/// intent names what owns it, and only a `memory_revision` owner lets a hit be
/// reported as a memory at all. Get the join wrong and retrieval returns an
/// empty answer rather than a wrong one -- indistinguishable, from outside,
/// from a corpus that genuinely matched nothing.
///
/// The projections here are real: a delivery job executed through the whole
/// result chain, with one of its sources materialized from a memory revision by
/// the same materializer the on-write route uses.
#[sqlx::test(migrations = false)]
async fn a_published_projection_resolves_to_its_memory_revision(pool: PgPool) {
    use vestrace_application::embedding::EmbeddingRetrievalRepository;

    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let accepted = common::prepare_delivery_embedding_job(&pool, &runtime).await;
    let workspace = accepted.context.workspace_id;

    let (memory, revision, memory_source) = e2e_memory_source(&pool, &runtime, &accepted).await;
    e2e_publish_projections(&pool, &runtime, &accepted, memory_source).await;

    // Two projections, and only one of them was computed from a memory.
    let all: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM embedding_projection_entries WHERE workspace_id=$1 ORDER BY id",
    )
    .bind(workspace.as_uuid())
    .fetch_all(&pool)
    .await
    .expect("the executed job published projections");
    assert_eq!(all.len(), E2E_OUTPUT_COUNT);

    let of_memory: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT dependency.projection_id \
           FROM embedding_projection_source_dependencies AS dependency \
          WHERE dependency.workspace_id=$1 AND dependency.source_material_id=$2",
    )
    .bind(workspace.as_uuid())
    .bind(memory_source)
    .fetch_all(&pool)
    .await
    .expect("the dependency the delivery recorded");
    assert!(
        !of_memory.is_empty(),
        "the delivery must have recorded the memory-owned source as a dependency"
    );

    let repository = vestrace_infrastructure::postgres::PgEmbeddingRetrievalRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    );
    let resolved = repository
        .resolve_members(&accepted.context, &all)
        .await
        .expect("resolution must not fail");

    assert_eq!(
        resolved.len(),
        of_memory.len(),
        "exactly the projections computed from a memory resolve; a projection whose source is \
         some other governed input has no memory to name"
    );
    for member in &resolved {
        assert!(
            of_memory.contains(&member.projection_id),
            "only a projection that depends on the memory-owned source may resolve"
        );
        assert_eq!(member.memory_id, memory, "the memory the projection means");
        assert_eq!(member.revision_id, revision, "and its exact revision");
    }

    // An unknown projection resolves to nothing rather than to something.
    let absent = repository
        .resolve_members(&accepted.context, &[Uuid::now_v7()])
        .await
        .expect("an unknown projection is not a failure");
    assert!(absent.is_empty());

    // That a projection stops resolving once its source leaves Live is not
    // asserted here. Taking a material out of Live is the two-phase erasure's
    // job and the schema refuses any shortcut to it -- `Live material requires
    // its exact Bound promotion` -- so proving it needs the vault sequence,
    // which `embedding_erasure_propagation` already drives. What that suite
    // does not do is read the answer afterwards.
    runtime.close().await;
}

/// The read model reports what the durable record holds, and nothing it does
/// not.
///
/// Driven through the real guarded functions rather than by inserting rows,
/// because the view's job is to state the outcome of those functions. Rows
/// written by hand would prove that the SELECT reads its own columns and
/// nothing about whether those columns mean what the view says they mean.
#[sqlx::test(migrations = false)]
async fn the_attempt_view_reports_what_the_durable_record_holds(pool: PgPool) {
    use vestrace_application::embedding::EmbeddingRetrievalRepository;

    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let request = Uuid::now_v7();
    let fence = accept_fence(&runtime, &a, request).await.unwrap();

    let context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(a.workspace),
        vestrace_domain::PrincipalId::new(),
    );
    let repository = vestrace_infrastructure::postgres::PgEmbeddingRetrievalRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    );
    let job_id = vestrace_domain::EmbeddingJobId::from_uuid(a.job);

    // An accepted attempt that has not finished: pinned, and undecided.
    let view = repository
        .attempt_view(&context, job_id)
        .await
        .expect("an accepted attempt is readable")
        .expect("an accepted attempt exists");
    assert_eq!(view.job_id.as_uuid(), a.job);
    assert_eq!(view.request_id.as_uuid(), request);
    assert_eq!(view.space_registration_id, a.space);
    assert_eq!(view.generation_id, a.generation);
    assert_eq!(
        view.reference_count, None,
        "no terminal result is absent, not zero"
    );
    assert_eq!(view.degradation_reason, None);
    assert_eq!(view.generation_changed_reason, None);
    assert!(!view.retry_available, "nothing has been declined yet");
    assert_eq!(view.predecessor_job_id, None);
    assert_eq!(view.successor_job_id, None);

    // What it reports about the generation is what the fence pinned, not what
    // the generation happens to be now.
    let (pinned_epoch, pinned_members): (i64, i64) = sqlx::query_as(
        "SELECT generation_epoch, member_count FROM embedding_retrieval_fences \
          WHERE workspace_id=$1 AND id=$2",
    )
    .bind(a.workspace)
    .bind(fence)
    .fetch_one(&pool)
    .await
    .expect("the fence this attempt took");
    assert_eq!(view.generation_epoch, pinned_epoch as u64);
    assert_eq!(view.generation_member_count, pinned_members as u64);

    // A job that exists but was never admitted as an attempt has no view. The
    // fence is what makes a job an attempt, so a job with none has not declined
    // and has not answered -- it has not started, and a zeroed view would say
    // otherwise.
    let base = common::prepare_delivery_embedding_job(&pool, &runtime).await;
    let unfenced = accept_retrieval_job(&pool, &runtime, &base).await;
    let unfenced_context = vestrace_application::RequestContext::new(
        base.context.workspace_id,
        vestrace_domain::PrincipalId::new(),
    );
    assert!(
        repository
            .attempt_view(
                &unfenced_context,
                vestrace_domain::EmbeddingJobId::from_uuid(unfenced)
            )
            .await
            .expect("a job with no fence is not a failure")
            .is_none(),
        "a job that took no fence must not read back as a retrieval that found nothing"
    );

    // And a job in another workspace is absent rather than readable. The view
    // is scoped by the request context, not by the identity the caller states.
    let elsewhere = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::new(),
        vestrace_domain::PrincipalId::new(),
    );
    assert!(
        repository
            .attempt_view(&elsewhere, job_id)
            .await
            .expect("a foreign attempt is not a failure")
            .is_none(),
        "an attempt must not be readable from another workspace"
    );

    runtime.close().await;
}

/// A terminal result reads back as its count, and a declined attempt reads back
/// as its closed reason and the successor it earned.
#[sqlx::test(migrations = false)]
async fn a_terminal_attempt_reads_back_as_its_outcome_and_its_lineage(pool: PgPool) {
    use vestrace_application::embedding::EmbeddingRetrievalRepository;

    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let repository = vestrace_infrastructure::postgres::PgEmbeddingRetrievalRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    );

    // One attempt that answered.
    let answered = attempt(&pool, &runtime).await;
    let answered_fence = accept_fence(&runtime, &answered, Uuid::now_v7())
        .await
        .unwrap();
    // The reference table names identities and does not resolve them; what a
    // reference means is `resolve_members`' job, proven in its own test.
    let (memory, revision) = (Uuid::now_v7(), Uuid::now_v7());
    finalize(
        &runtime,
        &answered,
        answered_fence,
        &[(memory, revision, 0, 0.75)],
    )
    .await
    .expect("a result lands on a current generation");

    let context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(answered.workspace),
        vestrace_domain::PrincipalId::new(),
    );
    let view = repository
        .attempt_view(
            &context,
            vestrace_domain::EmbeddingJobId::from_uuid(answered.job),
        )
        .await
        .unwrap()
        .expect("an answered attempt exists");
    assert_eq!(
        view.reference_count,
        Some(1),
        "the count of what answered, not the references themselves"
    );
    assert_eq!(
        view.degradation_reason, None,
        "an attempt that answered did not degrade"
    );
    assert!(!view.retry_available);

    // And one that was declined, then retried.
    let declined = attempt(&pool, &runtime).await;
    let declined_fence = accept_fence(&runtime, &declined, Uuid::now_v7())
        .await
        .unwrap();
    observe_change(&runtime, &declined, declined_fence, "corpus_changed")
        .await
        .expect("a generation change is terminal");
    let declined_context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(declined.workspace),
        vestrace_domain::PrincipalId::new(),
    );
    let declined_id = vestrace_domain::EmbeddingJobId::from_uuid(declined.job);

    let view = repository
        .attempt_view(&declined_context, declined_id)
        .await
        .unwrap()
        .expect("a declined attempt exists");
    assert_eq!(
        view.degradation_reason,
        Some("retrieval_generation_changed")
    );
    assert_eq!(
        view.generation_changed_reason,
        Some(vestrace_domain::embedding::RetrievalGenerationChangedReason::CorpusChanged)
    );
    assert!(
        view.retry_available,
        "a confirmed change with no successor is the one case a retry may follow"
    );
    assert_eq!(view.reference_count, None);

    let successor = Uuid::now_v7();
    authorize_retry(
        &runtime,
        &declined,
        successor,
        Uuid::now_v7(),
        "attempt-view-key",
    )
    .await
    .expect("a confirmed change authorizes one successor");

    let view = repository
        .attempt_view(&declined_context, declined_id)
        .await
        .unwrap()
        .expect("the predecessor is still readable");
    assert_eq!(
        view.successor_job_id.map(|id| id.as_uuid()),
        Some(successor),
        "the predecessor names the one successor it earned"
    );
    assert!(
        !view.retry_available,
        "a predecessor has exactly one successor; once spent, the retry is gone"
    );

    runtime.close().await;
}

/// The retry queue holds exactly the attempts a confirmed change left
/// retryable, and drops each one the moment its successor is authorized.
///
/// This is the route by which an HTTP caller learns a predecessor job id at
/// all, so what it must never do is name an attempt that the single read then
/// says is not retryable. Both are driven here against the same rows.
#[sqlx::test(migrations = false)]
async fn the_retry_queue_holds_only_unspent_confirmed_changes(pool: PgPool) {
    use vestrace_application::embedding::EmbeddingRetrievalRepository;

    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let repository = vestrace_infrastructure::postgres::PgEmbeddingRetrievalRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    );

    // One workspace holding three attempts in three different conditions.
    let base = common::prepare_delivery_embedding_job(&pool, &runtime).await;
    let workspace = base.context.workspace_id;
    let context =
        vestrace_application::RequestContext::new(workspace, vestrace_domain::PrincipalId::new());

    // Nothing has declined yet.
    let empty = repository
        .attempts_awaiting_retry(&context)
        .await
        .expect("an empty queue is an answer");
    assert!(empty.attempts.is_empty());
    assert!(!empty.truncated);

    let declined = attempt(&pool, &runtime).await;
    let declined_fence = accept_fence(&runtime, &declined, Uuid::now_v7())
        .await
        .unwrap();
    observe_change(&runtime, &declined, declined_fence, "revoked")
        .await
        .expect("a generation change is terminal");
    let declined_context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(declined.workspace),
        vestrace_domain::PrincipalId::new(),
    );

    // An attempt that answered is not in the queue: it has nothing to decide.
    let answered = attempt(&pool, &runtime).await;
    let answered_fence = accept_fence(&runtime, &answered, Uuid::now_v7())
        .await
        .unwrap();
    finalize(
        &runtime,
        &answered,
        answered_fence,
        &[(Uuid::now_v7(), Uuid::now_v7(), 0, 0.5)],
    )
    .await
    .expect("a result lands on a current generation");

    // Each `attempt` builds its own workspace, so the queue is read per
    // workspace -- which is itself the scoping claim worth making.
    let queue = repository
        .attempts_awaiting_retry(&declined_context)
        .await
        .expect("the declined workspace has a queue");
    assert_eq!(
        queue.attempts.len(),
        1,
        "exactly the one confirmed change is waiting"
    );
    assert!(!queue.truncated);
    let waiting = &queue.attempts[0];
    assert_eq!(waiting.job_id.as_uuid(), declined.job);
    assert!(waiting.retry_available);
    assert_eq!(
        waiting.generation_changed_reason,
        Some(vestrace_domain::embedding::RetrievalGenerationChangedReason::Revoked)
    );

    // The queue and the single read describe the same attempt identically.
    let single = repository
        .attempt_view(&declined_context, waiting.job_id)
        .await
        .unwrap()
        .expect("the queued attempt is readable on its own");
    assert_eq!(
        &single, waiting,
        "the queue must not describe an attempt differently from the read an \
         operator confirms against"
    );

    // The answered workspace's queue is empty.
    let answered_context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(answered.workspace),
        vestrace_domain::PrincipalId::new(),
    );
    assert!(
        repository
            .attempts_awaiting_retry(&answered_context)
            .await
            .unwrap()
            .attempts
            .is_empty(),
        "an attempt that answered has no decision waiting on it"
    );

    // Authorizing the successor empties the queue for all time: a predecessor
    // has exactly one successor, so the edge that appears is permanent.
    authorize_retry(
        &runtime,
        &declined,
        Uuid::now_v7(),
        Uuid::now_v7(),
        "queue-key",
    )
    .await
    .expect("a confirmed change authorizes one successor");
    assert!(
        repository
            .attempts_awaiting_retry(&declined_context)
            .await
            .unwrap()
            .attempts
            .is_empty(),
        "a spent retry leaves the queue"
    );

    runtime.close().await;
}

/// The signature whose primary predicate this qualification mutates.
const RETRY_AUTHORITY: &str =
    "public.vestrace_authorize_embedding_retrieval_retry(uuid,uuid,uuid,uuid,text)";

/// The one-successor lookup, and the smallest edit that disables it.
///
/// `AND FALSE` rather than a deleted block: the mutation has to be a *predicate*
/// change, so that what is being qualified is the rule and not the shape of the
/// function around it. It also restores by exact string, which is what makes
/// the byte-equality check afterwards meaningful.
const ONE_SUCCESSOR_NEEDLE: &str = "WHERE workspace_id=target_workspace AND predecessor_job_id=target_predecessor\n     FOR UPDATE";
const ONE_SUCCESSOR_MUTATION: &str = "WHERE workspace_id=target_workspace AND predecessor_job_id=target_predecessor AND FALSE\n     FOR UPDATE";

/// What the catalogue holds for a function: its definition and its posture.
///
/// Both, because a mutation that restored the text and lost the owner would
/// leave a SECURITY DEFINER function running as somebody else -- which is the
/// defect this package already found once, in the provisioner's hand-back.
async fn authority_state(pool: &PgPool, signature: &str) -> (String, String, Option<String>, bool) {
    sqlx::query_as(
        "SELECT pg_get_functiondef(oid), pg_get_userbyid(proowner), \
                array_to_string(proacl,'|'), \
                has_function_privilege('vestrace',oid,'EXECUTE') \
           FROM pg_proc WHERE oid=$1::regprocedure",
    )
    .bind(signature)
    .fetch_one(pool)
    .await
    .expect("the authority is in the catalogue")
}

/// Install one definition, leaving everything about the function except its
/// body exactly as it was.
///
/// Run as the connected superuser rather than as the guarded owner: the public
/// schema belongs to the runtime role and the guarded owner holds no CREATE on
/// it, so the owner cannot replace its own function. `CREATE OR REPLACE` keeps
/// the existing owner and ACL, and the caller checks that -- because a
/// SECURITY DEFINER function that changed hands would run the mutated body as
/// somebody else, and the observation would then be about the role rather than
/// about the predicate.
async fn install_authority(pool: &PgPool, definition: &str) {
    sqlx::raw_sql(definition)
        .execute(pool)
        .await
        .expect("the authority definition is installable");
}

/// Drive the one-successor rule and say how it was refused, or that it was not.
///
/// Returns the SQLSTATE and message of the refusal a second, differently-keyed
/// authorization earns. The point of naming both is that a mutation may move
/// the refusal to another defence rather than removing it, and those two
/// outcomes are not the same result.
async fn second_key_refusal(pool: &PgPool, runtime: &PgPool) -> Result<Uuid, (String, String)> {
    let a = attempt(pool, runtime).await;
    let fence = accept_fence(runtime, &a, Uuid::now_v7()).await.unwrap();
    observe_change(runtime, &a, fence, "revoked").await.unwrap();
    authorize_retry(runtime, &a, Uuid::now_v7(), Uuid::now_v7(), "key-1")
        .await
        .expect("a confirmed change authorizes its first successor");
    match authorize_retry(runtime, &a, Uuid::now_v7(), Uuid::now_v7(), "key-2").await {
        Ok(id) => Ok(id),
        Err(error) => {
            let database = error.as_database_error().expect("a database refusal");
            Err((
                database
                    .code()
                    .map(|code| code.into_owned())
                    .unwrap_or_default(),
                database.message().to_owned(),
            ))
        }
    }
}

/// Mutation qualification: one predicate, one run, restored byte-exactly.
///
/// The rule under test is that a predecessor has one successor for all time.
/// Two independent defences enforce it -- this function's own lookup for an
/// existing edge, and the table's primary key. Disabling the lookup is what
/// tells them apart: if the guarded refusal simply disappeared, the rule would
/// rest on a message; if some other defence catches it, the rule is layered and
/// the qualification says at which boundary.
///
/// The restore is checked on the definition *and* the posture, and the rule is
/// re-driven afterwards, because a qualification that left the authority
/// subtly different would make every later test in this suite a test of the
/// mutation.
#[sqlx::test(migrations = false)]
async fn mutating_the_one_successor_lookup_moves_the_refusal_and_restores_exactly(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;

    let (original, owner, acl, runtime_execute) = authority_state(&pool, RETRY_AUTHORITY).await;
    assert!(
        original.contains(ONE_SUCCESSOR_NEEDLE),
        "the predicate this qualification mutates is no longer in the authority; \
         the mutation would silently test nothing: {original}"
    );

    // Green before: the guarded refusal, by its own message.
    let before = second_key_refusal(&pool, &runtime).await;
    let (before_state, before_message) = before.expect_err("a second key must be refused");
    assert_eq!(before_state, "23505");
    assert!(
        before_message.contains("predecessor already has its successor"),
        "{before_message}"
    );

    // Mutate exactly one predicate.
    install_authority(
        &pool,
        &original.replace(ONE_SUCCESSOR_NEEDLE, ONE_SUCCESSOR_MUTATION),
    )
    .await;
    let (mutated, mutated_owner, mutated_acl, mutated_execute) =
        authority_state(&pool, RETRY_AUTHORITY).await;
    assert_ne!(mutated, original, "the mutation must actually be installed");
    // Only the body may differ. A mutation that also moved the owner would be
    // two mutations, and the one that mattered would be the one nobody chose.
    assert_eq!(
        mutated_owner, owner,
        "the mutation must not change the owner"
    );
    assert_eq!(mutated_acl, acl, "nor the access control list");
    assert_eq!(
        mutated_execute, runtime_execute,
        "nor whether the runtime role may call it"
    );

    // Red: the rule still holds, and the qualification records where.
    let (mutated_state, mutated_message) = second_key_refusal(&pool, &runtime)
        .await
        .expect_err("a second successor must not become reachable");
    assert_eq!(
        mutated_state, "23505",
        "the table's primary key is the independent defence, and it is still a \
         unique violation"
    );
    assert!(
        !mutated_message.contains("predecessor already has its successor"),
        "with the lookup disabled the guarded message cannot be what refused; \
         an unchanged message would mean the mutation never took: {mutated_message}"
    );
    assert!(
        mutated_message.contains("embedding_retrieval_retry_edges"),
        "the refusal now names the constraint rather than the rule, which is the \
         different boundary this run exists to record: {mutated_message}"
    );

    // Restore, byte-exactly, and prove it.
    install_authority(&pool, &original).await;
    let (restored, restored_owner, restored_acl, restored_execute) =
        authority_state(&pool, RETRY_AUTHORITY).await;
    assert_eq!(
        restored, original,
        "the definition must be restored exactly"
    );
    assert_eq!(restored_owner, owner, "and so must its owner");
    assert_eq!(restored_acl, acl, "and its access control list");
    assert_eq!(
        restored_execute, runtime_execute,
        "and the runtime role's ability to call it"
    );

    // Green after: the guarded refusal is back, by its own message.
    let (after_state, after_message) = second_key_refusal(&pool, &runtime)
        .await
        .expect_err("a second key must be refused again");
    assert_eq!(after_state, before_state);
    assert_eq!(after_message, before_message);

    runtime.close().await;
}

const FINALIZE_AUTHORITY: &str = "public.vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[])";

/// The pinned-generation revalidation, and the edit that disables all of it.
///
/// All six clauses, not one: they raise a single refusal between them, so the
/// primary predicate is the revalidation itself. Disabling one clause would
/// qualify the redundancy inside it; disabling the predicate asks the question
/// the qualification exists for -- whether anything else stands between a moved
/// corpus and a stored answer.
const FENCE_NEEDLE: &str = "    IF guard_row.current_generation_id IS DISTINCT FROM fence_row.generation_id\n       OR guard_row.guard_version <> fence_row.guard_version\n       OR generation_row.state <> 'ready'\n       OR generation_row.generation_epoch <> fence_row.generation_epoch\n       OR generation_row.corpus_revision <> fence_row.corpus_revision\n       OR generation_row.member_count <> fence_row.member_count THEN";
const FENCE_MUTATION: &str = "    IF FALSE THEN";

/// Move the corpus under an accepted attempt, then try to land its answer.
///
/// Returns the refusal, or the result identity if one was stored. A stored
/// result here is the unsafe state itself: an answer attributed to a generation
/// that no longer exists, which is exactly what the fence is for.
async fn land_on_a_moved_generation(
    pool: &PgPool,
    runtime: &PgPool,
) -> (Attempt, Result<Uuid, (String, String)>) {
    let a = attempt(pool, runtime).await;
    let fence = accept_fence(runtime, &a, Uuid::now_v7()).await.unwrap();
    advance_generation(pool, runtime, &a).await;
    let landed = finalize(
        runtime,
        &a,
        fence,
        &[(Uuid::now_v7(), Uuid::now_v7(), 0, 0.5)],
    )
    .await;
    let outcome = match landed {
        Ok(id) => Ok(id),
        Err(error) => {
            let database = error.as_database_error().expect("a database refusal");
            Err((
                database
                    .code()
                    .map(|code| code.into_owned())
                    .unwrap_or_default(),
                database.message().to_owned(),
            ))
        }
    };
    (a, outcome)
}

/// Mutation qualification: the pinned-generation fence is the only thing
/// between a moved corpus and a stored answer.
///
/// With the predicate disabled the result *lands*. Nothing else refuses it: no
/// constraint, no trigger, no later check. So unlike the one-successor rule --
/// which the table's primary key also enforces -- this rule rests on this
/// predicate alone, and that is what the run records.
///
/// It is worth saying plainly what the mutated world contains, because it is
/// the thing the fence prevents: a terminal retrieval result, attributed to a
/// job, whose fence names a generation the corpus has already replaced. A
/// caller reading it would be told what the corpus used to say, with nothing
/// marking it stale.
#[sqlx::test(migrations = false)]
async fn mutating_the_pinned_generation_fence_lets_a_stale_answer_land(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;

    let (original, owner, acl, runtime_execute) = authority_state(&pool, FINALIZE_AUTHORITY).await;
    assert!(
        original.contains(FENCE_NEEDLE),
        "the predicate this qualification mutates is no longer in the authority; \
         the mutation would silently test nothing: {original}"
    );

    // Green before: the fence refuses, by its own message, and stores nothing.
    let (before_attempt, before) = land_on_a_moved_generation(&pool, &runtime).await;
    let (before_state, before_message) =
        before.expect_err("a moved generation must refuse its answer");
    assert_eq!(before_state, "23514");
    assert!(
        before_message.contains("requires its exact pinned generation"),
        "{before_message}"
    );
    assert_eq!(
        counts(&pool, &before_attempt).await,
        (0, 0),
        "a refused answer leaves neither a result nor a change"
    );

    install_authority(&pool, &original.replace(FENCE_NEEDLE, FENCE_MUTATION)).await;
    let (mutated, mutated_owner, mutated_acl, mutated_execute) =
        authority_state(&pool, FINALIZE_AUTHORITY).await;
    assert_ne!(mutated, original, "the mutation must actually be installed");
    assert_eq!(
        mutated_owner, owner,
        "the mutation must not change the owner"
    );
    assert_eq!(mutated_acl, acl, "nor the access control list");
    assert_eq!(mutated_execute, runtime_execute, "nor its reachability");

    // Red, and red in the sharpest way: the answer lands.
    let (mutated_attempt, landed) = land_on_a_moved_generation(&pool, &runtime).await;
    landed.expect(
        "with the fence disabled nothing else refuses a stale answer; if this is a \
         refusal, some other defence exists and the qualification must say which",
    );
    assert_eq!(
        counts(&pool, &mutated_attempt).await,
        (1, 0),
        "the unsafe state is a stored terminal result against a generation the \
         corpus has already replaced"
    );

    // Restore, byte-exactly, and prove it.
    install_authority(&pool, &original).await;
    let (restored, restored_owner, restored_acl, restored_execute) =
        authority_state(&pool, FINALIZE_AUTHORITY).await;
    assert_eq!(
        restored, original,
        "the definition must be restored exactly"
    );
    assert_eq!(restored_owner, owner);
    assert_eq!(restored_acl, acl);
    assert_eq!(restored_execute, runtime_execute);

    // Green after: the same refusal, and nothing stored.
    let (after_attempt, after) = land_on_a_moved_generation(&pool, &runtime).await;
    let (after_state, after_message) = after.expect_err("the fence must refuse again");
    assert_eq!(after_state, before_state);
    assert_eq!(after_message, before_message);
    assert_eq!(counts(&pool, &after_attempt).await, (0, 0));

    runtime.close().await;
}
