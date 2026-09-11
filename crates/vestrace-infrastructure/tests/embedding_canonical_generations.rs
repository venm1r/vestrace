//! Canonical generation authority and the explicit legacy quarantine boundary.
mod common;

use common::result_preparation_fixture::{
    provision_result_behavior_database, provision_result_behavior_database_through,
};
use sqlx::PgPool;

async fn assert_canonical_schema(pool: &PgPool) {
    for table in [
        "embedding_space_registrations",
        "model_qualification_heads",
        "embedding_index_generation_guards",
        "embedding_corpus_generations",
        "embedding_corpus_generation_members",
        "memory_embeddings",
    ] {
        let owner_rls:(String,bool,bool)=sqlx::query_as("SELECT pg_get_userbyid(relowner),relrowsecurity,relforcerowsecurity FROM pg_class WHERE oid=$1::regclass").bind(table).fetch_one(pool).await.unwrap();
        assert_eq!(
            owner_rls,
            ("vestrace_guarded_owner".into(), true, true),
            "{table}"
        );
    }
    for (signature, executable) in [
        (
            "vestrace_register_canonical_embedding_space(uuid,uuid,text,uuid,uuid,uuid,text,text,integer)",
            true,
        ),
        (
            "vestrace_set_initial_embedding_active_space(uuid,uuid,bigint,uuid)",
            true,
        ),
        (
            "vestrace_capture_embedding_generation(uuid,uuid,uuid,bigint)",
            true,
        ),
        (
            "vestrace_publish_embedding_generation(uuid,uuid,uuid,bigint)",
            true,
        ),
        (
            "vestrace_assert_canonical_embedding_space(uuid,uuid)",
            false,
        ),
        ("vestrace_validate_embedding_active_space()", false),
        ("vestrace_invalidate_canonical_generation_guard()", false),
        ("vestrace_normalize_legacy_generation_member()", false),
        ("vestrace_set_canonical_generation_lifecycle()", false),
        (
            "vestrace_validate_embedding_corpus_generation_member()",
            false,
        ),
        ("vestrace_validate_canonical_generation()", false),
        ("vestrace_validate_canonical_generation_guard()", false),
        ("vestrace_validate_canonical_space_registration()", false),
        ("vestrace_validate_canonical_member_liveness()", false),
    ] {
        let authority:(String,bool,bool,bool)=sqlx::query_as("SELECT pg_get_userbyid(proowner),prosecdef,has_function_privilege('vestrace',oid,'EXECUTE'),EXISTS(SELECT 1 FROM aclexplode(coalesce(proacl,acldefault('f',proowner))) a WHERE a.grantee=0 AND a.privilege_type='EXECUTE') FROM pg_proc WHERE oid=$1::regprocedure").bind(signature).fetch_one(pool).await.unwrap();
        assert_eq!(
            authority,
            ("vestrace_guarded_owner".into(), true, executable, false),
            "{signature}"
        );
    }

    for (table, column) in [
        ("embedding_space_registrations", "registration_kind"),
        ("embedding_space_registrations", "model_revision_id"),
        (
            "embedding_space_registrations",
            "model_qualification_revision_id",
        ),
        ("embedding_space_registrations", "request_shape_revision_id"),
        ("embedding_space_registrations", "adapter_profile_revision"),
        ("model_qualification_heads", "active_space_registration_id"),
        ("embedding_index_generation_guards", "current_generation_id"),
        ("embedding_index_generation_guards", "guard_version"),
        ("embedding_corpus_generations", "member_representation"),
        ("embedding_corpus_generations", "created_at"),
        ("embedding_corpus_generations", "state_changed_at"),
        ("embedding_corpus_generations", "lifecycle_reason"),
        ("embedding_corpus_generation_members", "member_ordinal"),
        ("embedding_corpus_generation_members", "legacy_embedding_id"),
        (
            "embedding_corpus_generation_members",
            "embedding_projection_entry_id",
        ),
    ] {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema='public' AND table_name=$1 AND column_name=$2)")
            .bind(table).bind(column).fetch_one(pool).await.unwrap();
        assert!(exists, "canonical schema requires {table}.{column}");
    }
    let identity: String = sqlx::query_scalar("SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid='embedding_corpus_generation_members'::regclass AND contype='p'").fetch_one(pool).await.unwrap();
    assert_eq!(
        identity,
        "PRIMARY KEY (workspace_id, corpus_generation_id, member_ordinal)"
    );
    let branch_indexes: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_index WHERE indrelid='embedding_corpus_generation_members'::regclass AND indisunique AND indpred IS NOT NULL").fetch_one(pool).await.unwrap();
    assert_eq!(branch_indexes, 2);
    let authority: (String, bool, bool, bool) = sqlx::query_as("SELECT pg_get_userbyid(relowner),has_table_privilege('vestrace',oid,'INSERT'),has_table_privilege('vestrace',oid,'UPDATE'),has_table_privilege('vestrace',oid,'DELETE') FROM pg_class WHERE oid='memory_embeddings'::regclass")
        .fetch_one(pool).await.unwrap();
    assert_eq!(
        authority,
        ("vestrace_guarded_owner".into(), false, false, false)
    );
    let duplicate_pointer: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema='public' AND table_name='model_qualification_heads' AND column_name='current_generation_id')")
        .fetch_one(pool).await.unwrap();
    assert!(
        !duplicate_pointer,
        "the generation guard is the sole current pointer"
    );
}

#[sqlx::test(migrations = false)]
async fn fresh_install_has_canonical_authority(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    assert_canonical_schema(&pool).await;
}

#[sqlx::test(migrations = false)]
async fn runtime_0196_upgrade_installs_canonical_authority(pool: PgPool) {
    provision_result_behavior_database_through(&pool, 196).await;
    let before: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='embedding_index_generation_guards' AND column_name='current_generation_id')")
        .fetch_one(&pool).await.unwrap();
    assert!(!before);
    let runtime = common::runtime_pool(&pool).await;
    sqlx::migrate!("../../migrations")
        .run(&runtime)
        .await
        .unwrap();
    assert_canonical_schema(&pool).await;
}

use uuid::Uuid;
use vestrace_application::{
    QualificationJobRepository, QualificationProbeCompletion, RequestContext,
};
use vestrace_domain::{PrincipalId, QualificationJobId, QualificationProbeResult, WorkspaceId};
use vestrace_infrastructure::postgres::{PgQualificationJobRepository, PgStore};

struct QualificationFixture {
    workspace: WorkspaceId,
    principal: PrincipalId,
    job: QualificationJobId,
    target: Uuid,
    model: Uuid,
    qualification: Uuid,
    shape: Uuid,
}
async fn set_context(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, workspace: WorkspaceId) {
    sqlx::query("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .execute(&mut **tx)
        .await
        .unwrap();
}

// Q1 network evidence follows the existing provider_dispatch_is_atomic fixture.
fn q1_source_fields(
    ordinal: &str,
) -> (
    &'static str,
    &'static str,
    bool,
    &'static str,
    bool,
    bool,
    Option<&'static str>,
) {
    match ordinal {
        "10" | "20" | "90" => ("plain_text", "none", false, "none", false, false, None),
        "30" => ("plain_text", "none", false, "none", true, false, None),
        "35" => ("plain_text", "none", false, "none", true, true, None),
        "40" => (
            "plain_text",
            "named_probe",
            false,
            "none",
            false,
            false,
            None,
        ),
        "50" => (
            "assistant_tool_call_replay",
            "none",
            false,
            "none",
            false,
            false,
            Some("call_q1"),
        ),
        "60" => ("plain_text", "required", true, "none", false, false, None),
        "70" => (
            "plain_text",
            "none",
            false,
            "strict_nonce_json_schema",
            false,
            false,
            None,
        ),
        "80" => (
            "multipart_image_marker",
            "none",
            false,
            "none",
            false,
            false,
            None,
        ),
        _ => panic!("unknown q1 network ordinal {ordinal}"),
    }
}

async fn insert_finalizable_q1_network_probe(
    pool: &PgPool,
    qualification: &QualificationFixture,
    ordinal: &str,
) -> (Uuid, Uuid) {
    let effect_id = Uuid::now_v7();
    let evidence_id = Uuid::now_v7();
    let evidence_check_id = Uuid::now_v7();
    let receipt_id = Uuid::now_v7();
    let request_kind = match ordinal {
        "10" => "models_list",
        "90" => "embeddings",
        "20" | "30" | "35" | "40" | "50" | "60" | "70" | "80" => "chat_completions",
        _ => panic!("not a q1 network ordinal: {ordinal}"),
    };
    let (
        message_layout,
        tool_choice,
        parallel_tool_calls,
        response_format,
        stream,
        include_usage,
        replay,
    ) = q1_source_fields(ordinal);

    let mut transaction = pool.begin().await.unwrap();
    set_context(&mut transaction, qualification.workspace).await;
    sqlx::query(
        "INSERT INTO external_effect_intents(id,workspace_id,adapter,payload)
         VALUES($1,$2,'openai-compatible','{}'::jsonb)",
    )
    .bind(effect_id)
    .bind(qualification.workspace.as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO model_request_evidence_roots(
             id,workspace_id,external_effect_id,request_kind,
             qualification_target_binding_id,cause_kind,cause_id
         ) VALUES($1,$2,$3,$4,$5,'qualification_probe',$6)",
    )
    .bind(evidence_id)
    .bind(qualification.workspace.as_uuid())
    .bind(effect_id)
    .bind(request_kind)
    .bind(qualification.target)
    .bind(qualification.job.as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    for (node_ordinal, kind, reference, safe_ordinal) in [
        (0_i32, "external_effect", effect_id, None),
        (1_i32, "qualification_target", qualification.target, None),
        (
            2_i32,
            "qualification_probe",
            qualification.job.as_uuid(),
            Some(ordinal),
        ),
    ] {
        sqlx::query(
            "INSERT INTO model_request_evidence_nodes(
                 id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id,safe_ordinal
             ) VALUES($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(Uuid::now_v7())
        .bind(qualification.workspace.as_uuid())
        .bind(evidence_id)
        .bind(node_ordinal)
        .bind(kind)
        .bind(reference)
        .bind(safe_ordinal)
        .execute(&mut *transaction)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO model_request_evidence_checks(
             id,workspace_id,evidence_root_id,status,missing_reference_count
         ) VALUES($1,$2,$3,'complete',0)",
    )
    .bind(evidence_check_id)
    .bind(qualification.workspace.as_uuid())
    .bind(evidence_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_qualification_q1_mre_source(
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(evidence_id)
    .bind(qualification.workspace.as_uuid())
    .bind(ordinal)
    .bind(message_layout)
    .bind(tool_choice)
    .bind(parallel_tool_calls)
    .bind(response_format)
    .bind(stream)
    .bind(include_usage)
    .bind(replay)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO provider_dispatch_causes(
             external_effect_id,workspace_id,model_request_evidence_id,model_request_evidence_check_id,
             cause_kind,qualification_job_id,qualification_target_binding_id,qualification_probe_ordinal
         ) VALUES($1,$2,$3,$4,'qualification_probe',$5,$6,$7)",
    )
    .bind(effect_id)
    .bind(qualification.workspace.as_uuid())
    .bind(evidence_id)
    .bind(evidence_check_id)
    .bind(qualification.job.as_uuid())
    .bind(qualification.target)
    .bind(ordinal)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query("RESET ROLE")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_receipts(
             id,effect_id,workspace_id,outcome_status,payload
         ) VALUES($1,$2,$3,'acknowledged','{}'::jsonb)",
    )
    .bind(receipt_id)
    .bind(effect_id)
    .bind(qualification.workspace.as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions(
             id,effect_id,workspace_id,status,cause,cause_ref,recorded_at
         ) VALUES($1,$2,$3,'acknowledged','receipt_recorded',$4,NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(effect_id)
    .bind(qualification.workspace.as_uuid())
    .bind(receipt_id.to_string())
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    (effect_id, evidence_id)
}

async fn record_complete_q1_matrix(
    owner: &PgPool,
    runtime: &PgPool,
    qualification: &QualificationFixture,
) {
    let context = RequestContext::new(qualification.workspace, qualification.principal);
    let repository = PgQualificationJobRepository::new(PgStore::from_pool(runtime.clone()));
    let mut network = std::collections::BTreeMap::new();
    for ordinal in ["10", "20", "30", "35", "40", "50", "60", "70", "80", "90"] {
        network.insert(
            ordinal,
            insert_finalizable_q1_network_probe(owner, qualification, ordinal).await,
        );
    }
    for ordinal in [
        "00", "10", "15", "20", "30", "35", "40", "50", "60", "70", "80", "90",
    ] {
        let (external_effect_id, model_request_evidence_id) = network
            .get(ordinal)
            .copied()
            .map(|(effect, evidence)| (Some(effect), Some(evidence)))
            .unwrap_or((None, None));
        repository
            .record_probe_result(
                &context,
                QualificationProbeCompletion {
                    probe_result_id: Uuid::now_v7(),
                    job_id: qualification.job,
                    ordinal: ordinal.to_owned(),
                    result: QualificationProbeResult::Pass,
                    external_effect_id,
                    model_request_evidence_id,
                },
            )
            .await
            .expect("the complete q1 fixture must record every ordinal");
    }
}

async fn canonical_qualification(owner: &PgPool, runtime: &PgPool) -> QualificationFixture {
    let f = QualificationFixture {
        workspace: WorkspaceId::new(),
        principal: PrincipalId::new(),
        job: QualificationJobId::new(),
        target: Uuid::now_v7(),
        model: Uuid::now_v7(),
        qualification: Uuid::now_v7(),
        shape: Uuid::now_v7(),
    };
    let connector = Uuid::now_v7();
    let connection = Uuid::now_v7();
    let revision = Uuid::now_v7();
    let guard = Uuid::now_v7();
    let no_auth = Uuid::now_v7();
    let provider = Uuid::now_v7();
    let chat_model = Uuid::now_v7();
    let embedding_model = Uuid::now_v7();
    let chat_revision = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(f.workspace.as_uuid())
        .bind(format!("canonical-{}", f.workspace))
        .execute(owner)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,$3)")
        .bind(f.principal.as_uuid())
        .bind(f.workspace.as_uuid())
        .bind(f.principal.to_string())
        .execute(owner)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connectors(id,workspace_id,name,provider_type) VALUES($1,$2,'canonical','local')").bind(connector).bind(f.workspace.as_uuid()).execute(owner).await.unwrap();
    sqlx::query("INSERT INTO connections(id,connector_id,workspace_id,principal_id,name,status) VALUES($1,$2,$3,$4,'canonical','active')").bind(connection).bind(connector).bind(f.workspace.as_uuid()).bind(f.principal.as_uuid()).execute(owner).await.unwrap();
    sqlx::query(
        "INSERT INTO providers(id,workspace_id,name,locality) VALUES($1,$2,'canonical','local')",
    )
    .bind(provider)
    .bind(f.workspace.as_uuid())
    .execute(owner)
    .await
    .unwrap();
    for (id, name) in [(chat_model, "chat"), (embedding_model, "embedding")] {
        sqlx::query("INSERT INTO models(id,provider_id,workspace_id,model_name,context_window,input_cost_per_mtoken,output_cost_per_mtoken) VALUES($1,$2,$3,$4,4096,0,0)").bind(id).bind(provider).bind(f.workspace.as_uuid()).bind(name).execute(owner).await.unwrap();
    }
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(guard)
        .bind(f.workspace.as_uuid())
        .bind(connection)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,'lm_studio_local','http://127.0.0.1:1234/v1','http://127.0.0.1:1234/v1','q1','loopback_only','none',NULL,0)").bind(revision).bind(f.workspace.as_uuid()).bind(connection).bind(guard).execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)")
        .bind(no_auth)
        .bind(f.workspace.as_uuid())
        .bind(connection)
        .bind(revision)
        .execute(&mut *tx)
        .await
        .unwrap();
    for (id, model, wire, kind) in [
        (chat_revision, chat_model, "chat-model", "chat"),
        (
            f.model,
            embedding_model,
            "text-embedding-nomic-embed-text-v1.5",
            "embedding",
        ),
    ] {
        sqlx::query("SELECT vestrace_create_model_revision_and_advance_head($1,$2,$3,$4,$5,$6,$7,$8,NULL,NULL,NULL,NULL,NULL,NULL,0)").bind(id).bind(f.workspace.as_uuid()).bind(model).bind(connection).bind(guard).bind(revision).bind(wire).bind(kind).execute(&mut *tx).await.unwrap();
    }
    sqlx::query("SELECT vestrace_create_model_request_shape_revision($1,$2,1,'embeddings',false,ARRAY[]::TEXT[])").bind(f.shape).bind(f.workspace.as_uuid()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    // Structural Running target setup matches the existing Q1 repository tests.
    // Results and qualification publication themselves use their runtime commands.
    let mut tx = owner.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,state) VALUES($1,$2,$3,'q1','running')").bind(f.job.as_uuid()).bind(f.workspace.as_uuid()).bind(revision).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO qualification_target_bindings(id,workspace_id,qualification_job_id,connection_id,connection_revision_id,branch,no_auth_binding_revision_id,chat_model_revision_id,embedding_model_revision_id) VALUES($1,$2,$3,$4,$5,'no_auth',$6,$7,$8)").bind(f.target).bind(f.workspace.as_uuid()).bind(f.job.as_uuid()).bind(connection).bind(revision).bind(no_auth).bind(chat_revision).bind(f.model).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    record_complete_q1_matrix(owner, runtime, &f).await;
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_finalize_qualification_job($1,$2,$3,$4,$5)")
        .bind(f.workspace.as_uuid())
        .bind(f.job.as_uuid())
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(f.qualification)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    f
}

#[sqlx::test(migrations = false)]
async fn canonical_registration_requires_qualified_evidence_and_captures_exact_empty_snapshot(
    pool: PgPool,
) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let f = canonical_qualification(&pool, &runtime).await;
    let registration = Uuid::now_v7();
    let generation = Uuid::now_v7();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'canonical',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
    let version: i64 =
        sqlx::query_scalar("SELECT vestrace_set_initial_embedding_active_space($1,$2,1,$3)")
            .bind(f.workspace.as_uuid())
            .bind(f.model)
            .bind(registration)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(version, 2);
    let snapshot:(Uuid,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT generation_id,generation_epoch,guard_version,corpus_revision,built_through_projection_ordinal,member_count FROM vestrace_capture_embedding_generation($1,$2,$3,1)").bind(generation).bind(f.workspace.as_uuid()).bind(registration).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(snapshot, (generation, 1, 1, 0, 0, 0));
    tx.commit().await.unwrap();
    let unpublished: bool = sqlx::query_scalar(
        "SELECT published_at IS NULL AND lifecycle_reason='captured' AND created_at=state_changed_at FROM embedding_corpus_generations WHERE id=$1",
    )
    .bind(generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(unpublished, "Building is not a published generation");
    let mut forged = pool.begin().await.unwrap();
    set_context(&mut forged, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *forged)
        .await
        .unwrap();
    sqlx::query("UPDATE embedding_index_generation_guards SET current_generation_id=$1 WHERE workspace_id=$2 AND space_registration_id=$3").bind(generation).bind(f.workspace.as_uuid()).bind(registration).execute(&mut *forged).await.unwrap();
    let error = forged.commit().await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    let pointer:Option<Uuid>=sqlx::query_scalar("SELECT current_generation_id FROM embedding_index_generation_guards WHERE space_registration_id=$1").bind(registration).fetch_one(&pool).await.unwrap();
    assert_eq!(pointer, None);

    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    let published:(i64,i64)=sqlx::query_as("SELECT generation_epoch,guard_version FROM vestrace_publish_embedding_generation($1,$2,$3,1)").bind(f.workspace.as_uuid()).bind(registration).bind(generation).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(published, (1, 2));
    tx.commit().await.unwrap();
    let truthful: bool = sqlx::query_scalar("SELECT published_at IS NOT NULL AND lifecycle_reason='published' AND state_changed_at>=created_at FROM embedding_corpus_generations WHERE id=$1").bind(generation).fetch_one(&pool).await.unwrap();
    assert!(truthful);
    let persisted:(String,Option<Uuid>,i64)=sqlx::query_as("SELECT g.state,h.current_generation_id,h.guard_version FROM embedding_corpus_generations g JOIN embedding_index_generation_guards h USING(workspace_id,space_registration_id) WHERE g.id=$1").bind(generation).fetch_one(&pool).await.unwrap();
    assert_eq!(persisted, ("ready".into(), Some(generation), 2));
    let mut forged = pool.begin().await.unwrap();
    set_context(&mut forged, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *forged)
        .await
        .unwrap();
    sqlx::query("UPDATE embedding_index_generation_guards SET current_generation_id=NULL WHERE workspace_id=$1 AND space_registration_id=$2").bind(f.workspace.as_uuid()).bind(registration).execute(&mut *forged).await.unwrap();
    let attempted = forged.commit().await;
    let pointer:Option<Uuid>=sqlx::query_scalar("SELECT current_generation_id FROM embedding_index_generation_guards WHERE space_registration_id=$1").bind(registration).fetch_one(&pool).await.unwrap();
    assert_eq!(
        pointer,
        Some(generation),
        "a Ready canonical generation cannot be orphaned by clearing its current pointer"
    );
    assert_eq!(
        attempted
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .as_deref(),
        Some("23514")
    );
}

#[sqlx::test(migrations = false)]
async fn canonical_capture_enrolls_every_live_encrypted_output(pool: PgPool) {
    use common::result_preparation_fixture::*;
    use std::sync::Arc;
    use vestrace_application::{
        EmbeddingResultFinalizationAuthority, EmbeddingResultFinalizationService,
        EmbeddingResultPreparationId, ExternalEffectRepository,
    };
    use vestrace_domain::{EmbeddingJobId, ExternalEffectId};
    use vestrace_infrastructure::postgres::{
        EmbeddingOutputHmacCommitter, PgEmbeddingResultFinalizationRepository,
        PgExternalEffectRepository,
    };
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let f = canonical_qualification(&pool, &runtime).await;
    let registration = Uuid::now_v7();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'encrypted',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',768)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let (connection_id,connection_revision_id,connection_qualification_id,no_auth_binding_id):(Uuid,Uuid,Uuid,Uuid)=sqlx::query_as("SELECT r.connection_id,q.connection_revision_id,q.connection_qualification_revision_id,n.id FROM model_qualification_revisions q JOIN connection_revisions r ON r.id=q.connection_revision_id JOIN no_auth_binding_revisions n ON n.connection_revision_id=r.id WHERE q.id=$1").bind(f.qualification).fetch_one(&pool).await.unwrap();
    let context = RequestContext::new(f.workspace, f.principal);
    let snapshot_id = Uuid::now_v7();
    let intent = common::workspace_scoped_intent(&context, snapshot_id);
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .insert_intent(&context, &intent)
        .await
        .unwrap();
    let accepted = common::AcceptedJob {
        context,
        job_id: EmbeddingJobId::new(),
        connection_id,
        connection_revision_id,
        connection_qualification_id,
        model_revision_id: f.model,
        space_registration_id: registration,
        model_qualification_id: f.qualification,
        no_auth_binding_id,
        snapshot_id,
        external_effect_id: intent.id().as_uuid(),
        evidence_id: Uuid::now_v7(),
        intent,
        credential: None,
    };
    let mut tx = pool.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,branch,no_auth_binding_revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,'no_auth',$8)").bind(snapshot_id).bind(f.workspace.as_uuid()).bind(connection_id).bind(connection_revision_id).bind(connection_qualification_id).bind(f.model).bind(f.qualification).bind(no_auth_binding_id).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO model_binding_snapshot_scopes(workspace_id,snapshot_id,scope,transition_plan_id) VALUES($1,$2,'ordinary',NULL)").bind(f.workspace.as_uuid()).bind(snapshot_id).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let result = result_fixture_for_accepted(
        &pool,
        runtime.clone(),
        accepted,
        DeliveryPolicyCase::ExactAllowed,
        true,
    )
    .await;
    let preparation = Uuid::now_v7();
    try_commit_result(
        &result,
        preparation,
        Uuid::now_v7(),
        &exact_attempt(&result),
    )
    .await
    .unwrap();
    let authority = EmbeddingResultFinalizationAuthority {
        preparation_id: EmbeddingResultPreparationId::from_uuid(preparation),
        job_id: result.accepted.job_id,
        effect_id: ExternalEffectId::from_uuid(result.accepted.external_effect_id),
    };
    EmbeddingResultFinalizationService::new(
        Arc::new(PgEmbeddingResultFinalizationRepository::new(
            PgStore::from_pool(runtime.clone()),
        )),
        Arc::new(result.vault.vault(f.workspace)),
        Arc::new(EmbeddingOutputHmacCommitter::new()),
    )
    .finalize(&result.accepted.context, &authority)
    .await
    .unwrap();
    let generation = Uuid::now_v7();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    let version:i64=sqlx::query_scalar("SELECT guard_version FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$2").bind(f.workspace.as_uuid()).bind(registration).fetch_one(&mut *tx).await.unwrap();
    let snapshot:(Uuid,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT generation_id,generation_epoch,guard_version,corpus_revision,built_through_projection_ordinal,member_count FROM vestrace_capture_embedding_generation($1,$2,$3,$4)").bind(generation).bind(f.workspace.as_uuid()).bind(registration).bind(version).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(snapshot, (generation, 2, version, 1, 2, 2));
    tx.commit().await.unwrap();
    let members:(i64,i64,i64)=sqlx::query_as("SELECT count(*),count(m.legacy_embedding_id),count(p.id) FROM embedding_corpus_generation_members m LEFT JOIN embedding_projection_entries p ON p.id=m.embedding_projection_entry_id AND p.workspace_id=m.workspace_id AND p.state='live' WHERE m.corpus_generation_id=$1").bind(generation).fetch_one(&pool).await.unwrap();
    assert_eq!(members, (2, 0, 2));
    let ordered: Vec<(i64,i64)> = sqlx::query_as("SELECT m.member_ordinal,p.projection_ordinal FROM embedding_corpus_generation_members m JOIN embedding_projection_entries p ON p.workspace_id=m.workspace_id AND p.id=m.embedding_projection_entry_id WHERE m.corpus_generation_id=$1 ORDER BY m.member_ordinal").bind(generation).fetch_all(&pool).await.unwrap();
    assert_eq!(ordered, vec![(1, 1), (2, 2)]);
    let mut forged = pool.begin().await.unwrap();
    set_context(&mut forged, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *forged)
        .await
        .unwrap();
    sqlx::query("DELETE FROM embedding_corpus_generation_members WHERE corpus_generation_id=$1 AND member_ordinal=(SELECT min(member_ordinal) FROM embedding_corpus_generation_members WHERE corpus_generation_id=$1)").bind(generation).execute(&mut *forged).await.unwrap();
    let error = forged.commit().await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    let retained: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_corpus_generation_members WHERE corpus_generation_id=$1",
    )
    .bind(generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retained, 2);

    use vestrace_application::retrieval::CorpusGenerationResolver;
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_set_initial_embedding_active_space($1,$2,1,$3)")
        .bind(f.workspace.as_uuid())
        .bind(f.model)
        .bind(registration)
        .execute(&mut *tx)
        .await
        .unwrap();
    let published:(i64,i64)=sqlx::query_as("SELECT generation_epoch,guard_version FROM vestrace_publish_embedding_generation($1,$2,$3,$4)").bind(f.workspace.as_uuid()).bind(registration).bind(generation).bind(version).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(published, (2, version + 1));
    tx.commit().await.unwrap();
    let resolver = vestrace_infrastructure::postgres::PgCorpusGenerationResolver::new(
        PgStore::from_pool(runtime.clone()),
    );
    let resolved = resolver
        .resolve_canonical(&result.accepted.context, "encrypted", RESULT_MODEL)
        .await
        .unwrap();
    assert_eq!(resolved.generation_id.as_uuid(), generation);
    assert_eq!(
        (
            resolved.generation_epoch,
            resolved.guard_version,
            resolved.corpus_revision,
            resolved.built_through_projection_ordinal,
            resolved.member_count
        ),
        (2, u64::try_from(version + 1).unwrap(), 1, 2, 2)
    );
    assert!(resolved.space.is_canonical());
    assert_eq!(
        resolved
            .space
            .canonical_identity()
            .unwrap()
            .model_qualification_revision_id
            .as_uuid(),
        f.qualification
    );
    let second =
        common::prepare_additional_delivery_embedding_job(&runtime, &result.accepted).await;
    let second = result_fixture_for_accepted(
        &pool,
        runtime.clone(),
        second,
        DeliveryPolicyCase::ExactAllowed,
        false,
    )
    .await;
    let preparation = Uuid::now_v7();
    try_commit_result(
        &second,
        preparation,
        Uuid::now_v7(),
        &exact_attempt(&second),
    )
    .await
    .unwrap();
    let authority = EmbeddingResultFinalizationAuthority {
        preparation_id: EmbeddingResultPreparationId::from_uuid(preparation),
        job_id: second.accepted.job_id,
        effect_id: ExternalEffectId::from_uuid(second.accepted.external_effect_id),
    };
    EmbeddingResultFinalizationService::new(
        Arc::new(PgEmbeddingResultFinalizationRepository::new(
            PgStore::from_pool(runtime.clone()),
        )),
        Arc::new(second.vault.vault(f.workspace)),
        Arc::new(EmbeddingOutputHmacCommitter::new()),
    )
    .finalize(&second.accepted.context, &authority)
    .await
    .unwrap();
    let invalidated:(String,Option<Uuid>,i64,i64)=sqlx::query_as("SELECT g.state,h.current_generation_id,h.generation_epoch,h.guard_version FROM embedding_corpus_generations g JOIN embedding_index_generation_guards h USING(workspace_id,space_registration_id) WHERE g.id=$1").bind(generation).fetch_one(&pool).await.unwrap();
    assert_eq!(invalidated, ("stale".into(), None, 3, version + 2));
    let truthful: bool = sqlx::query_scalar("SELECT published_at IS NOT NULL AND lifecycle_reason='invalidated' AND state_changed_at>=published_at FROM embedding_corpus_generations WHERE id=$1").bind(generation).fetch_one(&pool).await.unwrap();
    assert!(
        truthful,
        "unchanged 0195 publication stamps canonical invalidation"
    );
    let error = resolver
        .resolve_canonical(&result.accepted.context, "encrypted", RESULT_MODEL)
        .await
        .unwrap_err();
    assert!(
        matches!(error,vestrace_application::ApplicationError::Unavailable(ref message) if message=="embedding-canonical-generation-required")
    );
}

async fn seed_legacy_generation(
    pool: &PgPool,
    runtime: &PgPool,
) -> (RequestContext, Uuid, Uuid, Uuid) {
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let memory = Uuid::now_v7();
    let embedding = Uuid::now_v7();
    let space = Uuid::now_v7();
    let registration = Uuid::now_v7();
    let generation = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("legacy-{}", context.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,'legacy')")
        .bind(context.principal_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memories(id,workspace_id,kind,status,state_revision) VALUES($1,$2,'fact','candidate',1)").bind(memory).bind(context.workspace_id.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) VALUES($1,$2,'legacy',2,'legacy-model')").bind(space).bind(context.workspace_id.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_embeddings(id,memory_id,workspace_id,space_id,embedding) VALUES($1,$2,$3,$4,'[1,0]'::vector)").bind(embedding).bind(memory).bind(context.workspace_id.as_uuid()).bind(space).execute(pool).await.unwrap();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, context.workspace_id).await;
    sqlx::query("SELECT vestrace_register_embedding_space($1,$2,$3,'legacy','legacy-model',2)")
        .bind(registration)
        .bind(context.workspace_id.as_uuid())
        .bind(space)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_open_embedding_corpus_generation($1,$2,$3)")
        .bind(generation)
        .bind(context.workspace_id.as_uuid())
        .bind(registration)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_enrol_embedding_corpus_generation_member($1,$2,$3)")
        .bind(context.workspace_id.as_uuid())
        .bind(generation)
        .bind(embedding)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_publish_embedding_corpus_generation($1,$2,$3,1)")
        .bind(generation)
        .bind(context.workspace_id.as_uuid())
        .bind(registration)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (context, registration, generation, embedding)
}

#[sqlx::test(migrations = false)]
async fn seeded_legacy_history_survives_upgrade_but_runtime_cannot_use_or_mutate_it(pool: PgPool) {
    use vestrace_application::{ApplicationError, retrieval::CorpusGenerationResolver};
    use vestrace_infrastructure::postgres::PgCorpusGenerationResolver;
    provision_result_behavior_database_through(&pool, 196).await;
    let runtime = common::runtime_pool(&pool).await;
    let (context, registration, generation, embedding) =
        seed_legacy_generation(&pool, &runtime).await;
    let bytes_before: String =
        sqlx::query_scalar("SELECT embedding::text FROM memory_embeddings WHERE id=$1")
            .bind(embedding)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::migrate!("../../migrations")
        .run(&runtime)
        .await
        .unwrap();
    let state:(String,String,Option<Uuid>,Option<Uuid>)=sqlx::query_as("SELECT g.state,g.member_representation,m.legacy_embedding_id,m.embedding_projection_entry_id FROM embedding_corpus_generations g JOIN embedding_corpus_generation_members m ON m.corpus_generation_id=g.id WHERE g.id=$1").bind(generation).fetch_one(&pool).await.unwrap();
    assert_eq!(
        state,
        (
            "ready".into(),
            "legacy_upgrade".into(),
            Some(embedding),
            None
        )
    );
    let legacy_metadata: (i64,String,bool) = sqlx::query_as("SELECT m.member_ordinal,g.lifecycle_reason,g.published_at=g.created_at AND g.created_at=g.state_changed_at FROM embedding_corpus_generations g JOIN embedding_corpus_generation_members m ON m.corpus_generation_id=g.id WHERE g.id=$1").bind(generation).fetch_one(&pool).await.unwrap();
    assert_eq!(legacy_metadata, (1, "legacy_upgrade".into(), true));
    let resolver = PgCorpusGenerationResolver::new(PgStore::from_pool(runtime.clone()));
    for error in [
        resolver
            .resolve(&context, "legacy", "legacy-model")
            .await
            .unwrap_err(),
        resolver
            .resolve_canonical(&context, "legacy", "legacy-model")
            .await
            .unwrap_err(),
    ] {
        assert!(
            matches!(error,ApplicationError::Unavailable(ref message) if message=="embedding-legacy-adoption-required"),
            "{error:?}"
        );
    }
    for statement in [
        "INSERT INTO memory_embeddings(id,memory_id,workspace_id,space_id,embedding) SELECT gen_random_uuid(),memory_id,workspace_id,space_id,embedding FROM memory_embeddings WHERE id=$1",
        "UPDATE memory_embeddings SET embedding='[0,1]'::vector WHERE id=$1",
        "DELETE FROM memory_embeddings WHERE id=$1",
        "UPDATE embedding_corpus_generation_members SET legacy_embedding_id=legacy_embedding_id WHERE legacy_embedding_id=$1",
        "DELETE FROM embedding_corpus_generation_members WHERE legacy_embedding_id=$1",
        "INSERT INTO embedding_corpus_generation_members(workspace_id,corpus_generation_id,legacy_embedding_id) SELECT workspace_id,corpus_generation_id,$1 FROM embedding_corpus_generation_members",
        "INSERT INTO embedding_corpus_generation_members(workspace_id,corpus_generation_id,embedding_projection_entry_id) SELECT workspace_id,corpus_generation_id,$1 FROM embedding_corpus_generation_members",
    ] {
        let mut tx = runtime.begin().await.unwrap();
        set_context(&mut tx, context.workspace_id).await;
        let error = sqlx::query(statement)
            .bind(embedding)
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("42501"),
            "{statement}: {error}"
        );
        tx.rollback().await.unwrap();
    }
    let bytes_after: String =
        sqlx::query_scalar("SELECT embedding::text FROM memory_embeddings WHERE id=$1")
            .bind(embedding)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(bytes_after, bytes_before);
    let counts:(i64,i64,Option<Uuid>)=sqlx::query_as("SELECT (SELECT count(*) FROM embedding_corpus_generation_members WHERE corpus_generation_id=$1),(SELECT count(*) FROM embedding_corpus_generations WHERE id=$1),current_generation_id FROM embedding_index_generation_guards WHERE space_registration_id=$2").bind(generation).bind(registration).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, None));
}

#[sqlx::test(migrations = "../../migrations")]
async fn sqlx_fallback_installs_the_same_canonical_authority(pool: PgPool) {
    assert_canonical_schema(&pool).await;
}

#[tokio::test]
async fn legacy_upsert_refuses_before_database_or_dimension_work() {
    use vestrace_application::{
        ApplicationError,
        retrieval::{EmbeddingSpace, EmbeddingStore},
    };
    use vestrace_domain::{EmbeddingSpaceId, MemoryId};
    use vestrace_infrastructure::postgres::PgEmbeddingStore;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    pool.close().await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool));
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let space = EmbeddingSpace {
        id: EmbeddingSpaceId::new(),
        name: "legacy".into(),
        model: "legacy".into(),
        dimensions: 2,
    };
    for input in [vec![1.0, 0.0], vec![], vec![f32::NAN]] {
        let error = store
            .upsert(&context, &space, MemoryId::new(), &input)
            .await
            .unwrap_err();
        assert!(
            matches!(error,ApplicationError::Unavailable(ref message) if message=="embedding-legacy-write-retired"),
            "{error:?}"
        );
    }
}

#[tokio::test]
async fn legacy_vector_search_never_calls_provider_policy_or_database() {
    use async_trait::async_trait;
    use std::{collections::BTreeSet, sync::Arc};
    use vestrace_application::{
        ApplicationError, EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
        EmbeddingDataPolicyGate, EmbeddingDataPolicyMode, EmbeddingDataPolicySettings,
        NormalizedRetrievalRequest, ProviderEgress, RetrievalRequest, VectorRetriever,
        retrieval::EmbeddingProvider,
    };
    use vestrace_domain::{
        DataDestination, DataPolicyId, Sensitivity, retrieval::ClassificationPolicy,
        trust::DataPolicy,
    };
    struct Never;
    #[async_trait]
    impl EmbeddingProvider for Never {
        fn model(&self) -> &str {
            panic!("retired vector channel must not inspect provider")
        }
        async fn embed(&self, _: &[String]) -> Result<Vec<Vec<f32>>, ApplicationError> {
            panic!("retired vector channel must not dispatch")
        }
    }
    #[async_trait]
    impl EmbeddingDataPolicyDecisionRepository for Never {
        async fn record(
            &self,
            _: &EmbeddingDataPolicyDecisionRecord,
        ) -> Result<(), ApplicationError> {
            panic!("retired vector channel must not authorize")
        }
    }
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    pool.close().await;
    let provider = EmbeddingDataPolicyGate::new(
        EmbeddingDataPolicySettings {
            classification_policy: ClassificationPolicy::new(Vec::<String>::new(), true).unwrap(),
            classification: Sensitivity::Internal,
            policy: DataPolicy::new(
                DataPolicyId::new(),
                "canonical-test",
                Sensitivity::Internal,
                BTreeSet::from([DataDestination::LocalModel]),
                None,
            )
            .unwrap(),
            mode: EmbeddingDataPolicyMode::Enforce,
        },
        Arc::new(Never),
    )
    .govern(
        Arc::new(Never),
        ProviderEgress::new(
            "http://127.0.0.1:1/embeddings",
            DataDestination::LocalModel,
            true,
            true,
        ),
    );
    let retriever = vestrace_infrastructure::postgres::PgVectorRetriever::new(
        PgStore::from_pool(pool),
        provider,
        "legacy",
    );
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let request =
        NormalizedRetrievalRequest::normalize(RetrievalRequest::new(context.workspace_id, "query"))
            .unwrap();
    let error = retriever.search(&context, &request).await.unwrap_err();
    assert!(
        matches!(error,ApplicationError::Unavailable(ref message) if message=="embedding-legacy-adoption-required")
    );
}

#[sqlx::test(migrations = false)]
async fn canonical_registration_and_head_refuse_crossed_pins_without_partial_state(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let f = canonical_qualification(&pool, &runtime).await;
    for (model, qualification, shape, returned, encoding, dimensions) in [
        (
            Uuid::now_v7(),
            f.qualification,
            f.shape,
            "text-embedding-nomic-embed-text-v1.5",
            "float",
            4,
        ),
        (
            f.model,
            Uuid::now_v7(),
            f.shape,
            "text-embedding-nomic-embed-text-v1.5",
            "float",
            4,
        ),
        (
            f.model,
            f.qualification,
            Uuid::now_v7(),
            "text-embedding-nomic-embed-text-v1.5",
            "float",
            4,
        ),
        (f.model, f.qualification, f.shape, "wrong-model", "float", 4),
        (
            f.model,
            f.qualification,
            f.shape,
            "text-embedding-nomic-embed-text-v1.5",
            "base64",
            4,
        ),
        (
            f.model,
            f.qualification,
            f.shape,
            "text-embedding-nomic-embed-text-v1.5",
            "float",
            0,
        ),
    ] {
        let mut tx = runtime.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        let attempt=sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'negative',$3,$4,$5,$6,$7,$8)").bind(Uuid::now_v7()).bind(f.workspace.as_uuid()).bind(model).bind(qualification).bind(shape).bind(returned).bind(encoding).bind(dimensions).execute(&mut *tx).await;
        let error = match attempt {
            Ok(_) => tx.commit().await.unwrap_err(),
            Err(error) => {
                tx.rollback().await.unwrap();
                error
            }
        };
        assert!(
            matches!(
                error.as_database_error().unwrap().code().as_deref(),
                Some("22023" | "23502" | "23503" | "23514")
            ),
            "{error}"
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM embedding_space_registrations WHERE workspace_id=$1",
        )
        .bind(f.workspace.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            count, 0,
            "refused registration cannot leave an authority row"
        );
    }
    let registration = Uuid::now_v7();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'negative',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let chat: Uuid = sqlx::query_scalar(
        "SELECT chat_model_revision_id FROM qualification_target_bindings WHERE id=$1",
    )
    .bind(f.target)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut forged = pool.begin().await.unwrap();
    set_context(&mut forged, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *forged)
        .await
        .unwrap();
    sqlx::query("UPDATE model_qualification_heads SET active_space_registration_id=$1 WHERE workspace_id=$2 AND model_revision_id=$3").bind(registration).bind(f.workspace.as_uuid()).bind(chat).execute(&mut *forged).await.unwrap();
    let error = forged.commit().await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    for (model, version) in [(f.model, 0_i64), (chat, 1)] {
        let mut tx = runtime.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        let error = sqlx::query("SELECT vestrace_set_initial_embedding_active_space($1,$2,$3,$4)")
            .bind(f.workspace.as_uuid())
            .bind(model)
            .bind(version)
            .bind(registration)
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().message(),
            "initial embedding active space compare-and-swap conflict"
        );
        tx.rollback().await.unwrap();
        let active:i64=sqlx::query_scalar("SELECT count(*) FROM model_qualification_heads WHERE workspace_id=$1 AND active_space_registration_id IS NOT NULL").bind(f.workspace.as_uuid()).fetch_one(&pool).await.unwrap();
        assert_eq!(active, 0);
    }
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    let error = sqlx::query("SELECT vestrace_capture_embedding_generation($1,$2,$3,0)")
        .bind(Uuid::now_v7())
        .bind(f.workspace.as_uuid())
        .bind(registration)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().message(),
        "canonical generation capture guard conflict"
    );
    tx.rollback().await.unwrap();
    let generations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_corpus_generations WHERE workspace_id=$1",
    )
    .bind(f.workspace.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(generations, 0);
}

#[sqlx::test(migrations = false)]
async fn observed_dimensions_are_exact_and_expired_qualification_cannot_activate(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let original = canonical_qualification(&pool, &runtime).await;
    let (connection,revision,no_auth,chat,model):(Uuid,Uuid,Uuid,Uuid,Uuid)=sqlx::query_as("SELECT t.connection_id,t.connection_revision_id,t.no_auth_binding_revision_id,t.chat_model_revision_id,m.model_id FROM qualification_target_bindings t JOIN model_revisions m ON m.id=t.embedding_model_revision_id WHERE t.id=$1").bind(original.target).fetch_one(&pool).await.unwrap();
    let guard: Uuid = sqlx::query_scalar(
        "SELECT id FROM connection_execution_guards WHERE workspace_id=$1 AND connection_id=$2",
    )
    .bind(original.workspace.as_uuid())
    .bind(connection)
    .fetch_one(&pool)
    .await
    .unwrap();
    let f = QualificationFixture {
        workspace: original.workspace,
        principal: original.principal,
        job: QualificationJobId::new(),
        target: Uuid::now_v7(),
        model: Uuid::now_v7(),
        qualification: Uuid::now_v7(),
        shape: original.shape,
    };
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_create_model_revision_and_advance_head($1,$2,$3,$4,$5,$6,'text-embedding-nomic-embed-text-v1.5','embedding',NULL,NULL,NULL,4,$7,'provider',1)").bind(f.model).bind(f.workspace.as_uuid()).bind(model).bind(connection).bind(guard).bind(revision).bind(original.qualification).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,state) VALUES($1,$2,$3,'q1','running')").bind(f.job.as_uuid()).bind(f.workspace.as_uuid()).bind(revision).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO qualification_target_bindings(id,workspace_id,qualification_job_id,connection_id,connection_revision_id,branch,no_auth_binding_revision_id,chat_model_revision_id,embedding_model_revision_id) VALUES($1,$2,$3,$4,$5,'no_auth',$6,$7,$8)").bind(f.target).bind(f.workspace.as_uuid()).bind(f.job.as_uuid()).bind(connection).bind(revision).bind(no_auth).bind(chat).bind(f.model).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    record_complete_q1_matrix(&pool, &runtime, &f).await;
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_finalize_qualification_job($1,$2,$3,$4,$5)")
        .bind(f.workspace.as_uuid())
        .bind(f.job.as_uuid())
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(f.qualification)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    let error=sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'observed',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',3)").bind(Uuid::now_v7()).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().message(),
        "canonical embedding space requires exact qualified structural evidence"
    );
    tx.rollback().await.unwrap();
    let registration = Uuid::now_v7();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'observed',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();

    // Controlled expired-head fixture: append an immutable revision with the
    // same real Q1 evidence and an elapsed deadline. No trigger is disabled and
    // no existing immutable qualification or evidence row is changed.
    let expired = Uuid::now_v7();
    let mut tx = pool.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_qualification_revisions(id,workspace_id,model_revision_id,connection_revision_id,connection_qualification_revision_id,qualification_job_id,capabilities,valid_until) SELECT $1,workspace_id,model_revision_id,connection_revision_id,connection_qualification_revision_id,qualification_job_id,capabilities,NOW()-INTERVAL '1 second' FROM model_qualification_revisions WHERE id=$2").bind(expired).bind(f.qualification).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE model_qualification_heads SET current_qualification_revision_id=$1,version=version+1 WHERE workspace_id=$2 AND model_revision_id=$3").bind(expired).bind(f.workspace.as_uuid()).bind(f.model).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let staged = Uuid::now_v7();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'expired-staged',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(staged).bind(f.workspace.as_uuid()).bind(f.model).bind(expired).bind(f.shape).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    let error = sqlx::query("SELECT vestrace_set_initial_embedding_active_space($1,$2,2,$3)")
        .bind(f.workspace.as_uuid())
        .bind(f.model)
        .bind(staged)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().message(),
        "initial embedding active space compare-and-swap conflict"
    );
    tx.rollback().await.unwrap();
    let head:(i64,Option<Uuid>)=sqlx::query_as("SELECT version,active_space_registration_id FROM model_qualification_heads WHERE workspace_id=$1 AND model_revision_id=$2").bind(f.workspace.as_uuid()).bind(f.model).fetch_one(&pool).await.unwrap();
    assert_eq!(head, (2, None));
}

#[sqlx::test(migrations = false)]
async fn concurrent_exact_canonical_registrations_converge_after_unique_key_wait(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let f = canonical_qualification(&pool, &runtime).await;
    let second_runtime = common::runtime_pool(&pool).await;
    let mut first = runtime.begin().await.unwrap();
    set_context(&mut first, f.workspace).await;
    let first_id = Uuid::now_v7();
    let stored:Uuid=sqlx::query_scalar("SELECT vestrace_register_canonical_embedding_space($1,$2,'concurrent',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(first_id).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).fetch_one(&mut *first).await.unwrap();
    assert_eq!(stored, first_id);
    let first_backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *first)
        .await
        .unwrap();
    let workspace = f.workspace;
    let model = f.model;
    let qualification = f.qualification;
    let shape = f.shape;
    let name = format!("canonical-register-{}", Uuid::now_v7());
    let child_name = name.clone();
    let second = tokio::spawn(async move {
        let mut tx = second_runtime.begin().await.unwrap();
        set_context(&mut tx, workspace).await;
        sqlx::query("SELECT set_config('application_name',$1,true)")
            .bind(child_name)
            .execute(&mut *tx)
            .await
            .unwrap();
        let attempt=sqlx::query_scalar::<_,Uuid>("SELECT vestrace_register_canonical_embedding_space($1,$2,'concurrent',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(Uuid::now_v7()).bind(workspace.as_uuid()).bind(model).bind(qualification).bind(shape).fetch_one(&mut *tx).await;
        match attempt {
            Ok(id) => tx.commit().await.map(|_| id),
            Err(error) => {
                tx.rollback().await.unwrap();
                Err(error)
            }
        }
    });
    let observed=tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND application_name=$1 AND $2=ANY(pg_blocking_pids(pid)))").bind(&name).bind(first_backend).fetch_one(&pool).await.unwrap();
            if waiting {break;}
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.is_ok();
    first.commit().await.unwrap();
    let second_result = second.await.unwrap();
    assert!(
        observed,
        "the independent registration must really wait for the first unique key"
    );
    let rows: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM embedding_space_registrations WHERE workspace_id=$1 AND name='concurrent'",
    )
    .bind(workspace.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows, vec![first_id]);
    assert_eq!(
        second_result.unwrap(),
        first_id,
        "both exact callers must receive the persisted registration"
    );
}

async fn assert_canonical_generation_history_refuses_rewrite(pool: PgPool, rewrite: &str) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let f = canonical_qualification(&pool, &runtime).await;
    let registration = Uuid::now_v7();
    let generation = Uuid::now_v7();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'canonical',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT * FROM vestrace_capture_embedding_generation($1,$2,$3,1)")
        .bind(generation)
        .bind(f.workspace.as_uuid())
        .bind(registration)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    if rewrite == "legacy" {
        let legacy_space = Uuid::now_v7();
        sqlx::query("INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) VALUES($1,$2,'legacy',4,'legacy-model')").bind(legacy_space).bind(f.workspace.as_uuid()).execute(&pool).await.unwrap();
        let mut tx = runtime.begin().await.unwrap();
        set_context(&mut tx, f.workspace).await;
        sqlx::query("SELECT vestrace_register_embedding_space($1,$2,$3,'legacy','legacy-model',4)")
            .bind(Uuid::now_v7())
            .bind(f.workspace.as_uuid())
            .bind(legacy_space)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(g) FROM embedding_corpus_generations g WHERE id=$1")
            .bind(generation)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut forged = pool.begin().await.unwrap();
    set_context(&mut forged, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *forged)
        .await
        .unwrap();
    let statement = match rewrite {
        "delete" => "DELETE FROM embedding_corpus_generations WHERE id=$1",
        "legacy" => {
            "UPDATE embedding_corpus_generations g SET member_representation='legacy_upgrade',state='stale',space_registration_id=(SELECT s.id FROM embedding_space_registrations s WHERE s.workspace_id=g.workspace_id AND s.registration_kind='legacy_upgrade'),generation_epoch=NULL,captured_guard_version=NULL,corpus_revision=NULL,built_through_projection_ordinal=NULL WHERE g.id=$1"
        }
        _ => "UPDATE embedding_corpus_generations SET id=gen_random_uuid() WHERE id=$1",
    };
    sqlx::query(statement)
        .bind(generation)
        .execute(&mut *forged)
        .await
        .unwrap();
    let attempted = forged.commit().await;
    let after: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT to_jsonb(g) FROM embedding_corpus_generations g WHERE space_registration_id=$1",
    )
    .bind(registration)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        after,
        vec![before],
        "canonical generation identity and history must survive guarded rewrite attempts"
    );
    assert_eq!(
        attempted
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .as_deref(),
        Some("23514")
    );
}

#[sqlx::test(migrations = false)]
async fn canonical_generation_history_refuses_identity_change(pool: PgPool) {
    assert_canonical_generation_history_refuses_rewrite(pool, "identity").await;
}

#[sqlx::test(migrations = false)]
async fn canonical_generation_history_refuses_delete(pool: PgPool) {
    assert_canonical_generation_history_refuses_rewrite(pool, "delete").await;
}

#[sqlx::test(migrations = false)]
async fn active_canonical_space_without_generation_takes_precedence_over_legacy_history(
    pool: PgPool,
) {
    use vestrace_application::{ApplicationError, retrieval::CorpusGenerationResolver};
    use vestrace_infrastructure::postgres::{PgCorpusGenerationResolver, PgStore};
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let f = canonical_qualification(&pool, &runtime).await;
    let registration = Uuid::now_v7();
    let legacy_space = Uuid::now_v7();
    sqlx::query("INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) VALUES($1,$2,'canonical',4,'text-embedding-nomic-embed-text-v1.5')").bind(legacy_space).bind(f.workspace.as_uuid()).execute(&pool).await.unwrap();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_register_embedding_space($1,$2,$3,'canonical','text-embedding-nomic-embed-text-v1.5',4)").bind(Uuid::now_v7()).bind(f.workspace.as_uuid()).bind(legacy_space).execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'canonical',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT vestrace_set_initial_embedding_active_space($1,$2,1,$3)")
        .bind(f.workspace.as_uuid())
        .bind(f.model)
        .bind(registration)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let resolver = PgCorpusGenerationResolver::new(PgStore::from_pool(runtime));
    let context = RequestContext::new(f.workspace, f.principal);
    let error = resolver
        .resolve_canonical(
            &context,
            "canonical",
            "text-embedding-nomic-embed-text-v1.5",
        )
        .await
        .unwrap_err();
    assert!(
        matches!(error,ApplicationError::Unavailable(ref reason) if reason=="embedding-canonical-generation-required"),
        "{error:?}"
    );
}

#[sqlx::test(migrations = false)]
async fn canonical_generation_history_refuses_representation_downgrade(pool: PgPool) {
    assert_canonical_generation_history_refuses_rewrite(pool, "legacy").await;
}

async fn assert_generation_guard_is_permanent(pool: PgPool, ready: bool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let f = canonical_qualification(&pool, &runtime).await;
    let registration = Uuid::now_v7();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'canonical',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)").bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
    if ready {
        let generation = Uuid::now_v7();
        sqlx::query("SELECT * FROM vestrace_capture_embedding_generation($1,$2,$3,1)")
            .bind(generation)
            .bind(f.workspace.as_uuid())
            .bind(registration)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT * FROM vestrace_publish_embedding_generation($1,$2,$3,1)")
            .bind(f.workspace.as_uuid())
            .bind(registration)
            .bind(generation)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    let before: serde_json::Value = sqlx::query_scalar("SELECT to_jsonb(g) FROM embedding_index_generation_guards g WHERE space_registration_id=$1").bind(registration).fetch_one(&pool).await.unwrap();
    let mut forged = pool.begin().await.unwrap();
    set_context(&mut forged, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *forged)
        .await
        .unwrap();
    sqlx::query("DELETE FROM embedding_index_generation_guards WHERE space_registration_id=$1")
        .bind(registration)
        .execute(&mut *forged)
        .await
        .unwrap();
    let attempted = forged.commit().await;
    let after: Option<serde_json::Value> = sqlx::query_scalar("SELECT to_jsonb(g) FROM embedding_index_generation_guards g WHERE space_registration_id=$1").bind(registration).fetch_optional(&pool).await.unwrap();
    assert_eq!(
        after,
        Some(before.clone()),
        "the generation guard is permanent even without a current generation"
    );
    assert_eq!(
        attempted
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .as_deref(),
        Some("23514")
    );
    // The existing composite FK independently refuses an absent destination.
    let mut forged = pool.begin().await.unwrap();
    set_context(&mut forged, f.workspace).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *forged)
        .await
        .unwrap();
    let error=sqlx::query("UPDATE embedding_index_generation_guards SET space_registration_id=gen_random_uuid() WHERE space_registration_id=$1").bind(registration).execute(&mut *forged).await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
    forged.rollback().await.unwrap();
    let after: serde_json::Value = sqlx::query_scalar("SELECT to_jsonb(g) FROM embedding_index_generation_guards g WHERE space_registration_id=$1").bind(registration).fetch_one(&pool).await.unwrap();
    assert_eq!(after, before);
}

#[sqlx::test(migrations = false)]
async fn generation_guard_is_permanent_when_ready(pool: PgPool) {
    assert_generation_guard_is_permanent(pool, true).await;
}

#[sqlx::test(migrations = false)]
async fn generation_guard_is_permanent_when_empty(pool: PgPool) {
    assert_generation_guard_is_permanent(pool, false).await;
}

/// The governed Models projection reports embedding readiness by its exact
/// names, for embedding models only, and never claims an index is loaded.
///
/// Two phases against one world, because the interesting property is that the
/// reported reason *changes* as adoption progresses. A projection that reported
/// the same thing before and after registering a canonical space would be
/// describing the installation rather than its state.
#[sqlx::test(migrations = false)]
async fn the_model_projection_reports_embedding_readiness_but_never_index_presence(pool: PgPool) {
    use vestrace_application::{ModelRevisionRepository, RequestContext};
    use vestrace_domain::embedding::EmbeddingReadinessReason;
    use vestrace_infrastructure::postgres::{PgModelRevisionRepository, PgStore};

    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let f = canonical_qualification(&pool, &runtime).await;
    let repository = PgModelRevisionRepository::new(PgStore::from_pool(runtime.clone()));
    let context = RequestContext::new(f.workspace, f.principal);

    // The projection for the embedding model revision, and for everything else.
    async fn split(
        repository: &PgModelRevisionRepository,
        context: &RequestContext,
        embedding_revision: Uuid,
    ) -> (Vec<String>, Vec<String>) {
        let models = repository
            .list_safe_models(context)
            .await
            .expect("the governed projection is readable");
        let mut embedding = Vec::new();
        let mut others = Vec::new();
        for model in models {
            if model.revision_id.map(|id| id.as_uuid()) == Some(embedding_revision) {
                embedding = model.blockers;
            } else {
                others.extend(model.blockers);
            }
        }
        (embedding, others)
    }

    fn readiness(blockers: &[String]) -> Vec<EmbeddingReadinessReason> {
        blockers
            .iter()
            .filter_map(|blocker| blocker.parse().ok())
            .collect()
    }

    // Phase one: a legacy corpus exists for this model's wire name and no
    // canonical space has been registered.
    let legacy_space = Uuid::now_v7();
    sqlx::query("INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) VALUES($1,$2,'canonical',4,'text-embedding-nomic-embed-text-v1.5')")
        .bind(legacy_space).bind(f.workspace.as_uuid()).execute(&pool).await.unwrap();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_register_embedding_space($1,$2,$3,'canonical','text-embedding-nomic-embed-text-v1.5',4)")
        .bind(Uuid::now_v7()).bind(f.workspace.as_uuid()).bind(legacy_space).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();

    let (embedding, others) = split(&repository, &context, f.model).await;
    assert_eq!(
        readiness(&embedding),
        vec![EmbeddingReadinessReason::LegacyAdoptionRequired],
        "an unadopted legacy corpus is the one thing blocking this model: {embedding:?}"
    );
    assert!(
        readiness(&others).is_empty(),
        "a chat model has no corpus, so no embedding reason may be reported against it: {others:?}"
    );

    // Phase two: a canonical space is registered and made active, and no
    // generation has been published into it yet.
    let registration = Uuid::now_v7();
    let mut tx = runtime.begin().await.unwrap();
    set_context(&mut tx, f.workspace).await;
    sqlx::query("SELECT vestrace_register_canonical_embedding_space($1,$2,'canonical',$3,$4,$5,'text-embedding-nomic-embed-text-v1.5','float',4)")
        .bind(registration).bind(f.workspace.as_uuid()).bind(f.model).bind(f.qualification).bind(f.shape).execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT vestrace_set_initial_embedding_active_space($1,$2,1,$3)")
        .bind(f.workspace.as_uuid())
        .bind(f.model)
        .bind(registration)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let (embedding, others) = split(&repository, &context, f.model).await;
    assert_eq!(
        readiness(&embedding),
        vec![EmbeddingReadinessReason::GenerationNotReady],
        "adoption has happened; what is missing now is a published generation: {embedding:?}"
    );
    assert!(readiness(&others).is_empty());

    // And in neither phase, for any model, does a database read claim to know
    // whether some process holds an index. It cannot: the process that would
    // know is not this one, and it may have died since anything was written.
    let all = repository.list_safe_models(&context).await.unwrap();
    for model in &all {
        for blocker in &model.blockers {
            assert_ne!(
                blocker.as_str(),
                EmbeddingReadinessReason::IndexNotLoaded.as_str(),
                "a storage projection must never assert process-local index presence"
            );
        }
    }

    runtime.close().await;
}
