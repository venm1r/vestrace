//! What still fences a retrieval query, now that the legacy path is retired.
//!
//! This suite used to drive `PgEmbeddingStore::upsert` and
//! `PgVectorRetriever::search`.  Both are permanent stubs since the canonical
//! transition: legacy writes are retired and the legacy retriever answers
//! nothing.  Twelve tests here were therefore exercising a subject that no
//! longer exists, and every one of them was red.
//!
//! The live authority is `PgCorpusGenerationResolver`.  It decides which
//! generation a query may be pinned to, so it is what this suite proves now.
//! The retirement itself is proven in embedding_canonical_generations:
//! `legacy_upsert_refuses_before_database_or_dimension_work`,
//! `legacy_vector_search_never_calls_provider_policy_or_database`, and
//! `seeded_legacy_history_survives_upgrade_but_runtime_cannot_use_or_mutate_it`.

mod common;

use common::result_preparation_fixture::provision_result_behavior_database;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use vestrace_application::{ApplicationError, retrieval::CorpusGenerationResolver};
use vestrace_infrastructure::postgres::{PgCorpusGenerationResolver, PgStore};

const SPACE_NAME: &str = "activation-canonical";

async fn scoped(transaction: &mut Transaction<'_, Postgres>, workspace: Uuid) {
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut **transaction)
        .await
        .unwrap();
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

/// The resolver starts from the qualification head, so a canonical space is
/// only reachable once the head actually names it as the active space.
async fn point_head_at(
    pool: &PgPool,
    fixture: &common::AcceptedJob,
    registration: Uuid,
    qualification: Uuid,
) {
    let workspace = fixture.context.workspace_id.as_uuid();
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, workspace).await;
    sqlx::query(
        "INSERT INTO model_qualification_heads(workspace_id,model_revision_id,         current_qualification_revision_id,active_space_registration_id,version)          VALUES($1,$2,$3,$4,1)",
    )
    .bind(workspace)
    .bind(fixture.model_revision_id)
    .bind(qualification)
    .bind(registration)
    .execute(&mut *owner)
    .await
    .expect("the head names its canonical active space");
    owner.commit().await.unwrap();
}

async fn wire_model(pool: &PgPool, fixture: &common::AcceptedJob) -> String {
    sqlx::query_scalar("SELECT wire_model_id FROM model_revisions WHERE workspace_id=$1 AND id=$2")
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.model_revision_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Captures and publishes one Ready canonical generation on `space`.
async fn publish_generation(pool: &PgPool, runtime: &PgPool, workspace: Uuid, space: Uuid) -> Uuid {
    let current: i64 = sqlx::query_scalar(
        "SELECT guard_version FROM embedding_index_generation_guards \
         WHERE workspace_id=$1 AND space_registration_id=$2",
    )
    .bind(workspace)
    .bind(space)
    .fetch_one(pool)
    .await
    .unwrap();
    let generation = Uuid::now_v7();
    let mut governed = runtime.begin().await.unwrap();
    scoped(&mut governed, workspace).await;
    sqlx::query("SELECT vestrace_capture_embedding_generation($1,$2,$3,$4)")
        .bind(generation)
        .bind(workspace)
        .bind(space)
        .bind(current)
        .execute(&mut *governed)
        .await
        .expect("one captured canonical generation");
    sqlx::query("SELECT vestrace_publish_embedding_generation($1,$2,$3,$4)")
        .bind(workspace)
        .bind(space)
        .bind(generation)
        .bind(current)
        .execute(&mut *governed)
        .await
        .expect("the captured generation must publish Ready");
    governed.commit().await.unwrap();
    generation
}

fn assert_canonical_required<T: std::fmt::Debug>(result: Result<T, ApplicationError>) {
    match result {
        Err(ApplicationError::Unavailable(message)) => assert_eq!(
            message, "embedding-canonical-generation-required",
            "the resolver must refuse with its exact closed reason"
        ),
        other => panic!("expected the canonical-generation refusal, got {other:?}"),
    }
}

/// The live fence: a space carrying a Ready canonical generation resolves to
/// exactly that generation.
#[sqlx::test(migrations = false)]
async fn the_current_canonical_generation_resolves_for_its_exact_space(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let accepted = common::prepare_delivery_embedding_job(&pool, &runtime).await;
    let workspace = accepted.context.workspace_id.as_uuid();
    let (space, qualification) = register_canonical_space(&pool, &runtime, &accepted).await;
    point_head_at(&pool, &accepted, space, qualification).await;
    let generation = publish_generation(&pool, &runtime, workspace, space).await;
    let model = wire_model(&pool, &accepted).await;

    let resolver = PgCorpusGenerationResolver::new(PgStore::from_pool(runtime.clone()));
    let snapshot = resolver
        .resolve_canonical(&accepted.context, SPACE_NAME, &model)
        .await
        .expect("a Ready canonical generation resolves");
    assert_eq!(snapshot.generation_id.as_uuid(), generation);
    assert_eq!(snapshot.member_count, 0);
    runtime.close().await;
}

/// A canonical space with no published generation is refused with the exact
/// closed reason rather than an empty answer.
#[sqlx::test(migrations = false)]
async fn a_space_without_a_published_generation_is_refused(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let accepted = common::prepare_delivery_embedding_job(&pool, &runtime).await;
    let (space, qualification) = register_canonical_space(&pool, &runtime, &accepted).await;
    point_head_at(&pool, &accepted, space, qualification).await;
    let model = wire_model(&pool, &accepted).await;

    let resolver = PgCorpusGenerationResolver::new(PgStore::from_pool(runtime.clone()));
    assert_canonical_required(
        resolver
            .resolve_canonical(&accepted.context, SPACE_NAME, &model)
            .await,
    );
    runtime.close().await;
}

/// Publishing a successor moves the fence, so a query pinned to the
/// predecessor can no longer claim to answer the current corpus.
#[sqlx::test(migrations = false)]
async fn a_superseded_generation_is_no_longer_what_the_fence_names(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let accepted = common::prepare_delivery_embedding_job(&pool, &runtime).await;
    let workspace = accepted.context.workspace_id.as_uuid();
    let (space, qualification) = register_canonical_space(&pool, &runtime, &accepted).await;
    point_head_at(&pool, &accepted, space, qualification).await;
    let first = publish_generation(&pool, &runtime, workspace, space).await;
    let model = wire_model(&pool, &accepted).await;

    let resolver = PgCorpusGenerationResolver::new(PgStore::from_pool(runtime.clone()));
    let before = resolver
        .resolve_canonical(&accepted.context, SPACE_NAME, &model)
        .await
        .unwrap();
    assert_eq!(before.generation_id.as_uuid(), first);

    let second = publish_generation(&pool, &runtime, workspace, space).await;
    assert_ne!(first, second);
    let after = resolver
        .resolve_canonical(&accepted.context, SPACE_NAME, &model)
        .await
        .unwrap();
    assert_eq!(after.generation_id.as_uuid(), second);
    assert!(
        after.guard_version > before.guard_version,
        "a successor must advance the guard the fence pins against"
    );
    runtime.close().await;
}

/// A pin cannot be borrowed from another name or another wire model.
#[sqlx::test(migrations = false)]
async fn another_name_or_model_is_not_a_valid_canonical_pin(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let accepted = common::prepare_delivery_embedding_job(&pool, &runtime).await;
    let workspace = accepted.context.workspace_id.as_uuid();
    let (space, qualification) = register_canonical_space(&pool, &runtime, &accepted).await;
    point_head_at(&pool, &accepted, space, qualification).await;
    publish_generation(&pool, &runtime, workspace, space).await;
    let model = wire_model(&pool, &accepted).await;

    let resolver = PgCorpusGenerationResolver::new(PgStore::from_pool(runtime.clone()));
    assert_canonical_required(
        resolver
            .resolve_canonical(&accepted.context, "some-other-space", &model)
            .await,
    );
    assert_canonical_required(
        resolver
            .resolve_canonical(&accepted.context, SPACE_NAME, "some-other-model")
            .await,
    );
    runtime.close().await;
}
