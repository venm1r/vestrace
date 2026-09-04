use std::{
    io::{Read, Write},
    net::TcpListener,
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{
    ConnectionAuth, ConnectionKind, EffectiveModelRequest, EffectiveModelResponse,
    MaterialKeyVault, ModelRequestEvidenceRepository, ModelRequestReconstruction, RequestContext,
    TransactionManager, VaultError,
};
use vestrace_domain::{
    ContentMaterialId, ErasureReceipt, IntentNonce, MaterialKeyId, PrincipalId, VaultReceipt,
    WorkspaceId, ZeroizingDek,
};
use vestrace_infrastructure::{
    PgModelRequestEvidenceRepository, PgStore, PgTransactionManager,
    crypto::{ContentMaterialCodec, ContentMaterialCodecError, MAX_FRAMED_MATERIAL_BYTES},
    providers::OpenAiCompatibleClient,
};

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var(RUNTIME_DATABASE_URL_ENV)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let _parsed = PgConnectOptions::from_str(&runtime_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .expect("runtime URL must contain a password");

    PgPoolOptions::new()
        .max_connections(4)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username("vestrace")
                .password(password),
        )
        .await
        .expect("the restricted runtime role must connect to the SQLx database")
}

#[derive(Clone)]
struct ProbeNodes {
    kinds: Vec<String>,
    ids: Vec<Uuid>,
    versions: Vec<Option<i64>>,
    safe_ordinals: Vec<Option<String>>,
}

impl ProbeNodes {
    fn remove(&mut self, index: usize) {
        self.kinds.remove(index);
        self.ids.remove(index);
        self.versions.remove(index);
        self.safe_ordinals.remove(index);
    }

    fn push(&mut self, kind: &str, id: Uuid, version: Option<i64>, safe_ordinal: Option<&str>) {
        self.kinds.push(kind.to_owned());
        self.ids.push(id);
        self.versions.push(version);
        self.safe_ordinals.push(safe_ordinal.map(str::to_owned));
    }
}

#[derive(Clone, Copy)]
struct ProbeCanonical {
    shape_id: Uuid,
    limits_id: Uuid,
}

async fn create_probe_canonical(runtime: &PgPool, fixture: &ProbeFixture) -> ProbeCanonical {
    let canonical = ProbeCanonical {
        shape_id: Uuid::now_v7(),
        limits_id: Uuid::now_v7(),
    };
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_shape_revision(
            $1,$2,1,'models_list',false,ARRAY[]::TEXT[]
        )",
    )
    .bind(canonical.shape_id)
    .bind(fixture.workspace_id.as_uuid())
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_limits_revision($1,$2,1,1,1,1)")
        .bind(canonical.limits_id)
        .bind(fixture.workspace_id.as_uuid())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    canonical
}

async fn create_probe_limits(runtime: &PgPool, fixture: &ProbeFixture, limits_id: Uuid) {
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_limits_revision($1,$2,1,1,1,1)")
        .bind(limits_id)
        .bind(fixture.workspace_id.as_uuid())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

fn probe_nodes(fixture: &ProbeFixture, canonical: ProbeCanonical) -> ProbeNodes {
    ProbeNodes {
        kinds: vec![
            "external_effect".into(),
            "connection_revision".into(),
            "qualification_target".into(),
            "qualification_probe".into(),
            "request_shape_revision".into(),
            "limits_revision".into(),
        ],
        ids: vec![
            fixture.external_effect_id,
            fixture.connection_revision_id,
            fixture.qualification_target_id,
            fixture.qualification_job_id,
            canonical.shape_id,
            canonical.limits_id,
        ],
        versions: vec![None, None, None, None, Some(1), Some(1)],
        safe_ordinals: vec![None, None, None, Some("00".into()), None, None],
    }
}

async fn call_probe_creator(
    runtime: &PgPool,
    fixture: &ProbeFixture,
    root_id: Uuid,
    request_kind: &str,
    nodes: &ProbeNodes,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await?;
    let result = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_evidence(
            $1,$2,$3,$4,NULL,$5,'qualification_probe',$6,$7,$8,$9,$10
        )",
    )
    .bind(root_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .bind(request_kind)
    .bind(fixture.qualification_target_id)
    .bind(fixture.qualification_job_id)
    .bind(&nodes.kinds)
    .bind(&nodes.ids)
    .bind(&nodes.versions)
    .bind(&nodes.safe_ordinals)
    .fetch_one(&mut *transaction)
    .await;
    match result {
        Ok(root) => {
            transaction.commit().await?;
            Ok(root)
        }
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

fn assert_sqlstate(error: &sqlx::Error, expected: &str) {
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some(expected),
        "{error}"
    );
}

#[derive(Clone, Default)]
struct AdapterObserver(Arc<Mutex<Vec<serde_json::Value>>>);

fn chat_adapter_and_observer() -> (OpenAiCompatibleClient, AdapterObserver) {
    let observer = AdapterObserver::default();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server_observer = observer.clone();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0, "adapter request ended before its headers");
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let header_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap()
            + 4;
        let headers = std::str::from_utf8(&request[..header_end]).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
            })
            .unwrap();
        while request.len() - header_end < content_length {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0, "adapter request ended before its body");
            request.extend_from_slice(&buffer[..read]);
        }
        let body =
            serde_json::from_slice(&request[header_end..header_end + content_length]).unwrap();
        server_observer.0.lock().unwrap().push(body);
        let response = serde_json::to_vec(&serde_json::json!({
            "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        }))
        .unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            response.len()
        )
        .unwrap();
        stream.write_all(&response).unwrap();
        stream.flush().unwrap();
    });
    (
        OpenAiCompatibleClient::for_connection(
            ConnectionKind::LMStudioLocal,
            format!("http://{address}"),
            ConnectionAuth::None,
        )
        .unwrap(),
        observer,
    )
}

#[test]
fn content_material_codec_round_trips_exact_revision_aad() {
    let codec = ContentMaterialCodec::new();
    let workspace_id = WorkspaceId::new();
    let material_id = ContentMaterialId::new();
    let material_key_id = MaterialKeyId::new();
    let dek = ZeroizingDek::new([0x31; 32]);
    let plaintext = zeroize::Zeroizing::new(b"governed semantic input".to_vec());

    let frame = codec
        .seal(workspace_id, material_id, material_key_id, &dek, &plaintext)
        .expect("a bounded governed input must seal");
    let opened = codec
        .open(workspace_id, material_id, material_key_id, &dek, &frame)
        .expect("the exact revision tuple must open");

    assert_eq!(opened.as_slice(), b"governed semantic input");
    assert!(frame.len() <= MAX_FRAMED_MATERIAL_BYTES);
    assert!(!frame.windows(8).any(|window| window == b"governed"));
}

#[test]
fn plaintext_error_intermediates_have_structural_zeroizing_ownership() {
    let repository = include_str!("../src/postgres/model_request_evidence_repository.rs");
    let codec = include_str!("../src/crypto/content_material_codec.rs");
    assert!(
        repository.contains(
            "fn zeroizing_utf8(mut bytes: Zeroizing<Vec<u8>>) -> Result<Zeroizing<String>"
        )
    );
    assert!(repository.contains("Zeroizing::new(error.into_bytes())"));
    assert!(codec.contains("Result<Zeroizing<Vec<u8>>, ContentMaterialCodecError>"));
    assert!(codec.contains("let mut encrypted = Zeroizing::new("));
}

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
        panic!("models-list reconstruction must not unwrap material")
    }

    fn prepare_erasure(
        &self,
        _key_id: MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    fn erase(&self, _key_id: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }
}

struct CountingVault {
    key: [u8; 32],
    unwraps: AtomicUsize,
}

impl CountingVault {
    fn new(key: [u8; 32]) -> Self {
        Self {
            key,
            unwraps: AtomicUsize::new(0),
        }
    }

    fn unwrap_count(&self) -> usize {
        self.unwraps.load(Ordering::SeqCst)
    }
}

impl MaterialKeyVault for CountingVault {
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
        use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        self.unwraps.fetch_add(1, Ordering::SeqCst);
        let dek = ZeroizingDek::new(self.key);
        use_dek(&dek);
        Ok(())
    }

    fn prepare_erasure(
        &self,
        _key_id: MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    fn erase(&self, _key_id: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }
}

#[derive(Clone, Copy)]
struct LiveMaterial {
    id: ContentMaterialId,
    key_id: MaterialKeyId,
    intent_id: Uuid,
}

struct ProbeFixture {
    workspace_id: WorkspaceId,
    principal_id: PrincipalId,
    connection_revision_id: Uuid,
    connection_id: Uuid,
    guard_id: Uuid,
    no_auth_id: Uuid,
    qualification_target_id: Uuid,
    qualification_job_id: Uuid,
    external_effect_id: Uuid,
}

async fn probe_fixture(pool: &PgPool) -> ProbeFixture {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let guard_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let no_auth_id = Uuid::now_v7();
    let qualification_job_id = Uuid::now_v7();
    let qualification_target_id = Uuid::now_v7();
    let external_effect_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1,$2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("mre-{workspace_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1,$2,$3)")
        .bind(principal_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(format!("principal-{principal_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1,$2,$3,'local')",
    )
    .bind(connector_id)
    .bind(workspace_id.as_uuid())
    .bind(format!("connector-{connector_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections
         (id, connector_id, workspace_id, principal_id, name, status)
         VALUES ($1,$2,$3,$4,$5,'active')",
    )
    .bind(connection_id)
    .bind(connector_id)
    .bind(workspace_id.as_uuid())
    .bind(principal_id.as_uuid())
    .bind(format!("connection-{connection_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_intents (id, workspace_id, adapter, payload)
         VALUES ($1,$2,'openai-compatible',$3)",
    )
    .bind(external_effect_id)
    .bind(workspace_id.as_uuid())
    .bind(serde_json::json!({}))
    .execute(pool)
    .await
    .unwrap();

    let mut transaction = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(guard_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_connection_revision_and_advance_head(
            $1,$2,$3,$4,'lm_studio_local','http://127.0.0.1:1234/v1',
            'http://127.0.0.1:1234/v1','lm-studio-local/v1','loopback_only','none',NULL,0
        )",
    )
    .bind(connection_revision_id)
    .bind(workspace_id.as_uuid())
    .bind(connection_id)
    .bind(guard_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)")
        .bind(no_auth_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .bind(connection_revision_id)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO qualification_jobs
         (id, workspace_id, connection_revision_id, profile_revision, state)
         VALUES ($1,$2,$3,'openai-chat-completions-v1/q1','running')",
    )
    .bind(qualification_job_id)
    .bind(workspace_id.as_uuid())
    .bind(connection_revision_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO qualification_target_bindings
         (id, workspace_id, qualification_job_id, connection_id,
          connection_revision_id, branch, no_auth_binding_revision_id)
         VALUES ($1,$2,$3,$4,$5,'no_auth',$6)",
    )
    .bind(qualification_target_id)
    .bind(workspace_id.as_uuid())
    .bind(qualification_job_id)
    .bind(connection_id)
    .bind(connection_revision_id)
    .bind(no_auth_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    ProbeFixture {
        workspace_id,
        principal_id,
        connection_revision_id,
        connection_id,
        guard_id,
        no_auth_id,
        qualification_target_id,
        qualification_job_id,
        external_effect_id,
    }
}

async fn create_chat_model(
    pool: &PgPool,
    runtime: &PgPool,
    fixture: &ProbeFixture,
    wire_model_id: &str,
) -> Uuid {
    create_model(pool, runtime, fixture, wire_model_id, "chat").await
}

async fn create_embedding_model(
    pool: &PgPool,
    runtime: &PgPool,
    fixture: &ProbeFixture,
    wire_model_id: &str,
) -> Uuid {
    create_model(pool, runtime, fixture, wire_model_id, "embedding").await
}

async fn create_model(
    pool: &PgPool,
    runtime: &PgPool,
    fixture: &ProbeFixture,
    wire_model_id: &str,
    kind: &str,
) -> Uuid {
    let provider_id = Uuid::now_v7();
    let model_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    sqlx::query("INSERT INTO providers (id,workspace_id,name,locality) VALUES ($1,$2,$3,'local')")
        .bind(provider_id)
        .bind(fixture.workspace_id.as_uuid())
        .bind(format!("provider-{provider_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO models
         (id,provider_id,workspace_id,model_name,context_window,input_cost_per_mtoken,output_cost_per_mtoken)
         VALUES ($1,$2,$3,$4,8192,0,0)",
    )
    .bind(model_id)
    .bind(provider_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(wire_model_id)
    .execute(pool)
    .await
    .unwrap();
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_revision_and_advance_head(
            $1,$2,$3,$4,$5,$6,$7,$8,NULL,NULL,NULL,NULL,NULL,NULL,0
        )",
    )
    .bind(revision_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(model_id)
    .bind(fixture.connection_id)
    .bind(fixture.guard_id)
    .bind(fixture.connection_revision_id)
    .bind(wire_model_id)
    .bind(kind)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    revision_id
}

#[derive(Clone, Copy)]
struct RunStepTuple {
    snapshot_id: Uuid,
    connection_qualification_id: Uuid,
    model_qualification_id: Uuid,
    cause_id: Uuid,
}

async fn create_run_step_tuple(
    pool: &PgPool,
    fixture: &ProbeFixture,
    model_revision_id: Uuid,
) -> RunStepTuple {
    create_run_step_tuple_with_capability(pool, fixture, model_revision_id, "chat").await
}

async fn create_run_step_tuple_with_capability(
    pool: &PgPool,
    fixture: &ProbeFixture,
    model_revision_id: Uuid,
    capability: &str,
) -> RunStepTuple {
    let tuple = RunStepTuple {
        snapshot_id: Uuid::now_v7(),
        connection_qualification_id: Uuid::now_v7(),
        model_qualification_id: Uuid::now_v7(),
        cause_id: Uuid::now_v7(),
    };
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_revisions
         (id,workspace_id,connection_revision_id,qualification_job_id,profile_revision,valid_until,capabilities)
         VALUES ($1,$2,$3,$4,'openai-chat-completions-v1/q1',NOW()+INTERVAL '1 hour',$5)",
    )
    .bind(tuple.connection_qualification_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(fixture.connection_revision_id)
    .bind(fixture.qualification_job_id)
    .bind(vec![capability])
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_qualification_revisions
         (id,workspace_id,model_revision_id,connection_revision_id,
          connection_qualification_revision_id,qualification_job_id,capabilities,valid_until)
         VALUES ($1,$2,$3,$4,$5,$6,$7,NOW()+INTERVAL '1 hour')",
    )
    .bind(tuple.model_qualification_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(model_revision_id)
    .bind(fixture.connection_revision_id)
    .bind(tuple.connection_qualification_id)
    .bind(fixture.qualification_job_id)
    .bind(vec![capability])
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_binding_snapshots
         (id,workspace_id,connection_id,connection_revision_id,
          connection_qualification_revision_id,model_revision_id,
          model_qualification_revision_id,branch,no_auth_binding_revision_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,'no_auth',$8)",
    )
    .bind(tuple.snapshot_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(tuple.connection_qualification_id)
    .bind(model_revision_id)
    .bind(tuple.model_qualification_id)
    .bind(fixture.no_auth_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    tuple
}

async fn create_live_material(
    runtime: &PgPool,
    fixture: &ProbeFixture,
    plaintext: &[u8],
    output_ordinal: i64,
    key: [u8; 32],
) -> LiveMaterial {
    let material = LiveMaterial {
        id: ContentMaterialId::new(),
        key_id: MaterialKeyId::new(),
        intent_id: Uuid::now_v7(),
    };
    let frame = ContentMaterialCodec::new()
        .seal(
            fixture.workspace_id,
            material.id,
            material.key_id,
            &ZeroizingDek::new(key),
            plaintext,
        )
        .unwrap();
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "SELECT vestrace_reserve_material_key_creation_intent(
            $1,$2,$3,$4,$5,'model_request_input',$6,$7
        )",
    )
    .bind(material.intent_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(material.id.as_uuid())
    .bind(material.key_id.as_uuid())
    .bind(Uuid::now_v7())
    .bind(fixture.qualification_job_id)
    .bind(output_ordinal)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
        .bind(material.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
        .bind(material.intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_prepare_content_material($1,$2,$3,$4)")
        .bind(material.intent_id)
        .bind(Uuid::now_v7())
        .bind(&frame)
        .bind(frame.len() as i64)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
        .bind(material.intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
        .bind(material.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    material
}

async fn replace_live_material_frame(
    pool: &PgPool,
    fixture: &ProbeFixture,
    material: LiveMaterial,
    frame: &[u8],
) {
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("UPDATE content_material_bytes SET ciphertext=$1 WHERE material_id=$2")
        .bind(frame)
        .bind(material.id.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("UPDATE content_materials SET size_class=$1 WHERE id=$2")
        .bind(frame.len() as i64)
        .bind(material.id.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

fn authenticated_bad_padding_frame(
    workspace_id: WorkspaceId,
    material: LiveMaterial,
    key: [u8; 32],
) -> Vec<u8> {
    const HEADER_BYTES: usize = 17;
    let codec = ContentMaterialCodec::new();
    let mut frame = codec
        .seal(
            workspace_id,
            material.id,
            material.key_id,
            &ZeroizingDek::new(key),
            b"padding fixture",
        )
        .unwrap();
    let nonce_bytes: [u8; 12] = frame[5..HEADER_BYTES].try_into().unwrap();
    let aad = format!(
        "vestrace-content-material-aead-v1|{}|{}|{}",
        workspace_id.as_uuid(),
        material.id.as_uuid(),
        material.key_id.as_uuid()
    );
    let mut encrypted = frame[HEADER_BYTES..].to_vec();
    let key_handle = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &key).unwrap());
    let plaintext_len = key_handle
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(aad.as_bytes()),
            &mut encrypted,
        )
        .unwrap()
        .len();
    encrypted.truncate(plaintext_len);
    *encrypted.last_mut().unwrap() = 1;
    key_handle
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(aad.as_bytes()),
            &mut encrypted,
        )
        .unwrap();
    frame.truncate(HEADER_BYTES);
    frame.extend_from_slice(&encrypted);
    frame
}

fn chat_creation(
    fixture: &ProbeFixture,
    model_revision_id: Uuid,
    materials: &[LiveMaterial],
    wire_model_id: &str,
) -> vestrace_application::CreateModelRequestEvidence {
    chat_creation_with_limit(fixture, model_revision_id, materials, wire_model_id, 65_536)
}

fn chat_creation_with_limit(
    fixture: &ProbeFixture,
    model_revision_id: Uuid,
    materials: &[LiveMaterial],
    wire_model_id: &str,
    max_input_bytes: u32,
) -> vestrace_application::CreateModelRequestEvidence {
    let shape_id = Uuid::now_v7();
    let sampling_id = Uuid::now_v7();
    let limits_id = Uuid::now_v7();
    let mut nodes = vec![
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::ExternalEffect,
            reference_id: fixture.external_effect_id,
            reference_version: None,
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::ConnectionRevision,
            reference_id: fixture.connection_revision_id,
            reference_version: None,
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::QualificationTarget,
            reference_id: fixture.qualification_target_id,
            reference_version: None,
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::QualificationProbe,
            reference_id: fixture.qualification_job_id,
            reference_version: None,
            safe_ordinal: Some("00".into()),
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::ModelRevision,
            reference_id: model_revision_id,
            reference_version: None,
            safe_ordinal: None,
        },
    ];
    nodes.extend(materials.iter().enumerate().map(|(ordinal, material)| {
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::GovernedInputMaterial,
            reference_id: material.id.as_uuid(),
            reference_version: None,
            safe_ordinal: Some(ordinal.to_string()),
        }
    }));
    nodes.extend([
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::RequestShapeRevision,
            reference_id: shape_id,
            reference_version: Some(1),
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::SamplingRevision,
            reference_id: sampling_id,
            reference_version: Some(1),
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::LimitsRevision,
            reference_id: limits_id,
            reference_version: Some(1),
            safe_ordinal: None,
        },
    ]);
    vestrace_application::CreateModelRequestEvidence::new(
        vestrace_application::ModelRequestEvidenceIdentity {
            root_id: Uuid::now_v7(),
            workspace_id: fixture.workspace_id,
            external_effect_id: fixture.external_effect_id,
            cause_kind: vestrace_application::ModelRequestCauseKind::QualificationProbe,
            cause_id: fixture.qualification_job_id,
            binding_snapshot_id: None,
            qualification_target_binding_id: Some(fixture.qualification_target_id),
        },
        vestrace_application::CanonicalRequestRevisions {
            request_shape: vestrace_application::RequestShapeRevisionInput {
                id: shape_id,
                version: 1,
                request_kind: vestrace_application::EffectiveRequestKind::ChatCompletions,
                stream: false,
                input_roles: vec![vestrace_application::EffectiveChatRole::User; materials.len()],
            },
            sampling: Some(vestrace_application::SamplingRevisionInput {
                id: sampling_id,
                version: 1,
                sampling: vestrace_application::EffectiveSampling::new(0.2, 0.9).unwrap(),
            }),
            limits: vestrace_application::LimitsRevisionInput {
                id: limits_id,
                version: 1,
                limits: vestrace_application::EffectiveRequestLimits::new(
                    128,
                    materials.len() as u32,
                    max_input_bytes,
                )
                .unwrap(),
            },
            tools: Vec::new(),
        },
        nodes,
    )
    .unwrap_or_else(|error| panic!("chat evidence for {wire_model_id} must be valid: {error}"))
}

fn embeddings_creation(
    fixture: &ProbeFixture,
    model_revision_id: Uuid,
    materials: &[LiveMaterial],
) -> vestrace_application::CreateModelRequestEvidence {
    let shape_id = Uuid::now_v7();
    let limits_id = Uuid::now_v7();
    let mut nodes = vec![
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::ExternalEffect,
            reference_id: fixture.external_effect_id,
            reference_version: None,
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::ConnectionRevision,
            reference_id: fixture.connection_revision_id,
            reference_version: None,
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::QualificationTarget,
            reference_id: fixture.qualification_target_id,
            reference_version: None,
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::QualificationProbe,
            reference_id: fixture.qualification_job_id,
            reference_version: None,
            safe_ordinal: Some("00".into()),
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::ModelRevision,
            reference_id: model_revision_id,
            reference_version: None,
            safe_ordinal: None,
        },
    ];
    nodes.extend(materials.iter().enumerate().map(|(ordinal, material)| {
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::GovernedInputMaterial,
            reference_id: material.id.as_uuid(),
            reference_version: None,
            safe_ordinal: Some(ordinal.to_string()),
        }
    }));
    nodes.extend([
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::RequestShapeRevision,
            reference_id: shape_id,
            reference_version: Some(1),
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::LimitsRevision,
            reference_id: limits_id,
            reference_version: Some(1),
            safe_ordinal: None,
        },
    ]);
    vestrace_application::CreateModelRequestEvidence::new(
        vestrace_application::ModelRequestEvidenceIdentity {
            root_id: Uuid::now_v7(),
            workspace_id: fixture.workspace_id,
            external_effect_id: fixture.external_effect_id,
            cause_kind: vestrace_application::ModelRequestCauseKind::QualificationProbe,
            cause_id: fixture.qualification_job_id,
            binding_snapshot_id: None,
            qualification_target_binding_id: Some(fixture.qualification_target_id),
        },
        vestrace_application::CanonicalRequestRevisions {
            request_shape: vestrace_application::RequestShapeRevisionInput {
                id: shape_id,
                version: 1,
                request_kind: vestrace_application::EffectiveRequestKind::Embeddings,
                stream: false,
                input_roles: Vec::new(),
            },
            sampling: None,
            limits: vestrace_application::LimitsRevisionInput {
                id: limits_id,
                version: 1,
                limits: vestrace_application::EffectiveRequestLimits::new(
                    1,
                    materials.len() as u32,
                    4 * 1024 * 1024,
                )
                .unwrap(),
            },
            tools: Vec::new(),
        },
        nodes,
    )
    .unwrap()
}

fn run_step_chat_creation(
    fixture: &ProbeFixture,
    tuple: RunStepTuple,
    model_revision_id: Uuid,
    material: LiveMaterial,
) -> vestrace_application::CreateModelRequestEvidence {
    let mut creation = chat_creation(fixture, model_revision_id, &[material], "run-step-model");
    let canonical = creation.canonical().clone();
    let mut nodes: Vec<_> = creation
        .nodes()
        .iter()
        .filter(|node| {
            !matches!(
                node.kind,
                vestrace_application::ModelRequestNodeKind::QualificationTarget
                    | vestrace_application::ModelRequestNodeKind::QualificationProbe
            )
        })
        .cloned()
        .collect();
    nodes.extend([
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::BindingSnapshot,
            reference_id: tuple.snapshot_id,
            reference_version: None,
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::ConnectionQualificationRevision,
            reference_id: tuple.connection_qualification_id,
            reference_version: None,
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::ModelQualificationRevision,
            reference_id: tuple.model_qualification_id,
            reference_version: None,
            safe_ordinal: None,
        },
    ]);
    creation = vestrace_application::CreateModelRequestEvidence::new(
        vestrace_application::ModelRequestEvidenceIdentity {
            root_id: Uuid::now_v7(),
            workspace_id: fixture.workspace_id,
            external_effect_id: fixture.external_effect_id,
            cause_kind: vestrace_application::ModelRequestCauseKind::RunStep,
            cause_id: tuple.cause_id,
            binding_snapshot_id: Some(tuple.snapshot_id),
            qualification_target_binding_id: None,
        },
        canonical,
        nodes,
    )
    .unwrap();
    creation
}

fn run_step_embeddings_creation(
    fixture: &ProbeFixture,
    tuple: RunStepTuple,
    model_revision_id: Uuid,
    materials: &[LiveMaterial],
) -> vestrace_application::CreateModelRequestEvidence {
    let creation = embeddings_creation(fixture, model_revision_id, materials);
    let canonical = creation.canonical().clone();
    let mut nodes: Vec<_> = creation
        .nodes()
        .iter()
        .filter(|node| {
            !matches!(
                node.kind,
                vestrace_application::ModelRequestNodeKind::QualificationTarget
                    | vestrace_application::ModelRequestNodeKind::QualificationProbe
            )
        })
        .cloned()
        .collect();
    nodes.extend([
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::BindingSnapshot,
            reference_id: tuple.snapshot_id,
            reference_version: None,
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::ConnectionQualificationRevision,
            reference_id: tuple.connection_qualification_id,
            reference_version: None,
            safe_ordinal: None,
        },
        vestrace_application::ModelRequestEvidenceNodeInput {
            kind: vestrace_application::ModelRequestNodeKind::ModelQualificationRevision,
            reference_id: tuple.model_qualification_id,
            reference_version: None,
            safe_ordinal: None,
        },
    ]);
    vestrace_application::CreateModelRequestEvidence::new(
        vestrace_application::ModelRequestEvidenceIdentity {
            root_id: Uuid::now_v7(),
            workspace_id: fixture.workspace_id,
            external_effect_id: fixture.external_effect_id,
            cause_kind: vestrace_application::ModelRequestCauseKind::RunStep,
            cause_id: tuple.cause_id,
            binding_snapshot_id: Some(tuple.snapshot_id),
            qualification_target_binding_id: None,
        },
        canonical,
        nodes,
    )
    .unwrap()
}

fn chat_creation_with_tool(
    fixture: &ProbeFixture,
    model_revision_id: Uuid,
    material: LiveMaterial,
    tool_id: Uuid,
) -> vestrace_application::CreateModelRequestEvidence {
    let creation = chat_creation(
        fixture,
        model_revision_id,
        &[material],
        "retained-tool-model",
    );
    let mut canonical = creation.canonical().clone();
    canonical
        .tools
        .push(vestrace_application::ToolSchemaRevisionInput {
            id: tool_id,
            version: 1,
            name: "valid_tool".into(),
            description: None,
            parameters_schema: serde_json::json!({"type": "object"}),
        });
    let mut nodes = creation.nodes().to_vec();
    nodes.push(vestrace_application::ModelRequestEvidenceNodeInput {
        kind: vestrace_application::ModelRequestNodeKind::ToolSchemaRevision,
        reference_id: tool_id,
        reference_version: Some(1),
        safe_ordinal: Some("0".into()),
    });
    vestrace_application::CreateModelRequestEvidence::new(
        vestrace_application::ModelRequestEvidenceIdentity {
            root_id: Uuid::now_v7(),
            workspace_id: fixture.workspace_id,
            external_effect_id: fixture.external_effect_id,
            cause_kind: vestrace_application::ModelRequestCauseKind::QualificationProbe,
            cause_id: fixture.qualification_job_id,
            binding_snapshot_id: None,
            qualification_target_binding_id: Some(fixture.qualification_target_id),
        },
        canonical,
        nodes,
    )
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn complete_evidence_reconstructs_from_exact_revisions(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool));
    let repository = PgModelRequestEvidenceRepository::new(Arc::new(RefusingVault));
    let root_id = Uuid::now_v7();
    let creation = vestrace_application::CreateModelRequestEvidence::models_list_probe(
        root_id,
        fixture.workspace_id,
        fixture.external_effect_id,
        fixture.connection_revision_id,
        fixture.qualification_target_id,
        fixture.qualification_job_id,
        "00",
    )
    .unwrap();
    let mut unit = manager.begin(&context).await.unwrap();
    let created = repository
        .create_in(unit.as_mut(), &creation)
        .await
        .unwrap();
    let reconstruction = repository
        .reconstruct_current_in(unit.as_mut(), fixture.workspace_id, created)
        .await
        .unwrap();
    let ModelRequestReconstruction::Complete(request) = reconstruction else {
        panic!("models-list evidence must reconstruct completely");
    };
    assert!(matches!(request, EffectiveModelRequest::ModelsList));
    unit.commit().await.unwrap();
    assert_eq!(created.as_uuid(), root_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn q1_source_round_trips_only_the_closed_structural_tuple(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let repository = PgModelRequestEvidenceRepository::new(Arc::new(RefusingVault));
    let root_id = Uuid::now_v7();
    let source = vestrace_application::Q1MreSource::new(
        "10",
        vestrace_application::Q1SafeMessageLayout::PlainText,
        vestrace_application::Q1SafeToolChoice::None,
        false,
        vestrace_application::Q1SafeResponseFormat::None,
        false,
        false,
        None,
    )
    .unwrap();
    let creation = vestrace_application::CreateModelRequestEvidence::models_list_probe(
        root_id,
        fixture.workspace_id,
        fixture.external_effect_id,
        fixture.connection_revision_id,
        fixture.qualification_target_id,
        fixture.qualification_job_id,
        "10",
    )
    .unwrap()
    .with_q1_source(source)
    .unwrap();
    let mut unit = manager.begin(&context).await.unwrap();
    repository
        .create_in(unit.as_mut(), &creation)
        .await
        .unwrap();
    unit.commit().await.unwrap();

    let row: (
        String,
        String,
        String,
        bool,
        String,
        bool,
        bool,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT probe_ordinal,message_layout,tool_choice,parallel_tool_calls,
                response_format,stream,stream_include_usage,assistant_tool_call_id
           FROM qualification_q1_mre_sources
          WHERE evidence_root_id=$1 AND workspace_id=$2",
    )
    .bind(root_id)
    .bind(fixture.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            "10".into(),
            "plain_text".into(),
            "none".into(),
            false,
            "none".into(),
            false,
            false,
            None,
        )
    );

    let mut guarded = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *guarded)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *guarded)
        .await
        .unwrap();
    let error = sqlx::query(
        "UPDATE qualification_q1_mre_sources
            SET stream=true
          WHERE evidence_root_id=$1",
    )
    .bind(root_id)
    .execute(&mut *guarded)
    .await
    .expect_err("q1 source must remain immutable after creation");
    let code = error
        .as_database_error()
        .expect("q1 source mutation must be a database error")
        .code()
        .map(|value| value.into_owned());
    assert_eq!(code.as_deref(), Some("42501"), "{error}");
    guarded.rollback().await.unwrap();

    // The source-creation entrypoint is also a durable per-field mutation
    // guard: no alternative tuple can replace or be mistaken for this root.
    for (ordinal, layout, tool_choice, parallel, response_format, stream, include_usage, replay) in [
        (
            "20",
            "plain_text",
            "none",
            false,
            "none",
            false,
            false,
            None,
        ),
        (
            "10",
            "multipart_image_marker",
            "none",
            false,
            "none",
            false,
            false,
            None,
        ),
        (
            "10",
            "plain_text",
            "named_probe",
            false,
            "none",
            false,
            false,
            None,
        ),
        ("10", "plain_text", "none", true, "none", false, false, None),
        (
            "10",
            "plain_text",
            "none",
            false,
            "strict_nonce_json_schema",
            false,
            false,
            None,
        ),
        ("10", "plain_text", "none", false, "none", true, false, None),
        ("10", "plain_text", "none", false, "none", false, true, None),
        (
            "10",
            "plain_text",
            "none",
            false,
            "none",
            false,
            false,
            Some("call_q1"),
        ),
    ] {
        let mut mutation = pool.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(fixture.workspace_id.to_string())
            .fetch_one(&mut *mutation)
            .await
            .unwrap();
        let error = sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_create_qualification_q1_mre_source(
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(root_id)
        .bind(fixture.workspace_id.as_uuid())
        .bind(ordinal)
        .bind(layout)
        .bind(tool_choice)
        .bind(parallel)
        .bind(response_format)
        .bind(stream)
        .bind(include_usage)
        .bind(replay)
        .fetch_one(&mut *mutation)
        .await
        .expect_err("each q1 source field mutation must be refused before adapter dispatch");
        assert_eq!(
            error
                .as_database_error()
                .expect("q1 source mutation must be a database error")
                .code()
                .as_deref(),
            Some("23514"),
            "mutated q1 source tuple unexpectedly passed: {ordinal}/{layout}/{tool_choice}"
        );
        mutation.rollback().await.unwrap();
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn embeddings_reconstructs_from_database_backed_revisions(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x39; 32];
    let model_revision_id =
        create_embedding_model(&pool, &runtime, &fixture, "embedding-db-model").await;
    let first = create_live_material(&runtime, &fixture, b"first embedding input", 0, key).await;
    let second = create_live_material(&runtime, &fixture, b"second embedding input", 1, key).await;
    let creation = embeddings_creation(&fixture, model_revision_id, &[first, second]);
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let repository = PgModelRequestEvidenceRepository::new(Arc::new(CountingVault::new(key)));
    let mut unit = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(unit.as_mut(), &creation)
        .await
        .unwrap();
    let reconstructed = repository
        .reconstruct_current_in(unit.as_mut(), fixture.workspace_id, root_id)
        .await
        .unwrap();
    let ModelRequestReconstruction::Complete(EffectiveModelRequest::Embeddings(request)) =
        reconstructed
    else {
        panic!("database-backed embeddings evidence must reconstruct completely");
    };
    assert_eq!(request.model(), "embedding-db-model");
    assert_eq!(
        request.inputs().collect::<Vec<_>>(),
        vec!["first embedding input", "second embedding input"]
    );
    unit.commit().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn mixed_role_chat_reconstructs_and_the_production_adapter_preserves_message_order(
    pool: PgPool,
) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x3a; 32];
    let model_revision_id = create_chat_model(&pool, &runtime, &fixture, "mixed-role-model").await;
    let system = create_live_material(&runtime, &fixture, b"system policy", 0, key).await;
    let user = create_live_material(&runtime, &fixture, b"user prompt", 1, key).await;
    let shape_id = Uuid::now_v7();
    let sampling_id = Uuid::now_v7();
    let limits_id = Uuid::now_v7();
    let root_id = Uuid::now_v7();
    let mut create = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *create)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_shape_revision(
            $1,$2,1,'chat_completions',false,ARRAY['system','user']::TEXT[]
        )",
    )
    .bind(shape_id)
    .bind(fixture.workspace_id.as_uuid())
    .fetch_one(&mut *create)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_sampling_revision($1,$2,1,0.2,0.9)",
    )
    .bind(sampling_id)
    .bind(fixture.workspace_id.as_uuid())
    .fetch_one(&mut *create)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_limits_revision($1,$2,1,128,2,65536)",
    )
    .bind(limits_id)
    .bind(fixture.workspace_id.as_uuid())
    .fetch_one(&mut *create)
    .await
    .unwrap();
    let kinds = vec![
        "external_effect",
        "connection_revision",
        "qualification_target",
        "qualification_probe",
        "model_revision",
        "governed_input_material",
        "governed_input_material",
        "request_shape_revision",
        "sampling_revision",
        "limits_revision",
    ];
    let ids = vec![
        fixture.external_effect_id,
        fixture.connection_revision_id,
        fixture.qualification_target_id,
        fixture.qualification_job_id,
        model_revision_id,
        system.id.as_uuid(),
        user.id.as_uuid(),
        shape_id,
        sampling_id,
        limits_id,
    ];
    let versions = vec![
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(1_i64),
        Some(1_i64),
        Some(1_i64),
    ];
    let ordinals = vec![
        None,
        None,
        None,
        Some("00"),
        None,
        Some("0"),
        Some("1"),
        None,
        None,
        None,
    ];
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_evidence(
            $1,$2,$3,'chat_completions',NULL,$4,'qualification_probe',$5,$6,$7,$8,$9
        )",
    )
    .bind(root_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .bind(fixture.qualification_target_id)
    .bind(fixture.qualification_job_id)
    .bind(&kinds)
    .bind(&ids)
    .bind(&versions)
    .bind(&ordinals)
    .fetch_one(&mut *create)
    .await
    .unwrap();
    create.commit().await.unwrap();

    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let repository = PgModelRequestEvidenceRepository::new(Arc::new(CountingVault::new(key)));
    let mut unit = manager.begin(&context).await.unwrap();
    let reconstructed = repository
        .reconstruct_current_in(
            unit.as_mut(),
            fixture.workspace_id,
            vestrace_domain::ModelRequestEvidenceId::from_uuid(root_id),
        )
        .await
        .unwrap();
    let ModelRequestReconstruction::Complete(request) = reconstructed else {
        panic!("mixed-role evidence must reconstruct completely");
    };
    unit.commit().await.unwrap();
    let (client, observer) = chat_adapter_and_observer();
    assert!(matches!(
        client.execute_effective(request).await.unwrap(),
        EffectiveModelResponse::ChatCompletions(_)
    ));
    let observed_messages = observer.0.lock().unwrap()[0]["messages"].clone();
    assert_eq!(
        observed_messages,
        serde_json::json!([
            {"role": "system", "content": "system policy"},
            {"role": "user", "content": "user prompt"}
        ])
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_step_reconstructs_only_the_exact_snapshot_qualification_tuple(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x49; 32];
    let model_revision_id = create_chat_model(&pool, &runtime, &fixture, "run-step-model").await;
    let tuple = create_run_step_tuple(&pool, &fixture, model_revision_id).await;
    let material = create_live_material(&runtime, &fixture, b"run step input", 0, key).await;
    let creation = run_step_chat_creation(&fixture, tuple, model_revision_id, material);
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let repository = PgModelRequestEvidenceRepository::new(Arc::new(CountingVault::new(key)));
    let mut unit = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(unit.as_mut(), &creation)
        .await
        .unwrap();
    let reconstructed = repository
        .reconstruct_current_in(unit.as_mut(), fixture.workspace_id, root_id)
        .await
        .unwrap();
    let ModelRequestReconstruction::Complete(EffectiveModelRequest::ChatCompletions(request)) =
        reconstructed
    else {
        panic!("the exact run-step tuple must reconstruct completely");
    };
    assert_eq!(request.model(), "run-step-model");
    assert_eq!(request.messages()[0].content(), "run step input");
    unit.commit().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_step_embeddings_reconstructs_the_exact_snapshot_qualification_tuple(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x4a; 32];
    let model_revision_id =
        create_embedding_model(&pool, &runtime, &fixture, "run-step-embedding-model").await;
    let tuple =
        create_run_step_tuple_with_capability(&pool, &fixture, model_revision_id, "embeddings")
            .await;
    let first = create_live_material(&runtime, &fixture, b"first run step input", 0, key).await;
    let second = create_live_material(&runtime, &fixture, b"second run step input", 1, key).await;
    let creation =
        run_step_embeddings_creation(&fixture, tuple, model_revision_id, &[first, second]);
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let repository = PgModelRequestEvidenceRepository::new(Arc::new(CountingVault::new(key)));
    let mut unit = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(unit.as_mut(), &creation)
        .await
        .unwrap();
    let reconstructed = repository
        .reconstruct_current_in(unit.as_mut(), fixture.workspace_id, root_id)
        .await
        .unwrap();
    let ModelRequestReconstruction::Complete(EffectiveModelRequest::Embeddings(request)) =
        reconstructed
    else {
        panic!("the exact run-step embeddings tuple must reconstruct completely");
    };
    assert_eq!(request.model(), "run-step-embedding-model");
    assert_eq!(
        request.inputs().collect::<Vec<_>>(),
        vec!["first run step input", "second run step input"]
    );
    unit.commit().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn sql_size_header_and_unframed_refusals_do_not_unwrap(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x74; 32];
    let model_revision_id = create_chat_model(&pool, &runtime, &fixture, "refusal-model").await;
    let material = create_live_material(&runtime, &fixture, b"valid input", 0, key).await;
    let creation = chat_creation(&fixture, model_revision_id, &[material], "refusal-model");
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let vault = Arc::new(CountingVault::new(key));
    let repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let mut create = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(create.as_mut(), &creation)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let mut legacy = vec![0_u8; 4096];
    legacy[..25].copy_from_slice(b"legacy unframed plaintext");
    replace_live_material_frame(&pool, &fixture, material, &legacy).await;
    let mut legacy_check = manager.begin(&context).await.unwrap();
    assert!(matches!(
        repository
            .reconstruct_current_in(legacy_check.as_mut(), fixture.workspace_id, root_id)
            .await
            .unwrap(),
        ModelRequestReconstruction::Incomplete(_)
    ));
    legacy_check.commit().await.unwrap();
    assert_eq!(
        vault.unwrap_count(),
        0,
        "legacy/header refusal must precede unwrap"
    );

    let oversized = vec![0_u8; 2 * MAX_FRAMED_MATERIAL_BYTES];
    replace_live_material_frame(&pool, &fixture, material, &oversized).await;
    let mut size_check = manager.begin(&context).await.unwrap();
    assert!(matches!(
        repository
            .reconstruct_current_in(size_check.as_mut(), fixture.workspace_id, root_id)
            .await
            .unwrap(),
        ModelRequestReconstruction::Incomplete(_)
    ));
    size_check.commit().await.unwrap();
    assert_eq!(
        vault.unwrap_count(),
        0,
        "SQL size refusal must precede unwrap"
    );
    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM model_request_evidence_checks WHERE evidence_root_id=$1",
    )
    .bind(root_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(statuses, vec!["incomplete", "incomplete"]);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn valid_decoded_request_above_four_mib_framed_reconstructs_completely(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x75; 32];
    let model_revision_id =
        create_chat_model(&pool, &runtime, &fixture, "aggregate-frame-model").await;
    let mut materials = Vec::new();
    for ordinal in 0..1025 {
        materials.push(create_live_material(&runtime, &fixture, b"x", ordinal, key).await);
    }
    let creation = chat_creation_with_limit(
        &fixture,
        model_revision_id,
        &materials,
        "aggregate-frame-model",
        4 * 1024 * 1024,
    );
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let vault = Arc::new(CountingVault::new(key));
    let repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let mut create = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(create.as_mut(), &creation)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let framed_total: i64 = sqlx::query_scalar(
        "SELECT SUM(octet_length(bytes.ciphertext))
           FROM content_material_bytes AS bytes
          WHERE bytes.material_id=ANY($1)",
    )
    .bind(
        materials
            .iter()
            .map(|material| material.id.as_uuid())
            .collect::<Vec<_>>(),
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(framed_total > 4 * 1024 * 1024);

    let mut reconstruct = manager.begin(&context).await.unwrap();
    let result = repository
        .reconstruct_current_in(reconstruct.as_mut(), fixture.workspace_id, root_id)
        .await
        .unwrap();
    let ModelRequestReconstruction::Complete(EffectiveModelRequest::ChatCompletions(request)) =
        result
    else {
        panic!("valid decoded aggregate must not be rejected for frame padding");
    };
    assert_eq!(request.messages().len(), 1025);
    reconstruct.commit().await.unwrap();
    assert_eq!(vault.unwrap_count(), 1025);
    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM model_request_evidence_checks WHERE evidence_root_id=$1",
    )
    .bind(root_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(statuses, vec!["complete"]);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn pathological_aggregate_frame_set_is_incomplete_before_unwrap(pool: PgPool) {
    const DECODED_AGGREGATE_LIMIT: i64 = 4 * 1024 * 1024;
    const MIN_FRAME_BYTES: i64 = 4096;

    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x76; 32];
    let model_revision_id =
        create_chat_model(&pool, &runtime, &fixture, "pathological-frame-model").await;
    let plaintext = vec![b'x'; 600_000];
    let mut materials = Vec::new();
    for ordinal in 0..9 {
        materials.push(create_live_material(&runtime, &fixture, &plaintext, ordinal, key).await);
    }
    let creation = chat_creation_with_limit(
        &fixture,
        model_revision_id,
        &materials,
        "pathological-frame-model",
        4 * 1024 * 1024,
    );
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let vault = Arc::new(CountingVault::new(key));
    let repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let mut create = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(create.as_mut(), &creation)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let framed_total: i64 = sqlx::query_scalar(
        "SELECT SUM(octet_length(bytes.ciphertext))
           FROM content_material_bytes AS bytes
          WHERE bytes.material_id=ANY($1)",
    )
    .bind(
        materials
            .iter()
            .map(|material| material.id.as_uuid())
            .collect::<Vec<_>>(),
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let derived_bound = materials.len() as i64 * MIN_FRAME_BYTES + 2 * DECODED_AGGREGATE_LIMIT;
    assert!(framed_total > derived_bound);
    assert!(
        framed_total <= materials.len() as i64 * MAX_FRAMED_MATERIAL_BYTES as i64,
        "the aggregate refusal must not rely on a per-frame violation"
    );

    let mut reconstruct = manager.begin(&context).await.unwrap();
    let result = repository
        .reconstruct_current_in(reconstruct.as_mut(), fixture.workspace_id, root_id)
        .await
        .unwrap();
    assert!(matches!(result, ModelRequestReconstruction::Incomplete(_)));
    reconstruct.commit().await.unwrap();
    assert_eq!(vault.unwrap_count(), 0);
    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM model_request_evidence_checks WHERE evidence_root_id=$1",
    )
    .bind(root_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(statuses, vec!["incomplete"]);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn wrong_aad_padding_and_open_failures_unwrap_once_and_zeroize(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x41; 32];
    let model_revision_id =
        create_chat_model(&pool, &runtime, &fixture, "crypto-refusal-model").await;
    let material = create_live_material(&runtime, &fixture, b"request bytes", 0, key).await;
    let creation = chat_creation(
        &fixture,
        model_revision_id,
        &[material],
        "crypto-refusal-model",
    );
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let vault = Arc::new(CountingVault::new(key));
    let repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let mut create = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(create.as_mut(), &creation)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let wrong_aad = ContentMaterialCodec::new()
        .seal(
            WorkspaceId::new(),
            material.id,
            material.key_id,
            &ZeroizingDek::new(key),
            b"request bytes",
        )
        .unwrap();
    replace_live_material_frame(&pool, &fixture, material, &wrong_aad).await;
    let mut aad_check = manager.begin(&context).await.unwrap();
    assert!(matches!(
        repository
            .reconstruct_current_in(aad_check.as_mut(), fixture.workspace_id, root_id)
            .await
            .unwrap(),
        ModelRequestReconstruction::Incomplete(_)
    ));
    aad_check.commit().await.unwrap();
    assert_eq!(vault.unwrap_count(), 1);

    let mut tampered = ContentMaterialCodec::new()
        .seal(
            fixture.workspace_id,
            material.id,
            material.key_id,
            &ZeroizingDek::new(key),
            b"request bytes",
        )
        .unwrap();
    *tampered.last_mut().unwrap() ^= 1;
    replace_live_material_frame(&pool, &fixture, material, &tampered).await;
    let mut open_check = manager.begin(&context).await.unwrap();
    assert!(matches!(
        repository
            .reconstruct_current_in(open_check.as_mut(), fixture.workspace_id, root_id)
            .await
            .unwrap(),
        ModelRequestReconstruction::Incomplete(_)
    ));
    open_check.commit().await.unwrap();
    assert_eq!(vault.unwrap_count(), 2);

    let bad_padding = authenticated_bad_padding_frame(fixture.workspace_id, material, key);
    assert!(matches!(
        ContentMaterialCodec::new().open(
            fixture.workspace_id,
            material.id,
            material.key_id,
            &ZeroizingDek::new(key),
            &bad_padding,
        ),
        Err(ContentMaterialCodecError::InvalidPadding)
    ));
    replace_live_material_frame(&pool, &fixture, material, &bad_padding).await;
    let mut padding_check = manager.begin(&context).await.unwrap();
    assert!(matches!(
        repository
            .reconstruct_current_in(padding_check.as_mut(), fixture.workspace_id, root_id)
            .await
            .unwrap(),
        ModelRequestReconstruction::Incomplete(_)
    ));
    padding_check.commit().await.unwrap();
    assert_eq!(vault.unwrap_count(), 3);
    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM model_request_evidence_checks WHERE evidence_root_id=$1",
    )
    .bind(root_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(statuses, vec!["incomplete", "incomplete", "incomplete"]);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn postgres_never_stores_the_raw_semantic_request_or_a_stable_digest(pool: PgPool) {
    let tables = [
        "model_request_shape_revisions",
        "model_sampling_revisions",
        "model_limits_revisions",
        "model_tool_schema_revisions",
        "model_request_evidence_roots",
        "model_request_evidence_nodes",
        "model_request_evidence_checks",
        "model_request_evidence_check_missing_references",
    ];
    let columns: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT table_name, column_name, data_type
           FROM information_schema.columns
          WHERE table_schema = 'public' AND table_name = ANY($1)
          ORDER BY table_name, ordinal_position",
    )
    .bind(tables.as_slice())
    .fetch_all(&pool)
    .await
    .unwrap();

    assert!(
        !columns.is_empty(),
        "Task 9 canonical evidence tables must exist"
    );
    for (table, column, data_type) in columns {
        let permitted_tool_schema =
            table == "model_tool_schema_revisions" && column == "parameters_schema";
        assert!(
            permitted_tool_schema || data_type != "jsonb",
            "{table}.{column} is a forbidden generic JSON payload"
        );
        assert!(
            ![
                "raw_request",
                "rendered_request",
                "canonical_json",
                "canonical_request_digest",
                "content_digest",
                "plaintext_length",
                "payload",
                "prompt",
                "output",
                "auth_header",
            ]
            .contains(&column.as_str()),
            "{table}.{column} retains forbidden semantic request data"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn wrong_kind_cross_workspace_wrong_version_duplicate_missing_and_extra_nodes_are_refused(
    pool: PgPool,
) {
    let guarded_tables = [
        "model_request_shape_revisions",
        "model_sampling_revisions",
        "model_limits_revisions",
        "model_tool_schema_revisions",
        "model_request_evidence_check_missing_references",
    ];
    let rows: Vec<(String, String, bool, bool)> = sqlx::query_as(
        "SELECT c.relname, pg_get_userbyid(c.relowner), c.relrowsecurity, c.relforcerowsecurity
           FROM pg_class AS c
           JOIN pg_namespace AS n ON n.oid = c.relnamespace
          WHERE n.nspname = 'public' AND c.relname = ANY($1)
          ORDER BY c.relname",
    )
    .bind(guarded_tables.as_slice())
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), guarded_tables.len());
    for (table, owner, rls, forced) in rows {
        assert_eq!(owner, "vestrace_guarded_owner", "{table}");
        assert!(rls && forced, "{table} must use forced RLS");
    }

    let runtime = runtime_pool(&pool).await;
    let signature: Option<String> = sqlx::query_scalar(
        "SELECT p.oid::regprocedure::text
           FROM pg_proc AS p
           JOIN pg_namespace AS n ON n.oid = p.pronamespace
          WHERE n.nspname = 'public'
            AND p.proname = 'vestrace_create_model_request_evidence'",
    )
    .fetch_optional(&runtime)
    .await
    .unwrap();
    assert!(signature.is_some(), "the guarded matrix creator must exist");

    let fixture = probe_fixture(&pool).await;
    let other_workspace = probe_fixture(&pool).await;
    let canonical = create_probe_canonical(&runtime, &fixture).await;
    let valid = probe_nodes(&fixture, canonical);

    let mut wrong_kind = valid.clone();
    assert_sqlstate(
        &call_probe_creator(
            &runtime,
            &fixture,
            Uuid::now_v7(),
            "chat_completions",
            &wrong_kind,
        )
        .await
        .unwrap_err(),
        "23514",
    );
    wrong_kind.ids[1] = other_workspace.connection_revision_id;
    assert_sqlstate(
        &call_probe_creator(
            &runtime,
            &fixture,
            Uuid::now_v7(),
            "models_list",
            &wrong_kind,
        )
        .await
        .unwrap_err(),
        "23514",
    );

    let mut wrong_version = valid.clone();
    wrong_version.versions[4] = Some(2);
    assert_sqlstate(
        &call_probe_creator(
            &runtime,
            &fixture,
            Uuid::now_v7(),
            "models_list",
            &wrong_version,
        )
        .await
        .unwrap_err(),
        "23514",
    );

    let mut duplicate = valid.clone();
    duplicate.push("limits_revision", canonical.limits_id, Some(1), None);
    assert_sqlstate(
        &call_probe_creator(
            &runtime,
            &fixture,
            Uuid::now_v7(),
            "models_list",
            &duplicate,
        )
        .await
        .unwrap_err(),
        "23514",
    );

    let mut missing = valid.clone();
    missing.remove(5);
    assert_sqlstate(
        &call_probe_creator(&runtime, &fixture, Uuid::now_v7(), "models_list", &missing)
            .await
            .unwrap_err(),
        "23514",
    );

    let mut extra = valid.clone();
    extra.push("sampling_revision", Uuid::now_v7(), Some(1), None);
    assert_sqlstate(
        &call_probe_creator(&runtime, &fixture, Uuid::now_v7(), "models_list", &extra)
            .await
            .unwrap_err(),
        "23514",
    );

    let mut non_contiguous = valid.clone();
    non_contiguous.push("tool_schema_revision", Uuid::now_v7(), Some(1), Some("1"));
    assert_sqlstate(
        &call_probe_creator(
            &runtime,
            &fixture,
            Uuid::now_v7(),
            "models_list",
            &non_contiguous,
        )
        .await
        .unwrap_err(),
        "23514",
    );

    let created = call_probe_creator(&runtime, &fixture, Uuid::now_v7(), "models_list", &valid)
        .await
        .unwrap();
    let persisted_nodes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM model_request_evidence_nodes WHERE evidence_root_id=$1",
    )
    .bind(created)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted_nodes, valid.kinds.len() as i64);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn models_list_run_step_is_refused_without_persisting_a_root_or_nodes(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let model_revision_id =
        create_chat_model(&pool, &runtime, &fixture, "models-list-run-step-model").await;
    let tuple = create_run_step_tuple(&pool, &fixture, model_revision_id).await;
    let canonical = create_probe_canonical(&runtime, &fixture).await;
    let root_id = Uuid::now_v7();
    let kinds = vec![
        "external_effect",
        "connection_revision",
        "binding_snapshot",
        "connection_qualification_revision",
        "model_qualification_revision",
        "request_shape_revision",
        "limits_revision",
    ];
    let ids = vec![
        fixture.external_effect_id,
        fixture.connection_revision_id,
        tuple.snapshot_id,
        tuple.connection_qualification_id,
        tuple.model_qualification_id,
        canonical.shape_id,
        canonical.limits_id,
    ];
    let versions = vec![None, None, None, None, None, Some(1_i64), Some(1_i64)];
    let ordinals = vec![None::<String>; kinds.len()];
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let result = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_evidence(
            $1,$2,$3,'models_list',$4,NULL,'run_step',$5,$6,$7,$8,$9
        )",
    )
    .bind(root_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .bind(tuple.snapshot_id)
    .bind(tuple.cause_id)
    .bind(&kinds)
    .bind(&ids)
    .bind(&versions)
    .bind(&ordinals)
    .fetch_one(&mut *transaction)
    .await;
    match &result {
        Ok(_) => transaction.commit().await.unwrap(),
        Err(_) => transaction.rollback().await.unwrap(),
    }
    assert_sqlstate(&result.unwrap_err(), "23514");
    let persisted: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM model_request_evidence_roots WHERE id=$1),
            (SELECT COUNT(*) FROM model_request_evidence_nodes WHERE evidence_root_id=$1)",
    )
    .bind(root_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, (0, 0));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn false_expired_check_cannot_cite_an_unrelated_erased_material(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let canonical = create_probe_canonical(&runtime, &fixture).await;
    let nodes = probe_nodes(&fixture, canonical);
    let root_id = call_probe_creator(&runtime, &fixture, Uuid::now_v7(), "models_list", &nodes)
        .await
        .unwrap();
    let unrelated = create_live_material(&runtime, &fixture, b"unrelated", 0, [0x29; 32]).await;
    let mut erasure = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *erasure)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(fixture.principal_id.to_string())
        .fetch_one(&mut *erasure)
        .await
        .unwrap();
    let (preparation_id, _, _): (Uuid, Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
            .bind(unrelated.id.as_uuid())
            .fetch_one(&mut *erasure)
            .await
            .unwrap();
    sqlx::query("SELECT vestrace_record_material_erasure_fence($1,$2)")
        .bind(preparation_id)
        .bind(Uuid::now_v7())
        .execute(&mut *erasure)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_finalize_content_material_erasure($1,$2)")
        .bind(preparation_id)
        .bind(Uuid::now_v7())
        .fetch_one(&mut *erasure)
        .await
        .unwrap();
    erasure.commit().await.unwrap();
    let tombstone_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM material_erasure_audit_tombstones WHERE preparation_id=$1",
    )
    .bind(preparation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let check_id = Uuid::now_v7();
    let mut append = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *append)
        .await
        .unwrap();
    let result = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_append_model_request_evidence_check(
            $1,$2,$3,'expired',ARRAY['governed_input_material']::TEXT[],$4,$5,$6
        )",
    )
    .bind(check_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(root_id)
    .bind(vec![unrelated.id.as_uuid()])
    .bind(vec![preparation_id])
    .bind(vec![tombstone_id])
    .fetch_one(&mut *append)
    .await;
    append.rollback().await.unwrap();
    assert_sqlstate(&result.unwrap_err(), "23514");
    let check_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM model_request_evidence_checks WHERE evidence_root_id=$1",
    )
    .bind(root_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(check_count, 0);
    runtime.close().await;
}

async fn two_input_chat_root(
    pool: &PgPool,
    runtime: &PgPool,
    fixture: &ProbeFixture,
    key: [u8; 32],
) -> (
    vestrace_domain::ModelRequestEvidenceId,
    LiveMaterial,
    LiveMaterial,
) {
    let model_revision_id =
        create_chat_model(pool, runtime, fixture, "expired-completeness-model").await;
    let first = create_live_material(runtime, fixture, b"first input", 0, key).await;
    let second = create_live_material(runtime, fixture, b"second input", 1, key).await;
    let creation = chat_creation(
        fixture,
        model_revision_id,
        &[first, second],
        "expired-completeness-model",
    );
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let repository = PgModelRequestEvidenceRepository::new(Arc::new(RefusingVault));
    let mut unit = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(unit.as_mut(), &creation)
        .await
        .unwrap();
    unit.commit().await.unwrap();
    (root_id, first, second)
}

async fn prepare_test_material_erasure(
    runtime: &PgPool,
    fixture: &ProbeFixture,
    material: LiveMaterial,
) -> Uuid {
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(fixture.principal_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let (preparation_id, _, _): (Uuid, Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
            .bind(material.id.as_uuid())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    transaction.commit().await.unwrap();
    preparation_id
}

async fn finalize_test_material_erasure(
    runtime: &PgPool,
    fixture: &ProbeFixture,
    material: LiveMaterial,
) -> (Uuid, Uuid) {
    let preparation_id = prepare_test_material_erasure(runtime, fixture, material).await;
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(fixture.principal_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_erasure_fence($1,$2)")
        .bind(preparation_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_finalize_content_material_erasure($1,$2)")
        .bind(preparation_id)
        .bind(Uuid::now_v7())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let tombstone_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM material_erasure_audit_tombstones WHERE preparation_id=$1",
    )
    .bind(preparation_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    (preparation_id, tombstone_id)
}

async fn append_test_expired_check(
    runtime: &PgPool,
    fixture: &ProbeFixture,
    check_id: Uuid,
    root_id: vestrace_domain::ModelRequestEvidenceId,
    material: LiveMaterial,
    preparation_id: Uuid,
    tombstone_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await?;
    let result = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_append_model_request_evidence_check(
            $1,$2,$3,'expired',ARRAY['governed_input_material']::TEXT[],$4,$5,$6
        )",
    )
    .bind(check_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(root_id.as_uuid())
    .bind(vec![material.id.as_uuid()])
    .bind(vec![preparation_id])
    .bind(vec![tombstone_id])
    .fetch_one(&mut *transaction)
    .await;
    match result {
        Ok(created) => {
            transaction.commit().await?;
            Ok(created)
        }
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

async fn assert_no_test_check_rows(pool: &PgPool, check_id: Uuid) {
    let rows: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM model_request_evidence_checks WHERE id=$1),
            (SELECT COUNT(*) FROM model_request_evidence_check_missing_references
              WHERE evidence_check_id=$1)",
    )
    .bind(check_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(rows, (0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_check_cannot_omit_a_prepared_governed_input(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let (root_id, erased, omitted) =
        two_input_chat_root(&pool, &runtime, &fixture, [0x6a; 32]).await;
    let (preparation_id, tombstone_id) =
        finalize_test_material_erasure(&runtime, &fixture, erased).await;
    prepare_test_material_erasure(&runtime, &fixture, omitted).await;
    let check_id = Uuid::now_v7();
    let error = append_test_expired_check(
        &runtime,
        &fixture,
        check_id,
        root_id,
        erased,
        preparation_id,
        tombstone_id,
    )
    .await
    .unwrap_err();
    assert_sqlstate(&error, "23514");
    assert_no_test_check_rows(&pool, check_id).await;
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_check_cannot_omit_a_live_governed_input_without_bytes(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let (root_id, erased, omitted) =
        two_input_chat_root(&pool, &runtime, &fixture, [0x6b; 32]).await;
    let (preparation_id, tombstone_id) =
        finalize_test_material_erasure(&runtime, &fixture, erased).await;
    let mut corrupt = pool.begin().await.unwrap();
    sqlx::query(
        "ALTER TABLE content_material_bytes
         DISABLE TRIGGER content_material_bytes_reject_raw_mutation",
    )
    .execute(&mut *corrupt)
    .await
    .unwrap();
    sqlx::query(
        "ALTER TABLE content_material_bytes
         DISABLE TRIGGER content_material_bytes_deferred_invariant",
    )
    .execute(&mut *corrupt)
    .await
    .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *corrupt)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *corrupt)
        .await
        .unwrap();
    let deleted = sqlx::query("DELETE FROM content_material_bytes WHERE material_id=$1")
        .bind(omitted.id.as_uuid())
        .execute(&mut *corrupt)
        .await
        .unwrap();
    assert_eq!(deleted.rows_affected(), 1);
    sqlx::query("RESET ROLE")
        .execute(&mut *corrupt)
        .await
        .unwrap();
    sqlx::query(
        "ALTER TABLE content_material_bytes
         ENABLE TRIGGER content_material_bytes_reject_raw_mutation",
    )
    .execute(&mut *corrupt)
    .await
    .unwrap();
    sqlx::query(
        "ALTER TABLE content_material_bytes
         ENABLE TRIGGER content_material_bytes_deferred_invariant",
    )
    .execute(&mut *corrupt)
    .await
    .unwrap();
    corrupt.commit().await.unwrap();

    let check_id = Uuid::now_v7();
    let error = append_test_expired_check(
        &runtime,
        &fixture,
        check_id,
        root_id,
        erased,
        preparation_id,
        tombstone_id,
    )
    .await
    .unwrap_err();
    assert_sqlstate(&error, "23514");
    assert_no_test_check_rows(&pool, check_id).await;
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_check_accepts_erased_input_when_every_omitted_input_is_live_with_bytes(
    pool: PgPool,
) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let (root_id, erased, _live) = two_input_chat_root(&pool, &runtime, &fixture, [0x6c; 32]).await;
    let (preparation_id, tombstone_id) =
        finalize_test_material_erasure(&runtime, &fixture, erased).await;
    let check_id = Uuid::now_v7();
    let created = append_test_expired_check(
        &runtime,
        &fixture,
        check_id,
        root_id,
        erased,
        preparation_id,
        tombstone_id,
    )
    .await
    .unwrap();
    assert_eq!(created, check_id);
    let evidence: (String, i32, i64) = sqlx::query_as(
        "SELECT checks.status,checks.missing_reference_count,COUNT(missing.id)
           FROM model_request_evidence_checks AS checks
           JOIN model_request_evidence_check_missing_references AS missing
             ON missing.evidence_check_id=checks.id
          WHERE checks.id=$1
          GROUP BY checks.status,checks.missing_reference_count",
    )
    .bind(check_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(evidence, ("expired".into(), 1, 1));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_check_cannot_omit_a_live_governed_input_with_malformed_frame(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let (root_id, erased, omitted) =
        two_input_chat_root(&pool, &runtime, &fixture, [0x6d; 32]).await;
    let (preparation_id, tombstone_id) =
        finalize_test_material_erasure(&runtime, &fixture, erased).await;
    replace_live_material_frame(&pool, &fixture, omitted, &vec![0_u8; 4096]).await;

    let check_id = Uuid::now_v7();
    let error = append_test_expired_check(
        &runtime,
        &fixture,
        check_id,
        root_id,
        erased,
        preparation_id,
        tombstone_id,
    )
    .await
    .unwrap_err();
    assert_sqlstate(&error, "23514");
    assert_no_test_check_rows(&pool, check_id).await;
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_incomplete_check_is_append_only(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let canonical = create_probe_canonical(&runtime, &fixture).await;
    let nodes = probe_nodes(&fixture, canonical);
    let root_id = call_probe_creator(&runtime, &fixture, Uuid::now_v7(), "models_list", &nodes)
        .await
        .unwrap();

    let incomplete_check_id = Uuid::now_v7();
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_append_model_request_evidence_check(
            $1,$2,$3,'incomplete',$4,$5,$6,$7
        )",
    )
    .bind(incomplete_check_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(root_id)
    .bind(vec!["request_shape_revision"])
    .bind(vec![canonical.shape_id])
    .bind(vec![None::<Uuid>])
    .bind(vec![None::<Uuid>])
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();

    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let repository = PgModelRequestEvidenceRepository::new(Arc::new(RefusingVault));
    let mut unit = manager.begin(&context).await.unwrap();
    let reconstructed = repository
        .reconstruct_current_in(
            unit.as_mut(),
            fixture.workspace_id,
            vestrace_domain::ModelRequestEvidenceId::from_uuid(root_id),
        )
        .await
        .unwrap();
    assert!(matches!(
        reconstructed,
        ModelRequestReconstruction::Complete(_)
    ));
    unit.commit().await.unwrap();

    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM model_request_evidence_checks
          WHERE evidence_root_id=$1 ORDER BY checked_at,id",
    )
    .bind(root_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(statuses, vec!["incomplete", "complete"]);
    let retained_missing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM model_request_evidence_check_missing_references
          WHERE evidence_check_id=$1 AND reference_kind='request_shape_revision'
            AND reference_id=$2",
    )
    .bind(incomplete_check_id)
    .bind(canonical.shape_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retained_missing, 1);

    let rewrite =
        sqlx::query("UPDATE model_request_evidence_checks SET status='complete' WHERE id=$1")
            .bind(incomplete_check_id)
            .execute(&runtime)
            .await
            .unwrap_err();
    assert_sqlstate(&rewrite, "42501");
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn canonical_revisions_and_nodes_are_an_exact_bijection_before_any_write(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let referenced = create_probe_canonical(&runtime, &fixture).await;
    let nodes = probe_nodes(&fixture, referenced);
    let orphan_shape_id = Uuid::now_v7();
    let orphan_limits_id = Uuid::now_v7();
    let root_id = Uuid::now_v7();
    let creation = vestrace_application::CreateModelRequestEvidence::new(
        vestrace_application::ModelRequestEvidenceIdentity {
            root_id,
            workspace_id: fixture.workspace_id,
            external_effect_id: fixture.external_effect_id,
            cause_kind: vestrace_application::ModelRequestCauseKind::QualificationProbe,
            cause_id: fixture.qualification_job_id,
            binding_snapshot_id: None,
            qualification_target_binding_id: Some(fixture.qualification_target_id),
        },
        vestrace_application::CanonicalRequestRevisions {
            request_shape: vestrace_application::RequestShapeRevisionInput {
                id: orphan_shape_id,
                version: 1,
                request_kind: vestrace_application::EffectiveRequestKind::ModelsList,
                stream: false,
                input_roles: Vec::new(),
            },
            sampling: None,
            limits: vestrace_application::LimitsRevisionInput {
                id: orphan_limits_id,
                version: 1,
                limits: vestrace_application::EffectiveRequestLimits::new(1, 1, 1).unwrap(),
            },
            tools: Vec::new(),
        },
        nodes
            .kinds
            .iter()
            .enumerate()
            .map(
                |(index, kind)| vestrace_application::ModelRequestEvidenceNodeInput {
                    kind: match kind.as_str() {
                        "external_effect" => {
                            vestrace_application::ModelRequestNodeKind::ExternalEffect
                        }
                        "connection_revision" => {
                            vestrace_application::ModelRequestNodeKind::ConnectionRevision
                        }
                        "qualification_target" => {
                            vestrace_application::ModelRequestNodeKind::QualificationTarget
                        }
                        "qualification_probe" => {
                            vestrace_application::ModelRequestNodeKind::QualificationProbe
                        }
                        "request_shape_revision" => {
                            vestrace_application::ModelRequestNodeKind::RequestShapeRevision
                        }
                        "limits_revision" => {
                            vestrace_application::ModelRequestNodeKind::LimitsRevision
                        }
                        other => panic!("unexpected probe kind {other}"),
                    },
                    reference_id: nodes.ids[index],
                    reference_version: nodes.versions[index].map(|version| version as u64),
                    safe_ordinal: nodes.safe_ordinals[index].clone(),
                },
            )
            .collect(),
    );
    let rejected_before_write = match creation {
        Err(_) => true,
        Ok(creation) => {
            let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
            let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
            let repository = PgModelRequestEvidenceRepository::new(Arc::new(RefusingVault));
            let mut unit = manager.begin(&context).await.unwrap();
            repository
                .create_in(unit.as_mut(), &creation)
                .await
                .unwrap();
            unit.commit().await.unwrap();
            false
        }
    };
    assert!(rejected_before_write);
    let persisted: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM model_request_shape_revisions WHERE id=$1),
            (SELECT COUNT(*) FROM model_limits_revisions WHERE id=$2),
            (SELECT COUNT(*) FROM model_request_evidence_roots WHERE id=$3)",
    )
    .bind(orphan_shape_id)
    .bind(orphan_limits_id)
    .bind(root_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, (0, 0, 0));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn tool_name_and_description_bounds_are_utf8_bytes_in_postgres(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let oversized_name_id = Uuid::now_v7();
    let oversized_name = "я".repeat(65);
    let name_error = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_tool_schema_revision($1,$2,1,$3,NULL,'{}'::JSONB)",
    )
    .bind(oversized_name_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(oversized_name)
    .fetch_one(&mut *transaction)
    .await
    .unwrap_err();
    assert_sqlstate(&name_error, "23514");
    transaction.rollback().await.unwrap();

    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let oversized_description_id = Uuid::now_v7();
    let oversized_description = "я".repeat(513);
    let description_error = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_tool_schema_revision($1,$2,1,'tool',$3,'{}'::JSONB)",
    )
    .bind(oversized_description_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(oversized_description)
    .fetch_one(&mut *transaction)
    .await
    .unwrap_err();
    assert_sqlstate(&description_error, "23514");
    transaction.rollback().await.unwrap();

    let persisted: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM model_tool_schema_revisions WHERE id=ANY($1)")
            .bind(vec![oversized_name_id, oversized_description_id])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(persisted, 0);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn retained_tool_dto_validation_failure_is_recorded_as_typed_incomplete(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x5a; 32];
    let model_revision_id =
        create_chat_model(&pool, &runtime, &fixture, "retained-tool-model").await;
    let material = create_live_material(&runtime, &fixture, b"tool input", 0, key).await;
    let tool_id = Uuid::now_v7();
    let creation = chat_creation_with_tool(&fixture, model_revision_id, material, tool_id);
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let vault = Arc::new(CountingVault::new(key));
    let repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let mut create = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(create.as_mut(), &creation)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let mut corrupt = pool.begin().await.unwrap();
    sqlx::query(
        "ALTER TABLE model_tool_schema_revisions
         DROP CONSTRAINT model_tool_schema_revisions_tool_name_check",
    )
    .execute(&mut *corrupt)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE model_tool_schema_revisions DISABLE TRIGGER model_tool_schema_revisions_immutable")
        .execute(&mut *corrupt)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *corrupt)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *corrupt)
        .await
        .unwrap();
    let updated = sqlx::query("UPDATE model_tool_schema_revisions SET tool_name=$1 WHERE id=$2")
        .bind("я".repeat(65))
        .bind(tool_id)
        .execute(&mut *corrupt)
        .await
        .unwrap();
    assert_eq!(updated.rows_affected(), 1);
    corrupt.commit().await.unwrap();

    let retained: (String, i32, i64) = sqlx::query_as(
        "SELECT tool.tool_name,octet_length(tool.tool_name),COUNT(nodes.id)
           FROM model_tool_schema_revisions AS tool
           LEFT JOIN model_request_evidence_nodes AS nodes
             ON nodes.reference_kind='tool_schema_revision' AND nodes.reference_id=tool.id
          WHERE tool.id=$1
          GROUP BY tool.tool_name",
    )
    .bind(tool_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retained.0.chars().count(), 65);
    assert_eq!(retained.1, 130);
    assert_eq!(retained.2, 1);

    let mut reconstruct = manager.begin(&context).await.unwrap();
    let result = repository
        .reconstruct_current_in(reconstruct.as_mut(), fixture.workspace_id, root_id)
        .await
        .unwrap();
    match result {
        ModelRequestReconstruction::Incomplete(_) => {}
        ModelRequestReconstruction::Complete(request) => {
            panic!(
                "invalid retained tool unexpectedly completed as {:?}",
                request.kind()
            )
        }
        ModelRequestReconstruction::Expired => {
            panic!("invalid retained tool unexpectedly produced Expired")
        }
    }
    reconstruct.commit().await.unwrap();
    assert_eq!(vault.unwrap_count(), 1);
    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM model_request_evidence_checks WHERE evidence_root_id=$1",
    )
    .bind(root_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(statuses, vec!["incomplete"]);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn missing_retained_source_is_incomplete_and_blocks_dispatch(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let repository = PgModelRequestEvidenceRepository::new(Arc::new(RefusingVault));
    let creation = vestrace_application::CreateModelRequestEvidence::models_list_probe(
        Uuid::now_v7(),
        fixture.workspace_id,
        fixture.external_effect_id,
        fixture.connection_revision_id,
        fixture.qualification_target_id,
        fixture.qualification_job_id,
        "00",
    )
    .unwrap();
    let missing_limits_id = creation.canonical().limits.id;
    let mut create = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(create.as_mut(), &creation)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let mut mutation = pool.begin().await.unwrap();
    sqlx::query(
        "ALTER TABLE model_limits_revisions
         DISABLE TRIGGER model_limits_revisions_immutable",
    )
    .execute(&mut *mutation)
    .await
    .unwrap();
    sqlx::query("DELETE FROM model_limits_revisions WHERE id=$1")
        .bind(missing_limits_id)
        .execute(&mut *mutation)
        .await
        .unwrap();
    sqlx::query(
        "ALTER TABLE model_limits_revisions
         ENABLE TRIGGER model_limits_revisions_immutable",
    )
    .execute(&mut *mutation)
    .await
    .unwrap();
    mutation.commit().await.unwrap();

    let mut reconstruct = manager.begin(&context).await.unwrap();
    let result = repository
        .reconstruct_current_in(reconstruct.as_mut(), fixture.workspace_id, root_id)
        .await
        .unwrap();
    let missing = match result {
        ModelRequestReconstruction::Incomplete(missing) => missing,
        _ => panic!("missing canonical source must not produce a dispatchable request"),
    };
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].kind.as_str(), "limits_revision");
    assert_eq!(missing[0].reference_id, missing_limits_id);
    let dispatch_count = if matches!(
        ModelRequestReconstruction::Incomplete(missing),
        ModelRequestReconstruction::Complete(_)
    ) {
        1
    } else {
        0
    };
    assert_eq!(dispatch_count, 0);
    reconstruct.commit().await.unwrap();

    let persisted: (String, i32) = sqlx::query_as(
        "SELECT status,missing_reference_count FROM model_request_evidence_checks
          WHERE evidence_root_id=$1 ORDER BY checked_at DESC,id DESC LIMIT 1",
    )
    .bind(root_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, ("incomplete".into(), 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn removed_required_input_node_is_incomplete_before_any_unwrap(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0xa7; 32];
    let model_revision_id = create_chat_model(&pool, &runtime, &fixture, "node-break-model").await;
    let material = create_live_material(&runtime, &fixture, b"must not dispatch", 0, key).await;
    let creation = chat_creation(&fixture, model_revision_id, &[material], "node-break-model");
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let vault = Arc::new(CountingVault::new(key));
    let repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let mut create = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(create.as_mut(), &creation)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let mut mutation = pool.begin().await.unwrap();
    sqlx::query(
        "ALTER TABLE model_request_evidence_nodes
         DISABLE TRIGGER model_request_evidence_nodes_immutable",
    )
    .execute(&mut *mutation)
    .await
    .unwrap();
    sqlx::query(
        "DELETE FROM model_request_evidence_nodes
          WHERE evidence_root_id=$1 AND reference_kind='governed_input_material'",
    )
    .bind(root_id.as_uuid())
    .execute(&mut *mutation)
    .await
    .unwrap();
    sqlx::query(
        "ALTER TABLE model_request_evidence_nodes
         ENABLE TRIGGER model_request_evidence_nodes_immutable",
    )
    .execute(&mut *mutation)
    .await
    .unwrap();
    mutation.commit().await.unwrap();

    let mut reconstruct = manager.begin(&context).await.unwrap();
    assert!(matches!(
        repository
            .reconstruct_current_in(reconstruct.as_mut(), fixture.workspace_id, root_id)
            .await
            .unwrap(),
        ModelRequestReconstruction::Incomplete(_)
    ));
    assert_eq!(vault.unwrap_count(), 0);
    reconstruct.commit().await.unwrap();
    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM model_request_evidence_checks WHERE evidence_root_id=$1",
    )
    .bind(root_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(statuses, vec!["incomplete"]);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn authorized_erasure_alone_derives_expired(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x63; 32];
    let model_revision_id = create_chat_model(&pool, &runtime, &fixture, "erasure-model").await;
    let material = create_live_material(&runtime, &fixture, b"erase me", 0, key).await;
    let creation = chat_creation(&fixture, model_revision_id, &[material], "erasure-model");
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let vault = Arc::new(CountingVault::new(key));
    let repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let mut create = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(create.as_mut(), &creation)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let mut erasure = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *erasure)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(fixture.principal_id.to_string())
        .fetch_one(&mut *erasure)
        .await
        .unwrap();
    let (preparation_id, prepared_key_id, prior_receipt): (Uuid, Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
            .bind(material.id.as_uuid())
            .fetch_one(&mut *erasure)
            .await
            .unwrap();
    assert_eq!(prepared_key_id, material.key_id.as_uuid());
    assert!(prior_receipt.is_none());
    sqlx::query("SELECT vestrace_record_material_erasure_fence($1,$2)")
        .bind(preparation_id)
        .bind(Uuid::now_v7())
        .execute(&mut *erasure)
        .await
        .unwrap();
    let erasure_receipt = Uuid::now_v7();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_finalize_content_material_erasure($1,$2)")
        .bind(preparation_id)
        .bind(erasure_receipt)
        .fetch_one(&mut *erasure)
        .await
        .unwrap();
    erasure.commit().await.unwrap();

    let mut reconstruct = manager.begin(&context).await.unwrap();
    let result = repository
        .reconstruct_current_in(reconstruct.as_mut(), fixture.workspace_id, root_id)
        .await
        .unwrap();
    assert!(matches!(result, ModelRequestReconstruction::Expired));
    assert_eq!(vault.unwrap_count(), 0);
    reconstruct.commit().await.unwrap();

    let exact_evidence: (String, Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT checks.status,missing.reference_id,
                missing.erasure_preparation_id,missing.erasure_tombstone_id
           FROM model_request_evidence_checks AS checks
           JOIN model_request_evidence_check_missing_references AS missing
             ON missing.evidence_check_id=checks.id
          WHERE checks.evidence_root_id=$1 ORDER BY checks.checked_at DESC LIMIT 1",
    )
    .bind(root_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(exact_evidence.0, "expired");
    assert_eq!(exact_evidence.1, material.id.as_uuid());
    assert_eq!(exact_evidence.2, preparation_id);
    let tombstone_preparation: Uuid = sqlx::query_scalar(
        "SELECT preparation_id FROM material_erasure_audit_tombstones WHERE id=$1",
    )
    .bind(exact_evidence.3)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tombstone_preparation, preparation_id);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_evidence_survives_adapter_default_changes(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let wire_model_id = "pinned-history-model";
    let model_revision_id = create_chat_model(&pool, &runtime, &fixture, wire_model_id).await;
    let key = [0x52; 32];
    let material = create_live_material(&runtime, &fixture, b"historical input", 0, key).await;
    let creation = chat_creation(&fixture, model_revision_id, &[material], wire_model_id);
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let vault = Arc::new(CountingVault::new(key));
    let repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let mut unit = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(unit.as_mut(), &creation)
        .await
        .unwrap();
    unit.commit().await.unwrap();

    let persisted: (f64, f64, i32, i32, i32) = sqlx::query_as(
        "SELECT sampling.temperature,sampling.top_p,limits.max_output_tokens,
                limits.max_inputs,limits.max_input_bytes
           FROM model_request_evidence_nodes AS sampling_node
           JOIN model_sampling_revisions AS sampling ON sampling.id=sampling_node.reference_id
           JOIN model_request_evidence_nodes AS limits_node
             ON limits_node.evidence_root_id=sampling_node.evidence_root_id
            AND limits_node.reference_kind='limits_revision'
           JOIN model_limits_revisions AS limits ON limits.id=limits_node.reference_id
          WHERE sampling_node.evidence_root_id=$1
            AND sampling_node.reference_kind='sampling_revision'",
    )
    .bind(root_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, (0.2, 0.9, 128, 1, 65_536));

    let mut reconstruct = manager.begin(&context).await.unwrap();
    let result = repository
        .reconstruct_current_in(reconstruct.as_mut(), fixture.workspace_id, root_id)
        .await
        .unwrap();
    let ModelRequestReconstruction::Complete(EffectiveModelRequest::ChatCompletions(request)) =
        result
    else {
        panic!("historical chat evidence must reconstruct its pinned request");
    };
    assert_eq!(request.model(), wire_model_id);
    assert_eq!(request.sampling().temperature(), 0.2);
    assert_eq!(request.sampling().top_p(), 0.9);
    assert_eq!(request.limits().max_output_tokens(), 128);
    assert_eq!(request.messages()[0].content(), "historical input");
    assert_eq!(vault.unwrap_count(), 1);
    reconstruct.commit().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_effect_preserves_the_same_evidence_root(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let canonical = create_probe_canonical(&runtime, &fixture).await;
    let nodes = probe_nodes(&fixture, canonical);
    let created = call_probe_creator(&runtime, &fixture, Uuid::now_v7(), "models_list", &nodes)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_receipts
         (id,effect_id,workspace_id,outcome_status,payload)
         VALUES ($1,$2,$3,'unknown',$4)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.external_effect_id)
    .bind(fixture.workspace_id.as_uuid())
    .bind(serde_json::json!({}))
    .execute(&pool)
    .await
    .unwrap();

    let replayed = call_probe_creator(&runtime, &fixture, Uuid::now_v7(), "models_list", &nodes)
        .await
        .unwrap();
    assert_eq!(replayed, created);
    let roots: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM model_request_evidence_roots WHERE external_effect_id=$1",
    )
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(roots, 1);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_exact_replay_returns_one_root_and_conflicting_replay_is_refused(pool: PgPool) {
    let exact_fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let exact_canonical = create_probe_canonical(&runtime, &exact_fixture).await;
    let exact_nodes = probe_nodes(&exact_fixture, exact_canonical);
    let root_a = Uuid::now_v7();
    let root_b = Uuid::now_v7();
    let (exact_a, exact_b) = tokio::join!(
        call_probe_creator(
            &runtime,
            &exact_fixture,
            root_a,
            "models_list",
            &exact_nodes,
        ),
        call_probe_creator(
            &runtime,
            &exact_fixture,
            root_b,
            "models_list",
            &exact_nodes,
        ),
    );
    let exact_a = exact_a.unwrap();
    let exact_b = exact_b.unwrap();
    assert_eq!(exact_a, exact_b);
    assert!(exact_a == root_a || exact_a == root_b);
    let exact_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM model_request_evidence_roots WHERE external_effect_id=$1",
    )
    .bind(exact_fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(exact_count, 1);

    let conflict_fixture = probe_fixture(&pool).await;
    let conflict_canonical = create_probe_canonical(&runtime, &conflict_fixture).await;
    let alternate_limits_id = Uuid::now_v7();
    create_probe_limits(&runtime, &conflict_fixture, alternate_limits_id).await;
    let first_nodes = probe_nodes(&conflict_fixture, conflict_canonical);
    let mut second_nodes = first_nodes.clone();
    second_nodes.ids[5] = alternate_limits_id;
    let (first, second) = tokio::join!(
        call_probe_creator(
            &runtime,
            &conflict_fixture,
            Uuid::now_v7(),
            "models_list",
            &first_nodes,
        ),
        call_probe_creator(
            &runtime,
            &conflict_fixture,
            Uuid::now_v7(),
            "models_list",
            &second_nodes,
        ),
    );
    let (winner, loser) = match (first, second) {
        (Ok(winner), Err(loser)) | (Err(loser), Ok(winner)) => (winner, loser),
        results => panic!("one conflicting replay must win and one must fail: {results:?}"),
    };
    assert_sqlstate(&loser, "40001");
    let conflict_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM model_request_evidence_roots WHERE external_effect_id=$1 AND id=$2",
    )
    .bind(conflict_fixture.external_effect_id)
    .bind(winner)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(conflict_count, 1);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn erasure_race_cannot_hydrate_after_complete_check(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x85; 32];
    let model_revision_id = create_chat_model(&pool, &runtime, &fixture, "race-model").await;
    let material = create_live_material(&runtime, &fixture, b"race input", 0, key).await;
    let creation = chat_creation(&fixture, model_revision_id, &[material], "race-model");
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let vault = Arc::new(CountingVault::new(key));
    let repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let mut create = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(create.as_mut(), &creation)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let mut guarded_dispatch = manager.begin(&context).await.unwrap();
    assert!(matches!(
        repository
            .reconstruct_current_in(guarded_dispatch.as_mut(), fixture.workspace_id, root_id,)
            .await
            .unwrap(),
        ModelRequestReconstruction::Complete(_)
    ));
    assert_eq!(vault.unwrap_count(), 1);

    let mut preparation = Box::pin(async {
        let mut transaction = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(fixture.workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        let prepared: (Uuid, Uuid, Option<Uuid>) =
            sqlx::query_as("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
                .bind(material.id.as_uuid())
                .fetch_one(&mut *transaction)
                .await
                .unwrap();
        transaction.commit().await.unwrap();
        prepared
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(200), preparation.as_mut())
            .await
            .is_err(),
        "preparation must wait for the guarded reconstruction transaction"
    );
    guarded_dispatch.commit().await.unwrap();
    let prepared = tokio::time::timeout(Duration::from_secs(5), preparation)
        .await
        .expect("preparation must resume without deadlock");
    assert_eq!(prepared.1, material.key_id.as_uuid());
    assert!(prepared.2.is_none());
    let preparation_id = prepared.0;

    let mut after_erasure_won = manager.begin(&context).await.unwrap();
    assert!(matches!(
        repository
            .reconstruct_current_in(after_erasure_won.as_mut(), fixture.workspace_id, root_id,)
            .await
            .unwrap(),
        ModelRequestReconstruction::Incomplete(_)
    ));
    assert_eq!(
        vault.unwrap_count(),
        1,
        "no bytes may hydrate after preparation wins"
    );

    let mut fence = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *fence)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_erasure_fence($1,$2)")
        .bind(preparation_id)
        .bind(Uuid::now_v7())
        .execute(&mut *fence)
        .await
        .unwrap();
    fence.commit().await.unwrap();

    let mut finalizer = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *finalizer)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(fixture.principal_id.to_string())
        .fetch_one(&mut *finalizer)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM material_erasure_preparations WHERE id=$1 FOR UPDATE")
        .bind(preparation_id)
        .execute(&mut *finalizer)
        .await
        .unwrap();
    let erasure_receipt = Uuid::now_v7();
    let mut finalize_first = Box::pin(async move {
        let result = sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_finalize_content_material_erasure($1,$2)",
        )
        .bind(preparation_id)
        .bind(erasure_receipt)
        .fetch_one(&mut *finalizer)
        .await;
        match result {
            Ok(receipt) => {
                finalizer.commit().await.unwrap();
                Ok(receipt)
            }
            Err(error) => {
                finalizer.rollback().await.unwrap();
                Err(error)
            }
        }
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(200), finalize_first.as_mut())
            .await
            .is_err(),
        "first-input finalization must wait on the held material row"
    );
    after_erasure_won.commit().await.unwrap();
    let receipt = tokio::time::timeout(Duration::from_secs(5), finalize_first)
        .await
        .expect("first-input finalization must resume without deadlock")
        .unwrap_or_else(|error| {
            assert_ne!(
                error
                    .as_database_error()
                    .and_then(|database| database.code())
                    .as_deref(),
                Some("40P01")
            );
            panic!("first-input finalization failed: {error}")
        });
    assert_eq!(receipt, erasure_receipt);

    let mut after_finalization = manager.begin(&context).await.unwrap();
    assert!(matches!(
        repository
            .reconstruct_current_in(after_finalization.as_mut(), fixture.workspace_id, root_id,)
            .await
            .unwrap(),
        ModelRequestReconstruction::Expired
    ));
    after_finalization.commit().await.unwrap();
    assert_eq!(vault.unwrap_count(), 1);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn multi_material_erasure_preparation_and_finalization_races_do_not_deadlock(pool: PgPool) {
    let fixture = probe_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let key = [0x96; 32];
    let model_revision_id = create_chat_model(&pool, &runtime, &fixture, "multi-race-model").await;
    let first = create_live_material(&runtime, &fixture, b"first input", 0, key).await;
    let later = create_live_material(&runtime, &fixture, b"later input", 1, key).await;
    let creation = chat_creation(
        &fixture,
        model_revision_id,
        &[first, later],
        "multi-race-model",
    );
    let context = RequestContext::new(fixture.workspace_id, fixture.principal_id);
    let manager = PgTransactionManager::new(PgStore::from_pool(pool.clone()));
    let vault = Arc::new(CountingVault::new(key));
    let repository = PgModelRequestEvidenceRepository::new(vault.clone());
    let mut create = manager.begin(&context).await.unwrap();
    let root_id = repository
        .create_in(create.as_mut(), &creation)
        .await
        .unwrap();
    create.commit().await.unwrap();

    let mut guarded_dispatch = manager.begin(&context).await.unwrap();
    assert!(matches!(
        repository
            .reconstruct_current_in(guarded_dispatch.as_mut(), fixture.workspace_id, root_id,)
            .await
            .unwrap(),
        ModelRequestReconstruction::Complete(_)
    ));
    assert_eq!(vault.unwrap_count(), 2);
    let mut prepare_later = Box::pin(async {
        let mut transaction = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(fixture.workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        let prepared: (Uuid, Uuid, Option<Uuid>) =
            sqlx::query_as("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
                .bind(later.id.as_uuid())
                .fetch_one(&mut *transaction)
                .await
                .unwrap();
        transaction.commit().await.unwrap();
        prepared
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(200), prepare_later.as_mut())
            .await
            .is_err()
    );
    guarded_dispatch.commit().await.unwrap();
    let (preparation_id, _, _) = tokio::time::timeout(Duration::from_secs(5), prepare_later)
        .await
        .expect("later-input preparation must resume without deadlock");

    let mut fence = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *fence)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_erasure_fence($1,$2)")
        .bind(preparation_id)
        .bind(Uuid::now_v7())
        .execute(&mut *fence)
        .await
        .unwrap();
    fence.commit().await.unwrap();

    let mut finalizer = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *finalizer)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(fixture.principal_id.to_string())
        .fetch_one(&mut *finalizer)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM material_erasure_preparations WHERE id=$1 FOR UPDATE")
        .bind(preparation_id)
        .execute(&mut *finalizer)
        .await
        .unwrap();

    let mut guarded_incomplete = manager.begin(&context).await.unwrap();
    let incomplete = tokio::time::timeout(
        Duration::from_secs(5),
        repository.reconstruct_current_in(
            guarded_incomplete.as_mut(),
            fixture.workspace_id,
            root_id,
        ),
    )
    .await
    .expect("reconstruction must not wait on the locked preparation row")
    .unwrap();
    assert!(matches!(
        incomplete,
        ModelRequestReconstruction::Incomplete(_)
    ));
    assert_eq!(
        vault.unwrap_count(),
        2,
        "a later missing input must be detected before any material unwrap"
    );

    let erasure_receipt = Uuid::now_v7();
    let mut finalize_later = Box::pin(async move {
        let result = sqlx::query_scalar::<_, Uuid>(
            "SELECT vestrace_finalize_content_material_erasure($1,$2)",
        )
        .bind(preparation_id)
        .bind(erasure_receipt)
        .fetch_one(&mut *finalizer)
        .await;
        match result {
            Ok(receipt) => {
                finalizer.commit().await.unwrap();
                Ok(receipt)
            }
            Err(error) => {
                finalizer.rollback().await.unwrap();
                Err(error)
            }
        }
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(200), finalize_later.as_mut())
            .await
            .is_err(),
        "finalization must wait on the held later material row"
    );
    guarded_incomplete.commit().await.unwrap();
    let receipt = tokio::time::timeout(Duration::from_secs(5), finalize_later)
        .await
        .expect("later-input finalization must resume without deadlock")
        .unwrap_or_else(|error| {
            assert_ne!(
                error
                    .as_database_error()
                    .and_then(|database| database.code())
                    .as_deref(),
                Some("40P01")
            );
            panic!("finalization failed: {error}")
        });
    assert_eq!(receipt, erasure_receipt);
    let states: (String, String) = sqlx::query_as(
        "SELECT material.state,intent.state
           FROM content_materials AS material
           JOIN material_key_creation_intents AS intent ON intent.id=material.intent_id
          WHERE material.id=$1",
    )
    .bind(later.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(states, ("tombstoned".into(), "tombstoned".into()));
    runtime.close().await;
}
