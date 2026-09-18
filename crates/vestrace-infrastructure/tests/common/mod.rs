//! The governed embedding fixture both embedding suites build on.
//!
//! Rust test binaries share no code, so without this module the dispatch suite
//! and the recovery suite would each carry their own definition of what a
//! governed embedding job is. Two copies of that definition drift, and a drifted
//! fixture produces evidence about a system that does not exist.
//!
//! Everything here goes through a guarded function or an ordinary runtime
//! insert, except the qualification and binding-snapshot rows P03 has no
//! publisher for -- a known gap this package records rather than papers over.

#![allow(dead_code)]

use std::{str::FromStr, sync::Arc};

use chrono::Utc;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, CredentialDispatchLeaseRepository, ExternalEffectRepository,
    IdempotencyRecord, ModelRequestEvidenceRepository, OutboxMessage, ProviderDispatchCause,
    ProviderDispatchFaultInjector, ProviderDispatchFaultPoint, ProviderDispatchPolicyEvaluator,
    ProviderDispatchRequest, RequestContext, retrieval::EmbeddingStore,
};
use vestrace_domain::{
    AuditEvent, AuthorizationRequest, Capability, ConnectionId, ConnectionRevisionId,
    DeliverySemantics, EffectPrecondition, EffectReversibility, EmbeddingJobId,
    ExternalEffectIntent, IdempotencyProfile, ModelRequestEvidenceId, PolicyDecision,
    PolicyDecisionId, PolicyDecisionReason, PolicyDecisionResult, PrincipalId, RiskCategory,
    WorkerId, WorkspaceId, id::AuditEventId, security::PolicyInputState,
};
use vestrace_infrastructure::{
    PgEmbeddingStore, PgExternalEffectRepository, PgGovernedMutationRepository,
    PgInstallationMutationPermit, PgModelDataPolicyDecisionRepository,
    PgProviderDispatchRepository, PgStore,
};

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

pub async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var(RUNTIME_DATABASE_URL_ENV)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let parsed = PgConnectOptions::from_str(&runtime_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .expect("runtime URL must contain a password");
    let runtime = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(parsed.get_username())
                .password(password),
        )
        .await
        .expect("runtime must connect to the SQLx test database");
    let identity: (String, bool) = sqlx::query_as(
        "SELECT current_user::TEXT, \
          (SELECT rolsuper FROM pg_roles WHERE rolname=current_user)::BOOLEAN",
    )
    .fetch_one(&runtime)
    .await
    .expect("runtime identity must be observable");
    assert_eq!(
        identity.0, "vestrace",
        "behavioral fixtures must use the runtime role"
    );
    assert!(
        !identity.1,
        "behavioral fixtures must not run as a superuser"
    );
    runtime
}

/// SQLx provisions its disposable databases as the test migrator, whereas the
/// deployment bootstrap has `vestrace` own these legacy runtime/retrieval tables.
/// Mirror that production ownership before exercising the real runtime paths;
/// no function owner or runtime privilege is changed here.
pub async fn prepare_legacy_embedding_runtime_ownership(pool: &PgPool) {
    // `memory_embeddings` is deliberately absent. Migration 0197 line 604 hands
    // it to `vestrace_guarded_owner`, and the real provisioner does the same at
    // init-runtime-role.sh:2306, so giving it back to `vestrace` here does not
    // mirror production -- it restores the pre-0197 owner. It also breaks the
    // canonical corpus outright: `vestrace_validate_embedding_corpus_generation_member`
    // is SECURITY DEFINER as the guarded owner and reads this table, so under
    // the wrong owner every canonical generation publication fails at COMMIT
    // with 42501, which is why no fixture built here could ever reach one.
    for table in [
        "embedding_spaces",
        "memories",
        "memory_revisions",
        "search_documents",
    ] {
        sqlx::query(&format!("ALTER TABLE public.{table} OWNER TO vestrace"))
            .execute(pool)
            .await
            .unwrap();
        let owner: String = sqlx::query_scalar(
            "SELECT pg_get_userbyid(class.relowner) FROM pg_class AS class \
              JOIN pg_namespace AS namespace ON namespace.oid = class.relnamespace \
             WHERE namespace.nspname='public' AND class.relname=$1",
        )
        .bind(table)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(
            owner, "vestrace",
            "fixture must mirror the production owner for {table}"
        );
    }
}

/// One embedding effect intent, scoped to the workspace rather than a run.
///
/// `workspace://` is what a non-Run governed effect says here, and it is what
/// P03's qualification probe already says. The domain's closed vocabulary is
/// `run://`, `execution://` and `workspace://`; an embedding delivery job has no
/// run behind it, and the reference names an execution rather than an owner, so
/// inventing `embedding://` would widen a vocabulary to record something it does
/// not mean. The job row references the effect; the effect does not reference
/// the job.
pub fn workspace_scoped_intent(
    context: &RequestContext,
    snapshot_id: Uuid,
) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        "workspace://",
        context.workspace_id,
        context.principal_id,
        "openai-compatible",
        "embeddings",
        "http://127.0.0.1:1234/v1/embeddings",
        "sha256:arguments",
        "produce a governed embedding",
        vec![EffectPrecondition::new("model-snapshot", snapshot_id.to_string()).unwrap()],
        "sha256:preconditions",
        RiskCategory::Medium,
        EffectReversibility::Unknown,
        IdempotencyProfile::ProviderKey,
        DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        None::<String>,
        None::<String>,
        Utc::now(),
    )
    .unwrap()
}

/// A fresh intent for a second job in an already-prepared fixture.
pub fn embedding_intent(fixture: &AcceptedJob, _job_id: EmbeddingJobId) -> ExternalEffectIntent {
    workspace_scoped_intent(&fixture.context, fixture.snapshot_id)
}

/// The durable identities one accepted embedding job is made of.
pub struct AcceptedJob {
    pub context: RequestContext,
    pub job_id: EmbeddingJobId,
    pub connection_id: Uuid,
    pub connection_revision_id: Uuid,
    pub connection_qualification_id: Uuid,
    pub model_revision_id: Uuid,
    pub space_registration_id: Uuid,
    pub model_qualification_id: Uuid,
    pub no_auth_binding_id: Uuid,
    pub snapshot_id: Uuid,
    pub external_effect_id: Uuid,
    pub evidence_id: Uuid,
    pub intent: ExternalEffectIntent,
    pub credential: Option<PinnedCredential>,
}

/// The immutable credential tuple a credential-branch snapshot pinned at
/// acceptance.  It is exposed only so a dispatch-boundary test can invoke the
/// same guarded lease issuer that production dispatch uses.
#[derive(Clone)]
pub struct PinnedCredential {
    pub slot_id: Uuid,
    pub revision_id: Uuid,
    pub activation_guard_id: Uuid,
    pub completion_blocker_id: Uuid,
}

/// Builds one no-auth embedding job the way a deployment would, and stops.
///
/// Everything here goes through a guarded function or an ordinary runtime
/// insert; nothing is written as `vestrace_guarded_owner` except the
/// qualification and snapshot rows P03 has no publisher for yet, which is a
/// known gap recorded in this package's evidence rather than something this
/// fixture may pretend away.  The credential variant below keeps only its
/// qualification head and immutable snapshot in that fixture boundary; it
/// publishes Candidate and Active credential state through the guarded path.
pub async fn accept_embedding_job(owner: &PgPool, runtime: &PgPool) -> AcceptedJob {
    accept_embedding_job_inner(owner, runtime, false, true, false, false).await
}

/// Builds every immutable prerequisite for a delivery embedding job but leaves
/// the job itself absent.  Task 14C uses this boundary to prove its receipt is
/// installed before the job/output/audit mutation rather than backfilling an
/// already accepted job.
pub async fn prepare_delivery_embedding_job(owner: &PgPool, runtime: &PgPool) -> AcceptedJob {
    accept_embedding_job_inner(owner, runtime, false, false, false, false).await
}

/// A second delivery owns a new effect/evidence/job while retaining the same
/// immutable binding snapshot, credential and registered space.
pub async fn prepare_additional_delivery_embedding_job(
    runtime: &PgPool,
    base: &AcceptedJob,
) -> AcceptedJob {
    let intent = workspace_scoped_intent(&base.context, base.snapshot_id);
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .insert_intent(&base.context, &intent)
        .await
        .unwrap();
    AcceptedJob {
        context: base.context.clone(),
        job_id: EmbeddingJobId::new(),
        connection_id: base.connection_id,
        connection_revision_id: base.connection_revision_id,
        connection_qualification_id: base.connection_qualification_id,
        model_revision_id: base.model_revision_id,
        space_registration_id: base.space_registration_id,
        model_qualification_id: base.model_qualification_id,
        no_auth_binding_id: base.no_auth_binding_id,
        snapshot_id: base.snapshot_id,
        external_effect_id: intent.id().as_uuid(),
        evidence_id: Uuid::now_v7(),
        intent,
        credential: base.credential.clone(),
    }
}

/// Builds the same pre-acceptance delivery boundary with a real guarded
/// credential Candidate -> Active chain and a credential-pinned snapshot.
pub async fn prepare_delivery_embedding_job_with_pinned_credential(
    owner: &PgPool,
    runtime: &PgPool,
) -> AcceptedJob {
    accept_embedding_job_inner(owner, runtime, true, false, false, false).await
}

/// Builds an accepted embedding job whose immutable binding snapshot carries a
/// current credential.  P03 still has no credential-snapshot publisher, so its
/// activation and snapshot rows use the same guarded-owner fixture boundary as
/// the no-auth qualification and snapshot rows above; the dispatch lease is
/// nevertheless issued through its real runtime guarded function.
pub async fn accept_embedding_job_with_pinned_credential(
    owner: &PgPool,
    runtime: &PgPool,
) -> AcceptedJob {
    accept_embedding_job_inner(owner, runtime, true, true, false, false).await
}

/// Historical 0194 allowed a nonterminal lease as credential protection.
/// Create that old state through the existing Candidate-only blocker command;
/// no terminalization, ownership transfer or erasure guard is manufactured.
pub async fn prepare_legacy_delivery_with_expiring_credential_lease(
    owner: &PgPool,
    runtime: &PgPool,
) -> AcceptedJob {
    accept_embedding_job_inner(owner, runtime, true, false, true, false).await
}

async fn accept_embedding_job_inner(
    owner: &PgPool,
    runtime: &PgPool,
    credential_backed: bool,
    accept_job: bool,
    legacy_expiring_lease: bool,
    canonical_capability: bool,
) -> AcceptedJob {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    let context = RequestContext::new(workspace_id, principal_id);
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let guard_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let no_auth_id = Uuid::now_v7();
    let credential = credential_backed.then(|| PinnedCredential {
        slot_id: Uuid::now_v7(),
        revision_id: Uuid::now_v7(),
        activation_guard_id: Uuid::now_v7(),
        completion_blocker_id: Uuid::now_v7(),
    });
    let credential_occupancy_id = Uuid::now_v7();
    let credential_intent_id = Uuid::now_v7();
    let credential_material_key_id = Uuid::now_v7();
    let credential_nonce = Uuid::now_v7();
    let credential_attachment_id = Uuid::now_v7();
    let credential_activation_audit_id = Uuid::now_v7();
    let provider_id = Uuid::now_v7();
    let model_id = Uuid::now_v7();
    let model_revision_id = Uuid::now_v7();
    let qualification_job_id = Uuid::now_v7();
    let connection_qualification_id = Uuid::now_v7();
    let model_qualification_id = Uuid::now_v7();
    let snapshot_id = Uuid::now_v7();
    let job_id = EmbeddingJobId::new();
    let evidence_id = Uuid::now_v7();

    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("embedding-{workspace_id}"))
        .execute(owner)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,$3)")
        .bind(principal_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(format!("embedding-{principal_id}"))
        .execute(owner)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors(id,workspace_id,name,provider_type) VALUES($1,$2,$3,'local')",
    )
    .bind(connector_id)
    .bind(workspace_id.as_uuid())
    .bind(format!("embedding-{connector_id}"))
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections(id,connector_id,workspace_id,principal_id,name,status) \
         VALUES($1,$2,$3,$4,$5,'active')",
    )
    .bind(connection_id)
    .bind(connector_id)
    .bind(workspace_id.as_uuid())
    .bind(principal_id.as_uuid())
    .bind(format!("embedding-{connection_id}"))
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO providers(id,workspace_id,name,locality) VALUES($1,$2,'provider','local')",
    )
    .bind(provider_id)
    .bind(workspace_id.as_uuid())
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO models(id,provider_id,workspace_id,model_name,context_window,\
         input_cost_per_mtoken,output_cost_per_mtoken) VALUES($1,$2,$3,'text-embedding-nomic-embed-text-v1.5',4096,0,0)",
    )
    .bind(model_id)
    .bind(provider_id)
    .bind(workspace_id.as_uuid())
    .execute(owner)
    .await
    .unwrap();

    prepare_legacy_embedding_runtime_ownership(owner).await;
    // The legacy space remains runtime-owned until its later replacement, so
    // seed it through the production writer before the guarded registration
    // function verifies its identity.
    let space = PgEmbeddingStore::new(PgStore::from_pool(runtime.clone()))
        .ensure_space(
            &context,
            "nomic-768",
            "text-embedding-nomic-embed-text-v1.5",
            768,
        )
        .await
        .expect("the runtime role creates and registers its legacy embedding space");

    let mut governed = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *governed)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(guard_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .execute(&mut *governed)
        .await
        .unwrap();
    if let Some(credential) = &credential {
        sqlx::query("SELECT vestrace_reserve_credential_slot($1,$2,$3,'provider','primary')")
            .bind(credential.slot_id)
            .bind(workspace_id.as_uuid())
            .bind(connection_id)
            .execute(&mut *governed)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1,$2,$3,$4)")
            .bind(credential.activation_guard_id)
            .bind(workspace_id.as_uuid())
            .bind(connection_id)
            .bind(credential.slot_id)
            .execute(&mut *governed)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1,$2,$3,$4)")
            .bind(credential_occupancy_id)
            .bind(workspace_id.as_uuid())
            .bind(connection_id)
            .bind(credential.slot_id)
            .execute(&mut *governed)
            .await
            .unwrap();
        sqlx::query(
            "SELECT vestrace_reserve_credential_key_creation_intent(\
             $1,$2,$3,$4,$5,$6,$7,$8,'credential_v2')",
        )
        .bind(credential_intent_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .bind(credential.slot_id)
        .bind(credential_occupancy_id)
        .bind(credential.revision_id)
        .bind(credential_material_key_id)
        .bind(credential_nonce)
        .execute(&mut *governed)
        .await
        .unwrap();
    }
    let (kind, logical_url, runtime_url, transport, auth_mode, credential_slot_id) =
        if let Some(credential) = &credential {
            (
                "open_ai_chat_completions_v1",
                "https://api.example.test/v1",
                "https://api.example.test/v1",
                "remote_https",
                "bearer",
                Some(credential.slot_id),
            )
        } else {
            (
                "lm_studio_local",
                "http://127.0.0.1:1234/v1",
                "http://127.0.0.1:1234/v1",
                "lm-studio-local/v1",
                "none",
                None,
            )
        };
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_connection_revision_and_advance_head(\
           $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,0)",
    )
    .bind(connection_revision_id)
    .bind(workspace_id.as_uuid())
    .bind(connection_id)
    .bind(guard_id)
    .bind(kind)
    .bind(logical_url)
    .bind(runtime_url)
    .bind(transport)
    .bind(if credential_backed {
        "remote_https"
    } else {
        "loopback_only"
    })
    .bind(auth_mode)
    .bind(credential_slot_id)
    .fetch_one(&mut *governed)
    .await
    .unwrap();
    if credential.is_none() {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)",
        )
        .bind(no_auth_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .bind(connection_revision_id)
        .fetch_one(&mut *governed)
        .await
        .unwrap();
    }
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_revision_and_advance_head(\
           $1,$2,$3,$4,$5,$6,'text-embedding-nomic-embed-text-v1.5','embedding',NULL,NULL,NULL,NULL,NULL,NULL,0)",
    )
    .bind(model_revision_id)
    .bind(workspace_id.as_uuid())
    .bind(model_id)
    .bind(connection_id)
    .bind(guard_id)
    .bind(connection_revision_id)
    .fetch_one(&mut *governed)
    .await
    .unwrap();
    let space_registration_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_space_registrations WHERE workspace_id=$1 AND space_id=$2",
    )
    .bind(workspace_id.as_uuid())
    .bind(space.id.as_uuid())
    .fetch_one(&mut *governed)
    .await
    .expect("the production space writer registered the exact legacy space");
    governed.commit().await.unwrap();

    if credential.is_some() {
        let mut candidate = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *candidate)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
            .bind(credential_intent_id)
            .execute(&mut *candidate)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1,$2)")
            .bind(credential_intent_id)
            .bind(Uuid::now_v7())
            .execute(&mut *candidate)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_create_credential_prepared_material($1,$2,$3)")
            .bind(credential_intent_id)
            .bind(credential_attachment_id)
            .bind(b"credential-fixture-ciphertext".as_slice())
            .execute(&mut *candidate)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1,$2)")
            .bind(credential_intent_id)
            .bind(Uuid::now_v7())
            .execute(&mut *candidate)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
            .bind(credential_intent_id)
            .execute(&mut *candidate)
            .await
            .unwrap();
        // The blocker is recorded only once the guarded key path has reached
        // Candidate; 0174 deliberately refuses credential blockers before
        // that state.  It remains nonterminal after Active so completion can
        // bind the exact credential authority without a raw fixture write.
        sqlx::query(
            "SELECT vestrace_record_material_erasure_blocker(\
             $1,'credential',NULL,$2,$3,$4)",
        )
        .bind(credential.as_ref().unwrap().completion_blocker_id)
        .bind(credential_intent_id)
        .bind(if legacy_expiring_lease {
            "lease"
        } else {
            "effect"
        })
        .bind(legacy_expiring_lease.then(|| Utc::now() + chrono::Duration::seconds(10)))
        .execute(&mut *candidate)
        .await
        .unwrap();
        candidate.commit().await.unwrap();
    }

    let intent = workspace_scoped_intent(&context, snapshot_id);
    let external_effect_id = intent.id().as_uuid();
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .insert_intent(&context, &intent)
        .await
        .unwrap();

    // Qualification and the binding snapshot have no governed publisher yet.
    // P03 left both to be written as the guarded owner, and P04's Task 4
    // deliberately fixed only the admission policy, so this is written the way
    // P03's own suites write it and the evidence says so.
    let mut seeded = owner.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *seeded)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *seeded)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,\
         state,completed_at) VALUES($1,$2,$3,'q1','succeeded',NOW())",
    )
    .bind(qualification_job_id)
    .bind(workspace_id.as_uuid())
    .bind(connection_revision_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_revisions(id,workspace_id,connection_revision_id,\
         qualification_job_id,profile_revision,valid_until,capabilities) \
         VALUES($1,$2,$3,$4,'q1',NOW()+INTERVAL '1 hour',$5::TEXT[])",
    )
    .bind(connection_qualification_id)
    .bind(workspace_id.as_uuid())
    .bind(connection_revision_id)
    .bind(qualification_job_id)
    .bind(if canonical_capability {
        vec!["embedding", "embeddings"]
    } else {
        vec!["embedding"]
    })
    .execute(&mut *seeded)
    .await
    .unwrap();
    if credential.is_some() {
        sqlx::query(
            "INSERT INTO connection_qualification_heads(\
             workspace_id,connection_revision_id,current_qualification_revision_id,version) \
             VALUES($1,$2,$3,1)",
        )
        .bind(workspace_id.as_uuid())
        .bind(connection_revision_id)
        .bind(connection_qualification_id)
        .execute(&mut *seeded)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO model_qualification_revisions(id,workspace_id,model_revision_id,\
         connection_revision_id,connection_qualification_revision_id,qualification_job_id,\
         capabilities,valid_until) \
         VALUES($1,$2,$3,$4,$5,$6,$7::TEXT[],NOW()+INTERVAL '1 hour')",
    )
    .bind(model_qualification_id)
    .bind(workspace_id.as_uuid())
    .bind(model_revision_id)
    .bind(connection_revision_id)
    .bind(connection_qualification_id)
    .bind(qualification_job_id)
    .bind(if canonical_capability {
        vec!["embedding", "embeddings"]
    } else {
        vec!["embedding"]
    })
    .execute(&mut *seeded)
    .await
    .unwrap();
    if let Some(credential) = &credential {
        sqlx::query(
            "INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,\
             connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,\
             branch,credential_revision_id,credential_slot_id,credential_activation_guard_id,expected_slot_version) \
             VALUES($1,$2,$3,$4,$5,$6,$7,'credential',$8,$9,$10,1)",
        )
        .bind(snapshot_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .bind(connection_revision_id)
        .bind(connection_qualification_id)
        .bind(model_revision_id)
        .bind(model_qualification_id)
        .bind(credential.revision_id)
        .bind(credential.slot_id)
        .bind(credential.activation_guard_id)
        .execute(&mut *seeded)
        .await
        .unwrap();
    } else {
        sqlx::query(
            "INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,\
             connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,\
             branch,no_auth_binding_revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,'no_auth',$8)",
        )
        .bind(snapshot_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .bind(connection_revision_id)
        .bind(connection_qualification_id)
        .bind(model_revision_id)
        .bind(model_qualification_id)
        .bind(no_auth_id)
        .execute(&mut *seeded)
        .await
        .unwrap();
    }
    // P04 Task 6 makes snapshot scope a positive, deferred invariant.  This
    // fixture is ordinary work, so it must write its immutable ordinary mark in
    // the same transaction as the snapshot it seeds.
    sqlx::query(
        "INSERT INTO model_binding_snapshot_scopes(\
         workspace_id,snapshot_id,scope,transition_plan_id) VALUES($1,$2,'ordinary',NULL)",
    )
    .bind(workspace_id.as_uuid())
    .bind(snapshot_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    seeded.commit().await.unwrap();

    if let Some(credential) = &credential {
        sqlx::query(
            "INSERT INTO audit_events(\
             id,workspace_id,principal_id,action,resource_type,resource_id,payload,created_at) \
             VALUES($1,$2,$3,'credential.fixture','credential',$4,'{}'::jsonb,NOW())",
        )
        .bind(credential_activation_audit_id)
        .bind(workspace_id.as_uuid())
        .bind(principal_id.as_uuid())
        .bind(credential.revision_id)
        .execute(owner)
        .await
        .unwrap();

        let mut activate = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *activate)
            .await
            .unwrap();
        let slot_version: i64 = sqlx::query_scalar(
            "SELECT vestrace_activate_first_credential(\
             $1,$2,$3,$4,$5,$6,$7,$8,$9,0)",
        )
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .bind(guard_id)
        .bind(credential.activation_guard_id)
        .bind(credential.slot_id)
        .bind(credential.revision_id)
        .bind(credential_intent_id)
        .bind(connection_qualification_id)
        .bind(credential_activation_audit_id)
        .fetch_one(&mut *activate)
        .await
        .expect("the candidate fixture must activate through the guarded publisher");
        assert_eq!(slot_version, 1);
        activate.commit().await.unwrap();
    }

    if accept_job {
        // Acceptance itself is guarded and runs as the runtime role, in the
        // canonical lock order.
        let mut accept = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace_id.to_string())
            .fetch_one(&mut *accept)
            .await
            .unwrap();
        let accepted = sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_accept_embedding_job($1,$2,$3,'delivery',$4,$5,$6,NULL,NULL::BIGINT)",
        )
        .bind(job_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(space_registration_id)
        .bind(snapshot_id)
        .bind(external_effect_id)
        .bind(evidence_id)
        .fetch_one(&mut *accept)
        .await
        .expect("the runtime role accepts an embedding job through its guarded function");
        assert_eq!(accepted, job_id.as_uuid());
        accept.commit().await.unwrap();
    }

    AcceptedJob {
        context,
        job_id,
        connection_id,
        connection_revision_id,
        connection_qualification_id,
        model_revision_id,
        space_registration_id,
        model_qualification_id,
        no_auth_binding_id: no_auth_id,
        snapshot_id,
        external_effect_id,
        evidence_id,
        intent,
        credential,
    }
}

/// The production dispatch authority, composed with the collaborators the two
/// methods under test never reach.
///
/// They are `unreachable!` rather than permissive doubles on purpose: plan
/// rediscovery and recovery must not evaluate policy, mint evidence, or take a
/// credential lease, and a double that quietly answered would hide it if they
/// did.
pub fn dispatch_repository(runtime: &PgPool) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    PgProviderDispatchRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(UnusedEvidence),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(UnusedCredentialLeases),
        Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
        Arc::new(PgGovernedMutationRepository::new(store)),
        Arc::new(UnusedPolicy),
    )
}

pub struct UnusedEvidence;

#[async_trait::async_trait]
impl ModelRequestEvidenceRepository for UnusedEvidence {
    async fn create_in(
        &self,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _creation: &vestrace_application::CreateModelRequestEvidence,
    ) -> Result<vestrace_domain::ModelRequestEvidenceId, ApplicationError> {
        unreachable!("rediscovery and recovery author no evidence")
    }

    async fn reconstruct_current_in(
        &self,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _workspace: WorkspaceId,
        _evidence: vestrace_domain::ModelRequestEvidenceId,
    ) -> Result<vestrace_application::ModelRequestReconstruction, ApplicationError> {
        unreachable!("rediscovery and recovery reconstruct no request")
    }
}

pub struct UnusedCredentialLeases;

#[async_trait::async_trait]
impl CredentialDispatchLeaseRepository for UnusedCredentialLeases {
    async fn issue(
        &self,
        _context: &RequestContext,
        _request: vestrace_application::CredentialDispatchLeaseRequest,
    ) -> Result<vestrace_application::CredentialDispatchLease, ApplicationError> {
        unreachable!("rediscovery and recovery take no credential lease")
    }

    async fn issue_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _request: vestrace_application::CredentialDispatchLeaseRequest,
    ) -> Result<vestrace_application::CredentialDispatchLease, ApplicationError> {
        unreachable!("rediscovery and recovery take no credential lease")
    }

    async fn consume_for_dispatch(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _lease: &vestrace_application::CredentialDispatchLease,
    ) -> Result<vestrace_application::ConnectionAuth, ApplicationError> {
        unreachable!("rediscovery and recovery consume no credential lease")
    }
}

pub struct UnusedPolicy;

#[async_trait::async_trait]
impl ProviderDispatchPolicyEvaluator for UnusedPolicy {
    async fn evaluate(
        &self,
        _context: &RequestContext,
        _intent: &vestrace_domain::ExternalEffectIntent,
        _cause: &vestrace_application::ProviderDispatchCause,
        _target: &vestrace_application::ProviderDispatchTarget,
        _request: &vestrace_application::EffectiveModelRequest,
    ) -> Result<vestrace_application::ProviderDispatchPolicyEvaluation, ApplicationError> {
        unreachable!("rediscovery and recovery evaluate no policy")
    }
}

/// Completes an accepted job into a dispatchable one.
///
/// Acceptance reserves the evidence identity; the root, its nodes and its
/// completeness check are authored afterwards, exactly as a Run step's are. The
/// admission policy is published through the guarded compare-and-swap this
/// package added, as the runtime role, so nothing here writes a row a
/// deployment could not.
pub async fn make_dispatchable(owner: &PgPool, runtime: &PgPool, fixture: &AcceptedJob) {
    make_dispatchable_with_policy(owner, runtime, fixture, true).await;
}
pub async fn make_dispatchable_with_policy(
    owner: &PgPool,
    runtime: &PgPool,
    fixture: &AcceptedJob,
    initialize_policy: bool,
) {
    let mut published = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *published)
        .await
        .unwrap();
    let shape_id = Uuid::now_v7();
    let limits_id = Uuid::now_v7();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_shape_revision(\
           $1,$2,1,'embeddings',false,ARRAY[]::TEXT[])",
    )
    .bind(shape_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&mut *published)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_limits_revision($1,$2,1,8,1,2048)")
        .bind(limits_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .fetch_one(&mut *published)
        .await
        .unwrap();
    if initialize_policy {
        sqlx::query_scalar::<_, i64>(
            "SELECT vestrace_publish_connection_admission_policy(\
           $1,$2,$3,0::BIGINT,1::SMALLINT,60000,30,900)",
        )
        .bind(Uuid::now_v7())
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.connection_id)
        .fetch_one(&mut *published)
        .await
        .unwrap();
    }
    published.commit().await.unwrap();

    let mut evidence = owner.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *evidence)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *evidence)
        .await
        .unwrap();
    // `cause_kind='embedding_job'` and `cause_id` the job: the third cause this
    // package added to the evidence vocabulary, with the same snapshot-rooted
    // shape as a Run step's.
    sqlx::query(
        "INSERT INTO model_request_evidence_roots\
         (id,workspace_id,external_effect_id,request_kind,binding_snapshot_id,cause_kind,cause_id) \
         VALUES ($1,$2,$3,'embeddings',$4,'embedding_job',$5)",
    )
    .bind(fixture.evidence_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .bind(fixture.snapshot_id)
    .bind(fixture.job_id.as_uuid())
    .execute(&mut *evidence)
    .await
    .unwrap();
    let kinds = [
        "external_effect",
        "binding_snapshot",
        "connection_revision",
        "connection_qualification_revision",
        "model_revision",
        "model_qualification_revision",
        "request_shape_revision",
        "limits_revision",
    ];
    let references = [
        fixture.external_effect_id,
        fixture.snapshot_id,
        fixture.connection_revision_id,
        fixture.connection_qualification_id,
        fixture.model_revision_id,
        fixture.model_qualification_id,
        shape_id,
        limits_id,
    ];
    for (ordinal, (kind, reference)) in kinds.iter().zip(references).enumerate() {
        let version =
            matches!(*kind, "request_shape_revision" | "limits_revision").then_some(1_i64);
        sqlx::query(
            "INSERT INTO model_request_evidence_nodes\
             (id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id,\
              reference_version) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(Uuid::now_v7())
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.evidence_id)
        .bind(ordinal as i32)
        .bind(kind)
        .bind(reference)
        .bind(version)
        .execute(&mut *evidence)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO model_request_evidence_checks\
         (id,workspace_id,evidence_root_id,status,missing_reference_count) \
         VALUES ($1,$2,$3,'complete',0)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.evidence_id)
    .execute(&mut *evidence)
    .await
    .unwrap();
    evidence.commit().await.unwrap();
}

pub fn embedding_dispatch_request(fixture: &AcceptedJob) -> ProviderDispatchRequest {
    let now = Utc::now();
    ProviderDispatchRequest {
        context: fixture.context.clone(),
        intent: fixture.intent.clone(),
        model_request_evidence_id: ModelRequestEvidenceId::from_uuid(fixture.evidence_id),
        connection_id: ConnectionId::from_uuid(fixture.connection_id),
        connection_revision_id: ConnectionRevisionId::from_uuid(fixture.connection_revision_id),
        cause: ProviderDispatchCause::EmbeddingJob {
            job_id: fixture.job_id,
            snapshot_id: fixture.snapshot_id,
        },
        admission_id: Uuid::now_v7(),
        wait_id: Uuid::now_v7(),
        concurrency_lease_id: Uuid::now_v7(),
        dispatch_ttl_seconds: 60,
        worker_id: WorkerId::new(),
        credential: None,
        audit: AuditEvent::new(
            AuditEventId::new(),
            fixture.context.workspace_id,
            fixture.context.principal_id,
            "provider.dispatch.prepared",
            "external_effect",
            fixture.external_effect_id,
            serde_json::json!({"phase": "pre_dispatch"}),
            now,
        )
        .unwrap(),
        idempotency: Some(IdempotencyRecord {
            idempotency_key: format!("embedding-dispatch:{}", fixture.external_effect_id),
            workspace_id: fixture.context.workspace_id,
            request_hash: "safe-embedding-dispatch-tuple-v1".to_owned(),
            response_payload: None,
            status: "completed".to_owned(),
            created_at: now,
            expires_at: now + chrono::Duration::hours(1),
        }),
        outbox: vec![OutboxMessage::new(
            fixture.context.workspace_id,
            "provider.dispatch.prepared",
            serde_json::json!({"effect_id": fixture.external_effect_id}),
            now,
        )],
    }
}

/// Reconstructs the one embedding request this suite dispatches.
///
/// It is a fixed value rather than a real reconstruction because the property
/// under test is the transaction, not the request: what must hold is that every
/// leg of one dispatch commits or rolls back together.
pub struct FixedEmbeddingEvidence;

#[async_trait::async_trait]
impl ModelRequestEvidenceRepository for FixedEmbeddingEvidence {
    async fn create_in(
        &self,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        creation: &vestrace_application::CreateModelRequestEvidence,
    ) -> Result<ModelRequestEvidenceId, ApplicationError> {
        Ok(ModelRequestEvidenceId::from_uuid(creation.root_id()))
    }

    async fn reconstruct_current_in(
        &self,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _workspace: WorkspaceId,
        _evidence: ModelRequestEvidenceId,
    ) -> Result<vestrace_application::ModelRequestReconstruction, ApplicationError> {
        Ok(vestrace_application::ModelRequestReconstruction::Complete(
            vestrace_application::EffectiveModelRequest::embeddings(
                "text-embedding-nomic-embed-text-v1.5",
                vec!["a governed embedding input".to_owned()],
                vestrace_application::EffectiveRequestLimits::new(8, 1, 2048).unwrap(),
            )
            .unwrap(),
        ))
    }
}

pub struct AllowEmbeddingPolicy;

#[async_trait::async_trait]
impl ProviderDispatchPolicyEvaluator for AllowEmbeddingPolicy {
    async fn evaluate(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
        cause: &vestrace_application::ProviderDispatchCause,
        _target: &vestrace_application::ProviderDispatchTarget,
        _request: &vestrace_application::EffectiveModelRequest,
    ) -> Result<vestrace_application::ProviderDispatchPolicyEvaluation, ApplicationError> {
        // The cause reaching policy evaluation must be the embedding one. A
        // Run-step cause here would mean the dispatch had been composed for
        // another owner, and this double would otherwise allow it silently.
        assert!(
            matches!(
                cause,
                vestrace_application::ProviderDispatchCause::EmbeddingJob { .. }
            ),
            "policy evaluation received {cause:?}"
        );
        let request = AuthorizationRequest::new(
            intent.required_capability(),
            intent.operation(),
            intent.target(),
            intent.risk(),
        );
        Ok(vestrace_application::ProviderDispatchPolicyEvaluation {
            authorization: PolicyDecision {
                id: PolicyDecisionId::new(),
                policy_id: None,
                policy_version: "embedding-dispatch-v1".into(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                capability: intent.required_capability(),
                operation: intent.operation().into(),
                resource_scope: intent.target().into(),
                result: PolicyDecisionResult::Allow,
                reason: PolicyDecisionReason::ConfiguredAllowance,
                input_state: PolicyInputState::from_request(
                    context.workspace_id,
                    context.principal_id,
                    &request,
                ),
                matched_grant_id: None,
                decided_at: Utc::now(),
            },
            // An embedding job's disclosure decision belongs to the embedding
            // data policy gate, not to the model data policy record, which names
            // a run and a step.
            model_data_policy: None,
        })
    }
}

pub fn dispatching_repository(
    runtime: &PgPool,
    fault: Option<ProviderDispatchFaultPoint>,
) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    let permit = Arc::new(PgInstallationMutationPermit::new(store.clone()));
    let evidence = Arc::new(FixedEmbeddingEvidence);
    let effects = Arc::new(PgExternalEffectRepository::new(store.clone()));
    let credential = Arc::new(UnusedCredentialLeases);
    let data_policy = Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone()));
    let governed = Arc::new(PgGovernedMutationRepository::new(store));
    let policy = Arc::new(AllowEmbeddingPolicy);
    match fault {
        Some(point) => PgProviderDispatchRepository::with_faults(
            permit,
            evidence,
            effects,
            credential,
            data_policy,
            governed,
            policy,
            Arc::new(FailAtDispatchPoint(point)),
        ),
        None => PgProviderDispatchRepository::new(
            permit,
            evidence,
            effects,
            credential,
            data_policy,
            governed,
            policy,
        ),
    }
}

/// Fails once at one named boundary, so the transaction is torn at exactly the
/// leg under test and nowhere else.
pub struct FailAtDispatchPoint(ProviderDispatchFaultPoint);

impl ProviderDispatchFaultInjector for FailAtDispatchPoint {
    fn check(&self, point: ProviderDispatchFaultPoint) -> Result<(), ApplicationError> {
        if point == self.0 {
            return Err(ApplicationError::Internal(format!(
                "injected embedding dispatch fault at {point:?}"
            )));
        }
        Ok(())
    }
}

pub fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

/// Shared guarded ResultPrepared fixture used by preparation and finalization probes.
pub(crate) mod result_preparation_fixture {
    use crate::common;
    use chrono::{DateTime, Duration, Utc};
    use sqlx::{PgPool, migrate::Migrator};
    use std::{fs, path::Path};
    use uuid::Uuid;
    use vestrace_application::{
        AcceptDeliveryOutputs, AcceptEmbeddingJob, DeliveryOutputIdentity,
        EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
        EmbeddingDataPolicyMode, EmbeddingOutputKeyProgress, EmbeddingOutputKeyRepository,
        EmbeddingOutputKeyService, IdempotencyRecord, OutboxMessage,
    };
    use vestrace_domain::{
        AuditEvent, ContentMaterialId, DataDestination, EmbeddingSpaceId, IntentNonce,
        MaterialKeyCreationIntentId, MaterialKeyId, ModelRequestEvidenceId, Sensitivity,
        embedding::EmbeddingJobKind,
        id::{AuditEventId, OutboxId},
        trust::{KeyPurpose, KeyReference, SecretResolutionRequest},
    };
    use vestrace_infrastructure::crypto::{HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER};
    use vestrace_infrastructure::postgres::{
        PgEmbeddingDataPolicyDecisionRepository, PgEmbeddingOutputKeyRepository, PgStore,
    };

    pub(crate) static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
    const PROVISIONER: &str = include_str!("../../../../docker/postgres/init-runtime-role.sh");
    pub(crate) fn provisioner_sql_from(marker: &str) -> &'static str {
        let start = PROVISIONER
            .find(marker)
            .unwrap_or_else(|| panic!("missing provisioner marker {marker}"));
        // The provisioner now has separate P05 psql heredocs after the
        // runtime bridge. The bridge ends at its first terminator; selecting
        // the final one would pass later shell text to PostgreSQL.
        PROVISIONER[start..]
            .split_once("\nSQL\n")
            .map(|(sql, _)| sql)
            .expect("the provisioner must contain the SQL heredoc terminator")
    }

    pub(crate) async fn install_extensions_from_real_provisioner(pool: &PgPool) {
        let statements = PROVISIONER
            .lines()
            .filter(|line| line.starts_with("CREATE EXTENSION IF NOT EXISTS "))
            .collect::<Vec<_>>()
            .join("\n");
        sqlx::raw_sql(&statements).execute(pool).await.unwrap();
    }

    pub(crate) async fn hand_database_to_runtime(pool: &PgPool) {
        sqlx::query("ALTER SCHEMA public OWNER TO vestrace")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
        "DO $$ BEGIN EXECUTE format('ALTER DATABASE %I OWNER TO vestrace', current_database()); END $$",
    )
    .execute(pool)
    .await
    .unwrap();
    }

    pub(crate) const RESULT_MODEL: &str = "text-embedding-nomic-embed-text-v1.5";
    pub(crate) const BOOTSTRAP_KEY_ID: &str = "result-preparation-output-key-bootstrap";
    pub(crate) const BOOTSTRAP_SCOPE: &str = "result-preparation-output-key-bootstrap";
    pub(crate) const BOOTSTRAP_ALGORITHM: &str = "aes-256-gcm-v1";

    pub(crate) struct ResultFixture {
        pub(crate) vault: OutputVaultFixture,
        pub(crate) runtime: PgPool,
        pub(crate) accepted: common::AcceptedJob,
        pub(crate) sources: Vec<ContentMaterialId>,
        pub(crate) outputs: Vec<DeliveryOutputIdentity>,
        pub(crate) policy_cause: Uuid,
    }

    #[derive(Clone, Copy, Debug)]
    pub(crate) enum DeliveryPolicyCase {
        ExactAllowed,
        WrongCause,
        WrongAttempt,
        WrongInputCount,
        Denied,
    }

    #[derive(Clone, Debug)]
    pub(crate) struct CommitAttempt {
        pub(crate) job_id: Uuid,
        pub(crate) effect_id: Uuid,
        pub(crate) expected_version_delta: i64,
        pub(crate) response_model: String,
        pub(crate) output_count: usize,
        pub(crate) dimensions: Vec<i32>,
    }

    pub(crate) struct OutputVaultFixture {
        bootstrap: tempfile::TempDir,
        vault: tempfile::TempDir,
    }

    impl OutputVaultFixture {
        pub(crate) fn new() -> Self {
            let bootstrap = tempfile::TempDir::new().unwrap();
            write_bootstrap(bootstrap.path());
            Self {
                bootstrap,
                vault: tempfile::TempDir::new().unwrap(),
            }
        }

        pub(crate) fn vault(
            &self,
            workspace: vestrace_domain::WorkspaceId,
        ) -> HostMaterialKeyVault {
            HostMaterialKeyVault::new(
                self.vault.path(),
                self.bootstrap.path(),
                KeyReference::new(
                    MOUNTED_SECRET_STORE_PROVIDER,
                    BOOTSTRAP_KEY_ID,
                    "v1",
                    KeyPurpose::Storage,
                    BOOTSTRAP_SCOPE,
                    BOOTSTRAP_ALGORITHM,
                )
                .unwrap(),
                SecretResolutionRequest::new(
                    workspace,
                    BOOTSTRAP_SCOPE,
                    "test://result-preparation-output-key",
                ),
            )
            .unwrap()
        }
    }

    pub(crate) fn write_bootstrap(root: &Path) {
        let key = root.join(BOOTSTRAP_KEY_ID);
        let version = key.join("v1");
        fs::create_dir_all(&version).unwrap();
        fs::write(key.join("scope"), BOOTSTRAP_SCOPE).unwrap();
        fs::write(key.join("purpose"), "storage").unwrap();
        fs::write(key.join("algorithm"), BOOTSTRAP_ALGORITHM).unwrap();
        fs::write(version.join("state"), "active").unwrap();
        fs::write(version.join("private.pkcs8"), [0x5A; 32]).unwrap();
    }

    pub(crate) async fn scoped(
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        workspace: Uuid,
    ) {
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace.to_string())
            .fetch_one(&mut **transaction)
            .await
            .unwrap();
    }

    pub(crate) async fn live_source(
        runtime: &PgPool,
        accepted: &common::AcceptedJob,
    ) -> ContentMaterialId {
        let material_id = ContentMaterialId::new();
        let intent_id = MaterialKeyCreationIntentId::new();
        let key_id = MaterialKeyId::new();
        let mut framed_ciphertext = vec![0x51_u8; 4096];
        framed_ciphertext[..5].copy_from_slice(b"VMRF\x01");
        let mut transaction = runtime.begin().await.unwrap();
        scoped(&mut transaction, accepted.context.workspace_id.as_uuid()).await;
        sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'model_request_input',$6,0)")
        .bind(intent_id.as_uuid()).bind(accepted.context.workspace_id.as_uuid())
        .bind(material_id.as_uuid()).bind(key_id.as_uuid()).bind(Uuid::now_v7())
        .bind(accepted.job_id.as_uuid()).execute(&mut *transaction).await.unwrap();
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
            .bind(intent_id.as_uuid())
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
            .bind(intent_id.as_uuid())
            .bind(Uuid::now_v7())
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_prepare_content_material($1,$2,$3,4096)")
            .bind(intent_id.as_uuid())
            .bind(Uuid::now_v7())
            .bind(framed_ciphertext)
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
            .bind(intent_id.as_uuid())
            .bind(Uuid::now_v7())
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
            .bind(intent_id.as_uuid())
            .execute(&mut *transaction)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        material_id
    }

    pub(crate) async fn attach_source_to_evidence(
        owner: &PgPool,
        accepted: &common::AcceptedJob,
        source: ContentMaterialId,
        ordinal: i64,
    ) {
        let mut transaction = owner.begin().await.unwrap();
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *transaction)
            .await
            .unwrap();
        scoped(&mut transaction, accepted.context.workspace_id.as_uuid()).await;
        sqlx::query("INSERT INTO model_request_evidence_nodes(id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id) VALUES($1,$2,$3,$4,'governed_input_material',$5)")
        .bind(Uuid::now_v7()).bind(accepted.context.workspace_id.as_uuid()).bind(accepted.evidence_id)
        .bind(ordinal).bind(source.as_uuid()).execute(&mut *transaction).await.unwrap();
        transaction.commit().await.unwrap();
    }

    pub(crate) fn outputs() -> Vec<DeliveryOutputIdentity> {
        (0..2)
            .map(|output_ordinal| DeliveryOutputIdentity {
                output_ordinal,
                intent_id: MaterialKeyCreationIntentId::new(),
                material_id: ContentMaterialId::new(),
                key_id: MaterialKeyId::new(),
                nonce: IntentNonce::new(),
            })
            .collect()
    }

    pub(crate) fn acceptance_command(
        accepted: &common::AcceptedJob,
        receipt_id: Uuid,
        output_set: Vec<DeliveryOutputIdentity>,
    ) -> AcceptDeliveryOutputs {
        let at = DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap();
        AcceptDeliveryOutputs {
            receipt_id,
            idempotency_key: format!("result-preparation-{receipt_id}"),
            acceptance: AcceptEmbeddingJob {
                job_id: accepted.job_id,
                space_registration_id: EmbeddingSpaceId::from_uuid(accepted.space_registration_id),
                kind: EmbeddingJobKind::Delivery,
                model_binding_snapshot_id: accepted.snapshot_id,
                intent: accepted.intent.clone(),
                model_request_evidence_id: ModelRequestEvidenceId::from_uuid(accepted.evidence_id),
                retries_unknown_embedding_job_id: None,
                expected_predecessor_version: None,
                idempotency: Some(IdempotencyRecord {
                    idempotency_key: format!("result-preparation-{receipt_id}"),
                    workspace_id: accepted.context.workspace_id,
                    request_hash: format!("result-preparation:{receipt_id}"),
                    response_payload: None,
                    status: "completed".into(),
                    created_at: at,
                    expires_at: at + Duration::hours(24),
                }),
                outbox: vec![OutboxMessage {
                    id: OutboxId::from_uuid(receipt_id),
                    workspace_id: accepted.context.workspace_id,
                    topic: "embedding.job.delivery_accepted".into(),
                    payload: serde_json::json!({"receipt_id": receipt_id}),
                    created_at: at,
                    attempts: 0,
                }],
                audit: AuditEvent::new(
                    AuditEventId::from_uuid(receipt_id),
                    accepted.context.workspace_id,
                    accepted.context.principal_id,
                    "embedding.job.delivery_accepted",
                    "embedding_job",
                    accepted.job_id.as_uuid(),
                    serde_json::json!({"receipt_id": receipt_id}),
                    at,
                )
                .unwrap(),
            },
            outputs: output_set,
        }
    }

    pub(crate) async fn reconcile_output_receipts(
        runtime: &PgPool,
        accepted: &common::AcceptedJob,
        output_set: &[DeliveryOutputIdentity],
        vault_fixture: &OutputVaultFixture,
    ) {
        let service = EmbeddingOutputKeyService::new(
            std::sync::Arc::new(PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(
                runtime.clone(),
            ))),
            std::sync::Arc::new(vault_fixture.vault(accepted.context.workspace_id)),
        );
        let mut reconciled = 0usize;
        let mut aggregate_prepared = false;
        for _ in 0..=output_set.len() {
            match service.reconcile_one(&accepted.context).await.unwrap() {
                Some(EmbeddingOutputKeyProgress::WaitingForResultKeys) => reconciled += 1,
                Some(EmbeddingOutputKeyProgress::Prepared { .. }) => {
                    reconciled += 1;
                    aggregate_prepared = true;
                }
                None => break,
                other => panic!("output-key fixture did not prepare its exact output: {other:?}"),
            }
        }
        assert_eq!(reconciled, output_set.len());
        assert!(
            aggregate_prepared,
            "the exact output aggregate was not prepared"
        );
    }

    pub(crate) async fn dispatch_through_embedding_fence(
        runtime: &PgPool,
        accepted: &common::AcceptedJob,
    ) {
        let mut transaction = runtime.begin().await.unwrap();
        scoped(&mut transaction, accepted.context.workspace_id.as_uuid()).await;
        sqlx::query("SELECT * FROM vestrace_try_admit_provider_dispatch($1,$2,$3,$4,$5,$6,$7,$8,'embedding_job',NULL,NULL,$9,NULL,NULL,NULL,60)")
        .bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(Uuid::now_v7())
        .bind(accepted.context.workspace_id.as_uuid()).bind(accepted.connection_id)
        .bind(accepted.connection_revision_id).bind(accepted.external_effect_id).bind(accepted.evidence_id)
        .bind(accepted.snapshot_id).execute(&mut *transaction).await.unwrap();
        let authorization_id = Uuid::now_v7();
        sqlx::query("INSERT INTO external_effect_authorizations(id,effect_id,workspace_id,policy_id,policy_version,subject_id,capability,operation,resource_scope,result,reason,input_state,matched_grant_id,decided_at,payload) SELECT $1,id,workspace_id,NULL,'result-preparation-v1',$2,'export.read','produce a governed embedding','http://127.0.0.1:1234/v1/embeddings','allow','configured_allowance','{}'::jsonb,NULL,NOW(),'{}'::jsonb FROM external_effect_intents WHERE id=$3 AND workspace_id=$4")
        .bind(authorization_id).bind(accepted.context.principal_id.as_uuid()).bind(accepted.external_effect_id)
        .bind(accepted.context.workspace_id.as_uuid()).execute(&mut *transaction).await.unwrap();
        sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'authorized','authorization_recorded',$3,NOW())")
        .bind(accepted.external_effect_id).bind(accepted.context.workspace_id.as_uuid()).bind(authorization_id.to_string())
        .execute(&mut *transaction).await.unwrap();
        sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),'result-preparation-test',NOW()+INTERVAL '60 seconds')")
        .bind(accepted.external_effect_id).bind(accepted.context.workspace_id.as_uuid()).bind(accepted.external_effect_id.to_string())
        .execute(&mut *transaction).await.unwrap();
        transaction.commit().await.unwrap();
    }

    pub(crate) async fn record_delivery_policy(
        runtime: &PgPool,
        cause: Uuid,
        policy: DeliveryPolicyCase,
    ) {
        let causal_reference_id = if matches!(policy, DeliveryPolicyCase::WrongCause) {
            Uuid::now_v7()
        } else {
            cause
        };
        let delivery_attempt = if matches!(policy, DeliveryPolicyCase::WrongAttempt) {
            Some(2)
        } else {
            Some(1)
        };
        let input_count = if matches!(policy, DeliveryPolicyCase::WrongInputCount) {
            1
        } else {
            2
        };
        let allowed = !matches!(policy, DeliveryPolicyCase::Denied);
        PgEmbeddingDataPolicyDecisionRepository::new(PgStore::from_pool(runtime.clone()))
            .record(&EmbeddingDataPolicyDecisionRecord {
                id: Uuid::now_v7(),
                purpose: vestrace_application::EmbeddingPurpose::Delivery,
                causal_reference_id,
                delivery_attempt,
                batch_ordinal: None,
                destination: DataDestination::LocalModel,
                classification: Sensitivity::Confidential,
                classification_labels: vec!["alpha".into(), "beta".into()],
                unclassified_count: 1,
                input_count,
                classification_allowed: allowed,
                destination_allowed: true,
                allowed,
                reason: "test allowed delivery".into(),
                policy_version: "result-preparation-v1".into(),
                mode: EmbeddingDataPolicyMode::Enforce,
                decided_at: Utc::now(),
            })
            .await
            .unwrap();
    }

    pub(crate) async fn result_fixture_with_policy_and_auth(
        pool: &PgPool,
        policy: DeliveryPolicyCase,
        credential_backed: bool,
    ) -> ResultFixture {
        let runtime = common::runtime_pool(pool).await;
        let accepted = if credential_backed {
            common::prepare_delivery_embedding_job_with_pinned_credential(pool, &runtime).await
        } else {
            common::prepare_delivery_embedding_job(pool, &runtime).await
        };
        result_fixture_for_accepted(pool, runtime, accepted, policy, true).await
    }

    pub(crate) async fn result_fixture_for_accepted(
        pool: &PgPool,
        runtime: PgPool,
        accepted: common::AcceptedJob,
        policy: DeliveryPolicyCase,
        initialize_policy: bool,
    ) -> ResultFixture {
        let sources = vec![
            live_source(&runtime, &accepted).await,
            live_source(&runtime, &accepted).await,
            live_source(&runtime, &accepted).await,
        ];
        common::make_dispatchable_with_policy(pool, &runtime, &accepted, initialize_policy).await;
        for (offset, source) in sources.iter().copied().enumerate() {
            attach_source_to_evidence(pool, &accepted, source, 8 + offset as i64).await;
        }
        let output_set = outputs();
        let acceptance_receipt = Uuid::now_v7();
        PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()))
            .accept_delivery_outputs(
                &accepted.context,
                acceptance_command(&accepted, acceptance_receipt, output_set.clone()),
            )
            .await
            .unwrap();
        let vault = OutputVaultFixture::new();
        reconcile_output_receipts(&runtime, &accepted, &output_set, &vault).await;
        record_delivery_policy(&runtime, acceptance_receipt, policy).await;
        dispatch_through_embedding_fence(&runtime, &accepted).await;
        ResultFixture {
            vault,
            runtime,
            accepted,
            sources,
            outputs: output_set,
            policy_cause: acceptance_receipt,
        }
    }

    pub(crate) async fn result_fixture_with_policy(
        pool: &PgPool,
        policy: DeliveryPolicyCase,
    ) -> ResultFixture {
        result_fixture_with_policy_and_auth(pool, policy, false).await
    }

    pub(crate) async fn result_fixture(pool: &PgPool) -> ResultFixture {
        result_fixture_with_policy(pool, DeliveryPolicyCase::ExactAllowed).await
    }

    pub(crate) async fn result_fixture_with_pinned_credential(pool: &PgPool) -> ResultFixture {
        result_fixture_with_policy_and_auth(pool, DeliveryPolicyCase::ExactAllowed, true).await
    }

    pub(crate) async fn provision_result_behavior_database(pool: &PgPool) {
        // This fixture validates P04 result-finalization behaviour. Its
        // ordinary test database must stop before the P05 safety suffix,
        // whose installer requires the separate three-phase deployment route.
        provision_result_behavior_database_through(pool, 208).await;
    }

    pub(crate) async fn provision_result_behavior_database_through(
        pool: &PgPool,
        last_version: i64,
    ) {
        install_extensions_from_real_provisioner(pool).await;
        hand_database_to_runtime(pool).await;
        sqlx::raw_sql(provisioner_sql_from(
            "-- P02 migrations run as the runtime role",
        ))
        .execute(pool)
        .await
        .expect("the real provisioner must install the runtime migration bridge");
        let runtime = common::runtime_pool(pool).await;
        let migrator = Migrator {
            migrations: std::borrow::Cow::Owned(
                MIGRATOR
                    .iter()
                    .filter(|m| m.version <= last_version)
                    .cloned()
                    .collect(),
            ),
            ..Migrator::DEFAULT
        };
        migrator
            .run(&runtime)
            .await
            .expect("the restricted runtime must apply the result-preparation migration");
        runtime.close().await;
    }

    pub(crate) fn exact_attempt(fixture: &ResultFixture) -> CommitAttempt {
        CommitAttempt {
            job_id: fixture.accepted.job_id.as_uuid(),
            effect_id: fixture.accepted.external_effect_id,
            expected_version_delta: 0,
            response_model: RESULT_MODEL.into(),
            output_count: fixture.outputs.len(),
            dimensions: vec![768; fixture.outputs.len()],
        }
    }

    pub(crate) async fn try_commit_result(
        fixture: &ResultFixture,
        preparation: Uuid,
        receipt: Uuid,
        attempt: &CommitAttempt,
    ) -> Result<Uuid, sqlx::Error> {
        let mut transaction = fixture.runtime.begin().await.unwrap();
        scoped(
            &mut transaction,
            fixture.accepted.context.workspace_id.as_uuid(),
        )
        .await;
        let mut framed = vec![0x51_u8; 4096];
        framed[..5].copy_from_slice(b"VMRF\x01");
        let result = async {
        let (lease, dispatch): (Uuid, Uuid) = sqlx::query_as(
            "SELECT lease.id,lifecycle.id FROM provider_concurrency_leases lease \
             JOIN external_effect_lifecycle_transitions lifecycle ON lifecycle.workspace_id=lease.workspace_id AND lifecycle.effect_id=lease.external_effect_id \
             WHERE lease.workspace_id=$1 AND lease.external_effect_id=$2 AND lifecycle.status='dispatching' \
             ORDER BY lifecycle.ordinal DESC LIMIT 1",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(fixture.accepted.external_effect_id)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_lock_embedding_result_completion_authority($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(attempt.job_id)
        .bind(attempt.effect_id)
        .bind(fixture.accepted.connection_id)
        .bind(fixture.accepted.connection_revision_id)
        .bind(dispatch)
        .bind(lease)
        .fetch_one(&mut *transaction)
        .await?;
        let version: i64 = sqlx::query_scalar(
            "SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(fixture.accepted.job_id.as_uuid())
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query_scalar(
            "SELECT vestrace_commit_embedding_result_preparation($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(fixture.accepted.context.workspace_id.as_uuid())
        .bind(attempt.job_id)
        .bind(attempt.effect_id)
        .bind(preparation)
        .bind(receipt)
        .bind(version + attempt.expected_version_delta)
        .bind(&attempt.response_model)
        .bind((0..attempt.output_count).map(|_| Uuid::now_v7()).collect::<Vec<_>>())
        .bind((0..attempt.output_count).map(|_| framed.clone()).collect::<Vec<_>>())
        .bind(&attempt.dimensions)
        .fetch_one(&mut *transaction)
        .await
    }
    .await;
        match result {
            Ok(prepared) => {
                transaction.commit().await?;
                Ok(prepared)
            }
            Err(error) => {
                transaction.rollback().await.unwrap();
                Err(error)
            }
        }
    }

    pub(crate) async fn commit_result(
        fixture: &ResultFixture,
        preparation: Uuid,
        receipt: Uuid,
    ) -> Uuid {
        try_commit_result(fixture, preparation, receipt, &exact_attempt(fixture))
            .await
            .unwrap()
    }
}

/// Canonical memory corpus built through the production materializer and delivery executor.
pub(crate) mod canonical_memory_fixture {
    use crate::common;
    use common::result_preparation_fixture::{OutputVaultFixture, scoped};
    use sqlx::PgPool;
    use uuid::Uuid;
    use vestrace_domain::{CorpusGenerationId, MemoryId, embedding::EmbeddingSpaceKey};

    pub(crate) struct Corpus {
        pub(crate) runtime: PgPool,
        pub(crate) accepted: common::AcceptedJob,
        pub(crate) vault: OutputVaultFixture,
        pub(crate) generation: CorpusGenerationId,
        pub(crate) space_key: EmbeddingSpaceKey,
        initialized_policy: bool,
    }

    pub(crate) async fn new(pool: &PgPool, name: &str) -> Corpus {
        common::result_preparation_fixture::provision_result_behavior_database(pool).await;
        new_in_database(pool, name).await
    }

    pub(crate) async fn new_in_database(pool: &PgPool, name: &str) -> Corpus {
        let runtime = common::runtime_pool(pool).await;
        let mut accepted =
            common::accept_embedding_job_inner(pool, &runtime, false, false, false, true).await;
        let (registration, qualification) = register_space(pool, &runtime, &accepted, name).await;
        accepted.space_registration_id = registration;
        accepted = canonical_snapshot(pool, &runtime, &accepted, qualification).await;
        let space_key = space_key(&runtime, &accepted, name).await;
        seed_qualification_heads(pool, &accepted).await;
        let mut initial = runtime.begin().await.unwrap();
        scoped(&mut initial, accepted.context.workspace_id.as_uuid()).await;
        sqlx::query("SELECT vestrace_set_initial_embedding_active_space($1,$2,1,$3)")
            .bind(accepted.context.workspace_id.as_uuid())
            .bind(accepted.model_revision_id)
            .bind(accepted.space_registration_id)
            .execute(&mut *initial)
            .await
            .unwrap();
        initial.commit().await.unwrap();
        Corpus {
            runtime,
            accepted,
            vault: OutputVaultFixture::new(),
            generation: CorpusGenerationId::new(),
            space_key,
            initialized_policy: false,
        }
    }

    /// A second registration reuses the exact qualified wire model and shape.
    /// The different name gives it its own identity and independent corpus.
    pub(crate) async fn additional_space(pool: &PgPool, base: &Corpus, name: &str) -> Corpus {
        let mut accepted =
            common::prepare_additional_delivery_embedding_job(&base.runtime, &base.accepted).await;
        let identity = base.space_key.canonical_identity().unwrap();
        let mut tx = base.runtime.begin().await.unwrap();
        scoped(&mut tx, accepted.context.workspace_id.as_uuid()).await;
        accepted.space_registration_id = sqlx::query_scalar(
            "SELECT vestrace_register_canonical_embedding_space($1,$2,$3,$4,$5,$6,$7,'float',768)",
        )
        .bind(Uuid::now_v7())
        .bind(accepted.context.workspace_id.as_uuid())
        .bind(name)
        .bind(identity.model_revision_id.as_uuid())
        .bind(identity.model_qualification_revision_id.as_uuid())
        .bind(identity.request_shape_revision_id)
        .bind(&identity.returned_model)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *tx)
            .await
            .unwrap();
        scoped(&mut tx, accepted.context.workspace_id.as_uuid()).await;
        sqlx::query("INSERT INTO embedding_space_corpus_states(workspace_id,space_registration_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(accepted.context.workspace_id.as_uuid()).bind(accepted.space_registration_id).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO embedding_index_generation_guards(workspace_id,space_registration_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(accepted.context.workspace_id.as_uuid()).bind(accepted.space_registration_id).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();

        let space_key = space_key(&base.runtime, &accepted, name).await;
        Corpus {
            runtime: base.runtime.clone(),
            accepted,
            vault: OutputVaultFixture::new(),
            generation: CorpusGenerationId::new(),
            space_key,
            initialized_policy: true,
        }
    }

    // This is fixture qualification evidence, not a live qualification-publisher proof.
    async fn seed_qualification_heads(pool: &PgPool, accepted: &common::AcceptedJob) {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *tx)
            .await
            .unwrap();
        scoped(&mut tx, accepted.context.workspace_id.as_uuid()).await;
        sqlx::query("INSERT INTO connection_qualification_heads(workspace_id,connection_revision_id,current_qualification_revision_id,version) VALUES($1,$2,$3,1)")
            .bind(accepted.context.workspace_id.as_uuid()).bind(accepted.connection_revision_id).bind(accepted.connection_qualification_id).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO model_qualification_heads(workspace_id,model_revision_id,current_qualification_revision_id,version) VALUES($1,$2,$3,1)")
            .bind(accepted.context.workspace_id.as_uuid()).bind(accepted.model_revision_id).bind(accepted.model_qualification_id).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }

    pub(crate) async fn space_key(
        runtime: &PgPool,
        accepted: &common::AcceptedJob,
        name: &str,
    ) -> EmbeddingSpaceKey {
        let mut tx = runtime.begin().await.unwrap();
        scoped(&mut tx, accepted.context.workspace_id.as_uuid()).await;
        let row: (Uuid,Uuid,String,Uuid,String,String,i32) = sqlx::query_as("SELECT model_revision_id,model_qualification_revision_id,adapter_profile_revision,request_shape_revision_id,returned_model,encoding_format,dimensions FROM embedding_space_registrations WHERE workspace_id=$1 AND id=$2")
            .bind(accepted.context.workspace_id.as_uuid()).bind(accepted.space_registration_id).fetch_one(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        EmbeddingSpaceKey::canonical(
            accepted.context.workspace_id,
            name,
            vestrace_domain::embedding::CanonicalEmbeddingSpace {
                model_revision_id: vestrace_domain::ModelRevisionId::from_uuid(row.0),
                model_qualification_revision_id:
                    vestrace_domain::ModelQualificationRevisionId::from_uuid(row.1),
                adapter_profile_revision: row.2,
                request_shape_revision_id: row.3,
                returned_model: row.4,
                encoding_format: row.5,
                dimensions: row.6 as u32,
            },
        )
        .unwrap()
    }

    pub(crate) async fn canonical_snapshot(
        pool: &PgPool,
        runtime: &PgPool,
        base: &common::AcceptedJob,
        qualification: Uuid,
    ) -> common::AcceptedJob {
        let snapshot = Uuid::now_v7();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *tx)
            .await
            .unwrap();
        scoped(&mut tx, base.context.workspace_id.as_uuid()).await;
        sqlx::query("INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,branch,no_auth_binding_revision_id) SELECT $1,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,$2,branch,no_auth_binding_revision_id FROM model_binding_snapshots WHERE workspace_id=$3 AND id=$4")
            .bind(snapshot).bind(qualification).bind(base.context.workspace_id.as_uuid()).bind(base.snapshot_id).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO model_binding_snapshot_scopes(workspace_id,snapshot_id,scope,transition_plan_id) VALUES($1,$2,'ordinary',NULL)")
            .bind(base.context.workspace_id.as_uuid()).bind(snapshot).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let mut accepted = common::prepare_additional_delivery_embedding_job(runtime, base).await;
        accepted.snapshot_id = snapshot;
        accepted.model_qualification_id = qualification;
        // Rebuild the effect against the new immutable snapshot before any evidence is published.
        common::prepare_additional_delivery_embedding_job(runtime, &accepted).await
    }

    impl Corpus {
        pub(crate) async fn publish_memories(
            &mut self,
            pool: &PgPool,
            memories: &[MemoryId],
        ) -> CorpusGenerationId {
            for memory in memories {
                let revision: Uuid = sqlx::query_scalar(
                    "SELECT active_revision_id FROM memories WHERE workspace_id=$1 AND id=$2",
                )
                .bind(self.accepted.context.workspace_id.as_uuid())
                .bind(memory.as_uuid())
                .fetch_one(pool)
                .await
                .unwrap();
                self.publish_revision(pool, revision).await;
            }
            self.capture().await
        }

        pub(crate) async fn publish_revision(&mut self, pool: &PgPool, revision: Uuid) -> Uuid {
            let materializer =
                vestrace_infrastructure::postgres::PgGovernedContentMaterializer::new(
                    vestrace_infrastructure::postgres::PgStore::from_pool(self.runtime.clone()),
                    std::sync::Arc::new(self.vault.vault(self.accepted.context.workspace_id)),
                    std::sync::Arc::new(
                        vestrace_infrastructure::crypto::ContentMaterialCodec::new(),
                    ),
                );
            let source = materializer
                .materialize_revision(&self.accepted.context, revision)
                .await
                .unwrap()
                .expect("intact revision materializes");
            let job =
                common::prepare_additional_delivery_embedding_job(&self.runtime, &self.accepted)
                    .await;
            publish_projections(
                pool,
                &self.runtime,
                &job,
                source.material_id,
                &self.vault,
                !self.initialized_policy,
            )
            .await;
            self.initialized_policy = true;
            source.material_id
        }

        pub(crate) async fn capture(&mut self) -> CorpusGenerationId {
            let workspace = self.accepted.context.workspace_id.as_uuid();
            let registration = self.accepted.space_registration_id;
            let generation = Uuid::now_v7();
            let mut tx = self.runtime.begin().await.unwrap();
            scoped(&mut tx, workspace).await;
            let version: i64 = sqlx::query_scalar("SELECT guard_version FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$2")
                .bind(workspace).bind(registration).fetch_one(&mut *tx).await.unwrap();
            sqlx::query("SELECT vestrace_capture_embedding_generation($1,$2,$3,$4)")
                .bind(generation)
                .bind(workspace)
                .bind(registration)
                .bind(version)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("SELECT vestrace_publish_embedding_generation($1,$2,$3,$4)")
                .bind(workspace)
                .bind(registration)
                .bind(generation)
                .bind(version)
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            self.generation = CorpusGenerationId::from_uuid(generation);
            self.generation
        }
    }
    pub(crate) async fn register_space(
        pool: &PgPool,
        runtime: &PgPool,
        fixture: &common::AcceptedJob,
        name: &str,
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
            "SELECT vestrace_register_canonical_embedding_space($1,$2,$7,$3,$4,$5,$6,'float',768)",
        )
        .bind(registration)
        .bind(workspace)
        .bind(fixture.model_revision_id)
        .bind(canonical_qualification)
        .bind(shape)
        .bind(&wire_model)
        .bind(name)
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

    async fn publish_projections(
        pool: &PgPool,
        runtime: &PgPool,
        accepted: &common::AcceptedJob,
        memory_source: Uuid,
        vault: &OutputVaultFixture,
        initialize_policy: bool,
    ) {
        use common::result_preparation_fixture::{
            DeliveryPolicyCase, acceptance_command, attach_source_to_evidence, live_source,
            outputs, reconcile_output_receipts, record_delivery_policy,
        };
        use vestrace_application::EmbeddingOutputKeyRepository;

        let sources = [
            vestrace_domain::ContentMaterialId::from_uuid(memory_source),
            live_source(runtime, accepted).await,
        ];
        common::make_dispatchable_with_policy(pool, runtime, accepted, initialize_policy).await;
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
        reconcile_output_receipts(runtime, accepted, &output_set, vault).await;
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
}

/// The production client and worker, with a real HTTP provider observed at its pinned endpoint.
pub(crate) mod canonical_query_fixture {
    use super::{AllowEmbeddingPolicy, UnusedCredentialLeases, canonical_memory_fixture::Corpus};
    use sqlx::PgPool;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use vestrace_application::{
        EffectiveRequestLimits, MaterialErasureService, NormalizedRetrievalRequest,
        embedding::{EmbeddingRetrievalJobClient, EmbeddingRetrievalOutcome, EmbeddingWorkKind},
    };
    use vestrace_infrastructure::postgres::*;

    pub(crate) async fn retrieve(
        _pool: &PgPool,
        corpus: &Corpus,
        request: &NormalizedRetrievalRequest,
    ) -> (EmbeddingRetrievalOutcome, usize) {
        retrieve_with_policy(_pool, corpus, request, true).await
    }

    pub(crate) async fn retrieve_with_policy(
        _pool: &PgPool,
        corpus: &Corpus,
        request: &NormalizedRetrievalRequest,
        allow_unclassified: bool,
    ) -> (EmbeddingRetrievalOutcome, usize) {
        retrieve_configured(&corpus.runtime, corpus, request, allow_unclassified, false).await
    }
    pub(crate) async fn retrieve_with_recording_failure(
        _pool: &PgPool,
        corpus: &Corpus,
        request: &NormalizedRetrievalRequest,
    ) -> (EmbeddingRetrievalOutcome, usize) {
        retrieve_configured(&corpus.runtime, corpus, request, true, true).await
    }
    pub(crate) async fn retrieve_on_runtime(
        runtime: &PgPool,
        corpus: &Corpus,
        request: &NormalizedRetrievalRequest,
    ) -> (EmbeddingRetrievalOutcome, usize) {
        retrieve_configured(runtime, corpus, request, true, false).await
    }
    struct FailedDecisionRepository;
    #[async_trait::async_trait]
    impl vestrace_application::EmbeddingDataPolicyDecisionRepository for FailedDecisionRepository {
        async fn record(
            &self,
            _: &vestrace_application::EmbeddingDataPolicyDecisionRecord,
        ) -> Result<(), vestrace_application::ApplicationError> {
            Err(vestrace_application::ApplicationError::Storage(
                "injected decision write failure".to_owned(),
            ))
        }

        async fn record_in(
            &self,
            _: &mut dyn vestrace_application::UnitOfWork,
            _: &vestrace_application::EmbeddingDataPolicyDecisionRecord,
        ) -> Result<(), vestrace_application::ApplicationError> {
            Err(vestrace_application::ApplicationError::Storage(
                "injected transaction-bound decision write failure".to_owned(),
            ))
        }
    }
    async fn retrieve_configured(
        runtime: &PgPool,
        corpus: &Corpus,
        request: &NormalizedRetrievalRequest,
        allow_unclassified: bool,
        fail_record: bool,
    ) -> (EmbeddingRetrievalOutcome, usize) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:1234")
            .await
            .expect("the fixture's exact pinned endpoint must be available");
        let observed = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let requests = observed.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let body_start;
                let content_length;
                loop {
                    let mut chunk = [0u8; 4096];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                    assert!(bytes.len() < 1_048_576);
                    if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        body_start = end + 4;
                        let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                        assert!(headers.starts_with("POST /v1/embeddings "));
                        content_length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        break;
                    }
                }
                while bytes.len() < body_start + content_length {
                    let mut chunk = [0u8; 4096];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                }
                let request: serde_json::Value =
                    serde_json::from_slice(&bytes[body_start..body_start + content_length])
                        .unwrap();
                requests.lock().unwrap().push(request);
                let body = serde_json::json!({"object":"list","model":"text-embedding-nomic-embed-text-v1.5","data":[{"object":"embedding","index":0,"embedding":vec![1.0;768]}],"usage":{"prompt_tokens":1,"total_tokens":1}}).to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let store = PgStore::from_pool(runtime.clone());
        let vault = Arc::new(corpus.vault.vault(corpus.accepted.context.workspace_id));
        let evidence = Arc::new(PgModelRequestEvidenceRepository::new(vault.clone()));
        let decisions: vestrace_application::SharedEmbeddingDataPolicyDecisionRepository =
            if fail_record {
                Arc::new(FailedDecisionRepository)
            } else {
                Arc::new(PgEmbeddingDataPolicyDecisionRepository::new(store.clone()))
            };
        let gate = Arc::new(vestrace_application::EmbeddingDataPolicyGate::new(
            vestrace_application::EmbeddingDataPolicySettings {
                classification_policy: vestrace_domain::retrieval::ClassificationPolicy::new(
                    Vec::<String>::new(),
                    allow_unclassified,
                )
                .unwrap(),
                classification: vestrace_domain::Sensitivity::Internal,
                policy: vestrace_domain::trust::DataPolicy::new(
                    vestrace_domain::DataPolicyId::new(),
                    "vector-query-policy-v1",
                    vestrace_domain::Sensitivity::Internal,
                    std::collections::BTreeSet::from([
                        vestrace_domain::DataDestination::LocalModel,
                    ]),
                    None,
                )
                .unwrap(),
                mode: vestrace_application::EmbeddingDataPolicyMode::Enforce,
            },
            decisions,
        ));
        let dispatch = Arc::new(
            PgProviderDispatchRepository::new(
                Arc::new(PgInstallationMutationPermit::new(store.clone())),
                evidence.clone(),
                Arc::new(PgExternalEffectRepository::new(store.clone())),
                Arc::new(UnusedCredentialLeases),
                Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
                Arc::new(PgGovernedMutationRepository::new(store.clone())),
                Arc::new(AllowEmbeddingPolicy),
            )
            .with_embedding_policy(gate),
        );
        let worker = EmbeddingWorkerRuntime::new(
            store.clone(),
            vault.clone(),
            dispatch,
            &vestrace_infrastructure::config::EmbeddingWorkerLimits::default(),
            "canonical-query-test",
            vestrace_domain::WorkerId::new(),
        )
        .unwrap();
        let client = PgEmbeddingRetrievalJobClient::new(
            store.clone(),
            Arc::new(PgGovernedContentMaterializer::new(
                store.clone(),
                vault.clone(),
                Arc::new(vestrace_infrastructure::crypto::ContentMaterialCodec::new()),
            )),
            Arc::new(PgGovernedEmbeddingJobFactory::new(
                store.clone(),
                Arc::new(PgEmbeddingJobRepository::new(store.clone())),
                evidence,
                EffectiveRequestLimits::new(8, 1, 2048).unwrap(),
            )),
            Arc::new(MaterialErasureService::new(
                PgMaterialErasureRepository::new(store),
                vault,
            )),
        );
        let context = &corpus.accepted.context;
        let client_future = client.retrieve(
            context,
            request.request_id,
            request,
            chrono::Utc::now()
                + chrono::Duration::seconds(if allow_unclassified && !fail_record {
                    20
                } else {
                    2
                }),
        );
        // Drive the worker independently so the waiting client keeps polling and
        // releases its scoped connections while dispatch records disclosure.
        let worker_context = context.clone();
        let worker_task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                let cycle = worker
                    .run_cycle(&worker_context, EmbeddingWorkKind::Dispatch)
                    .await
                    .unwrap();
                if allow_unclassified && !fail_record {
                    assert!(
                        !cycle.failed,
                        "the production worker must complete its query"
                    );
                } else if cycle.failed {
                    break;
                }
            }
        });
        let outcome = client_future.await.expect("canonical client must answer");
        if worker_task.is_finished() {
            worker_task.await.unwrap();
        } else {
            worker_task.abort();
        }
        server.abort();
        let requests = observed.lock().unwrap();
        for body in requests.iter() {
            assert_eq!(body["input"], serde_json::json!([request.query]));
        }
        (outcome, requests.len())
    }
}
