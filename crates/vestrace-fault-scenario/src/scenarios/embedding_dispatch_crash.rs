//! A self-contained, process-abort probe for governed embedding dispatch.
//!
//! The fixture is intentionally here instead of importing the infrastructure
//! test helper. This is a shipping binary crate: importing another crate's
//! `tests/common` would import its dev-only dependency graph and make this
//! executable depend on a test-only composition root.

use std::io::Write;
use std::str::FromStr;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{ExternalEffectRepository, RequestContext, retrieval::EmbeddingStore};
use vestrace_domain::{
    Capability, EffectPrecondition, EffectReversibility, EmbeddingJobId, ExternalEffectIntent,
    IdempotencyProfile, PrincipalId, RiskCategory, WorkspaceId, now,
};
use vestrace_infrastructure::{PgEmbeddingStore, PgExternalEffectRepository, PgStore};

use crate::ScenarioSettings;

const MARKER: &str = "vestrace-fault-scenario: embedding-dispatch";
const UNPROVABLE: &str = "unproved: no worker composes an embedding-job executor (crates/vestrace-cli/src/commands/worker.rs:119-134 and 423-455)";

pub(super) struct Fixture {
    pub(super) workspace_id: Uuid,
    pub(super) principal_id: Uuid,
    pub(super) job_id: Uuid,
    pub(super) effect_id: Uuid,
    pub(super) snapshot_id: Uuid,
    pub(super) connection_id: Uuid,
    pub(super) connection_revision_id: Uuid,
    pub(super) evidence_id: Uuid,
    /// The registration the job was accepted against, and the one every
    /// generation, index build and activation in these scenarios works over.
    pub(super) space_registration_id: Uuid,
    /// The qualification revision that registration is bound to.
    ///
    /// An activation names it twice -- once as the batch's target
    /// qualification and once as the head's current one -- because the head's
    /// deferred consistency trigger demands the active space and the head agree
    /// on it. A scenario that re-derived it would be re-deriving the thing
    /// under test.
    pub(super) canonical_qualification_id: Uuid,
    pub(super) intent: Option<ExternalEffectIntent>,
}

/// The child owns all setup so every reported identity is recovered from its
/// marker and then read from PostgreSQL by the parent.
pub async fn run_child(settings: &ScenarioSettings) -> ! {
    let owner = connect_owner(settings)
        .await
        .unwrap_or_else(|error| die(error));
    let runtime = connect_runtime(&owner)
        .await
        .unwrap_or_else(|error| die(error));
    let fixture = build_fixture(&owner, &runtime)
        .await
        .unwrap_or_else(|error| die(error));

    if std::env::var_os("VESTRACE_EMBEDDING_CONTROL").is_some() {
        drive(&runtime, &fixture, None)
            .await
            .unwrap_or_else(|error| die(error));
        announce("control", &fixture);
        std::process::exit(0);
    }

    let point = settings.embedding_dispatch_point();
    drive(&runtime, &fixture, Some(point))
        .await
        .unwrap_or_else(|error| die(error));
    unreachable!("a crash point must abort the process")
}

/// Run a passing control and then an aborting child, reading all fields back
/// from durable state or the separately-running loopback listener.
pub async fn run_parent(settings: &ScenarioSettings) -> Result<String, String> {
    let point = settings.embedding_dispatch_point();
    if point == vestrace_domain::external_effects::EffectFaultPoint::AfterOutcomeBeforeRunCommit {
        return Ok(serde_json::json!({
            "scenario": "embedding_dispatch_crash",
            "point": point_name(point),
            "proved": false,
            "reason": UNPROVABLE,
        })
        .to_string());
    }

    let listener = LoopbackCounter::start().await?;
    let control = run_same_child(settings, "control", listener.url()).await?;
    if !control.status.success() {
        return Err(format!(
            "embedding control child failed: {}",
            text(&control.stderr)
        ));
    }
    let control_fixture = marker(&text(&control.stderr), "control")?;
    let owner = connect_owner(settings).await?;
    let control_counts = read_control(&owner, &control_fixture).await?;

    let crashed = run_same_child(settings, "crash", listener.url()).await?;
    if crashed.status.success() {
        return Err("embedding fault child exited successfully instead of aborting".to_owned());
    }
    let survivor_fixture = marker(&text(&crashed.stderr), point_name(point))?;
    let observation = read_survivor(&owner, &survivor_fixture).await?;
    assert_survivor(point, &observation)?;

    // This count is owned by a listener outside the child process. It is read
    // only after the child is gone; it is never a value the child supplied.
    let loopback_requests = listener.count();
    let mut rendered = observation;
    rendered["scenario"] = serde_json::Value::String("embedding_dispatch_crash".to_owned());
    rendered["point"] = serde_json::Value::String(point_name(point).to_owned());
    rendered["proved"] = serde_json::Value::Bool(true);
    rendered["loopback_requests"] = serde_json::Value::from(loopback_requests as u64);
    rendered["control"] = control_counts;
    Ok(rendered.to_string())
}

pub(super) async fn connect_owner(settings: &ScenarioSettings) -> Result<PgPool, String> {
    PgPoolOptions::new()
        .max_connections(4)
        .connect(settings.database_url())
        .await
        .map_err(|error| format!("owner database connection failed: {error}"))
}

pub(super) async fn connect_runtime(owner: &PgPool) -> Result<PgPool, String> {
    let configured = std::env::var("VESTRACE_RUNTIME_DATABASE_URL").map_err(|_| {
        "VESTRACE_RUNTIME_DATABASE_URL is required for the runtime fixture".to_owned()
    })?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(owner)
        .await
        .map_err(|error| format!("fixture database name is unreadable: {error}"))?;
    let runtime_url = replace_database(&configured, &database)?;
    let parsed = PgConnectOptions::from_str(&runtime_url)
        .map_err(|error| format!("runtime database URL is invalid: {error}"))?;
    PgPoolOptions::new()
        .max_connections(4)
        .connect_with(parsed)
        .await
        .map_err(|error| format!("runtime database connection failed: {error}"))
}

fn replace_database(url: &str, database: &str) -> Result<String, String> {
    let (base, query) = url
        .split_once('?')
        .map_or((url, None), |(base, query)| (base, Some(query)));
    let (prefix, _) = base
        .rsplit_once('/')
        .ok_or_else(|| "runtime database URL has no database component".to_owned())?;
    Ok(query.map_or_else(
        || format!("{prefix}/{database}"),
        |query| format!("{prefix}/{database}?{query}"),
    ))
}

/// This is the same fixture shape as infrastructure's test helper, expressed
/// locally with only this crate's normal dependencies. It performs 31 durable
/// setup operations: tenancy, connection/model rows, space registration,
/// guarded revisions, pre-existing effect intent, qualification/snapshot rows,
/// acceptance, admission policy, and complete request evidence.
pub(super) async fn build_fixture(owner: &PgPool, runtime: &PgPool) -> Result<Fixture, String> {
    build_fixture_with_outputs(owner, runtime)
        .await
        .map(|(fixture, _, _)| fixture)
}

/// The same fixture, keeping the prepared outputs and the vault that holds
/// their keys.
///
/// A caller that wants to *execute* this job needs both, and preparing the
/// outputs a second time is not an option: the governed input material always
/// attaches at the same evidence ordinal, so a second preparation collides with
/// the first. The plain `build_fixture` above discards them because the
/// scenarios that crash before dispatch have no use for them.
#[allow(clippy::type_complexity)]
pub(super) async fn build_fixture_with_outputs(
    owner: &PgPool,
    runtime: &PgPool,
) -> Result<
    (
        Fixture,
        std::sync::Arc<vestrace_infrastructure::crypto::HostMaterialKeyVault>,
        Vec<vestrace_application::DeliveryOutputIdentity>,
    ),
    String,
> {
    // Job acceptance must atomically fix its outputs; an already accepted bare
    // job cannot be backfilled with guessed identities under the current gate.
    let fixture = build_result_preparation_fixture(owner, runtime).await?;
    let context = RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::from_uuid(fixture.principal_id),
    );
    let (vault, outputs) = super::embedding_result_preparation_crash::prepare_dispatch_outputs(
        owner, runtime, &fixture, &context,
    )
    .await?;
    assert_baseline(owner, &fixture).await?;
    Ok((fixture, vault, outputs))
}

/// Result preparation receives its delivery job from the guarded output
/// acceptance command.  Pre-creating that job would turn the acceptance into
/// a guessed-output backfill, which 0193 correctly refuses.
pub(super) async fn build_result_preparation_fixture(
    owner: &PgPool,
    runtime: &PgPool,
) -> Result<Fixture, String> {
    build_fixture_with_job(owner, runtime, false).await
}

async fn build_fixture_with_job(
    owner: &PgPool,
    runtime: &PgPool,
    accept_job: bool,
) -> Result<Fixture, String> {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    let context = RequestContext::new(workspace_id, principal_id);
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let guard_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let no_auth_id = Uuid::now_v7();
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
        .map_err(sql)?;
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,$3)")
        .bind(principal_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(format!("embedding-{principal_id}"))
        .execute(owner)
        .await
        .map_err(sql)?;
    sqlx::query(
        "INSERT INTO connectors(id,workspace_id,name,provider_type) VALUES($1,$2,$3,'local')",
    )
    .bind(connector_id)
    .bind(workspace_id.as_uuid())
    .bind(format!("embedding-{connector_id}"))
    .execute(owner)
    .await
    .map_err(sql)?;
    sqlx::query("INSERT INTO connections(id,connector_id,workspace_id,principal_id,name,status) VALUES($1,$2,$3,$4,$5,'active')")
        .bind(connection_id).bind(connector_id).bind(workspace_id.as_uuid()).bind(principal_id.as_uuid())
        .bind(format!("embedding-{connection_id}")).execute(owner).await.map_err(sql)?;
    sqlx::query(
        "INSERT INTO providers(id,workspace_id,name,locality) VALUES($1,$2,'provider','local')",
    )
    .bind(provider_id)
    .bind(workspace_id.as_uuid())
    .execute(owner)
    .await
    .map_err(sql)?;
    sqlx::query("INSERT INTO models(id,provider_id,workspace_id,model_name,context_window,input_cost_per_mtoken,output_cost_per_mtoken) VALUES($1,$2,$3,'embedding-model',4096,0,0)")
        .bind(model_id).bind(provider_id).bind(workspace_id.as_uuid()).execute(owner).await.map_err(sql)?;

    for table in [
        "embedding_spaces",
        "memory_embeddings",
        "memories",
        "memory_revisions",
        "search_documents",
    ] {
        sqlx::query(&format!("ALTER TABLE public.{table} OWNER TO vestrace"))
            .execute(owner)
            .await
            .map_err(sql)?;
    }
    // Called for its effect, not its answer. It creates the legacy space and
    // its registration, which the legacy quarantine assertions read out of the
    // database by name; nothing here needs the identity back now that the job
    // is accepted against the canonical registration.
    PgEmbeddingStore::new(PgStore::from_pool(runtime.clone()))
        .ensure_space(
            &context,
            "nomic-768",
            "text-embedding-nomic-embed-text-v1.5",
            768,
        )
        .await
        .map_err(|error| format!("legacy embedding space setup failed: {error}"))?;

    let mut governed = runtime.begin().await.map_err(sql)?;
    set_workspace(&mut governed, workspace_id.as_uuid()).await?;
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(guard_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .execute(&mut *governed)
        .await
        .map_err(sql)?;
    sqlx::query("SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,'lm_studio_local','http://127.0.0.1:1234/v1','http://127.0.0.1:1234/v1','lm-studio-local/v1','loopback_only','none',NULL,0)")
        .bind(connection_revision_id).bind(workspace_id.as_uuid()).bind(connection_id).bind(guard_id)
        .execute(&mut *governed).await.map_err(sql)?;
    sqlx::query("SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)")
        .bind(no_auth_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .bind(connection_revision_id)
        .execute(&mut *governed)
        .await
        .map_err(sql)?;
    // The revision speaks the model every other part of these scenarios already
    // names. It used to say `embedding-model`, which nothing else did: the
    // legacy space, the stubbed provider response and the result-eligibility
    // plan all name the nomic model, and a canonical registration must carry
    // `returned_model = wire_model_id`. With two different names the plan and
    // the response could never agree.
    sqlx::query("SELECT vestrace_create_model_revision_and_advance_head($1,$2,$3,$4,$5,$6,'text-embedding-nomic-embed-text-v1.5','embedding',NULL,NULL,NULL,NULL,NULL,NULL,0)")
        .bind(model_revision_id).bind(workspace_id.as_uuid()).bind(model_id).bind(connection_id).bind(guard_id).bind(connection_revision_id)
        .execute(&mut *governed).await.map_err(sql)?;
    // The legacy registration `ensure_space` created above is still there and
    // is still what the legacy quarantine assertions read. Its id is no longer
    // looked up here: the job is accepted against the canonical registration
    // below, and a lookup whose result nothing uses is a lookup that will one
    // day be believed to mean something.
    governed.commit().await.map_err(sql)?;

    let intent = ExternalEffectIntent::new(
        "workspace://",
        workspace_id,
        principal_id,
        "openai-compatible",
        "embeddings",
        "http://127.0.0.1:1234/v1/embeddings",
        "sha256:arguments",
        "produce a governed embedding",
        vec![
            EffectPrecondition::new("model-snapshot", snapshot_id.to_string())
                .map_err(|error| error.to_string())?,
        ],
        "sha256:preconditions",
        RiskCategory::Medium,
        EffectReversibility::Unknown,
        IdempotencyProfile::ProviderKey,
        vestrace_domain::DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        None::<String>,
        None::<String>,
        now(),
    )
    .map_err(|error| error.to_string())?;
    let effect_id = intent.id().as_uuid();
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .insert_intent(&context, &intent)
        .await
        .map_err(|error| format!("accepted-job intent persistence failed: {error}"))?;

    let mut seeded = owner.begin().await.map_err(sql)?;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *seeded)
        .await
        .map_err(sql)?;
    set_workspace(&mut seeded, workspace_id.as_uuid()).await?;
    sqlx::query("INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,state,completed_at) VALUES($1,$2,$3,'q1','succeeded',NOW())")
        .bind(qualification_job_id).bind(workspace_id.as_uuid()).bind(connection_revision_id).execute(&mut *seeded).await.map_err(sql)?;
    sqlx::query("INSERT INTO connection_qualification_revisions(id,workspace_id,connection_revision_id,qualification_job_id,profile_revision,valid_until,capabilities) VALUES($1,$2,$3,$4,'q1',NOW()+INTERVAL '1 hour',ARRAY['embedding']::TEXT[])")
        .bind(connection_qualification_id).bind(workspace_id.as_uuid()).bind(connection_revision_id).bind(qualification_job_id).execute(&mut *seeded).await.map_err(sql)?;
    sqlx::query("INSERT INTO model_qualification_revisions(id,workspace_id,model_revision_id,connection_revision_id,connection_qualification_revision_id,qualification_job_id,capabilities,valid_until) VALUES($1,$2,$3,$4,$5,$6,ARRAY['embedding']::TEXT[],NOW()+INTERVAL '1 hour')")
        .bind(model_qualification_id).bind(workspace_id.as_uuid()).bind(model_revision_id).bind(connection_revision_id).bind(connection_qualification_id).bind(qualification_job_id).execute(&mut *seeded).await.map_err(sql)?;
    sqlx::query("INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,branch,no_auth_binding_revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,'no_auth',$8)")
        .bind(snapshot_id).bind(workspace_id.as_uuid()).bind(connection_id).bind(connection_revision_id).bind(connection_qualification_id).bind(model_revision_id).bind(model_qualification_id).bind(no_auth_id).execute(&mut *seeded).await.map_err(sql)?;
    sqlx::query("INSERT INTO model_binding_snapshot_scopes(workspace_id,snapshot_id,scope,transition_plan_id) VALUES($1,$2,'ordinary',NULL)")
        .bind(workspace_id.as_uuid()).bind(snapshot_id).execute(&mut *seeded).await.map_err(sql)?;
    seeded.commit().await.map_err(sql)?;

    // The job is accepted against a *canonical* registration, not the legacy one
    // the store created.
    //
    // Migration 0197 refuses any generation of `legacy_upgrade` representation
    // reaching Ready, and every boundary past publication -- an index build, a
    // transition activation, a retrieval fence -- needs a Ready generation over
    // the job's own registration. A fixture that accepted the job against the
    // legacy space could build a world where results publish and nothing can
    // ever be drawn from them, which is not a world any deployment reaches.
    //
    // The legacy registration is still created beside it and still in the
    // database, because the legacy quarantine boundary is what several other
    // assertions are about. It is not carried on the fixture: nothing reads it,
    // and a field kept for a caller that does not exist is a field that drifts.
    let (canonical_registration_id, canonical_qualification_id) = register_canonical_space(
        owner,
        runtime,
        workspace_id.as_uuid(),
        qualification_job_id,
        connection_id,
        connection_revision_id,
        no_auth_id,
        model_revision_id,
        connection_qualification_id,
    )
    .await?;

    if accept_job {
        let mut accept = runtime.begin().await.map_err(sql)?;
        set_workspace(&mut accept, workspace_id.as_uuid()).await?;
        sqlx::query(
            "SELECT vestrace_accept_embedding_job($1,$2,$3,'delivery',$4,$5,$6,NULL,NULL::BIGINT)",
        )
        .bind(job_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(canonical_registration_id)
        .bind(snapshot_id)
        .bind(effect_id)
        .bind(evidence_id)
        .execute(&mut *accept)
        .await
        .map_err(sql)?;
        accept.commit().await.map_err(sql)?;
    }

    let mut published = runtime.begin().await.map_err(sql)?;
    set_workspace(&mut published, workspace_id.as_uuid()).await?;
    let shape_id = Uuid::now_v7();
    let limits_id = Uuid::now_v7();
    sqlx::query("SELECT vestrace_create_model_request_shape_revision($1,$2,1,'embeddings',false,ARRAY[]::TEXT[])")
        .bind(shape_id).bind(workspace_id.as_uuid()).execute(&mut *published).await.map_err(sql)?;
    sqlx::query("SELECT vestrace_create_model_limits_revision($1,$2,1,8,1,2048)")
        .bind(limits_id)
        .bind(workspace_id.as_uuid())
        .execute(&mut *published)
        .await
        .map_err(sql)?;
    sqlx::query("SELECT vestrace_publish_connection_admission_policy($1,$2,$3,0::BIGINT,1::SMALLINT,60000,30,900)")
        .bind(Uuid::now_v7()).bind(workspace_id.as_uuid()).bind(connection_id).execute(&mut *published).await.map_err(sql)?;
    published.commit().await.map_err(sql)?;

    let mut evidence = owner.begin().await.map_err(sql)?;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *evidence)
        .await
        .map_err(sql)?;
    set_workspace(&mut evidence, workspace_id.as_uuid()).await?;
    sqlx::query("INSERT INTO model_request_evidence_roots(id,workspace_id,external_effect_id,request_kind,binding_snapshot_id,cause_kind,cause_id) VALUES($1,$2,$3,'embeddings',$4,'embedding_job',$5)")
        .bind(evidence_id).bind(workspace_id.as_uuid()).bind(effect_id).bind(snapshot_id).bind(job_id.as_uuid()).execute(&mut *evidence).await.map_err(sql)?;
    for (ordinal, (kind, reference, version)) in [
        ("external_effect", effect_id, None),
        ("binding_snapshot", snapshot_id, None),
        ("connection_revision", connection_revision_id, None),
        (
            "connection_qualification_revision",
            connection_qualification_id,
            None,
        ),
        ("model_revision", model_revision_id, None),
        ("model_qualification_revision", model_qualification_id, None),
        ("request_shape_revision", shape_id, Some(1_i64)),
        ("limits_revision", limits_id, Some(1_i64)),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query("INSERT INTO model_request_evidence_nodes(id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id,reference_version) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(Uuid::now_v7()).bind(workspace_id.as_uuid()).bind(evidence_id).bind(ordinal as i32).bind(kind).bind(reference).bind(version)
            .execute(&mut *evidence).await.map_err(sql)?;
    }
    sqlx::query("INSERT INTO model_request_evidence_checks(id,workspace_id,evidence_root_id,status,missing_reference_count) VALUES($1,$2,$3,'complete',0)")
        .bind(Uuid::now_v7()).bind(workspace_id.as_uuid()).bind(evidence_id).execute(&mut *evidence).await.map_err(sql)?;
    evidence.commit().await.map_err(sql)?;

    let fixture = Fixture {
        workspace_id: workspace_id.as_uuid(),
        principal_id: principal_id.as_uuid(),
        job_id: job_id.as_uuid(),
        effect_id,
        snapshot_id,
        connection_id,
        connection_revision_id,
        evidence_id,
        space_registration_id: canonical_registration_id,
        canonical_qualification_id,
        intent: Some(intent),
    };
    if accept_job {
        assert_baseline(owner, &fixture).await?;
    } else {
        assert_result_preparation_baseline(owner, &fixture).await?;
    }
    Ok(fixture)
}

/// Seed the exact q1 structural evidence `vestrace_assert_canonical_embedding_space`
/// demands, then register one canonical space through the real guarded authority
/// and make it this model revision's active space.
///
/// The evidence chain is not faked past its own guard: the registration still
/// goes through `vestrace_register_canonical_embedding_space`, which asserts
/// every join below. What is seeded here is the durable evidence a real q1
/// qualification would have left behind, exactly as the infrastructure suite's
/// own canonical fixture seeds it.
#[allow(clippy::too_many_arguments)]
async fn register_canonical_space(
    owner: &PgPool,
    runtime: &PgPool,
    workspace: Uuid,
    qualification_job: Uuid,
    connection: Uuid,
    connection_revision: Uuid,
    no_auth_binding: Uuid,
    model_revision: Uuid,
    connection_qualification: Uuid,
) -> Result<(Uuid, Uuid), String> {
    let canonical_qualification = Uuid::now_v7();
    let probe_effect = Uuid::now_v7();
    let evidence_root = Uuid::now_v7();
    let target_binding = Uuid::now_v7();
    let registration = Uuid::now_v7();
    let shape = Uuid::now_v7();

    // `external_effect_intents` predates the guarded ownership and the guarded
    // owner holds no privilege on it, so the probe's effect is written before
    // the role switch rather than under that role.
    sqlx::query(
        "INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) \
         VALUES($1,$2,'local','{}'::jsonb)",
    )
    .bind(probe_effect)
    .bind(workspace)
    .execute(owner)
    .await
    .map_err(sql)?;

    let mut seeded = owner.begin().await.map_err(sql)?;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *seeded)
        .await
        .map_err(sql)?;
    set_workspace(&mut seeded, workspace).await?;
    // `model_qualification_revisions` is immutable evidence, so the fixture's
    // `embedding` capability cannot be widened in place. A second revision over
    // the same job and model states the `embeddings` request capability the
    // canonical assertion reads.
    sqlx::query(
        "INSERT INTO model_qualification_revisions(id,workspace_id,model_revision_id,\
         connection_revision_id,connection_qualification_revision_id,qualification_job_id,\
         capabilities,valid_until) \
         VALUES($1,$2,$3,$4,$5,$6,ARRAY['embedding','embeddings']::TEXT[],NOW()+INTERVAL '1 hour')",
    )
    .bind(canonical_qualification)
    .bind(workspace)
    .bind(model_revision)
    .bind(connection_revision)
    .bind(connection_qualification)
    .bind(qualification_job)
    .execute(&mut *seeded)
    .await
    .map_err(sql)?;
    sqlx::query(
        "INSERT INTO qualification_target_bindings(id,workspace_id,qualification_job_id,\
         connection_id,connection_revision_id,branch,no_auth_binding_revision_id,\
         embedding_model_revision_id) VALUES($1,$2,$3,$4,$5,'no_auth',$6,$7)",
    )
    .bind(target_binding)
    .bind(workspace)
    .bind(qualification_job)
    .bind(connection)
    .bind(connection_revision)
    .bind(no_auth_binding)
    .bind(model_revision)
    .execute(&mut *seeded)
    .await
    .map_err(sql)?;
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
    .execute(&mut *seeded)
    .await
    .map_err(sql)?;
    let evidence_check = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO model_request_evidence_checks(id,workspace_id,evidence_root_id,status) \
         VALUES($1,$2,$3,'complete')",
    )
    .bind(evidence_check)
    .bind(workspace)
    .bind(evidence_root)
    .execute(&mut *seeded)
    .await
    .map_err(sql)?;
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
    .execute(&mut *seeded)
    .await
    .map_err(sql)?;
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
    .execute(&mut *seeded)
    .await
    .map_err(sql)?;
    sqlx::query(
        "INSERT INTO qualification_q1_mre_sources(evidence_root_id,workspace_id,probe_ordinal,\
         message_layout,tool_choice,parallel_tool_calls,response_format,stream,\
         stream_include_usage) \
         VALUES($1,$2,'90','plain_text','none',false,'none',false,false)",
    )
    .bind(evidence_root)
    .bind(workspace)
    .execute(&mut *seeded)
    .await
    .map_err(sql)?;
    seeded.commit().await.map_err(sql)?;

    let mut governed = runtime.begin().await.map_err(sql)?;
    set_workspace(&mut governed, workspace).await?;
    sqlx::query(
        "SELECT vestrace_create_model_request_shape_revision($1,$2,1,'embeddings',false,\
         ARRAY[]::TEXT[])",
    )
    .bind(shape)
    .bind(workspace)
    .execute(&mut *governed)
    .await
    .map_err(sql)?;
    // The returned model is this revision's own `wire_model_id`, not the legacy
    // space's model name. `vestrace_assert_canonical_embedding_space` requires
    // `s.returned_model = m.wire_model_id`: a registration naming a model the
    // revision does not claim to speak would be a space nothing could serve.
    sqlx::query(
        "SELECT vestrace_register_canonical_embedding_space($1,$2,'nomic-768',$3,$4,$5,\
         'text-embedding-nomic-embed-text-v1.5','float',768)",
    )
    .bind(registration)
    .bind(workspace)
    .bind(model_revision)
    .bind(canonical_qualification)
    .bind(shape)
    .execute(&mut *governed)
    .await
    .map_err(|error| format!("canonical space registration failed: {}", sql(error)))?;
    // The registration is deliberately *not* made this model's active space.
    //
    // `vestrace_set_initial_embedding_active_space` additionally requires the
    // qualification head to already point at the registration's own
    // qualification revision, and advancing that head is a separate governed
    // decision about which space a retrieval should be served from. Nothing
    // these scenarios do needs it: capture, publication, index building and
    // transition activation all name a registration directly. Setting it here
    // would be the fixture making a routing decision it is not testing.
    governed.commit().await.map_err(sql)?;
    Ok((registration, canonical_qualification))
}

/// One more delivery job in a world that already exists, ready to be accepted.
///
/// A transition needs a second physical job: the source's projection is the one
/// being replaced, so the satisfier has to be somebody else's. Everything
/// immutable is reused -- the same workspace, principal, binding snapshot and
/// canonical registration -- and only the three identities a job owns
/// exclusively are fresh: its external effect, its evidence root and itself.
pub(super) async fn accept_additional_job(
    owner: &PgPool,
    runtime: &PgPool,
    base: &Fixture,
) -> Result<Fixture, String> {
    let evidence_id = Uuid::now_v7();
    let job_id = Uuid::now_v7();

    // A real intent, not a copied row: the result path takes the value rather
    // than the record, and an intent whose identity was chosen here could not be
    // the one the repository persisted.
    let workspace = WorkspaceId::from_uuid(base.workspace_id);
    let principal = PrincipalId::from_uuid(base.principal_id);
    let intent = ExternalEffectIntent::new(
        "workspace://",
        workspace,
        principal,
        "openai-compatible",
        "embeddings",
        "http://127.0.0.1:1234/v1/embeddings",
        "sha256:arguments",
        "produce a governed embedding",
        vec![
            EffectPrecondition::new("model-snapshot", base.snapshot_id.to_string())
                .map_err(|error| error.to_string())?,
        ],
        "sha256:preconditions",
        RiskCategory::Medium,
        EffectReversibility::Unknown,
        IdempotencyProfile::ProviderKey,
        vestrace_domain::DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        None::<String>,
        None::<String>,
        now(),
    )
    .map_err(|error| error.to_string())?;
    let effect_id = intent.id().as_uuid();
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .insert_intent(&RequestContext::new(workspace, principal), &intent)
        .await
        .map_err(|error| format!("the additional intent was refused: {error}"))?;

    // The job is deliberately *not* accepted here. `accept_delivery_outputs`
    // accepts it and fixes its outputs in one transaction, and an already
    // accepted bare job cannot be backfilled with guessed identities -- the same
    // rule the source fixture above is built around. Whoever prepares this job's
    // outputs accepts it.

    // The evidence root is copied from the first job's rather than rebuilt, so
    // the second job's request is provably the same shape as the first's and
    // any difference between them is one this fixture chose.
    //
    // The structural nodes only. A governed input material is attached later,
    // by whoever prepares this job's outputs, and it always takes the same
    // ordinal -- so copying the first job's would collide with the second job's
    // own the moment that job is prepared.
    let mut evidence = owner.begin().await.map_err(sql)?;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *evidence)
        .await
        .map_err(sql)?;
    set_workspace(&mut evidence, base.workspace_id).await?;
    sqlx::query(
        "INSERT INTO model_request_evidence_roots(id,workspace_id,external_effect_id,\
         request_kind,binding_snapshot_id,cause_kind,cause_id) \
         VALUES($1,$2,$3,'embeddings',$4,'embedding_job',$5)",
    )
    .bind(evidence_id)
    .bind(base.workspace_id)
    .bind(effect_id)
    .bind(base.snapshot_id)
    .bind(job_id)
    .execute(&mut *evidence)
    .await
    .map_err(sql)?;
    sqlx::query(
        "INSERT INTO model_request_evidence_nodes(id,workspace_id,evidence_root_id,ordinal,\
         reference_kind,reference_id,reference_version) \
         SELECT gen_random_uuid(),workspace_id,$1,ordinal,reference_kind,\
                CASE WHEN reference_kind='external_effect' THEN $2 ELSE reference_id END,\
                reference_version \
           FROM model_request_evidence_nodes           WHERE evidence_root_id=$3 AND workspace_id=$4             AND reference_kind<>'governed_input_material'",
    )
    .bind(evidence_id)
    .bind(effect_id)
    .bind(base.evidence_id)
    .bind(base.workspace_id)
    .execute(&mut *evidence)
    .await
    .map_err(sql)?;
    sqlx::query(
        "INSERT INTO model_request_evidence_checks(id,workspace_id,evidence_root_id,status,\
         missing_reference_count) VALUES(gen_random_uuid(),$1,$2,'complete',0)",
    )
    .bind(base.workspace_id)
    .bind(evidence_id)
    .execute(&mut *evidence)
    .await
    .map_err(sql)?;
    evidence.commit().await.map_err(sql)?;

    Ok(Fixture {
        workspace_id: base.workspace_id,
        principal_id: base.principal_id,
        job_id,
        effect_id,
        snapshot_id: base.snapshot_id,
        canonical_qualification_id: base.canonical_qualification_id,
        connection_id: base.connection_id,
        connection_revision_id: base.connection_revision_id,
        evidence_id,
        space_registration_id: base.space_registration_id,
        intent: Some(intent),
    })
}

/// The allowed branch is driven with the guarded admission function. The
/// intent re-insert is deliberately ON CONFLICT DO NOTHING: acceptance already
/// persisted it. Authorization and lifecycle writes match the repository's
/// SQL, while the transaction remains open until a requested process abort.
pub(super) async fn drive(
    runtime: &PgPool,
    fixture: &Fixture,
    crash: Option<vestrace_domain::external_effects::EffectFaultPoint>,
) -> Result<(), String> {
    let mut tx = runtime.begin().await.map_err(sql)?;
    set_workspace(&mut tx, fixture.workspace_id).await?;
    sqlx::query("SELECT * FROM vestrace_try_admit_provider_dispatch($1,$2,$3,$4,$5,$6,$7,$8,'embedding_job',NULL,NULL,$9,NULL,NULL,NULL,60)")
        .bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(fixture.workspace_id)
        .bind(fixture.connection_id).bind(fixture.connection_revision_id).bind(fixture.effect_id).bind(fixture.evidence_id).bind(fixture.snapshot_id)
        .execute(&mut *tx).await.map_err(|error| format!("provider dispatch admission failed: {}", sql(error)))?;
    sqlx::query("INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) SELECT id,workspace_id,adapter,payload FROM external_effect_intents WHERE id=$1 AND workspace_id=$2 ON CONFLICT(id) DO NOTHING")
        .bind(fixture.effect_id).bind(fixture.workspace_id).execute(&mut *tx).await.map_err(sql)?;
    checkpoint(
        crash,
        vestrace_domain::external_effects::EffectFaultPoint::AfterIntentPersistence,
        fixture,
    );

    let authorization_id = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_authorizations(id,effect_id,workspace_id,policy_id,policy_version,subject_id,capability,operation,resource_scope,result,reason,input_state,matched_grant_id,decided_at,payload) SELECT $1,id,workspace_id,NULL,'embedding-dispatch-v1',$2,'export.read','produce a governed embedding','http://127.0.0.1:1234/v1/embeddings','allow','configured_allowance','{}'::jsonb,NULL,NOW(),'{}'::jsonb FROM external_effect_intents WHERE id=$3 AND workspace_id=$4")
        .bind(authorization_id).bind(fixture.principal_id).bind(fixture.effect_id).bind(fixture.workspace_id).execute(&mut *tx).await.map_err(sql)?;
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'authorized','authorization_recorded',$3,NOW())")
        .bind(fixture.effect_id).bind(fixture.workspace_id).bind(authorization_id.to_string()).execute(&mut *tx).await.map_err(sql)?;
    checkpoint(
        crash,
        vestrace_domain::external_effects::EffectFaultPoint::AfterAuthorizationBeforeDispatch,
        fixture,
    );

    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),'embedding-fault-child',NOW()+INTERVAL '60 seconds')")
        .bind(fixture.effect_id).bind(fixture.workspace_id).bind(fixture.effect_id.to_string()).execute(&mut *tx).await.map_err(|error| format!("provider dispatch lifecycle write failed: {}", sql(error)))?;
    checkpoint(
        crash,
        vestrace_domain::external_effects::EffectFaultPoint::AfterDispatchBeforeReceipt,
        fixture,
    );
    tx.commit().await.map_err(sql)?;

    if crash.is_none() {
        return Ok(());
    }
    // A post-dispatch call is allowed only after the dispatch transaction is
    // durable. The listener is outside this child and counts the request.
    let url = std::env::var("VESTRACE_EMBEDDING_DISPATCH_URL")
        .map_err(|_| "embedding child has no loopback dispatch URL".to_owned())?;
    reqwest::Client::new()
        .post(url)
        .body("embedding-fault-probe")
        .send()
        .await
        .map_err(|error| format!("loopback probe failed: {error}"))?;
    let mut receipt = runtime.begin().await.map_err(sql)?;
    set_workspace(&mut receipt, fixture.workspace_id).await?;
    let receipt_id = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload) VALUES($1,$2,$3,'acknowledged','{}'::jsonb)")
        .bind(receipt_id).bind(fixture.effect_id).bind(fixture.workspace_id).execute(&mut *receipt).await.map_err(sql)?;
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,'acknowledged','receipt_recorded',$3,NOW())")
        .bind(fixture.effect_id).bind(fixture.workspace_id).bind(receipt_id.to_string()).execute(&mut *receipt).await.map_err(sql)?;
    sqlx::query("SELECT vestrace_release_provider_dispatch($1,$2,$3)")
        .bind(fixture.workspace_id)
        .bind(fixture.effect_id)
        .bind(receipt_id)
        .execute(&mut *receipt)
        .await
        .map_err(sql)?;
    receipt.commit().await.map_err(sql)?;
    checkpoint(
        crash,
        vestrace_domain::external_effects::EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation,
        fixture,
    );
    Err("no embedding fault point was selected".to_owned())
}

fn checkpoint(
    requested: Option<vestrace_domain::external_effects::EffectFaultPoint>,
    current: vestrace_domain::external_effects::EffectFaultPoint,
    fixture: &Fixture,
) {
    if requested == Some(current) {
        announce(point_name(current), fixture);
        // Do not replace this with panic or an injected error. The whole point
        // is that the process loses all in-memory transaction state.
        std::process::abort();
    }
}

fn announce(stage: &str, fixture: &Fixture) {
    let mut stderr = std::io::stderr().lock();
    writeln!(
        stderr,
        "{MARKER} stage={stage} workspace={} principal={} job={} effect={}",
        fixture.workspace_id, fixture.principal_id, fixture.job_id, fixture.effect_id
    )
    .expect("marker write");
    stderr.flush().expect("marker flush");
}

pub(super) async fn set_workspace(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
) -> Result<(), String> {
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut **tx)
        .await
        .map_err(sql)?;
    Ok(())
}

async fn assert_baseline(pool: &PgPool, fixture: &Fixture) -> Result<(), String> {
    let observed = read_counts(pool, fixture).await?;
    let expected = (1_i64, 1_i64, 0_i64, 0_i64, 0_i64, 0_i64, 0_i64, 0_i64);
    let actual = (
        number(&observed, "job_count")?,
        number(&observed, "intent_count")?,
        number(&observed, "authorization_count")?,
        number(&observed, "admission_count")?,
        number(&observed, "dispatching_count")?,
        number(&observed, "deadline_count")?,
        number(&observed, "receipt_count")?,
        number(&observed, "reconciliation_count")?,
    );
    if actual != expected {
        return Err(format!(
            "accepted embedding baseline is not durable-intent/no-dispatch: {observed}"
        ));
    }
    Ok(())
}

async fn assert_result_preparation_baseline(
    pool: &PgPool,
    fixture: &Fixture,
) -> Result<(), String> {
    let observed = read_counts(pool, fixture).await?;
    let expected = (0_i64, 1_i64, 0_i64, 0_i64, 0_i64, 0_i64, 0_i64, 0_i64);
    let actual = (
        number(&observed, "job_count")?,
        number(&observed, "intent_count")?,
        number(&observed, "authorization_count")?,
        number(&observed, "admission_count")?,
        number(&observed, "dispatching_count")?,
        number(&observed, "deadline_count")?,
        number(&observed, "receipt_count")?,
        number(&observed, "reconciliation_count")?,
    );
    if actual != expected {
        return Err(format!(
            "result-preparation fixture must defer job acceptance to the output authority: {observed}"
        ));
    }
    Ok(())
}

async fn read_control(pool: &PgPool, fixture: &Fixture) -> Result<serde_json::Value, String> {
    let counts = read_counts(pool, fixture).await?;
    let expected = (1, 1, 1);
    let actual = (
        number(&counts, "authorization_count")?,
        number(&counts, "admission_count")?,
        number(&counts, "dispatching_count")?,
    );
    if actual != expected {
        return Err(format!(
            "allowed control did not create exactly one authorization, admission and dispatching transition: {counts}"
        ));
    }
    Ok(counts)
}

async fn read_survivor(pool: &PgPool, fixture: &Fixture) -> Result<serde_json::Value, String> {
    read_counts(pool, fixture).await
}

async fn read_counts(pool: &PgPool, fixture: &Fixture) -> Result<serde_json::Value, String> {
    let row: (i64, i64, i64, i64, i64, i64, i64, i64, Option<String>) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM embedding_jobs WHERE id=$1), \
          (SELECT COUNT(*) FROM external_effect_intents WHERE id=$2), \
          (SELECT COUNT(*) FROM external_effect_authorizations WHERE effect_id=$2), \
          (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$2), \
          (SELECT COUNT(*) FROM external_effect_lifecycle_transitions WHERE effect_id=$2 AND status='dispatching'), \
          (SELECT COUNT(*) FROM external_effect_lifecycle_transitions WHERE effect_id=$2 AND dispatch_expires_at IS NOT NULL), \
          (SELECT COUNT(*) FROM external_effect_receipts WHERE effect_id=$2), \
          (SELECT COUNT(*) FROM external_reconciliations WHERE effect_id=$2), \
          (SELECT state FROM embedding_jobs WHERE id=$1)",
    ).bind(fixture.job_id).bind(fixture.effect_id).fetch_one(pool).await.map_err(sql)?;
    Ok(serde_json::json!({
        "job_count": row.0, "intent_count": row.1, "authorization_count": row.2,
        "admission_count": row.3, "dispatching_count": row.4, "deadline_count": row.5,
        "receipt_count": row.6, "reconciliation_count": row.7, "job_state": row.8,
    }))
}

fn assert_survivor(
    point: vestrace_domain::external_effects::EffectFaultPoint,
    observation: &serde_json::Value,
) -> Result<(), String> {
    let expected = match point {
        vestrace_domain::external_effects::EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation => (1, 1, 1, 1),
        vestrace_domain::external_effects::EffectFaultPoint::AfterIntentPersistence
        | vestrace_domain::external_effects::EffectFaultPoint::AfterAuthorizationBeforeDispatch
        | vestrace_domain::external_effects::EffectFaultPoint::AfterDispatchBeforeReceipt => (0, 0, 0, 0),
        vestrace_domain::external_effects::EffectFaultPoint::AfterOutcomeBeforeRunCommit => return Ok(()),
        _ => return Err("embedding scenario received a non-effect fault point".to_owned()),
    };
    let actual = (
        number(observation, "authorization_count")?,
        number(observation, "admission_count")?,
        number(observation, "dispatching_count")?,
        number(observation, "receipt_count")?,
    );
    if actual != expected {
        return Err(format!(
            "{point:?} left an unexpected durable survivor tuple: {observation}"
        ));
    }
    Ok(())
}

fn number(value: &serde_json::Value, name: &str) -> Result<i64, String> {
    value[name]
        .as_i64()
        .ok_or_else(|| format!("{name} was not a persisted count"))
}

async fn run_same_child(
    settings: &ScenarioSettings,
    kind: &str,
    dispatch_url: String,
) -> Result<std::process::Output, String> {
    let program = std::env::current_exe()
        .map_err(|error| format!("current executable is unreadable: {error}"))?;
    let url_file = crate::argument(std::env::args().skip(1), "--database-url-file")
        .ok_or_else(|| "database URL file is required".to_owned())?;
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .map_err(|_| "VESTRACE_RUNTIME_DATABASE_URL is required for the child".to_owned())?;
    let mut command = tokio::process::Command::new(program);
    command
        .arg("--database-url-file")
        .arg(url_file)
        .arg("--scenario")
        .arg("embedding_dispatch_crash")
        .env("VESTRACE_FAULT_CHILD", "1")
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        .env(
            "VESTRACE_FAULT_POINT",
            point_name(settings.embedding_dispatch_point()),
        )
        .env("VESTRACE_RUNTIME_DATABASE_URL", runtime_url)
        .env("VESTRACE_EMBEDDING_DISPATCH_URL", dispatch_url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if kind == "control" {
        command.env("VESTRACE_EMBEDDING_CONTROL", "1");
    }
    command
        .output()
        .await
        .map_err(|error| format!("embedding {kind} child could not start: {error}"))
}

fn marker(stderr: &str, stage: &str) -> Result<Fixture, String> {
    let prefix = format!("{MARKER} stage={stage} ");
    let line = stderr
        .lines()
        .find(|line| line.starts_with(&prefix))
        .ok_or_else(|| format!("child did not reach {stage}; stderr was: {stderr}"))?;
    let mut values = std::collections::BTreeMap::new();
    for part in line[prefix.len()..].split_whitespace() {
        let (name, value) = part
            .split_once('=')
            .ok_or_else(|| format!("malformed child marker: {line}"))?;
        values.insert(name, value);
    }
    let id = |name: &str| {
        values
            .get(name)
            .ok_or_else(|| format!("marker omitted {name}"))
            .and_then(|value| {
                value
                    .parse::<Uuid>()
                    .map_err(|error| format!("marker {name} is invalid: {error}"))
            })
    };
    Ok(Fixture {
        workspace_id: id("workspace")?,
        principal_id: id("principal")?,
        job_id: id("job")?,
        effect_id: id("effect")?,
        snapshot_id: Uuid::nil(),
        canonical_qualification_id: Uuid::nil(),
        connection_id: Uuid::nil(),
        connection_revision_id: Uuid::nil(),
        evidence_id: Uuid::nil(),
        space_registration_id: Uuid::nil(),
        intent: None,
    })
}

fn point_name(point: vestrace_domain::external_effects::EffectFaultPoint) -> &'static str {
    use vestrace_domain::external_effects::EffectFaultPoint::*;
    match point {
        AfterIntentPersistence => "after_intent_persistence",
        AfterAuthorizationBeforeDispatch => "after_authorization_before_dispatch",
        AfterDispatchBeforeReceipt => "after_dispatch_before_receipt",
        AfterReceiptBeforeOutcomeConfirmation => "after_receipt_before_outcome_confirmation",
        AfterOutcomeBeforeRunCommit => "after_outcome_before_run_commit",
        _ => "invalid_embedding_fault_point",
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
pub(super) fn sql(error: sqlx::Error) -> String {
    error.to_string()
}
fn die(error: String) -> ! {
    eprintln!("{MARKER} setup failed: {error}");
    std::process::exit(2)
}

struct LoopbackCounter {
    url: String,
    count: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl LoopbackCounter {
    async fn start() -> Result<Self, String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|error| format!("loopback listener bind failed: {error}"))?;
        let url = format!(
            "http://{}/embedding",
            listener.local_addr().map_err(|error| error.to_string())?
        );
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                observed.fetch_add(1, Ordering::SeqCst);
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer).await;
                let _ = stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                    .await;
            }
        });
        Ok(Self { url, count, task })
    }
    fn url(&self) -> String {
        self.url.clone()
    }
    fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl Drop for LoopbackCounter {
    fn drop(&mut self) {
        self.task.abort();
    }
}
