//! What governed legacy adoption fixes, refuses, and never fabricates.
//!
//! The happy-path cutover is not proven here. Committing one needs a canonical
//! generation holding real encrypted projections, which needs the full q1
//! evidence chain and a provider round trip; that composition arrives with the
//! worker in Task 11. What is proven here is everything the database decides on
//! its own: what a plan fixes, what it refuses, which blockers are visible, and
//! that the retirement gate stays shut.

mod common;

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, CreateModelRequestEvidence, EffectiveRequestLimits, FenceReceipt,
    ModelRequestEvidenceRepository, RequestContext, TransactionManager,
    embedding::{
        EmbeddingLegacyAdoptionRepository, LegacyAdoptionBlocker, LegacyAdoptionMember,
        LegacyAdoptionMemberState, LegacyAdoptionRebuildFactory, LegacyAdoptionSourceMaterializer,
        MaterializedSource, StartLegacyAdoption,
    },
    material::{MaterialKeyVault, VaultError},
};
use vestrace_domain::{
    ErasureReceipt, IntentNonce, MaterialKeyId, PrincipalId, VaultReceipt, WorkspaceId,
    ZeroizingDek, embedding::LegacyAdoptionState, id::LegacyAdoptionId,
};
use vestrace_infrastructure::crypto::ContentMaterialCodec;
use vestrace_infrastructure::postgres::{
    PgEmbeddingJobRepository, PgEmbeddingLegacyAdoptionRepository, PgLegacyAdoptionRebuildFactory,
    PgLegacyAdoptionSourceMaterializer, PgModelRequestEvidenceRepository, PgStore,
    PgTransactionManager,
};

struct LegacySpace {
    context: RequestContext,
    legacy_registration: Uuid,
    canonical_registration: Uuid,
    live_memory: Uuid,
    erased_memory: Uuid,
}

/// Seeds one legacy space holding two vectors: one whose memory is intact and
/// one whose memory has been deleted.
///
/// The canonical target is written directly rather than through
/// `vestrace_register_canonical_embedding_space`, because what is under test is
/// adoption, not registration. Registration is proven in
/// embedding_canonical_generations.
async fn seed_legacy_space(pool: &PgPool) -> LegacySpace {
    let workspace = WorkspaceId::new();
    let principal = PrincipalId::new();
    let context = RequestContext::new(workspace, principal);
    let live_memory = Uuid::now_v7();
    let erased_memory = Uuid::now_v7();
    let revision = Uuid::now_v7();
    let space = Uuid::now_v7();
    let legacy_registration = Uuid::now_v7();
    let canonical_registration = Uuid::now_v7();

    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(workspace.as_uuid())
        .bind(format!("adoption-{}", workspace.as_uuid()))
        .execute(pool)
        .await
        .expect("workspace");
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,'adoption')")
        .bind(principal.as_uuid())
        .bind(workspace.as_uuid())
        .execute(pool)
        .await
        .expect("principal");
    sqlx::query(
        "INSERT INTO memories(id,workspace_id,kind,status,state_revision) \
         VALUES($1,$3,'fact','candidate',1),($2,$3,'fact','deleted',1)",
    )
    .bind(live_memory)
    .bind(erased_memory)
    .bind(workspace.as_uuid())
    .execute(pool)
    .await
    .expect("memories");
    sqlx::query(
        "INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance) \
         VALUES($1,$2,$3,1,'adoption source content',1.0,0.5)",
    )
    .bind(revision)
    .bind(live_memory)
    .bind(workspace.as_uuid())
    .execute(pool)
    .await
    .expect("revision");
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision)
        .bind(live_memory)
        .execute(pool)
        .await
        .expect("active revision");
    sqlx::query("INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) VALUES($1,$2,'legacy',2,'legacy-model')")
        .bind(space)
        .bind(workspace.as_uuid())
        .execute(pool)
        .await
        .expect("space");

    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *owner)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO embedding_space_registrations(id,workspace_id,space_id,name,model,dimensions,registration_kind) \
         VALUES($1,$2,$3,'legacy','legacy-model',2,'legacy_upgrade')",
    )
    .bind(legacy_registration)
    .bind(workspace.as_uuid())
    .bind(space)
    .execute(&mut *owner)
    .await
    .expect("legacy registration");
    owner.commit().await.unwrap();

    sqlx::query(
        "INSERT INTO memory_embeddings(id,memory_id,workspace_id,space_id,embedding) \
         VALUES($1,$3,$5,$6,'[1,0]'::vector),($2,$4,$5,$6,'[0,1]'::vector)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(live_memory)
    .bind(erased_memory)
    .bind(workspace.as_uuid())
    .bind(space)
    .execute(pool)
    .await
    .expect("two legacy vectors");

    seed_canonical_registration(pool, workspace.as_uuid(), canonical_registration).await;

    LegacySpace {
        context,
        legacy_registration,
        canonical_registration,
        live_memory,
        erased_memory,
    }
}

async fn seed_canonical_registration(pool: &PgPool, workspace: Uuid, registration: Uuid) {
    let mut owner = pool.begin().await.unwrap();
    // 0197 requires a canonical registration to carry its full q1 structural
    // evidence, enforced by both a CHECK and a validating trigger. Producing
    // that evidence is registration's business and is proven in
    // embedding_canonical_generations; here the canonical space is scaffolding
    // for adoption's own refusals, so both are relaxed inside this transaction.
    //
    // What this costs: no test in this file proves adoption against a space the
    // real registration authority accepted. That composition arrives with the
    // worker in Task 11.
    sqlx::query("ALTER TABLE embedding_space_registrations DROP CONSTRAINT embedding_space_registration_representation")
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE embedding_space_registrations DISABLE TRIGGER embedding_space_registrations_canonical_consistent")
        .execute(&mut *owner)
        .await
        .ok();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *owner)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO embedding_space_registrations(id,workspace_id,space_id,name,model,dimensions,\
         registration_kind,adapter_profile_revision,returned_model,encoding_format) \
         VALUES($1,$2,NULL,'canonical','legacy-model',2,'canonical','p','legacy-model','float')",
    )
    .bind(registration)
    .bind(workspace)
    .execute(&mut *owner)
    .await
    .expect("canonical registration");
    owner.commit().await.unwrap();
}

fn repository(pool: &PgPool) -> PgEmbeddingLegacyAdoptionRepository {
    PgEmbeddingLegacyAdoptionRepository::new(PgStore::from_pool(pool.clone()))
}

fn start(fixture: &LegacySpace, key: &str) -> StartLegacyAdoption {
    StartLegacyAdoption {
        plan_id: LegacyAdoptionId::new(),
        legacy_space_registration_id: fixture.legacy_registration,
        target_space_registration_id: fixture.canonical_registration,
        idempotency_key: key.to_owned(),
    }
}

/// The plan fixes what it covers once: which rows, which revisions, how many.
#[sqlx::test(migrations = "../../migrations")]
async fn a_plan_fixes_its_members_and_watermark_once(pool: PgPool) {
    let fixture = seed_legacy_space(&pool).await;
    let repository = repository(&pool);

    let command = start(&fixture, "adopt-once");
    let planned = command.plan_id;
    let progress = repository
        .start_or_resume(&fixture.context, command.clone())
        .await
        .expect("one plan");

    assert_eq!(progress.plan_id, planned);
    assert_eq!(progress.state, LegacyAdoptionState::Planned);
    assert_eq!(progress.total_members, 2, "one member per legacy vector");

    let watermark: i64 = sqlx::query_scalar(
        "SELECT legacy_watermark FROM embedding_legacy_adoptions WHERE workspace_id=$1 AND id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(planned.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(watermark, 2);

    // A replay under the same key is the same plan, not a second one.
    let replay = repository
        .start_or_resume(&fixture.context, start(&fixture, "adopt-once"))
        .await
        .expect("the replay returns its one prior plan");
    assert_eq!(replay.plan_id, planned);
    let plans: i64 =
        sqlx::query_scalar("SELECT count(*) FROM embedding_legacy_adoptions WHERE workspace_id=$1")
            .bind(fixture.context.workspace_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(plans, 1);
}

/// One legacy space takes one plan. A different key is a second plan for the
/// same rows, and two plans could each believe they own the cutover.
#[sqlx::test(migrations = "../../migrations")]
async fn a_second_key_against_the_same_legacy_space_conflicts(pool: PgPool) {
    let fixture = seed_legacy_space(&pool).await;
    let repository = repository(&pool);
    repository
        .start_or_resume(&fixture.context, start(&fixture, "first"))
        .await
        .expect("the first plan");

    let error = repository
        .start_or_resume(&fixture.context, start(&fixture, "second"))
        .await
        .expect_err("a second key must not open a second plan");
    assert!(
        matches!(error, ApplicationError::Conflict(_)),
        "expected a conflict, got {error:?}"
    );
}

/// An unadoptable source is visible, not silently omitted: the member is
/// blocked with the exact reason and the plan still covers it.
#[sqlx::test(migrations = "../../migrations")]
async fn an_erased_source_is_a_visible_blocker_and_not_an_omission(pool: PgPool) {
    let fixture = seed_legacy_space(&pool).await;
    let repository = repository(&pool);
    let progress = repository
        .start_or_resume(&fixture.context, start(&fixture, "blocked"))
        .await
        .expect("one plan");

    assert_eq!(progress.total_members, 2);
    assert_eq!(progress.blocked_members, 1);
    assert_eq!(progress.blockers.len(), 1);
    assert_eq!(
        progress.blockers[0].reason,
        LegacyAdoptionBlocker::SourceContentErased
    );

    let blocked_memory: Uuid = sqlx::query_scalar(
        "SELECT memory_id FROM embedding_legacy_adoption_members \
          WHERE workspace_id=$1 AND adoption_id=$2 AND state='blocked'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(progress.plan_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(blocked_memory, fixture.erased_memory);
    assert_ne!(blocked_memory, fixture.live_memory);
}

/// Adoption may only target a canonical space. A legacy target would adopt one
/// legacy space into another and retire nothing.
#[sqlx::test(migrations = "../../migrations")]
async fn a_non_canonical_target_is_refused(pool: PgPool) {
    let fixture = seed_legacy_space(&pool).await;
    let repository = repository(&pool);
    let mut command = start(&fixture, "bad-target");
    command.target_space_registration_id = fixture.legacy_registration;

    let error = repository
        .start_or_resume(&fixture.context, command)
        .await
        .expect_err("a legacy target must be refused");
    assert!(
        matches!(error, ApplicationError::Policy(message) if message.contains("canonical")),
        "expected the canonical-target refusal"
    );
}

/// A cutover before every member is satisfied would delete plaintext whose
/// vector was never recomputed.
#[sqlx::test(migrations = "../../migrations")]
async fn a_plan_with_an_unsatisfied_member_cannot_become_ready(pool: PgPool) {
    let fixture = seed_legacy_space(&pool).await;
    let repository = repository(&pool);
    let progress = repository
        .start_or_resume(&fixture.context, start(&fixture, "unready"))
        .await
        .expect("one plan");

    // Still 'planned': nothing has been sourced, so readiness cannot be claimed.
    let error = repository
        .prove_ready(&fixture.context, progress.plan_id, Uuid::now_v7())
        .await
        .expect_err("a planned adoption is not ready to cut over");
    assert!(
        matches!(error, ApplicationError::Policy(_)),
        "expected a policy refusal, got {error:?}"
    );

    let legacy_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory_embeddings WHERE workspace_id=$1")
            .bind(fixture.context.workspace_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(legacy_rows, 2, "a refused readiness must delete nothing");
}

/// The installation gate stays shut while any adoption is outstanding, and it
/// is deliberately blind to workspaces.
#[sqlx::test(migrations = "../../migrations")]
async fn the_retirement_gate_refuses_while_an_adoption_is_outstanding(pool: PgPool) {
    let fixture = seed_legacy_space(&pool).await;
    let repository = repository(&pool);
    repository
        .start_or_resume(&fixture.context, start(&fixture, "gate"))
        .await
        .expect("one plan");

    let error = repository
        .commit_plaintext_retirement(&fixture.context)
        .await
        .expect_err("an outstanding adoption must keep the gate shut");
    assert!(
        matches!(error, ApplicationError::Policy(_)),
        "expected a policy refusal, got {error:?}"
    );
    let committed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM embedding_legacy_retirement_gate")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(committed, 0);
}

/// A legacy space nobody ever planned is still a legacy space. The gate counts
/// registrations, not plans, so an unadopted space cannot pass by being absent.
#[sqlx::test(migrations = "../../migrations")]
async fn the_retirement_gate_refuses_a_legacy_space_no_plan_ever_covered(pool: PgPool) {
    let fixture = seed_legacy_space(&pool).await;
    let repository = repository(&pool);

    // No plan at all: every adoption is vacuously complete, and the gate must
    // still refuse because a legacy registration exists.
    let error = repository
        .commit_plaintext_retirement(&fixture.context)
        .await
        .expect_err("an unadopted legacy space must keep the gate shut");
    assert!(
        matches!(error, ApplicationError::Policy(message) if message.contains("legacy space")),
        "expected the unadopted-space refusal"
    );
}

/// The runtime role reads adoption state and changes none of it.
#[sqlx::test(migrations = "../../migrations")]
async fn the_runtime_role_cannot_write_adoption_tables_directly(pool: PgPool) {
    let fixture = seed_legacy_space(&pool).await;
    let repository = repository(&pool);
    let progress = repository
        .start_or_resume(&fixture.context, start(&fixture, "runtime"))
        .await
        .expect("one plan");

    let runtime = common::runtime_pool(&pool).await;
    for statement in [
        "INSERT INTO embedding_legacy_adoptions(id,workspace_id,legacy_space_registration_id,\
         legacy_space_id,legacy_watermark,target_space_registration_id,state,idempotency_key) \
         VALUES(gen_random_uuid(),$1,gen_random_uuid(),gen_random_uuid(),0,gen_random_uuid(),'planned','x')",
        "UPDATE embedding_legacy_adoptions SET state='completed' WHERE workspace_id=$1",
        "DELETE FROM embedding_legacy_adoption_members WHERE workspace_id=$1",
        "INSERT INTO embedding_legacy_retirement_gate(singleton,completed_adoption_count) VALUES(TRUE,0)",
    ] {
        let mut scoped = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(fixture.context.workspace_id.to_string())
            .fetch_one(&mut *scoped)
            .await
            .unwrap();
        let outcome = sqlx::query(statement)
            .bind(fixture.context.workspace_id.as_uuid())
            .execute(&mut *scoped)
            .await;
        let error = outcome.expect_err("the runtime role must not write adoption state directly");
        let code = error
            .as_database_error()
            .and_then(|database| database.code())
            .map(|code| code.to_string())
            .unwrap_or_default();
        assert_eq!(
            code, "42501",
            "expected insufficient_privilege, got {error}"
        );
    }

    // The plan the guarded path wrote is still exactly what it was.
    let state: String = sqlx::query_scalar(
        "SELECT state FROM embedding_legacy_adoptions WHERE workspace_id=$1 AND id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(progress.plan_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "planned");
    runtime.close().await;
}

/// The database accepts model-request evidence built by `for_embedding_job`,
/// and the materializer produces the source it names.
///
/// This is the one proof that matters for the new cause: the shape is agreed by
/// the application layer and the durable authority, not merely self-consistent.
/// It exercises the materializer at the same time, because the evidence needs a
/// real Live content material owned by a memory revision to point at.
#[sqlx::test(migrations = false)]
async fn embedding_job_evidence_and_its_materialized_source_are_accepted(pool: PgPool) {
    common::result_preparation_fixture::provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let accepted = common::prepare_delivery_embedding_job(&pool, &runtime).await;
    let workspace = accepted.context.workspace_id;

    // One memory revision to embed, in the fixture's own workspace.
    let memory = Uuid::now_v7();
    let revision = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO memories(id,workspace_id,kind,status,state_revision) \
         VALUES($1,$2,'fact','candidate',1)",
    )
    .bind(memory)
    .bind(workspace.as_uuid())
    .execute(&pool)
    .await
    .expect("memory");
    sqlx::query(
        "INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance) \
         VALUES($1,$2,$3,1,'the text a rebuild would embed',1.0,0.5)",
    )
    .bind(revision)
    .bind(memory)
    .bind(workspace.as_uuid())
    .execute(&pool)
    .await
    .expect("revision");
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision)
        .bind(memory)
        .execute(&pool)
        .await
        .expect("active revision");

    let vault_fixture = common::result_preparation_fixture::OutputVaultFixture::new();
    let vault = Arc::new(vault_fixture.vault(workspace));
    let materializer = PgLegacyAdoptionSourceMaterializer::new(
        PgStore::from_pool(runtime.clone()),
        vault.clone(),
        Arc::new(ContentMaterialCodec::new()),
    );
    let member = LegacyAdoptionMember {
        ordinal: 1,
        legacy_embedding_id: Uuid::now_v7(),
        memory_id: memory,
        memory_revision_id: revision,
        source_material_id: None,
        source_intent_id: None,
        rebuild_job_id: None,
        state: LegacyAdoptionMemberState::Planned,
    };

    let source = materializer
        .materialize(&accepted.context, &member)
        .await
        .expect("materialization must not fail")
        .expect("an intact revision is not a blocker");

    // The material is Live and owned by the revision, which is the join a
    // canonical member needs to reach a memory at all.
    let (state, owner_kind, owner_id): (String, String, Uuid) = sqlx::query_as(
        "SELECT material.state, reference.owner_kind, reference.owner_id \
           FROM content_materials AS material \
           JOIN content_material_ordinary_references AS reference \
             ON reference.workspace_id = material.workspace_id \
            AND reference.material_id = material.id \
          WHERE material.workspace_id = $1 AND material.id = $2",
    )
    .bind(workspace.as_uuid())
    .bind(source.material_id)
    .fetch_one(&pool)
    .await
    .expect("the materialized source must be readable");
    assert_eq!(state, "live");
    assert_eq!(owner_kind, "memory_revision");
    assert_eq!(owner_id, revision);

    // Nothing stored the plaintext: the material carries ciphertext only.
    let ciphertext: Vec<u8> = sqlx::query_scalar(
        "SELECT ciphertext FROM content_material_bytes WHERE workspace_id=$1 AND material_id=$2",
    )
    .bind(workspace.as_uuid())
    .bind(source.material_id)
    .fetch_one(&pool)
    .await
    .expect("sealed bytes");
    assert!(
        !String::from_utf8_lossy(&ciphertext).contains("the text a rebuild would embed"),
        "the source plaintext must not survive in the material"
    );

    // The evidence the delivery authority demands of a rebuild.
    let snapshot: (Uuid, Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT connection_revision_id, connection_qualification_revision_id, \
                model_revision_id, model_qualification_revision_id \
           FROM model_binding_snapshots WHERE workspace_id=$1 AND id=$2",
    )
    .bind(workspace.as_uuid())
    .bind(accepted.snapshot_id)
    .fetch_one(&pool)
    .await
    .expect("the fixture snapshot");

    let root_id = Uuid::now_v7();
    let creation = CreateModelRequestEvidence::for_embedding_job(
        root_id,
        workspace,
        accepted.external_effect_id,
        accepted.snapshot_id,
        snapshot.0,
        snapshot.1,
        snapshot.2,
        snapshot.3,
        accepted.job_id.as_uuid(),
        &[source.material_id],
        EffectiveRequestLimits::new(256, 4, 32_768).unwrap(),
    )
    .expect("the embedding job evidence is well formed");

    // The job row first. `vestrace_create_model_request_evidence` proves an
    // embedding-job cause by finding the job that owns this exact effect and
    // pinned this exact snapshot, so evidence cannot precede its own job.
    let mut accept = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *accept)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_accept_embedding_job($1,$2,$3,'rebuild',$4,$5,$6,NULL,NULL::BIGINT)",
    )
    .bind(accepted.job_id.as_uuid())
    .bind(workspace.as_uuid())
    .bind(accepted.space_registration_id)
    .bind(accepted.snapshot_id)
    .bind(accepted.external_effect_id)
    .bind(root_id)
    .fetch_one(&mut *accept)
    .await
    .expect("one accepted rebuild job");
    accept.commit().await.unwrap();

    let manager = PgTransactionManager::new(PgStore::from_pool(runtime.clone()));
    let repository = PgModelRequestEvidenceRepository::new(vault);
    let mut unit = manager.begin(&accepted.context).await.unwrap();
    let created = repository
        .create_in(unit.as_mut(), &creation)
        .await
        .expect("the durable authority must accept this evidence");
    unit.commit().await.unwrap();

    // The root landed under the cause the delivery authority reads.
    let (cause_kind, cause_id, request_kind, binding): (String, Uuid, String, Option<Uuid>) =
        sqlx::query_as(
            "SELECT cause_kind, cause_id, request_kind, binding_snapshot_id \
               FROM model_request_evidence_roots WHERE workspace_id=$1 AND id=$2",
        )
        .bind(workspace.as_uuid())
        .bind(created.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("the persisted root");
    assert_eq!(cause_kind, "embedding_job");
    assert_eq!(cause_id, accepted.job_id.as_uuid());
    assert_eq!(request_kind, "embeddings");
    assert_eq!(binding, Some(accepted.snapshot_id));

    // And its sources are exactly what the delivery authority will read back.
    // `safe_ordinal` is the per-kind ordinal this constructor assigns, which
    // migration 0183 requires to be contiguous from zero; `ordinal` is the
    // node's position in the whole evidence, and that is what the delivery
    // authority reads as the job's source ordinal.
    let sources: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT safe_ordinal, reference_id FROM model_request_evidence_nodes \
          WHERE workspace_id=$1 AND evidence_root_id=$2 \
            AND reference_kind='governed_input_material' ORDER BY ordinal",
    )
    .bind(workspace.as_uuid())
    .bind(created.as_uuid())
    .fetch_all(&pool)
    .await
    .expect("the persisted sources");
    assert_eq!(sources, vec![("0".to_owned(), source.material_id)]);

    runtime.close().await;
}

/// A canonical space that no transition ever established has no binding the
/// factory may use, and the factory refuses rather than minting a second one.
///
/// This is the property that matters most about the lookup. A factory that
/// invented a binding would let two jobs claim the same corpus under different
/// qualification, and nothing downstream would notice: both would be
/// structurally valid embedding jobs.
#[sqlx::test(migrations = "../../migrations")]
async fn a_target_space_without_a_transition_binding_is_refused(pool: PgPool) {
    let fixture = seed_legacy_space(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let factory = PgLegacyAdoptionRebuildFactory::new(
        PgStore::from_pool(runtime.clone()),
        Arc::new(PgEmbeddingJobRepository::new(PgStore::from_pool(
            runtime.clone(),
        ))),
        Arc::new(PgModelRequestEvidenceRepository::new(Arc::new(
            RefusingVault,
        ))),
        EffectiveRequestLimits::new(256, 4, 32_768).unwrap(),
    );
    let member = LegacyAdoptionMember {
        ordinal: 1,
        legacy_embedding_id: Uuid::now_v7(),
        memory_id: fixture.live_memory,
        memory_revision_id: Uuid::now_v7(),
        source_material_id: None,
        source_intent_id: None,
        rebuild_job_id: None,
        state: LegacyAdoptionMemberState::Planned,
    };

    let error = factory
        .create_rebuild(
            &fixture.context,
            fixture.canonical_registration,
            &member,
            MaterializedSource {
                material_id: Uuid::now_v7(),
                intent_id: Uuid::now_v7(),
            },
        )
        .await
        .expect_err("a space with no transition binding cannot take a rebuild");
    assert!(
        matches!(&error, ApplicationError::Unavailable(message)
            if message.contains("transition-issued binding snapshot")),
        "expected the missing-binding refusal, got {error:?}"
    );

    // Nothing was accepted on the way to that refusal.
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM embedding_jobs WHERE workspace_id=$1")
        .bind(fixture.context.workspace_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(jobs, 0, "a refused rebuild must accept no job");
    runtime.close().await;
}

/// The vault an evidence repository is handed when the test never expects it to
/// be asked for a key.
struct RefusingVault;

impl MaterialKeyVault for RefusingVault {
    fn create_if_absent(
        &self,
        _key_id: MaterialKeyId,
        _nonce: IntentNonce,
    ) -> Result<VaultReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    fn unwrap(
        &self,
        _key_id: MaterialKeyId,
        _use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        panic!("a refused rebuild must not unwrap material")
    }

    fn prepare_erasure(&self, _key_id: MaterialKeyId) -> Result<FenceReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    fn erase(&self, _key_id: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }
}
