//! Real process death at the persisted delivery finalization boundaries.
//! The existing 14D child supplies the sole provider call and prepared fixture.
use crate::{ScenarioSettings, embedding_dispatch_crash as dispatch, intent_support};
use std::{
    future::Future,
    io::Write,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, EmbeddingOutputCommitment, EmbeddingOutputKeyBinding,
    EmbeddingResultFinalizationAuthority, EmbeddingResultFinalizationProgress,
    EmbeddingResultFinalizationRepository, EmbeddingResultFinalizationService,
    EmbeddingResultPreparationId, EmbeddingResultPublication, InstallationMutationPermit,
    MaterialKeyVault, PermitMode, RequestContext, VaultError,
};
use vestrace_domain::{
    EmbeddingJobId, ErasureReceipt, ExternalEffectId, IntentNonce, MaterialKeyBindingReceipt,
    MaterialKeyId, PrincipalId, VaultReceipt, WorkspaceId, ZeroizingDek,
    trust::{KeyPurpose, KeyReference, SecretResolutionRequest},
};
use vestrace_infrastructure::{
    crypto::{HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER},
    postgres::{
        EmbeddingOutputHmacCommitter, PgEmbeddingResultFinalizationRepository,
        PgInstallationMutationPermit, PgScopedTransaction, PgStore,
    },
};
const MARKER: &str = "vestrace-fault-scenario: embedding-result-finalization";
const POINTS: [&str; 5] = [
    "host_bound_before_sql",
    "strict_receipt_subset",
    "all_bound_before_publish",
    "sql_before_commit",
    "after_commit",
];

fn abort_at(point: &str) -> ! {
    let mut stderr = std::io::stderr().lock();
    writeln!(stderr, "{MARKER} point={point} pid={}", std::process::id())
        .expect("checkpoint marker");
    stderr.flush().expect("checkpoint flush");
    std::process::abort()
}
struct CrashRepository {
    inner: PgEmbeddingResultFinalizationRepository,
    store: PgStore,
    point: String,
}
impl EmbeddingResultFinalizationRepository for CrashRepository {
    fn load_progress<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        c: &'life1 RequestContext,
        a: &'life2 EmbeddingResultFinalizationAuthority,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<EmbeddingResultFinalizationProgress, ApplicationError>>
                + Send
                + 'async_trait,
        >,
    >
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        Box::pin(async move { self.inner.load_progress(c, a).await })
    }
    fn record_binding<'life0, 'life1, 'life2, 'life3, 'life4, 'async_trait>(
        &'life0 self,
        c: &'life1 RequestContext,
        a: &'life2 EmbeddingResultFinalizationAuthority,
        b: &'life3 EmbeddingOutputKeyBinding,
        r: &'life4 MaterialKeyBindingReceipt,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<MaterialKeyBindingReceipt, ApplicationError>>
                + Send
                + 'async_trait,
        >,
    >
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        'life3: 'async_trait,
        'life4: 'async_trait,
    {
        Box::pin(async move {
            if self.point == "host_bound_before_sql" {
                abort_at(&self.point);
            }
            let result = self.inner.record_binding(c, a, b, r).await?;
            if self.point == "strict_receipt_subset" {
                abort_at(&self.point);
            }
            Ok(result)
        })
    }
    fn publish<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        c: &'life1 RequestContext,
        a: &'life2 EmbeddingResultFinalizationAuthority,
        p: Uuid,
        e: Uuid,
        commitments: Vec<EmbeddingOutputCommitment>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<EmbeddingResultPublication, ApplicationError>>
                + Send
                + 'async_trait,
        >,
    >
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        Box::pin(async move {
            if self.point == "all_bound_before_publish" {
                abort_at(&self.point);
            }
            if self.point == "sql_before_commit" || self.point == "sql_leg" {
                let permit = PgInstallationMutationPermit::new(self.store.clone());
                let mut handle = permit.acquire(PermitMode::Shared, c).await?;
                let tx = handle
                    .unit_of_work_mut()
                    .as_any_mut()
                    .downcast_mut::<PgScopedTransaction>()
                    .ok_or_else(|| {
                        ApplicationError::Internal("expected PostgreSQL fault transaction".into())
                    })?;
                if self.point == "sql_leg" {
                    let stage = std::env::var("VESTRACE_FINALIZATION_SQL_STAGE")
                        .map_err(|_| ApplicationError::Policy("missing SQL stage".into()))?;
                    sqlx::query("SELECT set_config('vestrace.fault_finalization_stage',$1,true),set_config('application_name',$2,true)")
                        .bind(&stage).bind(format!("14e-fault-{stage}")).execute(tx.connection()).await
                        .map_err(|_|ApplicationError::Storage("SQL stage setup failed".into()))?;
                }
                let projection: Vec<_> = commitments.iter().map(|x| x.projection_id).collect();
                let ordinal = commitments
                    .iter()
                    .map(|x| {
                        i64::try_from(x.output_ordinal)
                            .map_err(|_| ApplicationError::Policy("ordinal overflow".into()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let bytes: Vec<_> = commitments.iter().map(|x| x.commitment.to_vec()).collect();
                let _: serde_json::Value = sqlx::query_scalar(
                    "SELECT vestrace_publish_embedding_job_result($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                )
                .bind(c.workspace_id.as_uuid())
                .bind(a.preparation_id.as_uuid())
                .bind(a.job_id.as_uuid())
                .bind(a.effect_id.as_uuid())
                .bind(p)
                .bind(e)
                .bind(projection)
                .bind(ordinal)
                .bind(bytes)
                .fetch_one(tx.connection())
                .await
                .map_err(|_| {
                    ApplicationError::Storage(
                        "guarded publication SQL failed before abort checkpoint".into(),
                    )
                })?;
                // All real publication legs have executed on this connection.
                // Process death, not rollback/drop, must discard them.
                abort_at(&self.point);
            }
            let result = self.inner.publish(c, a, p, e, commitments).await?;
            if self.point == "after_commit" {
                abort_at(&self.point);
            }
            Ok(result)
        })
    }
}
fn host(workspace: WorkspaceId) -> Result<HostMaterialKeyVault, String> {
    let (vault, bootstrap) = intent_support::vault_roots()?;
    HostMaterialKeyVault::new(
        vault,
        bootstrap,
        KeyReference::new(
            MOUNTED_SECRET_STORE_PROVIDER,
            "material-vault-bootstrap",
            "v1",
            KeyPurpose::Storage,
            "material-vault-bootstrap",
            "aes-256-gcm-v1",
        )
        .map_err(|e| e.to_string())?,
        SecretResolutionRequest::new(
            workspace,
            "material-vault-bootstrap",
            "fault://result-finalization",
        ),
    )
    .map_err(|e| e.to_string())
}
fn env_id(name: &str) -> Result<Uuid, String> {
    std::env::var(name)
        .map_err(|_| format!("missing {name}"))?
        .parse()
        .map_err(|_| format!("invalid {name}"))
}
pub async fn run_child(settings: &ScenarioSettings) -> ! {
    let result=async {
        let owner=dispatch::connect_owner(settings).await?;let runtime=dispatch::connect_runtime(&owner).await?;
        let workspace=env_id("VESTRACE_FINALIZATION_WORKSPACE")?;let job=env_id("VESTRACE_FINALIZATION_JOB")?;
        let principal:Uuid=sqlx::query_scalar("SELECT c.principal_id FROM connections c JOIN model_binding_snapshots s ON s.connection_id=c.id AND s.workspace_id=c.workspace_id JOIN embedding_jobs j ON j.model_binding_snapshot_id=s.id AND j.workspace_id=s.workspace_id WHERE j.workspace_id=$1 AND j.id=$2").bind(workspace).bind(job).fetch_one(&owner).await.map_err(dispatch::sql)?;
        let context=RequestContext::new(WorkspaceId::from_uuid(workspace),PrincipalId::from_uuid(principal));
        let authority=EmbeddingResultFinalizationAuthority {preparation_id:EmbeddingResultPreparationId::from_uuid(env_id("VESTRACE_FINALIZATION_PREPARATION")?),job_id:EmbeddingJobId::from_uuid(job),effect_id:ExternalEffectId::from_uuid(env_id("VESTRACE_FINALIZATION_EFFECT")?)};
        let point=std::env::var("VESTRACE_FINALIZATION_POINT").map_err(|_|"missing finalization point".to_owned())?;
        if !POINTS.contains(&point.as_str()) && point != "sql_leg"{return Err("unknown child checkpoint".into());}
        let store=PgStore::from_pool(runtime);let repo=CrashRepository {inner:PgEmbeddingResultFinalizationRepository::new(store.clone()),store,point};
        EmbeddingResultFinalizationService::new(Arc::new(repo),Arc::new(host(context.workspace_id)?),Arc::new(EmbeddingOutputHmacCommitter::new())).finalize(&context,&authority).await.map_err(|e|e.to_string())?;
        Err::<(),String>("child returned without reaching its checkpoint".into())
    }.await;
    eprintln!("{MARKER} setup failed: {}", result.unwrap_err());
    std::process::exit(2)
}

#[derive(Clone)]
struct Identity {
    workspace: Uuid,
    job: Uuid,
    effect: Uuid,
    preparation: Uuid,
}
fn parse_preparation(stderr: &str) -> Result<Identity, String> {
    let prefix = "vestrace-fault-scenario: embedding-result-preparation";
    let line = stderr
        .lines()
        .find(|line| line.starts_with(prefix) && line.contains(" workspace="))
        .ok_or_else(|| "preparation child omitted its committed identity".to_owned())?;
    let values = line[prefix.len()..]
        .split_whitespace()
        .filter_map(|part| part.split_once('='))
        .collect::<std::collections::BTreeMap<_, _>>();
    let id = |name| {
        values
            .get(name)
            .ok_or_else(|| format!("missing preparation {name}"))?
            .parse::<Uuid>()
            .map_err(|_| format!("invalid preparation {name}"))
    };
    Ok(Identity {
        workspace: id("workspace")?,
        job: id("job")?,
        effect: id("effect")?,
        preparation: id("preparation")?,
    })
}
fn command(scenario: &str, point: &str) -> Result<tokio::process::Command, String> {
    let mut c = tokio::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    let file = crate::argument(std::env::args().skip(1), "--database-url-file")
        .ok_or_else(|| "database URL file required".to_owned())?;
    c.arg("--database-url-file")
        .arg(file)
        .arg("--scenario")
        .arg(scenario)
        .env("VESTRACE_FAULT_CHILD", "1")
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        .env("VESTRACE_FAULT_POINT", point)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    Ok(c)
}
async fn snapshot(pool: &sqlx::PgPool, id: &Identity) -> Result<serde_json::Value, String> {
    sqlx::query_scalar("SELECT jsonb_build_object( \
      'publications',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]') FROM embedding_job_result_publications r WHERE preparation_id=$1), \
      'events',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]') FROM embedding_index_rebuild_events r WHERE workspace_id=$2), \
      'bindings',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.output_ordinal),'[]') FROM embedding_result_key_binding_receipts r WHERE preparation_id=$1), \
      'history',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.output_ordinal),'[]') FROM embedding_job_result_prepared_attachments r WHERE preparation_id=$1), \
      'attachments',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]') FROM prepared_material_attachments r JOIN embedding_job_result_prepared_attachments h ON h.prepared_attachment_id=r.id WHERE h.preparation_id=$1), \
      'projections',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.output_ordinal),'[]') FROM embedding_projection_entries r WHERE preparation_id=$1), \
      'materials',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]') FROM content_materials r JOIN embedding_projection_entries p ON p.material_id=r.id AND p.workspace_id=r.workspace_id WHERE p.preparation_id=$1), \
      'intents',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]') FROM material_key_creation_intents r JOIN embedding_projection_entries p ON p.intent_id=r.id AND p.workspace_id=r.workspace_id WHERE p.preparation_id=$1), \
      'blockers',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]') FROM material_erasure_blockers r JOIN embedding_delivery_source_memberships s ON s.blocker_id=r.id AND s.workspace_id=r.workspace_id WHERE s.job_id=$3), \
      'job',(SELECT to_jsonb(r) FROM embedding_jobs r WHERE id=$3), \
      'corpus',(SELECT to_jsonb(r) FROM embedding_space_corpus_states r JOIN embedding_job_result_preparations p ON p.space_registration_id=r.space_registration_id AND p.workspace_id=r.workspace_id WHERE p.id=$1), \
      'ready_generations',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]') FROM embedding_corpus_generations r JOIN embedding_job_result_preparations p ON p.space_registration_id=r.space_registration_id AND p.workspace_id=r.workspace_id WHERE p.id=$1), \
      'generation',(SELECT to_jsonb(r) FROM embedding_index_generation_guards r JOIN embedding_job_result_preparations p ON p.space_registration_id=r.space_registration_id AND p.workspace_id=r.workspace_id WHERE p.id=$1))")
        .bind(id.preparation).bind(id.workspace).bind(id.job).fetch_one(pool).await.map_err(dispatch::sql)
}
fn array_len(snapshot: &serde_json::Value, key: &str) -> Result<usize, String> {
    snapshot[key]
        .as_array()
        .map(Vec::len)
        .ok_or_else(|| format!("snapshot lacks {key}"))
}
async fn host_bound_count(pool: &sqlx::PgPool, id: &Identity) -> Result<usize, String> {
    let rows:Vec<(Uuid,Uuid,Uuid,Uuid,i64)>=sqlx::query_as("SELECT p.intent_id,p.material_id,p.material_key_id,i.nonce,p.output_ordinal FROM embedding_projection_entries p JOIN material_key_creation_intents i ON i.id=p.intent_id AND i.workspace_id=p.workspace_id WHERE p.preparation_id=$1 ORDER BY p.output_ordinal").bind(id.preparation).fetch_all(pool).await.map_err(dispatch::sql)?;
    let (root, _) = intent_support::vault_roots()?;
    let vault = host(WorkspaceId::from_uuid(id.workspace))?;
    let mut bound = 0;
    for (intent, material, key, nonce, ordinal) in rows {
        let path = root.join(key.to_string()).join("output-disposition");
        if !path.exists() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if value["kind"] != "bound" || value["preparation_id"] != id.preparation.to_string() {
            return Err("unexpected host disposition".into());
        }
        let receipt = value["binding_receipt"]
            .as_str()
            .ok_or_else(|| "missing host binding witness".to_owned())?
            .parse::<Uuid>()
            .map_err(|_| "invalid host witness".to_owned())?;
        let binding = EmbeddingOutputKeyBinding {
            workspace_id: WorkspaceId::from_uuid(id.workspace),
            job_id: EmbeddingJobId::from_uuid(id.job),
            intent_id: vestrace_domain::MaterialKeyCreationIntentId::from_uuid(intent),
            material_id: vestrace_domain::ContentMaterialId::from_uuid(material),
            key_id: MaterialKeyId::from_uuid(key),
            nonce: IntentNonce::from_uuid(nonce),
            output_ordinal: u64::try_from(ordinal).map_err(|_| "negative ordinal".to_owned())?,
        };
        let mut calls = 0;
        vault
            .with_bound_embedding_output_key(
                &binding,
                EmbeddingResultPreparationId::from_uuid(id.preparation),
                MaterialKeyBindingReceipt::from_uuid(receipt),
                &mut |_| calls += 1,
            )
            .map_err(|e| e.to_string())?;
        if calls != 1 {
            return Err("reopened exact bound callback did not run once".into());
        }
        bound += 1;
    }
    Ok(bound)
}
struct NeverVault;
impl MaterialKeyVault for NeverVault {
    fn create_if_absent(
        &self,
        _: MaterialKeyId,
        _: IntentNonce,
    ) -> Result<VaultReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }
    fn unwrap(&self, _: MaterialKeyId, _: &mut dyn FnMut(&ZeroizingDek)) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
    fn prepare_erasure(
        &self,
        _: MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }
    fn erase(&self, _: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }
}
pub async fn run_parent(settings: &ScenarioSettings) -> Result<String, String> {
    let listener = LoopbackCounter::start().await?;
    let mut setup = command(
        "embedding_result_preparation_crash",
        "after_result_prepared_before_return",
    )?;
    setup.env("VESTRACE_RESULT_PREPARATION_DISPATCH_URL", &listener.url);
    let output = setup.output().await.map_err(|e| e.to_string())?;
    if output.status.success() {
        return Err("preparation child did not abort".into());
    }
    let id = parse_preparation(&String::from_utf8_lossy(&output.stderr))?;
    let owner = dispatch::connect_owner(settings).await?;
    let runtime = dispatch::connect_runtime(&owner).await?;
    let ready = prepare_ready_generation(&runtime, &id).await?;
    let initial = snapshot(&owner, &id).await?;
    if array_len(&initial, "history")? != 2
        || array_len(&initial, "attachments")? != 2
        || array_len(&initial, "bindings")? != 0
    {
        return Err("prepared fixture incomplete".into());
    }
    let mut previous = initial.clone();
    let mut observations = Vec::new();
    let mut sql_legs = Vec::new();
    let mut pids = std::collections::HashSet::new();
    for (index, point) in POINTS.iter().enumerate() {
        if index == 4 {
            sql_legs = run_sql_legs(&owner, &id, ready, &previous).await?;
        }
        let mut child = command(
            "embedding_result_finalization_crash",
            "finalization_checkpoint_matrix",
        )?;
        child
            .env("VESTRACE_FINALIZATION_POINT", point)
            .env("VESTRACE_FINALIZATION_WORKSPACE", id.workspace.to_string())
            .env("VESTRACE_FINALIZATION_JOB", id.job.to_string())
            .env("VESTRACE_FINALIZATION_EFFECT", id.effect.to_string())
            .env(
                "VESTRACE_FINALIZATION_PREPARATION",
                id.preparation.to_string(),
            );
        let child = child.spawn().map_err(|e| e.to_string())?;
        let pid = child.id().ok_or_else(|| "child omitted PID".to_owned())?;
        let output = child.wait_with_output().await.map_err(|e| e.to_string())?;
        if output.status.success()
            || !String::from_utf8_lossy(&output.stderr)
                .lines()
                .any(|line| line == format!("{MARKER} point={point} pid={pid}"))
        {
            return Err(format!(
                "finalization child failed before actual abort checkpoint {point}"
            ));
        }
        pids.insert(pid);
        let state = snapshot(&owner, &id).await?;
        let bindings = array_len(&state, "bindings")?;
        let published = array_len(&state, "publications")?;
        let host_bound = host_bound_count(&owner, &id).await?;
        let expected_bindings = [0, 1, 2, 2, 2][index];
        let expected_host = [1, 1, 2, 2, 2][index];
        if bindings != expected_bindings
            || host_bound != expected_host
            || state["history"] != initial["history"]
        {
            return Err(format!("durable binding/history mismatch at {point}"));
        }
        if index < 4 {
            if published != 0
                || array_len(&state, "events")? != 0
                || array_len(&state, "attachments")? != 2
                || state["job"]["state"] != "running"
                || state["corpus"] != initial["corpus"]
                || state["generation"] != initial["generation"]
                || state["ready_generations"] != initial["ready_generations"]
                || state["materials"] != initial["materials"]
                || state["projections"] != initial["projections"]
                || state["blockers"] != initial["blockers"]
            {
                return Err(format!("partial publication escaped at {point}"));
            }
            if index == 3 && state != previous {
                return Err("aborted publication SQL legs changed durable state".into());
            }
        } else if published != 1
            || array_len(&state, "events")? != 1
            || array_len(&state, "attachments")? != 0
            || state["job"]["state"] != "succeeded"
            || state["job"]["version"].as_u64()
                != initial["job"]["version"]
                    .as_u64()
                    .and_then(|n| n.checked_add(1))
            || state["corpus"]["corpus_revision"].as_u64()
                != initial["corpus"]["corpus_revision"]
                    .as_u64()
                    .and_then(|n| n.checked_add(1))
            || state["corpus"]["live_member_count"].as_u64()
                != initial["corpus"]["live_member_count"]
                    .as_u64()
                    .and_then(|n| n.checked_add(2))
            || state["generation"]["generation_epoch"].as_u64()
                != initial["generation"]["generation_epoch"]
                    .as_u64()
                    .and_then(|n| n.checked_add(1))
            || !state["intents"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["state"] == "live")
            || !state["blockers"]
                .as_array()
                .unwrap()
                .iter()
                .all(|b| b["state"] == "terminal")
            || !state["ready_generations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|g| g["state"] == "stale")
            || !state["materials"]
                .as_array()
                .unwrap()
                .iter()
                .all(|m| m["state"] == "live")
            || !state["projections"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["state"] == "live")
        {
            return Err("committed publication incomplete after child death".into());
        }
        observations.push(serde_json::json!({"point":point,"pid":pid,"aborted":true,"host_bound":host_bound,"sql_bindings":bindings,"publications":published,"sql_rollback_exact":index==3}));
        previous = state;
    }
    let runtime = dispatch::connect_runtime(&owner).await?;
    let principal:Uuid=sqlx::query_scalar("SELECT c.principal_id FROM connections c JOIN model_binding_snapshots s ON s.connection_id=c.id AND s.workspace_id=c.workspace_id JOIN embedding_jobs j ON j.model_binding_snapshot_id=s.id AND j.workspace_id=s.workspace_id WHERE j.workspace_id=$1 AND j.id=$2").bind(id.workspace).bind(id.job).fetch_one(&owner).await.map_err(dispatch::sql)?;
    let context = RequestContext::new(
        WorkspaceId::from_uuid(id.workspace),
        PrincipalId::from_uuid(principal),
    );
    let authority = EmbeddingResultFinalizationAuthority {
        preparation_id: EmbeddingResultPreparationId::from_uuid(id.preparation),
        job_id: EmbeddingJobId::from_uuid(id.job),
        effect_id: ExternalEffectId::from_uuid(id.effect),
    };
    let publication = EmbeddingResultFinalizationService::new(
        Arc::new(PgEmbeddingResultFinalizationRepository::new(
            PgStore::from_pool(runtime),
        )),
        Arc::new(NeverVault),
        Arc::new(EmbeddingOutputHmacCommitter::new()),
    )
    .finalize(&context, &authority)
    .await
    .map_err(|e| e.to_string())?;
    if publication.output_count != 2
        || snapshot(&owner, &id).await? != previous
        || listener.count.load(Ordering::SeqCst) != 1
    {
        return Err("published replay changed state or repeated provider call".into());
    }
    Ok(serde_json::json!({"scenario":"embedding_result_finalization_crash","point":"finalization_checkpoint_matrix","proved":true,"loopback_requests":listener.count.load(Ordering::SeqCst),"checkpoints":observations,"sql_legs":sql_legs,"distinct_child_pids":pids.len(),"published_replay_without_vault":true,"output_count":publication.output_count}).to_string())
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
            .map_err(|e| e.to_string())?;
        let url = format!(
            "http://{}/embeddings",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut bytes = [0; 1024];
                let _ = stream.read(&mut bytes).await;
                observed.fetch_add(1, Ordering::SeqCst);
                let _ = stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                    .await;
            }
        });
        Ok(Self { url, count, task })
    }
}
impl Drop for LoopbackCounter {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn prepare_ready_generation(runtime: &sqlx::PgPool, id: &Identity) -> Result<Uuid, String> {
    let mut tx = runtime.begin().await.map_err(dispatch::sql)?;
    dispatch::set_workspace(&mut tx, id.workspace).await?;
    let space:Uuid=sqlx::query_scalar("SELECT space_registration_id FROM embedding_job_result_preparations WHERE id=$1 AND workspace_id=$2").bind(id.preparation).bind(id.workspace).fetch_one(&mut *tx).await.map_err(dispatch::sql)?;
    let ready: Uuid =
        sqlx::query_scalar("SELECT vestrace_open_embedding_corpus_generation($1,$2,$3)")
            .bind(Uuid::now_v7())
            .bind(id.workspace)
            .bind(space)
            .fetch_one(&mut *tx)
            .await
            .map_err(dispatch::sql)?;
    sqlx::query("SELECT vestrace_publish_embedding_corpus_generation($1,$2,$3,0::BIGINT)")
        .bind(ready)
        .bind(id.workspace)
        .bind(space)
        .execute(&mut *tx)
        .await
        .map_err(dispatch::sql)?;
    tx.commit().await.map_err(dispatch::sql)?;
    Ok(ready)
}
struct SqlLeg {
    name: String,
    table: &'static str,
    event: &'static str,
    key: &'static str,
    value: Uuid,
    changed_key: &'static str,
    changed_value: String,
}
fn leg(
    name: impl Into<String>,
    table: &'static str,
    event: &'static str,
    key: &'static str,
    value: Uuid,
    changed_key: &'static str,
    changed_value: impl Into<String>,
) -> SqlLeg {
    SqlLeg {
        name: name.into(),
        table,
        event,
        key,
        value,
        changed_key,
        changed_value: changed_value.into(),
    }
}
async fn run_sql_legs(
    owner: &sqlx::PgPool,
    id: &Identity,
    ready: Uuid,
    baseline: &serde_json::Value,
) -> Result<Vec<serde_json::Value>, String> {
    let space: Uuid = sqlx::query_scalar(
        "SELECT space_registration_id FROM embedding_job_result_preparations WHERE id=$1",
    )
    .bind(id.preparation)
    .fetch_one(owner)
    .await
    .map_err(dispatch::sql)?;
    let outputs:Vec<(i64,Uuid,Uuid,Uuid,Uuid)>=sqlx::query_as("SELECT p.output_ordinal,p.material_id,p.intent_id,p.id,h.prepared_attachment_id FROM embedding_projection_entries p JOIN embedding_job_result_prepared_attachments h ON h.preparation_id=p.preparation_id AND h.projection_id=p.id WHERE p.preparation_id=$1 ORDER BY p.output_ordinal").bind(id.preparation).fetch_all(owner).await.map_err(dispatch::sql)?;
    if outputs.len() != 2 {
        return Err("SQL leg proof requires both exact outputs".into());
    }
    let mut legs = vec![
        leg(
            "publication_insert",
            "embedding_job_result_publications",
            "INSERT",
            "preparation_id",
            id.preparation,
            "preparation_id",
            id.preparation.to_string(),
        ),
        leg(
            "event_insert",
            "embedding_index_rebuild_events",
            "INSERT",
            "space_registration_id",
            space,
            "space_registration_id",
            space.to_string(),
        ),
    ];
    for (ordinal, material, intent, projection, attachment) in outputs {
        legs.push(leg(
            format!("material_{ordinal}"),
            "content_materials",
            "UPDATE",
            "id",
            material,
            "state",
            "live",
        ));
        legs.push(leg(
            format!("attachment_{ordinal}"),
            "prepared_material_attachments",
            "DELETE",
            "id",
            attachment,
            "id",
            attachment.to_string(),
        ));
        legs.push(leg(
            format!("intent_{ordinal}"),
            "material_key_creation_intents",
            "UPDATE",
            "id",
            intent,
            "state",
            "live",
        ));
        legs.push(leg(
            format!("projection_{ordinal}"),
            "embedding_projection_entries",
            "UPDATE",
            "id",
            projection,
            "state",
            "live",
        ));
    }
    let next_revision = baseline["corpus"]["corpus_revision"]
        .as_u64()
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| "corpus revision malformed".to_owned())?;
    let next_epoch = baseline["generation"]["generation_epoch"]
        .as_u64()
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| "generation epoch malformed".to_owned())?;
    legs.push(leg(
        "corpus_update",
        "embedding_space_corpus_states",
        "UPDATE",
        "space_registration_id",
        space,
        "corpus_revision",
        next_revision.to_string(),
    ));
    legs.push(leg(
        "generation_update",
        "embedding_index_generation_guards",
        "UPDATE",
        "space_registration_id",
        space,
        "generation_epoch",
        next_epoch.to_string(),
    ));
    legs.push(leg(
        "ready_stale",
        "embedding_corpus_generations",
        "UPDATE",
        "id",
        ready,
        "state",
        "stale",
    ));
    let blockers:Vec<Uuid>=sqlx::query_scalar("SELECT blocker_id FROM embedding_delivery_source_memberships WHERE workspace_id=$1 AND job_id=$2 ORDER BY blocker_id").bind(id.workspace).bind(id.job).fetch_all(owner).await.map_err(dispatch::sql)?;
    for (index, blocker) in blockers.into_iter().enumerate() {
        legs.push(leg(
            format!("blocker_{index}"),
            "material_erasure_blockers",
            "UPDATE",
            "id",
            blocker,
            "state",
            "terminal",
        ));
    }
    legs.push(leg(
        "job_succeeded",
        "embedding_jobs",
        "UPDATE",
        "id",
        id.job,
        "state",
        "succeeded",
    ));
    let suffix = Uuid::now_v7().simple().to_string();
    let function = format!("vestrace_fault_14e_{suffix}");
    // Ephemeral observer only: no production function, grants or mutation body
    // changes. An AFTER trigger proves the exact row mutation already ran.
    let ddl = format!(
        "CREATE FUNCTION {function}() RETURNS TRIGGER LANGUAGE plpgsql AS $fault$ DECLARE row_value JSONB; BEGIN \
        IF current_setting('vestrace.fault_finalization_stage',true) IS DISTINCT FROM TG_ARGV[0] THEN RETURN NULL; END IF; \
        IF TG_OP='DELETE' THEN row_value:=to_jsonb(OLD); ELSE row_value:=to_jsonb(NEW); END IF; \
        IF row_value->>'workspace_id' IS DISTINCT FROM TG_ARGV[1] OR row_value->>TG_ARGV[2] IS DISTINCT FROM TG_ARGV[3] THEN RETURN NULL; END IF; \
        IF row_value->>TG_ARGV[4] IS DISTINCT FROM TG_ARGV[5] THEN RAISE EXCEPTION 'fault observer did not see expected changed row'; END IF; \
        PERFORM pg_advisory_xact_lock(904195,TG_ARGV[6]::INTEGER); RETURN NULL; END $fault$; \
        REVOKE ALL ON FUNCTION {function}() FROM PUBLIC;"
    );
    sqlx::raw_sql(&ddl)
        .execute(owner)
        .await
        .map_err(dispatch::sql)?;
    let mut observations = Vec::new();
    for (index, stage) in legs.iter().enumerate() {
        let lock_id = i32::try_from(index + 1).map_err(|_| "too many SQL stages".to_owned())?;
        let trigger = format!("fault_14e_{suffix}_{index}");
        // Every identifier is a fixed table/name or a generated UUID; values
        // are UUIDs, fixed labels or checked integers, never external SQL text.
        let install = format!(
            "CREATE TRIGGER {trigger} AFTER {} ON {} FOR EACH ROW EXECUTE FUNCTION {function}('{}','{}','{}','{}','{}','{}','{}')",
            stage.event,
            stage.table,
            stage.name,
            id.workspace,
            stage.key,
            stage.value,
            stage.changed_key,
            stage.changed_value,
            lock_id
        );
        sqlx::raw_sql(&install)
            .execute(owner)
            .await
            .map_err(dispatch::sql)?;
        let mut gate = owner.begin().await.map_err(dispatch::sql)?;
        let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *gate)
            .await
            .map_err(dispatch::sql)?;
        sqlx::query("SELECT pg_advisory_xact_lock(904195,$1)")
            .bind(lock_id)
            .execute(&mut *gate)
            .await
            .map_err(dispatch::sql)?;
        let mut child = command(
            "embedding_result_finalization_crash",
            "finalization_checkpoint_matrix",
        )?;
        child
            .env("VESTRACE_FINALIZATION_POINT", "sql_leg")
            .env("VESTRACE_FINALIZATION_SQL_STAGE", &stage.name)
            .env("VESTRACE_FINALIZATION_WORKSPACE", id.workspace.to_string())
            .env("VESTRACE_FINALIZATION_JOB", id.job.to_string())
            .env("VESTRACE_FINALIZATION_EFFECT", id.effect.to_string())
            .env(
                "VESTRACE_FINALIZATION_PREPARATION",
                id.preparation.to_string(),
            );
        let mut child = child.spawn().map_err(|e| e.to_string())?;
        let pid = child.id().ok_or_else(|| "child PID absent".to_owned())?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let backend = loop {
            let observed:Option<i32>=sqlx::query_scalar("SELECT a.pid FROM pg_stat_activity a JOIN pg_locks l ON l.pid=a.pid WHERE a.application_name=$1 AND l.locktype='advisory' AND l.classid=904195::OID AND l.objid=$2::OID AND l.objsubid=2 AND NOT l.granted AND $3=ANY(pg_blocking_pids(a.pid))")
                .bind(format!("14e-fault-{}",stage.name)).bind(lock_id).bind(holder).fetch_optional(owner).await.map_err(dispatch::sql)?;
            if let Some(backend) = observed {
                break backend;
            }
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                gate.rollback().await.map_err(dispatch::sql)?;
                return Err(format!(
                    "child exited before SQL AFTER-row barrier {}",
                    stage.name
                ));
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill().await;
                gate.rollback().await.map_err(dispatch::sql)?;
                return Err(format!("SQL row barrier was not observed: {}", stage.name));
            }
            tokio::task::yield_now().await;
        };
        // The exact AFTER-row observer is waiting; kill the OS process while
        // it still cannot receive its SQL result or issue COMMIT.
        child.kill().await.map_err(|e| e.to_string())?;
        let output = child.wait_with_output().await.map_err(|e| e.to_string())?;
        if output.status.success() {
            return Err("SQL-stage child was not killed".into());
        }
        gate.rollback().await.map_err(dispatch::sql)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1)")
                    .bind(backend)
                    .fetch_one(owner)
                    .await
                    .map_err(dispatch::sql)?;
            if !exists {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err("killed child's database backend did not exit".into());
            }
            tokio::task::yield_now().await;
        }
        if snapshot(owner, id).await? != *baseline {
            return Err(format!("SQL stage {} escaped rollback", stage.name));
        }
        if host_bound_count(owner, id).await? != 2 {
            return Err("SQL abort lost bound host keys".into());
        }
        sqlx::raw_sql(&format!("DROP TRIGGER {trigger} ON {}", stage.table))
            .execute(owner)
            .await
            .map_err(dispatch::sql)?;
        observations.push(serde_json::json!({"stage":stage.name,"pid":pid,"backend_pid":backend,"after_row_observed":true,"child_killed":true,"backend_ended":true,"rollback_exact":true}));
    }
    sqlx::raw_sql(&format!("DROP FUNCTION {function}()"))
        .execute(owner)
        .await
        .map_err(dispatch::sql)?;
    Ok(observations)
}
