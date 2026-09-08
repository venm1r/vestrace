mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    str::FromStr,
    sync::Arc,
};

use chrono::{Duration, Utc};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tempfile::TempDir;
use vestrace_application::{
    AcceptDeliveryOutputs, AcceptEmbeddingJob, ApplicationError, DeliveryOutputIdentity,
    EmbeddingJobRepository, EmbeddingOutputKeyBinding, EmbeddingOutputKeyPlan,
    EmbeddingOutputKeyProgress, EmbeddingOutputKeyRepository, EmbeddingOutputKeyService,
    IdempotencyRecord, MaterialKeyVault, OutboxMessage, PreDispatchTerminalState,
    PreDispatchTerminationEvidence, RequestContext, RequestEmbeddingOutputRetirement,
    TerminateEmbeddingJobPreDispatch, VaultError,
};
use vestrace_domain::trust::{KeyPurpose, KeyReference, SecretResolutionRequest};
use vestrace_domain::{
    AuditEvent, AuthorizationRequest, Capability, ContentMaterialId, EmbeddingJobId,
    EmbeddingSpaceId, ErasureReceipt, IntentNonce, MaterialKeyCreationIntentId, MaterialKeyId,
    ModelRequestEvidenceId, PolicyDecision, PolicyDecisionReason, PolicyDecisionResult,
    PolicyInputState, PrincipalId, ResourceScope, RiskCategory, VaultReceipt, WorkspaceId,
    embedding::EmbeddingJobKind,
    id::{AuditEventId, OutboxId, PolicyDecisionId},
};
use vestrace_infrastructure::crypto::{HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER};
use vestrace_infrastructure::postgres::{
    PgEmbeddingJobRepository, PgEmbeddingOutputKeyRepository, PgStore,
};

const BOOTSTRAP_KEY_ID: &str = "output-key-bootstrap";
const BOOTSTRAP_SCOPE: &str = "output-key-bootstrap";
const BOOTSTRAP_ALGORITHM: &str = "aes-256-gcm-v1";

struct Fixture {
    bootstrap: TempDir,
    vault: TempDir,
    reference: KeyReference,
    request: SecretResolutionRequest,
}

impl Fixture {
    fn new() -> Self {
        let bootstrap = TempDir::new().unwrap();
        write_bootstrap(bootstrap.path());
        Self {
            vault: TempDir::new().unwrap(),
            reference: KeyReference::new(
                MOUNTED_SECRET_STORE_PROVIDER,
                BOOTSTRAP_KEY_ID,
                "v1",
                KeyPurpose::Storage,
                BOOTSTRAP_SCOPE,
                BOOTSTRAP_ALGORITHM,
            )
            .unwrap(),
            request: SecretResolutionRequest::new(
                WorkspaceId::new(),
                BOOTSTRAP_SCOPE,
                "test://embedding-output-key",
            ),
            bootstrap,
        }
    }

    fn vault(&self) -> HostMaterialKeyVault {
        HostMaterialKeyVault::new(
            self.vault.path(),
            self.bootstrap.path(),
            self.reference.clone(),
            self.request.clone(),
        )
        .unwrap()
    }

    fn vault_for_workspace(&self, workspace_id: WorkspaceId) -> HostMaterialKeyVault {
        HostMaterialKeyVault::new(
            self.vault.path(),
            self.bootstrap.path(),
            self.reference.clone(),
            SecretResolutionRequest::new(
                workspace_id,
                BOOTSTRAP_SCOPE,
                "test://embedding-output-key-process-observer",
            ),
        )
        .unwrap()
    }

    fn binding(&self) -> EmbeddingOutputKeyBinding {
        EmbeddingOutputKeyBinding {
            workspace_id: WorkspaceId::new(),
            job_id: EmbeddingJobId::new(),
            intent_id: MaterialKeyCreationIntentId::new(),
            material_id: ContentMaterialId::new(),
            key_id: MaterialKeyId::new(),
            nonce: IntentNonce::new(),
            output_ordinal: 0,
        }
    }
}

fn write_bootstrap(root: &Path) {
    let key = root.join(BOOTSTRAP_KEY_ID);
    let version = key.join("v1");
    fs::create_dir_all(&version).unwrap();
    fs::write(key.join("scope"), BOOTSTRAP_SCOPE).unwrap();
    fs::write(key.join("purpose"), "storage").unwrap();
    fs::write(key.join("algorithm"), BOOTSTRAP_ALGORITHM).unwrap();
    fs::write(version.join("state"), "active").unwrap();
    fs::write(version.join("private.pkcs8"), [0x5A; 32]).unwrap();
}

#[test]
fn provisional_output_claim_is_replayable_but_never_ordinary_unwrap_capable() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    let first = fixture
        .vault()
        .create_embedding_output_if_absent(&binding)
        .unwrap();
    let replay = fixture
        .vault()
        .create_embedding_output_if_absent(&binding)
        .unwrap();
    assert_eq!(
        replay, first,
        "a second host process adopts the claim receipt"
    );
    assert!(matches!(
        fixture.vault().unwrap(binding.key_id, &mut |_| panic!(
            "must not expose provisional key"
        )),
        Err(VaultError::Provisional)
    ));
    let mut changed = binding.clone();
    changed.nonce = IntentNonce::new();
    assert!(matches!(
        fixture.vault().create_embedding_output_if_absent(&changed),
        Err(VaultError::BindingMismatch)
    ));
}

#[test]
fn result_preparation_can_lend_only_the_exact_full_provisional_binding() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    fixture
        .vault()
        .create_embedding_output_if_absent(&binding)
        .unwrap();

    let mut callback_ran = false;
    fixture
        .vault()
        .with_embedding_output_key(&binding, &mut |dek| {
            let _ = dek;
            callback_ran = true;
        })
        .unwrap();
    assert!(
        callback_ran,
        "the exact output authority receives the scoped DEK"
    );

    let mismatches = [
        {
            let mut changed = binding.clone();
            changed.workspace_id = WorkspaceId::new();
            changed
        },
        {
            let mut changed = binding.clone();
            changed.job_id = EmbeddingJobId::new();
            changed
        },
        {
            let mut changed = binding.clone();
            changed.intent_id = MaterialKeyCreationIntentId::new();
            changed
        },
        {
            let mut changed = binding.clone();
            changed.material_id = ContentMaterialId::new();
            changed
        },
        {
            let mut changed = binding.clone();
            changed.nonce = IntentNonce::new();
            changed
        },
        {
            let mut changed = binding.clone();
            changed.output_ordinal = 1;
            changed
        },
    ];
    for changed in mismatches {
        assert!(matches!(
            fixture
                .vault()
                .with_embedding_output_key(&changed, &mut |_| panic!(
                    "mismatched claim leaked a DEK"
                )),
            Err(VaultError::BindingMismatch)
        ));
    }
    let mut changed_key = binding.clone();
    changed_key.key_id = MaterialKeyId::new();
    assert!(matches!(
        fixture
            .vault()
            .with_embedding_output_key(&changed_key, &mut |_| panic!(
                "different key id leaked a DEK"
            )),
        Err(VaultError::NotFound)
    ));
    assert!(matches!(
        fixture.vault().unwrap(binding.key_id, &mut |_| panic!(
            "ordinary unwrap leaked a provisional DEK"
        )),
        Err(VaultError::Provisional)
    ));
}

#[test]
fn provisional_retirement_removes_the_envelope_and_replays_its_witness() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    fixture
        .vault()
        .create_embedding_output_if_absent(&binding)
        .unwrap();
    let first = fixture.vault().retire_embedding_output(&binding).unwrap();
    let replay = fixture.vault().retire_embedding_output(&binding).unwrap();
    assert_eq!(replay, first);
    assert!(
        !fixture
            .vault
            .path()
            .join(binding.key_id.as_uuid().to_string())
            .join("active")
            .join("envelope")
            .exists(),
        "retirement rechecks removal before it returns an installed erased receipt"
    );
    assert!(matches!(
        fixture.vault().unwrap(binding.key_id, &mut |_| panic!(
            "must not expose retired key"
        )),
        Err(VaultError::Provisional)
    ));
}

struct OrdinaryOnlyVault;

impl MaterialKeyVault for OrdinaryOnlyVault {
    fn create_if_absent(
        &self,
        _key_id: MaterialKeyId,
        _nonce: IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    fn unwrap(
        &self,
        _key_id: MaterialKeyId,
        _use_dek: &mut dyn FnMut(&vestrace_domain::ZeroizingDek),
    ) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }

    fn prepare_erasure(
        &self,
        _key_id: MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    fn erase(&self, _key_id: MaterialKeyId) -> Result<vestrace_domain::ErasureReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }
}

#[test]
fn arc_trait_object_forwards_output_operations_and_defaults_remain_unavailable() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    let host: Arc<dyn MaterialKeyVault> = Arc::new(fixture.vault());
    let receipt =
        <Arc<dyn MaterialKeyVault> as MaterialKeyVault>::create_embedding_output_if_absent(
            &host, &binding,
        )
        .unwrap();
    assert_eq!(
        <Arc<dyn MaterialKeyVault> as MaterialKeyVault>::create_embedding_output_if_absent(
            &host, &binding,
        )
        .unwrap(),
        receipt
    );
    <Arc<dyn MaterialKeyVault> as MaterialKeyVault>::retire_embedding_output(&host, &binding)
        .unwrap();

    let ordinary_only: Arc<dyn MaterialKeyVault> = Arc::new(OrdinaryOnlyVault);
    assert_eq!(
        <Arc<dyn MaterialKeyVault> as MaterialKeyVault>::create_embedding_output_if_absent(
            &ordinary_only,
            &binding,
        ),
        Err(VaultError::Unavailable)
    );
    assert_eq!(
        <Arc<dyn MaterialKeyVault> as MaterialKeyVault>::retire_embedding_output(
            &ordinary_only,
            &binding,
        ),
        Err(VaultError::Unavailable)
    );
}

#[test]
fn legacy_ordinary_record_survives_restart_without_becoming_provisional() {
    let fixture = Fixture::new();
    let key_id = MaterialKeyId::new();
    let nonce = IntentNonce::new();
    let receipt = fixture.vault().create_if_absent(key_id, nonce).unwrap();
    let key_dir = fixture.vault.path().join(key_id.as_uuid().to_string());
    let envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(key_dir.join("active").join("envelope")).unwrap())
            .unwrap();
    let legacy = serde_json::json!({
        "nonce": nonce.as_uuid(),
        "receipt": receipt.as_uuid(),
        "envelope": envelope.get("envelope").unwrap(),
        "fence_receipt": null,
        "erasure_receipt": null,
    });
    fs::remove_file(key_dir.join("active").join("envelope")).unwrap();
    fs::remove_dir(key_dir.join("active")).unwrap();
    fs::remove_file(key_dir.join("claim")).unwrap();
    fs::remove_dir(&key_dir).unwrap();
    fs::write(
        fixture
            .vault
            .path()
            .join(format!("{}.json", key_id.as_uuid())),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();

    let restarted = fixture.vault();
    assert_eq!(restarted.create_if_absent(key_id, nonce).unwrap(), receipt);
    let mut observed = false;
    restarted
        .unwrap(key_id, &mut |_| observed = true)
        .expect("legacy ordinary keys remain unwrap-compatible");
    assert!(observed);
    let mut output_binding = fixture.binding();
    output_binding.key_id = key_id;
    output_binding.nonce = nonce;
    assert_eq!(
        restarted.create_embedding_output_if_absent(&output_binding),
        Err(VaultError::BindingMismatch)
    );
}

#[test]
fn malformed_and_incomplete_fixed_stages_fail_closed() {
    let fixture = Fixture::new();
    let malformed = fixture.binding();
    let malformed_dir = fixture
        .vault
        .path()
        .join(malformed.key_id.as_uuid().to_string());
    fs::create_dir(&malformed_dir).unwrap();
    fs::write(malformed_dir.join("claim"), b"{").unwrap();
    assert_eq!(
        fixture
            .vault()
            .create_embedding_output_if_absent(&malformed),
        Err(VaultError::Unavailable)
    );

    let incomplete = fixture.binding();
    let incomplete_dir = fixture
        .vault
        .path()
        .join(incomplete.key_id.as_uuid().to_string());
    fs::create_dir(&incomplete_dir).unwrap();
    fs::create_dir(incomplete_dir.join("active")).unwrap();
    fs::write(
        incomplete_dir.join("active").join("envelope"),
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "claim_receipt": uuid::Uuid::now_v7(),
            "envelope": {"nonce": vec![0_u8; 12], "ciphertext": []},
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture
            .vault()
            .create_embedding_output_if_absent(&incomplete),
        Err(VaultError::Unavailable)
    );
}

const CHILD_OPERATION: &str = "VESTRACE_OUTPUT_VAULT_CHILD_OPERATION";
const CHILD_RESULT: &str = "VESTRACE_OUTPUT_VAULT_CHILD_RESULT";
const CHILD_CHECKPOINT: &str = "VESTRACE_OUTPUT_VAULT_CHILD_CHECKPOINT";
const CHILD_CHECKPOINT_MARKER: &str = "VESTRACE_OUTPUT_VAULT_CHILD_CHECKPOINT_MARKER";
const DB_CHILD_BOUNDARY: &str = "VESTRACE_OUTPUT_DB_CHILD_BOUNDARY";
const DB_CHILD_DATABASE: &str = "VESTRACE_OUTPUT_DB_CHILD_DATABASE";
const DB_CHILD_PRINCIPAL: &str = "VESTRACE_OUTPUT_DB_CHILD_PRINCIPAL";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputServiceBoundary {
    ReservationCommitted,
    HostCreatedBeforeReceipt,
    ReceiptPersisted,
    RetirementVaultBeforeReceipt,
}

impl OutputServiceBoundary {
    fn as_str(self) -> &'static str {
        match self {
            Self::ReservationCommitted => "reservation_committed",
            Self::HostCreatedBeforeReceipt => "host_created_before_receipt",
            Self::ReceiptPersisted => "receipt_persisted",
            Self::RetirementVaultBeforeReceipt => "retirement_vault_before_receipt",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "reservation_committed" => Self::ReservationCommitted,
            "host_created_before_receipt" => Self::HostCreatedBeforeReceipt,
            "receipt_persisted" => Self::ReceiptPersisted,
            "retirement_vault_before_receipt" => Self::RetirementVaultBeforeReceipt,
            other => panic!("unknown output service checkpoint {other}"),
        }
    }
}

struct CheckpointOutputRepository {
    inner: Arc<PgEmbeddingOutputKeyRepository>,
    boundary: OutputServiceBoundary,
    marker: PathBuf,
}

impl CheckpointOutputRepository {
    fn checkpoint(&self) -> ! {
        fs::write(&self.marker, self.boundary.as_str()).unwrap();
        loop {
            std::thread::park();
        }
    }
}

#[async_trait::async_trait]
impl EmbeddingOutputKeyRepository for CheckpointOutputRepository {
    async fn claim_next(
        &self,
        context: &RequestContext,
    ) -> Result<Option<EmbeddingOutputKeyPlan>, ApplicationError> {
        let plan = self.inner.claim_next(context).await?;
        if plan.is_some() && self.boundary == OutputServiceBoundary::ReservationCommitted {
            self.checkpoint();
        }
        Ok(plan)
    }

    async fn record_receipt(
        &self,
        context: &RequestContext,
        binding: &EmbeddingOutputKeyBinding,
        receipt: VaultReceipt,
    ) -> Result<EmbeddingOutputKeyProgress, ApplicationError> {
        if self.boundary == OutputServiceBoundary::HostCreatedBeforeReceipt {
            self.checkpoint();
        }
        let progress = self.inner.record_receipt(context, binding, receipt).await?;
        if self.boundary == OutputServiceBoundary::ReceiptPersisted {
            self.checkpoint();
        }
        Ok(progress)
    }

    async fn record_retirement(
        &self,
        context: &RequestContext,
        binding: &EmbeddingOutputKeyBinding,
        receipt: ErasureReceipt,
    ) -> Result<(), ApplicationError> {
        if self.boundary == OutputServiceBoundary::RetirementVaultBeforeReceipt {
            self.checkpoint();
        }
        self.inner
            .record_retirement(context, binding, receipt)
            .await
    }
}

fn child_command(
    fixture: &Fixture,
    binding: &EmbeddingOutputKeyBinding,
    operation: &str,
    result: &Path,
) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg("output_vault_process_actor")
        .arg("--nocapture")
        .env(CHILD_OPERATION, operation)
        .env(CHILD_RESULT, result)
        .env("VESTRACE_OUTPUT_VAULT_ROOT", fixture.vault.path())
        .env("VESTRACE_OUTPUT_BOOTSTRAP_ROOT", fixture.bootstrap.path())
        .env(
            "VESTRACE_OUTPUT_WORKSPACE_ID",
            binding.workspace_id.to_string(),
        )
        .env("VESTRACE_OUTPUT_JOB_ID", binding.job_id.to_string())
        .env("VESTRACE_OUTPUT_INTENT_ID", binding.intent_id.to_string())
        .env(
            "VESTRACE_OUTPUT_MATERIAL_ID",
            binding.material_id.to_string(),
        )
        .env("VESTRACE_OUTPUT_KEY_ID", binding.key_id.to_string())
        .env("VESTRACE_OUTPUT_NONCE", binding.nonce.to_string())
        .env(
            "VESTRACE_OUTPUT_ORDINAL",
            binding.output_ordinal.to_string(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

fn db_service_child_command(
    fixture: &Fixture,
    binding: &EmbeddingOutputKeyBinding,
    context: &RequestContext,
    owner: &PgPool,
    boundary: OutputServiceBoundary,
    marker: &Path,
) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg("output_db_service_process_actor")
        .arg("--nocapture")
        .env(DB_CHILD_BOUNDARY, boundary.as_str())
        .env(
            DB_CHILD_DATABASE,
            owner
                .connect_options()
                .get_database()
                .expect("SQLx test database has a name"),
        )
        .env(DB_CHILD_PRINCIPAL, context.principal_id.to_string())
        .env(CHILD_CHECKPOINT_MARKER, marker)
        .env("VESTRACE_OUTPUT_VAULT_ROOT", fixture.vault.path())
        .env("VESTRACE_OUTPUT_BOOTSTRAP_ROOT", fixture.bootstrap.path())
        .env(
            "VESTRACE_OUTPUT_WORKSPACE_ID",
            binding.workspace_id.to_string(),
        )
        .env("VESTRACE_OUTPUT_JOB_ID", binding.job_id.to_string())
        .env("VESTRACE_OUTPUT_INTENT_ID", binding.intent_id.to_string())
        .env(
            "VESTRACE_OUTPUT_MATERIAL_ID",
            binding.material_id.to_string(),
        )
        .env("VESTRACE_OUTPUT_KEY_ID", binding.key_id.to_string())
        .env("VESTRACE_OUTPUT_NONCE", binding.nonce.to_string())
        .env(
            "VESTRACE_OUTPUT_ORDINAL",
            binding.output_ordinal.to_string(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

fn finish_child(child: Child, result: &Path) -> String {
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(result).unwrap()
}

fn fault_child_command(
    fixture: &Fixture,
    binding: &EmbeddingOutputKeyBinding,
    operation: &str,
    checkpoint: &str,
    marker: &Path,
    result: &Path,
) -> Command {
    let mut command = child_command(fixture, binding, operation, result);
    command
        .env(CHILD_CHECKPOINT, checkpoint)
        .env(CHILD_CHECKPOINT_MARKER, marker);
    command
}

fn wait_for_checkpoint(child: &mut Child, marker: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !marker.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "child exited before publishing checkpoint {}",
            marker.display()
        );
        assert!(
            std::time::Instant::now() < deadline,
            "child did not reach checkpoint {}",
            marker.display()
        );
        std::thread::yield_now();
    }
}

fn kill_at_checkpoint(mut child: Child, marker: &Path) {
    wait_for_checkpoint(&mut child, marker);
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(!status.success(), "checkpoint child was not terminated");
}

fn parse_child_uuid(result: &str) -> uuid::Uuid {
    result
        .strip_prefix("ok:")
        .unwrap_or_else(|| panic!("child did not return a witness: {result}"))
        .parse()
        .unwrap()
}

fn child_binding_from_environment() -> EmbeddingOutputKeyBinding {
    let parse = |name: &str| std::env::var(name).unwrap().parse::<uuid::Uuid>().unwrap();
    EmbeddingOutputKeyBinding {
        workspace_id: WorkspaceId::from_uuid(parse("VESTRACE_OUTPUT_WORKSPACE_ID")),
        job_id: EmbeddingJobId::from_uuid(parse("VESTRACE_OUTPUT_JOB_ID")),
        intent_id: MaterialKeyCreationIntentId::from_uuid(parse("VESTRACE_OUTPUT_INTENT_ID")),
        material_id: ContentMaterialId::from_uuid(parse("VESTRACE_OUTPUT_MATERIAL_ID")),
        key_id: MaterialKeyId::from_uuid(parse("VESTRACE_OUTPUT_KEY_ID")),
        nonce: IntentNonce::from_uuid(parse("VESTRACE_OUTPUT_NONCE")),
        output_ordinal: std::env::var("VESTRACE_OUTPUT_ORDINAL")
            .unwrap()
            .parse()
            .unwrap(),
    }
}

fn child_vault(binding: &EmbeddingOutputKeyBinding) -> HostMaterialKeyVault {
    let reference = KeyReference::new(
        MOUNTED_SECRET_STORE_PROVIDER,
        BOOTSTRAP_KEY_ID,
        "v1",
        KeyPurpose::Storage,
        BOOTSTRAP_SCOPE,
        BOOTSTRAP_ALGORITHM,
    )
    .unwrap();
    let request = SecretResolutionRequest::new(
        binding.workspace_id,
        BOOTSTRAP_SCOPE,
        "test://embedding-output-key-child",
    );
    HostMaterialKeyVault::new(
        PathBuf::from(std::env::var_os("VESTRACE_OUTPUT_VAULT_ROOT").unwrap()),
        PathBuf::from(std::env::var_os("VESTRACE_OUTPUT_BOOTSTRAP_ROOT").unwrap()),
        reference,
        request,
    )
    .unwrap()
}

#[test]
fn output_vault_process_actor() {
    let Some(operation) = std::env::var_os(CHILD_OPERATION) else {
        return;
    };
    let binding = child_binding_from_environment();
    let mut vault = child_vault(&binding);
    if let Some(checkpoint) = std::env::var_os(CHILD_CHECKPOINT) {
        vault = vault.with_test_fault_checkpoint(
            checkpoint.to_string_lossy(),
            PathBuf::from(std::env::var_os(CHILD_CHECKPOINT_MARKER).unwrap()),
        );
    }
    let rendered = match operation.to_str().unwrap() {
        "output_create" => vault
            .create_embedding_output_if_absent(&binding)
            .map(|receipt| format!("ok:{}", receipt.as_uuid())),
        "output_retire" => vault
            .retire_embedding_output(&binding)
            .map(|receipt| format!("ok:{}", receipt.as_uuid())),
        "output_bind" => vault
            .bind_embedding_output(
                &binding,
                vestrace_application::EmbeddingResultPreparationId::from_uuid(
                    uuid::Uuid::parse_str(
                        &std::env::var("VESTRACE_OUTPUT_PREPARATION_ID").unwrap(),
                    )
                    .unwrap(),
                ),
            )
            .map(|receipt| format!("ok:{}", receipt.as_uuid())),
        "ordinary_create" => vault
            .create_if_absent(binding.key_id, binding.nonce)
            .map(|receipt| format!("ok:{}", receipt.as_uuid())),
        other => panic!("unknown child operation {other}"),
    }
    .unwrap_or_else(|error| format!("err:{error:?}"));
    fs::write(
        PathBuf::from(std::env::var_os(CHILD_RESULT).unwrap()),
        rendered,
    )
    .unwrap();
}

#[test]
fn output_db_service_process_actor() {
    let Some(boundary) = std::env::var_os(DB_CHILD_BOUNDARY) else {
        return;
    };
    let boundary = OutputServiceBoundary::parse(&boundary.to_string_lossy());
    let binding = child_binding_from_environment();
    let context = RequestContext::new(
        binding.workspace_id,
        PrincipalId::from_uuid(std::env::var(DB_CHILD_PRINCIPAL).unwrap().parse().unwrap()),
    );
    let marker = PathBuf::from(std::env::var_os(CHILD_CHECKPOINT_MARKER).unwrap());
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL").unwrap();
            let options = PgConnectOptions::from_str(&runtime_url)
                .unwrap()
                .database(&std::env::var(DB_CHILD_DATABASE).unwrap());
            let pool = PgPoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await
                .unwrap();
            let identity: (String, bool) = sqlx::query_as(
                "SELECT current_user::TEXT, (SELECT rolsuper FROM pg_roles WHERE rolname=current_user)::BOOLEAN",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(identity, ("vestrace".to_owned(), false));
            let repository = Arc::new(PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(
                pool.clone(),
            )));
            let checkpoint = Arc::new(CheckpointOutputRepository {
                inner: repository,
                boundary,
                marker,
            });
            let service = EmbeddingOutputKeyService::new(checkpoint, Arc::new(child_vault(&binding)));
            service
                .reconcile_one(&context)
                .await
                .expect("child output service must reach its configured durable boundary");
            panic!("child output service returned without reaching its configured boundary");
        });
}

#[test]
fn independent_processes_converge_on_one_claim_and_refuse_mismatches() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    let results = TempDir::new().unwrap();
    let first_path = results.path().join("first");
    let second_path = results.path().join("second");
    let first = child_command(&fixture, &binding, "output_create", &first_path)
        .spawn()
        .unwrap();
    let second = child_command(&fixture, &binding, "output_create", &second_path)
        .spawn()
        .unwrap();
    let first_receipt = parse_child_uuid(&finish_child(first, &first_path));
    let second_receipt = parse_child_uuid(&finish_child(second, &second_path));
    assert_eq!(first_receipt, second_receipt);

    let mut changed = binding.clone();
    changed.nonce = IntentNonce::new();
    let mismatch_path = results.path().join("mismatch");
    let mismatch = child_command(&fixture, &changed, "output_create", &mismatch_path)
        .spawn()
        .unwrap();
    assert_eq!(
        finish_child(mismatch, &mismatch_path),
        "err:BindingMismatch"
    );
}

#[test]
fn independent_processes_converge_for_both_create_retire_orders() {
    let fixture = Fixture::new();
    let results = TempDir::new().unwrap();

    let create_first = fixture.binding();
    let create_path = results.path().join("create-first");
    let created = child_command(&fixture, &create_first, "output_create", &create_path)
        .spawn()
        .unwrap();
    parse_child_uuid(&finish_child(created, &create_path));
    let retire_path = results.path().join("retire-second");
    let retired = child_command(&fixture, &create_first, "output_retire", &retire_path)
        .spawn()
        .unwrap();
    parse_child_uuid(&finish_child(retired, &retire_path));
    assert!(
        !fixture
            .vault
            .path()
            .join(create_first.key_id.as_uuid().to_string())
            .join("active")
            .join("envelope")
            .exists()
    );

    let retire_first = fixture.binding();
    let early_retire_path = results.path().join("retire-first");
    let early_retire = child_command(&fixture, &retire_first, "output_retire", &early_retire_path)
        .spawn()
        .unwrap();
    parse_child_uuid(&finish_child(early_retire, &early_retire_path));
    let late_create_path = results.path().join("create-second");
    let late_create = child_command(&fixture, &retire_first, "output_create", &late_create_path)
        .spawn()
        .unwrap();
    assert_eq!(finish_child(late_create, &late_create_path), "err:Erased");
    assert!(
        !fixture
            .vault
            .path()
            .join(retire_first.key_id.as_uuid().to_string())
            .join("active")
            .join("envelope")
            .exists()
    );
}

#[test]
fn independent_processes_fence_ordinary_and_output_claim_namespaces() {
    let fixture = Fixture::new();
    let results = TempDir::new().unwrap();

    let output_first = fixture.binding();
    let output_path = results.path().join("output-first");
    let output = child_command(&fixture, &output_first, "output_create", &output_path)
        .spawn()
        .unwrap();
    parse_child_uuid(&finish_child(output, &output_path));
    let ordinary_late_path = results.path().join("ordinary-late");
    let ordinary_late = child_command(
        &fixture,
        &output_first,
        "ordinary_create",
        &ordinary_late_path,
    )
    .spawn()
    .unwrap();
    assert_eq!(
        finish_child(ordinary_late, &ordinary_late_path),
        "err:BindingMismatch"
    );

    let ordinary_first = fixture.binding();
    let ordinary_path = results.path().join("ordinary-first");
    let ordinary = child_command(&fixture, &ordinary_first, "ordinary_create", &ordinary_path)
        .spawn()
        .unwrap();
    parse_child_uuid(&finish_child(ordinary, &ordinary_path));
    let output_late_path = results.path().join("output-late");
    let output_late = child_command(
        &fixture,
        &ordinary_first,
        "output_create",
        &output_late_path,
    )
    .spawn()
    .unwrap();
    assert_eq!(
        finish_child(output_late, &output_late_path),
        "err:NonceMismatch"
    );
}

#[test]
fn child_death_at_every_fixed_stage_replays_without_a_late_envelope() {
    let fixture = Fixture::new();
    let artifacts = TempDir::new().unwrap();

    for checkpoint in ["claim", "envelope"] {
        let binding = fixture.binding();
        let marker = artifacts.path().join(format!("{checkpoint}-marker"));
        let result = artifacts.path().join(format!("{checkpoint}-result"));
        let child = fault_child_command(
            &fixture,
            &binding,
            "output_create",
            checkpoint,
            &marker,
            &result,
        )
        .spawn()
        .unwrap();
        kill_at_checkpoint(child, &marker);
        let replay_path = artifacts.path().join(format!("{checkpoint}-replay"));
        let replay = child_command(&fixture, &binding, "output_create", &replay_path)
            .spawn()
            .unwrap();
        parse_child_uuid(&finish_child(replay, &replay_path));
    }

    for checkpoint in ["fence", "erased"] {
        let binding = fixture.binding();
        let create_path = artifacts.path().join(format!("{checkpoint}-create"));
        let create = child_command(&fixture, &binding, "output_create", &create_path)
            .spawn()
            .unwrap();
        parse_child_uuid(&finish_child(create, &create_path));
        let marker = artifacts.path().join(format!("{checkpoint}-marker"));
        let result = artifacts.path().join(format!("{checkpoint}-result"));
        let child = fault_child_command(
            &fixture,
            &binding,
            "output_retire",
            checkpoint,
            &marker,
            &result,
        )
        .spawn()
        .unwrap();
        kill_at_checkpoint(child, &marker);
        let replay_path = artifacts.path().join(format!("{checkpoint}-replay"));
        let replay = child_command(&fixture, &binding, "output_retire", &replay_path)
            .spawn()
            .unwrap();
        parse_child_uuid(&finish_child(replay, &replay_path));
        let key_dir = fixture
            .vault
            .path()
            .join(binding.key_id.as_uuid().to_string());
        assert!(!key_dir.join("active").join("envelope").exists());
        assert!(!key_dir.join("retired-active").join("envelope").exists());
    }
}

#[test]
fn retirement_moves_the_active_namespace_before_a_late_creator_can_link() {
    let fixture = Fixture::new();
    let artifacts = TempDir::new().unwrap();
    let binding = fixture.binding();
    let marker = artifacts.path().join("before-envelope-marker");
    let create_result = artifacts.path().join("late-create-result");
    let mut creator = fault_child_command(
        &fixture,
        &binding,
        "output_create",
        "before_envelope",
        &marker,
        &create_result,
    )
    .spawn()
    .unwrap();
    wait_for_checkpoint(&mut creator, &marker);

    let retire_result = artifacts.path().join("retire-result");
    let retire = child_command(&fixture, &binding, "output_retire", &retire_result)
        .spawn()
        .unwrap();
    parse_child_uuid(&finish_child(retire, &retire_result));
    fs::write(marker.with_extension("release"), b"continue").unwrap();
    assert_eq!(finish_child(creator, &create_result), "err:ErasurePrepared");
    let key_dir = fixture
        .vault
        .path()
        .join(binding.key_id.as_uuid().to_string());
    assert!(!key_dir.join("active").join("envelope").exists());
    assert!(!key_dir.join("retired-active").join("envelope").exists());
}

async fn live_source(runtime: &PgPool, fixture: &common::AcceptedJob) -> ContentMaterialId {
    let material_id = ContentMaterialId::new();
    let intent_id = MaterialKeyCreationIntentId::new();
    let key_id = MaterialKeyId::new();
    let mut framed_ciphertext = vec![0x51_u8; 4096];
    framed_ciphertext[..5].copy_from_slice(b"VMRF\x01");
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'model_request_input',$6,0)")
        .bind(intent_id.as_uuid())
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(material_id.as_uuid())
        .bind(key_id.as_uuid())
        .bind(uuid::Uuid::now_v7())
        .bind(fixture.job_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
        .bind(intent_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
        .bind(intent_id.as_uuid())
        .bind(uuid::Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_prepare_content_material($1,$2,$3,4096)")
        .bind(intent_id.as_uuid())
        .bind(uuid::Uuid::now_v7())
        .bind(framed_ciphertext)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
        .bind(intent_id.as_uuid())
        .bind(uuid::Uuid::now_v7())
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

async fn prepare_delivery_fixture(
    owner: &PgPool,
    runtime: &PgPool,
) -> (common::AcceptedJob, ContentMaterialId) {
    let fixture = common::prepare_delivery_embedding_job(owner, runtime).await;
    complete_delivery_fixture(owner, runtime, fixture).await
}

async fn pre_dispatch_gate(
    owner: &PgPool,
    fixture: &common::AcceptedJob,
) -> Result<String, sqlx::Error> {
    let mut transaction = owner.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    for (setting, value) in [
        (
            "vestrace.workspace_id",
            fixture.context.workspace_id.as_uuid().to_string(),
        ),
        (
            "vestrace.principal_id",
            fixture.context.principal_id.as_uuid().to_string(),
        ),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1,$2,true)")
            .bind(setting)
            .bind(value)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    }
    let result =
        sqlx::query_scalar("SELECT vestrace_lock_embedding_job_pre_dispatch_gate($1,$2,false)")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.external_effect_id)
            .fetch_one(&mut *transaction)
            .await;
    transaction.rollback().await.unwrap();
    result
}

async fn dispatch_through_embedding_fence(runtime: &PgPool, fixture: &common::AcceptedJob) {
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "SELECT * FROM vestrace_try_admit_provider_dispatch(\
           $1,$2,$3,$4,$5,$6,$7,$8,'embedding_job',NULL,NULL,$9,NULL,NULL,NULL,60)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(uuid::Uuid::now_v7())
    .bind(uuid::Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(fixture.external_effect_id)
    .bind(fixture.evidence_id)
    .bind(fixture.snapshot_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    let authorization_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO external_effect_authorizations(\
           id,effect_id,workspace_id,policy_id,policy_version,subject_id,capability,operation,\
           resource_scope,result,reason,input_state,matched_grant_id,decided_at,payload) \
         SELECT $1,id,workspace_id,NULL,'embedding-dispatch-v1',$2,'export.read',\
           'produce a governed embedding','http://127.0.0.1:1234/v1/embeddings',\
           'allow','configured_allowance','{}'::jsonb,NULL,NOW(),'{}'::jsonb \
           FROM external_effect_intents WHERE id=$3 AND workspace_id=$4",
    )
    .bind(authorization_id)
    .bind(fixture.context.principal_id.as_uuid())
    .bind(fixture.external_effect_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions(\
           effect_id,workspace_id,status,cause,cause_ref,recorded_at) \
         VALUES($1,$2,'authorized','authorization_recorded',$3,NOW())",
    )
    .bind(fixture.external_effect_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(authorization_id.to_string())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions(\
           effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) \
         VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),'output-key-readiness',\
           NOW()+INTERVAL '60 seconds')",
    )
    .bind(fixture.external_effect_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id.to_string())
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

async fn prepare_credential_delivery_fixture(
    owner: &PgPool,
    runtime: &PgPool,
) -> (common::AcceptedJob, ContentMaterialId) {
    let fixture =
        common::prepare_delivery_embedding_job_with_pinned_credential(owner, runtime).await;
    complete_delivery_fixture(owner, runtime, fixture).await
}

async fn complete_delivery_fixture(
    owner: &PgPool,
    runtime: &PgPool,
    fixture: common::AcceptedJob,
) -> (common::AcceptedJob, ContentMaterialId) {
    let source = live_source(runtime, &fixture).await;
    common::make_dispatchable(owner, runtime, &fixture).await;
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
    sqlx::query(
        "INSERT INTO model_request_evidence_nodes(\
         id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id) \
         VALUES($1,$2,$3,8,'governed_input_material',$4)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.evidence_id)
    .bind(source.as_uuid())
    .execute(&mut *evidence)
    .await
    .unwrap();
    evidence.commit().await.unwrap();
    (fixture, source)
}

fn acceptance_command(
    fixture: &common::AcceptedJob,
    receipt_id: uuid::Uuid,
    idempotency_key: &str,
    outputs: Vec<DeliveryOutputIdentity>,
) -> AcceptDeliveryOutputs {
    let at = chrono::DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap();
    AcceptDeliveryOutputs {
        receipt_id,
        idempotency_key: idempotency_key.to_owned(),
        acceptance: AcceptEmbeddingJob {
            job_id: fixture.job_id,
            space_registration_id: EmbeddingSpaceId::from_uuid(fixture.space_registration_id),
            kind: EmbeddingJobKind::Delivery,
            model_binding_snapshot_id: fixture.snapshot_id,
            intent: fixture.intent.clone(),
            model_request_evidence_id: ModelRequestEvidenceId::from_uuid(fixture.evidence_id),
            retries_unknown_embedding_job_id: None,
            expected_predecessor_version: None,
            idempotency: Some(IdempotencyRecord {
                idempotency_key: idempotency_key.to_owned(),
                workspace_id: fixture.context.workspace_id,
                request_hash: format!("delivery-output:{receipt_id}"),
                response_payload: None,
                status: "completed".to_owned(),
                created_at: at,
                expires_at: at + Duration::hours(24),
            }),
            outbox: vec![OutboxMessage {
                id: OutboxId::from_uuid(receipt_id),
                workspace_id: fixture.context.workspace_id,
                topic: "embedding.job.delivery_accepted".to_owned(),
                payload: serde_json::json!({"receipt_id": receipt_id}),
                created_at: at,
                attempts: 0,
            }],
            audit: AuditEvent::new(
                AuditEventId::from_uuid(receipt_id),
                fixture.context.workspace_id,
                fixture.context.principal_id,
                "embedding.job.delivery_accepted",
                "embedding_job",
                fixture.job_id.as_uuid(),
                serde_json::json!({"receipt_id": receipt_id}),
                at,
            )
            .unwrap(),
        },
        outputs,
    }
}

fn outputs(count: usize) -> Vec<DeliveryOutputIdentity> {
    (0..count)
        .map(|ordinal| DeliveryOutputIdentity {
            output_ordinal: ordinal as u64,
            intent_id: MaterialKeyCreationIntentId::new(),
            material_id: ContentMaterialId::new(),
            key_id: MaterialKeyId::new(),
            nonce: IntentNonce::new(),
        })
        .collect()
}

fn cancellation_termination(
    fixture: &common::AcceptedJob,
    receipt_id: uuid::Uuid,
    idempotency_key: &str,
) -> TerminateEmbeddingJobPreDispatch {
    let request = AuthorizationRequest::new(
        Capability::ExecutionWrite,
        "embedding.job.cancel",
        ResourceScope::workspace().to_string(),
        RiskCategory::Low,
    );
    TerminateEmbeddingJobPreDispatch {
        receipt_id,
        job_id: fixture.job_id,
        expected_version: 1,
        idempotency_key: idempotency_key.to_owned(),
        terminal_state: PreDispatchTerminalState::Cancelled,
        evidence: PreDispatchTerminationEvidence::CancellationAuthorization(Box::new(
            PolicyDecision {
                id: PolicyDecisionId::new(),
                policy_id: None,
                policy_version: "output-retirement-test-v1".to_owned(),
                workspace_id: fixture.context.workspace_id,
                subject_id: fixture.context.principal_id,
                capability: Capability::ExecutionWrite,
                operation: "embedding.job.cancel".to_owned(),
                resource_scope: ResourceScope::workspace().to_string(),
                result: PolicyDecisionResult::Allow,
                reason: PolicyDecisionReason::ConfiguredAllowance,
                input_state: PolicyInputState::from_request(
                    fixture.context.workspace_id,
                    fixture.context.principal_id,
                    &request,
                ),
                matched_grant_id: None,
                decided_at: Utc::now(),
            },
        )),
    }
}

async fn prepare_process_output(
    owner: &PgPool,
    runtime: &PgPool,
    label: &str,
) -> (common::AcceptedJob, DeliveryOutputIdentity) {
    let (fixture, _) = prepare_delivery_fixture(owner, runtime).await;
    let output = outputs(1).remove(0);
    PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()))
        .accept_delivery_outputs(
            &fixture.context,
            acceptance_command(
                &fixture,
                uuid::Uuid::now_v7(),
                &format!("process-restart-{label}"),
                vec![output.clone()],
            ),
        )
        .await
        .unwrap();
    (fixture, output)
}

async fn prepare_retired_output(
    owner: &PgPool,
    runtime: &PgPool,
    label: &str,
) -> (
    common::AcceptedJob,
    ContentMaterialId,
    TerminateEmbeddingJobPreDispatch,
) {
    let (fixture, source) = prepare_delivery_fixture(owner, runtime).await;
    let repository = Arc::new(PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(
        runtime.clone(),
    )));
    repository
        .accept_delivery_outputs(
            &fixture.context,
            acceptance_command(
                &fixture,
                uuid::Uuid::now_v7(),
                &format!("terminal-race-accept-{label}"),
                outputs(1),
            ),
        )
        .await
        .unwrap();
    let vault = Fixture::new();
    let service = EmbeddingOutputKeyService::new(repository, Arc::new(vault.vault()));
    assert!(matches!(
        service.reconcile_one(&fixture.context).await.unwrap(),
        Some(EmbeddingOutputKeyProgress::Prepared { .. })
    ));
    let termination = cancellation_termination(
        &fixture,
        uuid::Uuid::now_v7(),
        &format!("terminal-race-authority-{label}"),
    );
    service
        .request_retirement(
            &fixture.context,
            RequestEmbeddingOutputRetirement {
                termination: termination.clone(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        service.reconcile_one(&fixture.context).await.unwrap(),
        Some(EmbeddingOutputKeyProgress::Retired { .. })
    ));
    (fixture, source, termination)
}

async fn named_runtime_pool(source: &PgPool, application_name: &str) -> PgPool {
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL").unwrap();
    let parsed = PgConnectOptions::from_str(&runtime_url).unwrap();
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .unwrap();
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(parsed.get_username())
                .password(password)
                .application_name(application_name),
        )
        .await
        .unwrap()
}

async fn request_output_retirement_without_committing(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &RequestContext,
    command: &TerminateEmbeddingJobPreDispatch,
) -> uuid::Uuid {
    let PreDispatchTerminationEvidence::CancellationAuthorization(decision) = &command.evidence
    else {
        panic!("the lock-order fixture requires cancellation authorization");
    };
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut **transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(context.principal_id.to_string())
        .fetch_one(&mut **transaction)
        .await
        .unwrap();
    sqlx::query_scalar(
        "SELECT vestrace_request_embedding_output_retirement(\
         $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(command.receipt_id)
    .bind(context.workspace_id.as_uuid())
    .bind(context.principal_id.as_uuid())
    .bind(command.job_id.as_uuid())
    .bind(i64::try_from(command.expected_version).unwrap())
    .bind(&command.idempotency_key)
    .bind(command.terminal_state.as_str())
    .bind(command.evidence.kind())
    .bind(command.evidence.id())
    .bind(&decision.policy_version)
    .bind("execution.write")
    .bind("embedding.job.cancel")
    .bind("workspace://")
    .bind("low")
    .fetch_one(&mut **transaction)
    .await
    .unwrap()
}

async fn wait_for_database_lock(observer: &PgPool, application_name: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity \
              WHERE application_name=$1 AND wait_event_type='Lock')",
        )
        .bind(application_name)
        .fetch_one(observer)
        .await
        .unwrap();
        if waiting {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "database actor {application_name} did not reach its exact lock boundary"
        );
        tokio::task::yield_now().await;
    }
}

async fn wait_for_database_activity(observer: &PgPool, application_name: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity \
              WHERE application_name=$1 AND state='active')",
        )
        .bind(application_name)
        .fetch_one(observer)
        .await
        .unwrap();
        if active {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "database actor {application_name} did not start its command"
        );
        tokio::task::yield_now().await;
    }
}

async fn prepare_source_erasure(
    runtime: PgPool,
    context: RequestContext,
    source: ContentMaterialId,
) -> Result<(), sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await?;
    sqlx::query("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
        .bind(source.as_uuid())
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await
}

fn fresh_output_service(
    runtime: &PgPool,
    vault: &Fixture,
    workspace_id: WorkspaceId,
) -> EmbeddingOutputKeyService<HostMaterialKeyVault> {
    EmbeddingOutputKeyService::new(
        Arc::new(PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(
            runtime.clone(),
        ))),
        Arc::new(vault.vault_for_workspace(workspace_id)),
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn child_process_death_recovers_every_db_and_vault_output_boundary(pool: PgPool) {
    let runtime = common::runtime_pool(&pool).await;
    let artifacts = TempDir::new().unwrap();

    let (reserved, reserved_output) = prepare_process_output(&pool, &runtime, "reservation").await;
    let reserved_vault = Fixture::new();
    let reserved_marker = artifacts.path().join("reservation-committed");
    let reserved_child = db_service_child_command(
        &reserved_vault,
        &EmbeddingOutputKeyBinding {
            workspace_id: reserved.context.workspace_id,
            job_id: reserved.job_id,
            intent_id: reserved_output.intent_id,
            material_id: reserved_output.material_id,
            key_id: reserved_output.key_id,
            nonce: reserved_output.nonce,
            output_ordinal: reserved_output.output_ordinal,
        },
        &reserved.context,
        &pool,
        OutputServiceBoundary::ReservationCommitted,
        &reserved_marker,
    )
    .spawn()
    .unwrap();
    kill_at_checkpoint(reserved_child, &reserved_marker);
    let reservation_facts: (String, i64) = sqlx::query_as(
        "SELECT intent.state, (SELECT COUNT(*) FROM embedding_output_key_receipts receipt WHERE receipt.intent_id=intent.id) \
           FROM material_key_creation_intents intent WHERE intent.id=$1",
    )
    .bind(reserved_output.intent_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reservation_facts, ("reserved".to_owned(), 0));
    assert!(
        !reserved_vault
            .vault
            .path()
            .join(reserved_output.key_id.to_string())
            .exists(),
        "death after the committed reservation precedes any host publication"
    );
    assert!(matches!(
        fresh_output_service(&runtime, &reserved_vault, reserved.context.workspace_id)
            .reconcile_one(&reserved.context)
            .await
            .unwrap(),
        Some(EmbeddingOutputKeyProgress::Prepared { .. })
    ));

    let (host_created, host_output) = prepare_process_output(&pool, &runtime, "host-created").await;
    let host_vault = Fixture::new();
    let host_marker = artifacts.path().join("host-created-before-receipt");
    let host_child = db_service_child_command(
        &host_vault,
        &EmbeddingOutputKeyBinding {
            workspace_id: host_created.context.workspace_id,
            job_id: host_created.job_id,
            intent_id: host_output.intent_id,
            material_id: host_output.material_id,
            key_id: host_output.key_id,
            nonce: host_output.nonce,
            output_ordinal: host_output.output_ordinal,
        },
        &host_created.context,
        &pool,
        OutputServiceBoundary::HostCreatedBeforeReceipt,
        &host_marker,
    )
    .spawn()
    .unwrap();
    kill_at_checkpoint(host_child, &host_marker);
    let host_receipts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM embedding_output_key_receipts WHERE intent_id=$1")
            .bind(host_output.intent_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(host_receipts, 0);
    assert!(
        host_vault
            .vault
            .path()
            .join(host_output.key_id.to_string())
            .join("active")
            .join("envelope")
            .exists(),
        "the child published the host envelope before its DB receipt"
    );
    assert!(matches!(
        fresh_output_service(&runtime, &host_vault, host_created.context.workspace_id)
            .reconcile_one(&host_created.context)
            .await
            .unwrap(),
        Some(EmbeddingOutputKeyProgress::Prepared { .. })
    ));

    let (receipted, receipted_output) =
        prepare_process_output(&pool, &runtime, "receipt-persisted").await;
    let receipted_vault = Fixture::new();
    let receipt_marker = artifacts.path().join("receipt-persisted");
    let receipted_binding = EmbeddingOutputKeyBinding {
        workspace_id: receipted.context.workspace_id,
        job_id: receipted.job_id,
        intent_id: receipted_output.intent_id,
        material_id: receipted_output.material_id,
        key_id: receipted_output.key_id,
        nonce: receipted_output.nonce,
        output_ordinal: receipted_output.output_ordinal,
    };
    let receipt_child = db_service_child_command(
        &receipted_vault,
        &receipted_binding,
        &receipted.context,
        &pool,
        OutputServiceBoundary::ReceiptPersisted,
        &receipt_marker,
    )
    .spawn()
    .unwrap();
    kill_at_checkpoint(receipt_child, &receipt_marker);
    let stored_receipt: uuid::Uuid = sqlx::query_scalar(
        "SELECT vault_receipt FROM embedding_output_key_receipts WHERE intent_id=$1",
    )
    .bind(receipted_output.intent_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let fresh_receipt = receipted_vault
        .vault_for_workspace(receipted.context.workspace_id)
        .create_embedding_output_if_absent(&receipted_binding)
        .unwrap();
    assert_eq!(fresh_receipt.as_uuid(), stored_receipt);
    assert_eq!(
        fresh_output_service(&runtime, &receipted_vault, receipted.context.workspace_id)
            .reconcile_one(&receipted.context)
            .await
            .unwrap(),
        None,
        "a fresh service skips an already receipted reservation"
    );

    let (retiring, retiring_output) =
        prepare_process_output(&pool, &runtime, "retirement-boundary").await;
    let retiring_vault = Fixture::new();
    let retiring_service =
        fresh_output_service(&runtime, &retiring_vault, retiring.context.workspace_id);
    assert!(matches!(
        retiring_service
            .reconcile_one(&retiring.context)
            .await
            .unwrap(),
        Some(EmbeddingOutputKeyProgress::Prepared { .. })
    ));
    retiring_service
        .request_retirement(
            &retiring.context,
            RequestEmbeddingOutputRetirement {
                termination: cancellation_termination(
                    &retiring,
                    uuid::Uuid::now_v7(),
                    "process-restart-retirement-authority",
                ),
            },
        )
        .await
        .unwrap();
    let retiring_binding = EmbeddingOutputKeyBinding {
        workspace_id: retiring.context.workspace_id,
        job_id: retiring.job_id,
        intent_id: retiring_output.intent_id,
        material_id: retiring_output.material_id,
        key_id: retiring_output.key_id,
        nonce: retiring_output.nonce,
        output_ordinal: retiring_output.output_ordinal,
    };
    let retirement_marker = artifacts.path().join("retirement-vault-before-receipt");
    let retirement_child = db_service_child_command(
        &retiring_vault,
        &retiring_binding,
        &retiring.context,
        &pool,
        OutputServiceBoundary::RetirementVaultBeforeReceipt,
        &retirement_marker,
    )
    .spawn()
    .unwrap();
    kill_at_checkpoint(retirement_child, &retirement_marker);
    let retirement_facts: (String, i64) = sqlx::query_as(
        "SELECT intent.state, (SELECT COUNT(*) FROM embedding_output_key_retirement_receipts receipt WHERE receipt.intent_id=intent.id) \
           FROM material_key_creation_intents intent WHERE intent.id=$1",
    )
    .bind(retiring_output.intent_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        retirement_facts,
        ("pre_prepared_abandon_prepared".to_owned(), 0)
    );
    let retired_dir = retiring_vault
        .vault
        .path()
        .join(retiring_output.key_id.to_string());
    assert!(retired_dir.join("erased").exists());
    assert!(!retired_dir.join("active").join("envelope").exists());
    assert!(matches!(
        fresh_output_service(&runtime, &retiring_vault, retiring.context.workspace_id)
            .reconcile_one(&retiring.context)
            .await
            .unwrap(),
        Some(EmbeddingOutputKeyProgress::Retired { .. })
    ));
    let recovered_retirement: (String, i64) = sqlx::query_as(
        "SELECT intent.state, (SELECT COUNT(*) FROM embedding_output_key_retirement_receipts receipt WHERE receipt.intent_id=intent.id) \
           FROM material_key_creation_intents intent WHERE intent.id=$1",
    )
    .bind(retiring_output.intent_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recovered_retirement, ("abandoned".to_owned(), 1));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn delivery_acceptance_uses_exact_no_auth_and_live_credential_snapshot_branches(
    pool: PgPool,
) {
    let runtime = common::runtime_pool(&pool).await;
    let (no_auth, _) = prepare_delivery_fixture(&pool, &runtime).await;
    PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()))
        .accept_delivery_outputs(
            &no_auth.context,
            acceptance_command(
                &no_auth,
                uuid::Uuid::now_v7(),
                "delivery-no-auth-branch",
                outputs(1),
            ),
        )
        .await
        .unwrap();
    let no_auth_branch: (String, Option<uuid::Uuid>, Option<uuid::Uuid>, bool) = sqlx::query_as(
        "SELECT snapshot.branch,snapshot.no_auth_binding_revision_id, \
                    snapshot.credential_slot_id, \
                    EXISTS(SELECT 1 FROM no_auth_binding_revisions binding \
                            WHERE binding.workspace_id=snapshot.workspace_id \
                              AND binding.id=snapshot.no_auth_binding_revision_id \
                              AND binding.connection_id=snapshot.connection_id \
                              AND binding.connection_revision_id=snapshot.connection_revision_id) \
               FROM model_binding_snapshots snapshot WHERE snapshot.id=$1",
    )
    .bind(no_auth.snapshot_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        no_auth_branch,
        (
            "no_auth".to_owned(),
            Some(no_auth.no_auth_binding_id),
            None,
            true
        )
    );

    let (credential, _) = prepare_credential_delivery_fixture(&pool, &runtime).await;
    let pinned = credential
        .credential
        .as_ref()
        .expect("the credential fixture exposes its guarded pinned tuple");
    PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()))
        .accept_delivery_outputs(
            &credential.context,
            acceptance_command(
                &credential,
                uuid::Uuid::now_v7(),
                "delivery-credential-branch",
                outputs(1),
            ),
        )
        .await
        .unwrap();
    let credential_branch: (
        String,
        Option<uuid::Uuid>,
        Option<uuid::Uuid>,
        Option<uuid::Uuid>,
        bool,
    ) = sqlx::query_as(
        "SELECT snapshot.branch,snapshot.credential_slot_id,snapshot.credential_revision_id, \
                snapshot.credential_activation_guard_id, \
                EXISTS(SELECT 1 FROM credential_slots slot \
                        JOIN credential_activation_guards guard \
                          ON guard.workspace_id=slot.workspace_id \
                         AND guard.connection_id=slot.connection_id \
                         AND guard.credential_slot_id=slot.id \
                       WHERE slot.workspace_id=snapshot.workspace_id \
                         AND slot.id=snapshot.credential_slot_id \
                         AND slot.current_revision_id=snapshot.credential_revision_id \
                         AND slot.tombstoned_at IS NULL \
                         AND guard.id=snapshot.credential_activation_guard_id) \
           FROM model_binding_snapshots snapshot WHERE snapshot.id=$1",
    )
    .bind(credential.snapshot_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        credential_branch,
        (
            "credential".to_owned(),
            Some(pinned.slot_id),
            Some(pinned.revision_id),
            Some(pinned.activation_guard_id),
            true
        )
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn aggregate_output_progress_refuses_partial_sets_and_opens_only_a_complete_exact_set(
    pool: PgPool,
) {
    let runtime = common::runtime_pool(&pool).await;
    let (fixture, _) = prepare_delivery_fixture(&pool, &runtime).await;
    let accepted_outputs = outputs(2);
    let repository = Arc::new(PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(
        runtime.clone(),
    )));
    repository
        .accept_delivery_outputs(
            &fixture.context,
            acceptance_command(
                &fixture,
                uuid::Uuid::now_v7(),
                "delivery-aggregate-progress",
                accepted_outputs,
            ),
        )
        .await
        .unwrap();
    let vault = Fixture::new();
    let service = EmbeddingOutputKeyService::new(repository, Arc::new(vault.vault()));
    assert_eq!(
        service.reconcile_one(&fixture.context).await.unwrap(),
        Some(EmbeddingOutputKeyProgress::WaitingForResultKeys)
    );
    let partial = pre_dispatch_gate(&pool, &fixture)
        .await
        .expect_err("the existing readiness gate must refuse a partial output set");
    assert_eq!(
        partial
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    assert!(matches!(
        service.reconcile_one(&fixture.context).await.unwrap(),
        Some(EmbeddingOutputKeyProgress::Prepared { .. })
    ));
    assert_eq!(
        pre_dispatch_gate(&pool, &fixture).await.expect(
            "only the complete exact receipted output set may pass the existing readiness gate"
        ),
        "requested"
    );
    let before_dispatch: (String, i64) =
        sqlx::query_as("SELECT state,version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before_dispatch.0, "requested");
    dispatch_through_embedding_fence(&runtime, &fixture).await;
    let after_dispatch: (String, i64) =
        sqlx::query_as("SELECT state,version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        after_dispatch,
        ("running".to_owned(), before_dispatch.1 + 1)
    );
    let mut replay = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *replay)
        .await
        .unwrap();
    let replay_refusal = sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions(\
           effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) \
         VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),'output-key-replay',\
           NOW()+INTERVAL '60 seconds')",
    )
    .bind(fixture.external_effect_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id.to_string())
    .execute(&mut *replay)
    .await
    .expect_err("a historical dispatch must not re-enter the requested-to-running transition");
    assert_eq!(
        replay_refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    replay.rollback().await.unwrap();
    let after_replay: (String, i64) =
        sqlx::query_as("SELECT state,version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after_replay, after_dispatch);
    let facts: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM embedding_output_key_receipts WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT COUNT(*) FROM material_key_creation_intents intent \
            JOIN embedding_job_material_intents member ON member.intent_id=intent.id \
           WHERE member.workspace_id=$1 AND member.job_id=$2 AND intent.state='provisional_receipted'), \
          (SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
           WHERE workspace_id=$1 AND effect_id=$3 AND status='dispatching')",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(facts, (2, 2, 1));
    runtime.close().await;
}

async fn acceptance_must_refuse(
    repository: &PgEmbeddingOutputKeyRepository,
    context: &RequestContext,
    command: AcceptDeliveryOutputs,
) {
    assert!(
        repository
            .accept_delivery_outputs(context, command)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_delivery_authority_and_output_identity_matrix_rolls_back_wholly(pool: PgPool) {
    let runtime = common::runtime_pool(&pool).await;
    let (fixture, _) = prepare_delivery_fixture(&pool, &runtime).await;
    let repository = PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()));

    acceptance_must_refuse(
        &repository,
        &RequestContext::new(WorkspaceId::new(), fixture.context.principal_id),
        acceptance_command(
            &fixture,
            uuid::Uuid::now_v7(),
            "invalid-workspace",
            outputs(2),
        ),
    )
    .await;
    acceptance_must_refuse(
        &repository,
        &RequestContext::new(fixture.context.workspace_id, PrincipalId::new()),
        acceptance_command(
            &fixture,
            uuid::Uuid::now_v7(),
            "invalid-principal",
            outputs(2),
        ),
    )
    .await;

    let mut wrong_kind =
        acceptance_command(&fixture, uuid::Uuid::now_v7(), "invalid-kind", outputs(2));
    wrong_kind.acceptance.kind = EmbeddingJobKind::Rebuild;
    acceptance_must_refuse(&repository, &fixture.context, wrong_kind).await;

    let mut wrong_job_and_cause = acceptance_command(
        &fixture,
        uuid::Uuid::now_v7(),
        "invalid-job-cause",
        outputs(2),
    );
    wrong_job_and_cause.acceptance.job_id = EmbeddingJobId::new();
    acceptance_must_refuse(&repository, &fixture.context, wrong_job_and_cause).await;

    let mut wrong_effect =
        acceptance_command(&fixture, uuid::Uuid::now_v7(), "invalid-effect", outputs(2));
    wrong_effect.acceptance.intent =
        common::workspace_scoped_intent(&fixture.context, fixture.snapshot_id);
    acceptance_must_refuse(&repository, &fixture.context, wrong_effect).await;

    let mut wrong_snapshot = acceptance_command(
        &fixture,
        uuid::Uuid::now_v7(),
        "invalid-snapshot",
        outputs(2),
    );
    wrong_snapshot.acceptance.model_binding_snapshot_id = uuid::Uuid::now_v7();
    acceptance_must_refuse(&repository, &fixture.context, wrong_snapshot).await;

    let mut wrong_mre =
        acceptance_command(&fixture, uuid::Uuid::now_v7(), "invalid-mre", outputs(2));
    wrong_mre.acceptance.model_request_evidence_id = ModelRequestEvidenceId::new();
    acceptance_must_refuse(&repository, &fixture.context, wrong_mre).await;

    let mut noncontiguous = outputs(2);
    noncontiguous[1].output_ordinal = 2;
    acceptance_must_refuse(
        &repository,
        &fixture.context,
        acceptance_command(
            &fixture,
            uuid::Uuid::now_v7(),
            "invalid-noncontiguous",
            noncontiguous,
        ),
    )
    .await;
    let mut duplicate_ordinal = outputs(2);
    duplicate_ordinal[1].output_ordinal = 0;
    acceptance_must_refuse(
        &repository,
        &fixture.context,
        acceptance_command(
            &fixture,
            uuid::Uuid::now_v7(),
            "invalid-duplicate-ordinal",
            duplicate_ordinal,
        ),
    )
    .await;

    for (label, duplicate) in [("intent", 0_u8), ("material", 1), ("key", 2), ("nonce", 3)] {
        let mut duplicate_outputs = outputs(2);
        match duplicate {
            0 => duplicate_outputs[1].intent_id = duplicate_outputs[0].intent_id,
            1 => duplicate_outputs[1].material_id = duplicate_outputs[0].material_id,
            2 => duplicate_outputs[1].key_id = duplicate_outputs[0].key_id,
            3 => duplicate_outputs[1].nonce = duplicate_outputs[0].nonce,
            _ => unreachable!(),
        }
        acceptance_must_refuse(
            &repository,
            &fixture.context,
            acceptance_command(
                &fixture,
                uuid::Uuid::now_v7(),
                &format!("invalid-duplicate-{label}"),
                duplicate_outputs,
            ),
        )
        .await;
    }

    let footprint: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM embedding_delivery_acceptance_receipts WHERE workspace_id=$1), \
          (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
          (SELECT COUNT(*) FROM embedding_job_material_intents WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT COUNT(*) FROM embedding_delivery_source_memberships WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT COUNT(*) FROM audit_events WHERE workspace_id=$1 AND action='embedding.job.delivery_accepted'), \
          (SELECT COUNT(*) FROM outbox WHERE workspace_id=$1 AND topic='embedding.job.delivery_accepted')",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(footprint, (0, 0, 0, 0, 0, 0));
    let invalid_effects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM external_effect_intents WHERE workspace_id=$1 AND id<>$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(invalid_effects, 0);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn receipt_and_retirement_serialize_without_deadlock_in_both_orders(pool: PgPool) {
    let runtime = common::runtime_pool(&pool).await;

    let (receipt_first, _) = prepare_delivery_fixture(&pool, &runtime).await;
    let receipt_first_output = outputs(1).remove(0);
    PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()))
        .accept_delivery_outputs(
            &receipt_first.context,
            acceptance_command(
                &receipt_first,
                uuid::Uuid::now_v7(),
                "lock-order-receipt-first",
                vec![receipt_first_output.clone()],
            ),
        )
        .await
        .unwrap();
    let receipt_first_binding = EmbeddingOutputKeyBinding {
        workspace_id: receipt_first.context.workspace_id,
        job_id: receipt_first.job_id,
        intent_id: receipt_first_output.intent_id,
        material_id: receipt_first_output.material_id,
        key_id: receipt_first_output.key_id,
        nonce: receipt_first_output.nonce,
        output_ordinal: receipt_first_output.output_ordinal,
    };
    let receipt_id = uuid::Uuid::now_v7();
    let receipt_pool = named_runtime_pool(&pool, "task14c-receipt-first").await;
    let mut receipt_transaction = receipt_pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(receipt_first.context.workspace_id.to_string())
        .fetch_one(&mut *receipt_transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_embedding_output_key_receipt($1,$2,$3)")
        .bind(receipt_first.context.workspace_id.as_uuid())
        .bind(receipt_first_binding.intent_id.as_uuid())
        .bind(receipt_id)
        .execute(&mut *receipt_transaction)
        .await
        .unwrap();

    let retirement_pool = named_runtime_pool(&pool, "task14c-retirement-after-receipt").await;
    let retirement_context = receipt_first.context.clone();
    let retirement_command = cancellation_termination(
        &receipt_first,
        uuid::Uuid::now_v7(),
        "lock-order-retirement-after-receipt",
    );
    let retirement_task = tokio::spawn(async move {
        PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(retirement_pool))
            .request_retirement(
                &retirement_context,
                RequestEmbeddingOutputRetirement {
                    termination: retirement_command,
                },
            )
            .await
    });
    wait_for_database_lock(&pool, "task14c-retirement-after-receipt").await;
    let aggregate = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        sqlx::query_scalar::<_, String>("SELECT vestrace_embedding_output_key_progress($1,$2)")
            .bind(receipt_first.context.workspace_id.as_uuid())
            .bind(receipt_first.job_id.as_uuid())
            .fetch_one(&mut *receipt_transaction),
    )
    .await
    .expect("aggregate progress must not deadlock against output retirement")
    .unwrap();
    assert_eq!(aggregate, "prepared");
    receipt_transaction.commit().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), retirement_task)
        .await
        .expect("retirement must resume after the receipt transaction commits")
        .unwrap()
        .unwrap();
    let receipt_winner: (String, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT intent.state, \
          (SELECT COUNT(*) FROM embedding_output_key_receipts receipt WHERE receipt.workspace_id=$1 AND receipt.intent_id=$2), \
          (SELECT COUNT(*) FROM embedding_output_key_retirement_requests request WHERE request.workspace_id=$1 AND request.intent_id=$2), \
          (SELECT COUNT(*) FROM embedding_output_key_retirement_receipts retired WHERE retired.workspace_id=$1 AND retired.intent_id=$2), \
          (SELECT COUNT(*) FROM material_key_creation_intents exact_intent WHERE exact_intent.workspace_id=$1 AND exact_intent.id=$2 AND exact_intent.vault_receipt=$3) \
         FROM material_key_creation_intents intent WHERE intent.workspace_id=$1 AND intent.id=$2",
    )
    .bind(receipt_first.context.workspace_id.as_uuid())
    .bind(receipt_first_binding.intent_id.as_uuid())
    .bind(receipt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        receipt_winner,
        ("pre_prepared_abandon_prepared".to_owned(), 1, 1, 0, 1)
    );

    let (retirement_first, _) = prepare_delivery_fixture(&pool, &runtime).await;
    let retirement_first_output = outputs(1).remove(0);
    PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()))
        .accept_delivery_outputs(
            &retirement_first.context,
            acceptance_command(
                &retirement_first,
                uuid::Uuid::now_v7(),
                "lock-order-retirement-first",
                vec![retirement_first_output.clone()],
            ),
        )
        .await
        .unwrap();
    let retirement_first_binding = EmbeddingOutputKeyBinding {
        workspace_id: retirement_first.context.workspace_id,
        job_id: retirement_first.job_id,
        intent_id: retirement_first_output.intent_id,
        material_id: retirement_first_output.material_id,
        key_id: retirement_first_output.key_id,
        nonce: retirement_first_output.nonce,
        output_ordinal: retirement_first_output.output_ordinal,
    };
    let retirement_first_command = cancellation_termination(
        &retirement_first,
        uuid::Uuid::now_v7(),
        "lock-order-retirement-first-authority",
    );
    let retirement_first_pool = named_runtime_pool(&pool, "task14c-retirement-first").await;
    let mut retirement_transaction = retirement_first_pool.begin().await.unwrap();
    assert_eq!(
        request_output_retirement_without_committing(
            &mut retirement_transaction,
            &retirement_first.context,
            &retirement_first_command,
        )
        .await,
        retirement_first_command.receipt_id
    );

    let losing_receipt_pool = named_runtime_pool(&pool, "task14c-receipt-after-retirement").await;
    let losing_receipt_context = retirement_first.context.clone();
    let losing_receipt_binding = retirement_first_binding.clone();
    let losing_receipt_id = uuid::Uuid::now_v7();
    let losing_receipt_task = tokio::spawn(async move {
        PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(losing_receipt_pool))
            .record_receipt(
                &losing_receipt_context,
                &losing_receipt_binding,
                VaultReceipt::from_uuid(losing_receipt_id),
            )
            .await
    });
    wait_for_database_lock(&pool, "task14c-receipt-after-retirement").await;
    retirement_transaction.commit().await.unwrap();
    let losing_receipt =
        tokio::time::timeout(std::time::Duration::from_secs(10), losing_receipt_task)
            .await
            .expect("receipt refusal must resume after retirement commits")
            .unwrap();
    assert!(
        losing_receipt.is_err(),
        "a committed retirement request must win over a later receipt"
    );
    let retirement_winner: (String, i64, i64, i64, bool) = sqlx::query_as(
        "SELECT intent.state, \
          (SELECT COUNT(*) FROM embedding_output_key_receipts receipt WHERE receipt.workspace_id=$1 AND receipt.intent_id=$2), \
          (SELECT COUNT(*) FROM embedding_output_key_retirement_requests request WHERE request.workspace_id=$1 AND request.intent_id=$2), \
          (SELECT COUNT(*) FROM embedding_output_key_retirement_receipts retired WHERE retired.workspace_id=$1 AND retired.intent_id=$2), \
          intent.vault_receipt IS NULL \
         FROM material_key_creation_intents intent WHERE intent.workspace_id=$1 AND intent.id=$2",
    )
    .bind(retirement_first.context.workspace_id.as_uuid())
    .bind(retirement_first_binding.intent_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        retirement_winner,
        ("pre_prepared_abandon_prepared".to_owned(), 0, 1, 0, true)
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn source_erasure_and_fresh_acceptance_serialize_in_both_lock_orders(pool: PgPool) {
    let runtime = common::runtime_pool(&pool).await;

    let (acceptance_first, acceptance_source) = prepare_delivery_fixture(&pool, &runtime).await;
    let mut membership_gate = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE embedding_delivery_source_memberships IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *membership_gate)
        .await
        .unwrap();
    let accept_pool = named_runtime_pool(&pool, "task14c-acceptance-first").await;
    let accept_context = acceptance_first.context.clone();
    let accept_command = acceptance_command(
        &acceptance_first,
        uuid::Uuid::now_v7(),
        "concurrent-acceptance-first",
        outputs(1),
    );
    let accept_task = tokio::spawn(async move {
        PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(accept_pool))
            .accept_delivery_outputs(&accept_context, accept_command)
            .await
    });
    wait_for_database_lock(&pool, "task14c-acceptance-first").await;
    let erase_pool = named_runtime_pool(&pool, "task14c-erasure-after-acceptance").await;
    let erase_context = acceptance_first.context.clone();
    let erase_task = tokio::spawn(prepare_source_erasure(
        erase_pool,
        erase_context,
        acceptance_source,
    ));
    wait_for_database_lock(&pool, "task14c-erasure-after-acceptance").await;
    membership_gate.commit().await.unwrap();
    accept_task.await.unwrap().unwrap();
    let erasure_refusal = erase_task.await.unwrap().unwrap_err();
    assert_eq!(
        erasure_refusal
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );
    let acceptance_winner: (String, i64, i64, i64) = sqlx::query_as(
        "SELECT material.state, \
          (SELECT COUNT(*) FROM embedding_delivery_source_memberships source WHERE source.workspace_id=$1 AND source.job_id=$2), \
          (SELECT COUNT(*) FROM material_erasure_blockers blocker WHERE blocker.workspace_id=$1 AND blocker.content_material_id=$3), \
          (SELECT COUNT(*) FROM material_erasure_blockers blocker \
            LEFT JOIN embedding_delivery_source_memberships source ON source.blocker_id=blocker.id \
           WHERE blocker.workspace_id=$1 AND blocker.content_material_id=$3 AND source.blocker_id IS NULL) \
         FROM content_materials material WHERE material.workspace_id=$1 AND material.id=$3",
    )
    .bind(acceptance_first.context.workspace_id.as_uuid())
    .bind(acceptance_first.job_id.as_uuid())
    .bind(acceptance_source.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(acceptance_winner, ("live".to_owned(), 1, 1, 0));

    let (erasure_first, erasure_source) = prepare_delivery_fixture(&pool, &runtime).await;
    let mut erasure_gate = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE material_erasure_preparations IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *erasure_gate)
        .await
        .unwrap();
    let erase_first_task = tokio::spawn(prepare_source_erasure(
        named_runtime_pool(&pool, "task14c-erasure-first").await,
        erasure_first.context.clone(),
        erasure_source,
    ));
    wait_for_database_lock(&pool, "task14c-erasure-first").await;
    let losing_pool = named_runtime_pool(&pool, "task14c-acceptance-after-erasure").await;
    let losing_context = erasure_first.context.clone();
    let losing_command = acceptance_command(
        &erasure_first,
        uuid::Uuid::now_v7(),
        "concurrent-erasure-first",
        outputs(1),
    );
    let losing_acceptance = tokio::spawn(async move {
        PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(losing_pool))
            .accept_delivery_outputs(&losing_context, losing_command)
            .await
    });
    wait_for_database_activity(&pool, "task14c-acceptance-after-erasure").await;
    erasure_gate.commit().await.unwrap();
    erase_first_task.await.unwrap().unwrap();
    assert!(losing_acceptance.await.unwrap().is_err());
    let erasure_winner: (String, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT material.state, \
          (SELECT COUNT(*) FROM embedding_delivery_acceptance_receipts WHERE workspace_id=$1), \
          (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
          (SELECT COUNT(*) FROM embedding_delivery_source_memberships WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT COUNT(*) FROM material_erasure_blockers WHERE workspace_id=$1 AND content_material_id=$3) \
         FROM content_materials material WHERE material.workspace_id=$1 AND material.id=$3",
    )
    .bind(erasure_first.context.workspace_id.as_uuid())
    .bind(erasure_first.job_id.as_uuid())
    .bind(erasure_source.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(erasure_winner, ("erasure_prepared".to_owned(), 0, 0, 0, 0));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn source_erasure_and_terminal_retirement_preserve_both_transaction_orders(pool: PgPool) {
    let runtime = common::runtime_pool(&pool).await;

    let (terminal_first, terminal_source, terminal_command) =
        prepare_retired_output(&pool, &runtime, "terminal-first").await;
    let mut terminal_gate = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE provider_admission_waits IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *terminal_gate)
        .await
        .unwrap();
    let terminal_pool = named_runtime_pool(&pool, "task14c-terminal-first").await;
    let terminal_context = terminal_first.context.clone();
    let terminal_task = tokio::spawn(async move {
        PgEmbeddingJobRepository::new(PgStore::from_pool(terminal_pool))
            .terminate_pre_dispatch(terminal_context, terminal_command)
            .await
    });
    wait_for_database_lock(&pool, "task14c-terminal-first").await;
    let uncommitted_terminal_refusal = prepare_source_erasure(
        named_runtime_pool(&pool, "task14c-erasure-during-terminal").await,
        terminal_first.context.clone(),
        terminal_source,
    )
    .await
    .unwrap_err();
    assert_eq!(
        uncommitted_terminal_refusal
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );
    terminal_gate.commit().await.unwrap();
    terminal_task.await.unwrap().unwrap();
    prepare_source_erasure(
        named_runtime_pool(&pool, "task14c-erasure-after-terminal").await,
        terminal_first.context.clone(),
        terminal_source,
    )
    .await
    .unwrap();
    let terminal_winner: (String, String, i64, i64) = sqlx::query_as(
        "SELECT job.state,blocker.state, \
          (SELECT COUNT(*) FROM embedding_job_termination_receipts receipt WHERE receipt.workspace_id=$1 AND receipt.job_id=$2), \
          (SELECT COUNT(*) FROM material_erasure_preparations preparation WHERE preparation.workspace_id=$1 AND preparation.content_material_id=$3) \
         FROM embedding_jobs job \
         JOIN embedding_delivery_source_memberships source ON source.workspace_id=job.workspace_id AND source.job_id=job.id \
         JOIN material_erasure_blockers blocker ON blocker.id=source.blocker_id \
         WHERE job.workspace_id=$1 AND job.id=$2",
    )
    .bind(terminal_first.context.workspace_id.as_uuid())
    .bind(terminal_first.job_id.as_uuid())
    .bind(terminal_source.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        terminal_winner,
        ("cancelled".to_owned(), "terminal".to_owned(), 1, 1)
    );

    let (erasure_first, erasure_source, erasure_command) =
        prepare_retired_output(&pool, &runtime, "erasure-first").await;
    let mut receipt_gate = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE embedding_job_termination_receipts IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *receipt_gate)
        .await
        .unwrap();
    let late_terminal_pool = named_runtime_pool(&pool, "task14c-terminal-after-erasure").await;
    let late_terminal_context = erasure_first.context.clone();
    let late_terminal = tokio::spawn(async move {
        PgEmbeddingJobRepository::new(PgStore::from_pool(late_terminal_pool))
            .terminate_pre_dispatch(late_terminal_context, erasure_command)
            .await
    });
    wait_for_database_lock(&pool, "task14c-terminal-after-erasure").await;
    let blocked_erasure = prepare_source_erasure(
        named_runtime_pool(&pool, "task14c-erasure-before-terminal").await,
        erasure_first.context.clone(),
        erasure_source,
    )
    .await
    .unwrap_err();
    assert_eq!(
        blocked_erasure
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );
    let premature_preparations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM material_erasure_preparations WHERE workspace_id=$1 AND content_material_id=$2",
    )
    .bind(erasure_first.context.workspace_id.as_uuid())
    .bind(erasure_source.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(premature_preparations, 0);
    receipt_gate.commit().await.unwrap();
    late_terminal.await.unwrap().unwrap();
    prepare_source_erasure(
        named_runtime_pool(&pool, "task14c-erasure-retry").await,
        erasure_first.context.clone(),
        erasure_source,
    )
    .await
    .unwrap();
    let erasure_then_terminal: (String, String, i64, i64) = sqlx::query_as(
        "SELECT job.state,blocker.state, \
          (SELECT COUNT(*) FROM embedding_job_termination_receipts receipt WHERE receipt.workspace_id=$1 AND receipt.job_id=$2), \
          (SELECT COUNT(*) FROM material_erasure_preparations preparation WHERE preparation.workspace_id=$1 AND preparation.content_material_id=$3) \
         FROM embedding_jobs job \
         JOIN embedding_delivery_source_memberships source ON source.workspace_id=job.workspace_id AND source.job_id=job.id \
         JOIN material_erasure_blockers blocker ON blocker.id=source.blocker_id \
         WHERE job.workspace_id=$1 AND job.id=$2",
    )
    .bind(erasure_first.context.workspace_id.as_uuid())
    .bind(erasure_first.job_id.as_uuid())
    .bind(erasure_source.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        erasure_then_terminal,
        ("cancelled".to_owned(), "terminal".to_owned(), 1, 1)
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn delivery_acceptance_is_atomic_replayable_and_source_blocked(pool: PgPool) {
    let runtime = common::runtime_pool(&pool).await;
    let (fixture, source) = prepare_delivery_fixture(&pool, &runtime).await;
    let repository = PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()));
    let receipt_id = uuid::Uuid::now_v7();
    let accepted_outputs = outputs(2);
    let first = repository
        .accept_delivery_outputs(
            &fixture.context,
            acceptance_command(
                &fixture,
                receipt_id,
                "delivery-outputs",
                accepted_outputs.clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(first.receipt_id, receipt_id);
    assert_eq!(first.job_id, fixture.job_id);
    assert_eq!(first.outputs, accepted_outputs);

    let counts: (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM embedding_jobs WHERE workspace_id=$1 AND id=$2), \
          (SELECT COUNT(*) FROM embedding_job_material_intents WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT COUNT(*) FROM embedding_delivery_source_memberships WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT COUNT(*) FROM material_erasure_blockers WHERE workspace_id=$1 AND content_material_id=$3 AND state='nonterminal'), \
          (SELECT COUNT(*) FROM audit_events WHERE workspace_id=$1 AND action='embedding.job.delivery_accepted'), \
          (SELECT COUNT(*) FROM idempotency_keys WHERE workspace_id=$1 AND idempotency_key='delivery-outputs'), \
          (SELECT COUNT(*) FROM outbox WHERE workspace_id=$1 AND topic='embedding.job.delivery_accepted')",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind(source.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 2, 2, 2, 1, 1, 1));

    let replay = repository
        .accept_delivery_outputs(
            &fixture.context,
            acceptance_command(
                &fixture,
                receipt_id,
                "delivery-outputs",
                accepted_outputs.clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(replay, first);
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE workspace_id=$1 AND action='embedding.job.delivery_accepted'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        audit_count, 1,
        "receipt replay must not rerun governed audit"
    );

    let mut changed_governance = acceptance_command(
        &fixture,
        receipt_id,
        "delivery-outputs",
        accepted_outputs.clone(),
    );
    changed_governance.acceptance.audit.payload = serde_json::json!({"changed": true});
    assert!(
        repository
            .accept_delivery_outputs(&fixture.context, changed_governance)
            .await
            .is_err(),
        "replay must compare the governed audit/idempotency/outbox tuple"
    );

    let mut changed = accepted_outputs;
    changed[1].nonce = IntentNonce::new();
    assert!(
        repository
            .accept_delivery_outputs(
                &fixture.context,
                acceptance_command(&fixture, receipt_id, "delivery-outputs", changed),
            )
            .await
            .is_err()
    );
    let counts_after: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM embedding_job_material_intents WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT COUNT(*) FROM embedding_delivery_source_memberships WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT COUNT(*) FROM audit_events WHERE workspace_id=$1 AND action='embedding.job.delivery_accepted')",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts_after, (2, 2, 1));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_second_output_rolls_back_receipt_job_reservations_and_audit(pool: PgPool) {
    let runtime = common::runtime_pool(&pool).await;
    let (fixture, _) = prepare_delivery_fixture(&pool, &runtime).await;
    let repository = PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()));
    let receipt_id = uuid::Uuid::now_v7();
    let mut invalid = outputs(2);
    invalid[1].key_id = invalid[0].key_id;
    assert!(
        repository
            .accept_delivery_outputs(
                &fixture.context,
                acceptance_command(&fixture, receipt_id, "delivery-rollback", invalid),
            )
            .await
            .is_err()
    );
    let facts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM embedding_delivery_acceptance_receipts WHERE id=$1), \
          (SELECT COUNT(*) FROM embedding_jobs WHERE id=$2), \
          (SELECT COUNT(*) FROM embedding_job_material_intents WHERE job_id=$2), \
          (SELECT COUNT(*) FROM audit_events WHERE workspace_id=$3 AND action='embedding.job.delivery_accepted')",
    )
    .bind(receipt_id)
    .bind(fixture.job_id.as_uuid())
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(facts, (0, 0, 0, 0));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn generic_material_abandonment_cannot_replace_output_retirement_receipt(pool: PgPool) {
    let runtime = common::runtime_pool(&pool).await;
    let (fixture, source) = prepare_delivery_fixture(&pool, &runtime).await;
    let output = outputs(1).remove(0);
    let output_repository =
        PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()));
    output_repository
        .accept_delivery_outputs(
            &fixture.context,
            acceptance_command(
                &fixture,
                uuid::Uuid::now_v7(),
                "generic-abandonment-does-not-prove-output-retirement",
                vec![output.clone()],
            ),
        )
        .await
        .unwrap();
    let termination = cancellation_termination(
        &fixture,
        uuid::Uuid::now_v7(),
        "generic-abandonment-terminal-refusal",
    );
    output_repository
        .request_retirement(
            &fixture.context,
            RequestEmbeddingOutputRetirement {
                termination: termination.clone(),
            },
        )
        .await
        .unwrap();

    let generic_erasure_receipt = uuid::Uuid::now_v7();
    let mut generic_resumption = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *generic_resumption)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_prepare_pre_prepared_material_abandon($1)")
        .bind(output.intent_id.as_uuid())
        .execute(&mut *generic_resumption)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_unbound_material_key_erasure($1,$2)")
        .bind(output.intent_id.as_uuid())
        .bind(generic_erasure_receipt)
        .execute(&mut *generic_resumption)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_material_key_abandon($1)")
        .bind(output.intent_id.as_uuid())
        .execute(&mut *generic_resumption)
        .await
        .unwrap();
    generic_resumption.commit().await.unwrap();

    let job_repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    assert!(
        job_repository
            .terminate_pre_dispatch(fixture.context.clone(), termination)
            .await
            .is_err(),
        "generic intent abandonment must not substitute for the specialized immutable output receipt"
    );

    let observer = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            pool.connect_options()
                .as_ref()
                .clone()
                .application_name("task14c-generic-abandonment-observer"),
        )
        .await
        .unwrap();
    let facts: (String, String, String, i64, i64, i64) = sqlx::query_as(
        "SELECT job.state,intent.state,blocker.state, \
          (SELECT COUNT(*) FROM material_key_creation_intent_erasure_receipts generic_receipt \
            WHERE generic_receipt.workspace_id=$1 AND generic_receipt.intent_id=$3 \
              AND generic_receipt.erasure_receipt=$4), \
          (SELECT COUNT(*) FROM embedding_output_key_retirement_receipts specialized_receipt \
            WHERE specialized_receipt.workspace_id=$1 AND specialized_receipt.job_id=$2 \
              AND specialized_receipt.intent_id=$3), \
          (SELECT COUNT(*) FROM embedding_job_termination_receipts terminal_receipt \
            WHERE terminal_receipt.workspace_id=$1 AND terminal_receipt.job_id=$2) \
         FROM embedding_jobs job \
         JOIN embedding_job_material_intents member \
           ON member.workspace_id=job.workspace_id AND member.job_id=job.id \
         JOIN material_key_creation_intents intent \
           ON intent.workspace_id=member.workspace_id AND intent.id=member.intent_id \
         JOIN embedding_delivery_source_memberships source_membership \
           ON source_membership.workspace_id=member.workspace_id \
          AND source_membership.job_id=member.job_id \
          AND source_membership.output_ordinal=member.output_ordinal \
          AND source_membership.intent_id=member.intent_id \
         JOIN material_erasure_blockers blocker ON blocker.id=source_membership.blocker_id \
         WHERE job.workspace_id=$1 AND job.id=$2 AND intent.id=$3 \
           AND source_membership.source_material_id=$5",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind(output.intent_id.as_uuid())
    .bind(generic_erasure_receipt)
    .bind(source.as_uuid())
    .fetch_one(&observer)
    .await
    .unwrap();
    assert!(matches!(facts.0.as_str(), "requested" | "running"));
    assert_eq!(facts.1, "abandoned");
    assert_eq!(facts.2, "nonterminal");
    assert_eq!((facts.3, facts.4, facts.5), (1, 0, 0));
    observer.close().await;
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn retirement_requires_owned_authority_then_erases_and_releases_source_blocker(pool: PgPool) {
    let runtime = common::runtime_pool(&pool).await;
    let (fixture, source) = prepare_delivery_fixture(&pool, &runtime).await;
    let outputs = outputs(2);
    let repository = Arc::new(PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(
        runtime.clone(),
    )));
    repository
        .accept_delivery_outputs(
            &fixture.context,
            acceptance_command(
                &fixture,
                uuid::Uuid::now_v7(),
                "delivery-retirement",
                outputs.clone(),
            ),
        )
        .await
        .unwrap();

    let mut generic = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *generic)
        .await
        .unwrap();
    let refused = sqlx::query("SELECT vestrace_prepare_pre_prepared_material_abandon($1)")
        .bind(outputs[0].intent_id.as_uuid())
        .execute(&mut *generic)
        .await
        .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );
    generic.rollback().await.unwrap();
    let before: (String, String) = sqlx::query_as(
        "SELECT intent.state,blocker.state FROM material_key_creation_intents intent \
         JOIN embedding_delivery_source_memberships source ON source.intent_id=intent.id \
         JOIN material_erasure_blockers blocker ON blocker.id=source.blocker_id \
         WHERE intent.id=$1",
    )
    .bind(outputs[0].intent_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, ("reserved".into(), "nonterminal".into()));

    let vault_fixture = Fixture::new();
    let service =
        EmbeddingOutputKeyService::new(repository.clone(), Arc::new(vault_fixture.vault()));
    assert_eq!(
        service.reconcile_one(&fixture.context).await.unwrap(),
        Some(EmbeddingOutputKeyProgress::WaitingForResultKeys)
    );
    assert!(matches!(
        service.reconcile_one(&fixture.context).await.unwrap(),
        Some(EmbeddingOutputKeyProgress::Prepared { .. })
    ));
    let unproven = RequestEmbeddingOutputRetirement {
        termination: TerminateEmbeddingJobPreDispatch {
            receipt_id: uuid::Uuid::now_v7(),
            job_id: fixture.job_id,
            expected_version: 1,
            idempotency_key: "unproven-output-retirement".to_owned(),
            terminal_state: PreDispatchTerminalState::FailedDefinite,
            evidence: PreDispatchTerminationEvidence::ExternalEffectDenied {
                authorization_id: uuid::Uuid::now_v7(),
            },
        },
    };
    assert!(
        repository
            .request_retirement(&fixture.context, unproven)
            .await
            .is_err(),
        "an absent denied-effect witness must not mint retirement authority"
    );
    let unproven_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM embedding_output_key_retirement_requests WHERE workspace_id=$1",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unproven_rows, 0);

    let termination = cancellation_termination(
        &fixture,
        uuid::Uuid::now_v7(),
        "authorized-output-retirement",
    );
    service
        .request_retirement(
            &fixture.context,
            RequestEmbeddingOutputRetirement {
                termination: termination.clone(),
            },
        )
        .await
        .unwrap();
    let job_repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    assert!(
        job_repository
            .terminate_pre_dispatch(fixture.context.clone(), termination.clone())
            .await
            .is_err(),
        "the terminal command must refuse before every output has an immutable retirement receipt"
    );
    for _ in &outputs {
        assert!(matches!(
            service.reconcile_one(&fixture.context).await.unwrap(),
            Some(EmbeddingOutputKeyProgress::Retired { .. })
        ));
    }
    let after: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT COUNT(*) FROM material_key_creation_intents intent \
            JOIN embedding_job_material_intents member ON member.intent_id=intent.id \
           WHERE member.workspace_id=$1 AND member.job_id=$2 AND intent.state='abandoned'), \
          (SELECT COUNT(*) FROM embedding_output_key_retirement_receipts \
           WHERE workspace_id=$1 AND job_id=$2), \
          (SELECT COUNT(*) FROM material_erasure_blockers blocker \
            JOIN embedding_delivery_source_memberships source ON source.blocker_id=blocker.id \
           WHERE source.workspace_id=$1 AND source.job_id=$2 AND blocker.state='nonterminal')",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, (2, 2, 2));
    for output in &outputs {
        assert!(
            !vault_fixture
                .vault
                .path()
                .join(output.key_id.as_uuid().to_string())
                .join("active")
                .join("envelope")
                .exists()
        );
    }

    let mut premature_erasure = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *premature_erasure)
        .await
        .unwrap();
    assert!(
        sqlx::query("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
            .bind(source.as_uuid())
            .execute(&mut *premature_erasure)
            .await
            .is_err(),
        "source erasure stays blocked after all output witnesses while the job remains active"
    );
    premature_erasure.rollback().await.unwrap();

    let mut changed_terminal = termination.clone();
    changed_terminal.receipt_id = uuid::Uuid::now_v7();
    assert!(
        job_repository
            .terminate_pre_dispatch(fixture.context.clone(), changed_terminal)
            .await
            .is_err(),
        "final termination must reuse the exact output-retirement authority"
    );
    let terminal = job_repository
        .terminate_pre_dispatch(fixture.context.clone(), termination)
        .await
        .unwrap();
    assert_eq!(terminal.version, 2);
    let terminal_blockers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM material_erasure_blockers blocker \
          JOIN embedding_delivery_source_memberships source ON source.blocker_id=blocker.id \
         WHERE source.workspace_id=$1 AND source.job_id=$2 AND blocker.state='terminal'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(terminal_blockers, 2);

    let mut erasure = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *erasure)
        .await
        .unwrap();
    sqlx::query("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
        .bind(source.as_uuid())
        .execute(&mut *erasure)
        .await
        .expect("the exact retired output releases its source erasure blocker");
    erasure.rollback().await.unwrap();
    runtime.close().await;
}

#[test]
fn output_binding_is_exact_durable_and_refuses_generic_and_provisional_access() {
    use vestrace_application::EmbeddingResultPreparationId;
    use vestrace_domain::MaterialKeyBindingReceipt;
    let f = Fixture::new();
    let b = f.binding();
    let p = EmbeddingResultPreparationId::new();
    f.vault().create_embedding_output_if_absent(&b).unwrap();
    let receipt = f.vault().bind_embedding_output(&b, p).unwrap();
    assert_eq!(f.vault().bind_embedding_output(&b, p).unwrap(), receipt);
    let mut calls = 0;
    f.vault()
        .with_bound_embedding_output_key(&b, p, receipt, &mut |_| calls += 1)
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(
        f.vault()
            .bind_embedding_output(&b, EmbeddingResultPreparationId::new()),
        Err(VaultError::BindingMismatch)
    );
    assert!(
        f.vault()
            .with_bound_embedding_output_key(&b, p, MaterialKeyBindingReceipt::new(), &mut |_| {
                calls += 1
            })
            .is_err()
    );
    assert!(
        f.vault()
            .with_bound_embedding_output_key(
                &b,
                EmbeddingResultPreparationId::new(),
                receipt,
                &mut |_| calls += 1
            )
            .is_err()
    );
    for changed in 0..7 {
        let mut other = b.clone();
        match changed {
            0 => other.workspace_id = WorkspaceId::new(),
            1 => other.job_id = EmbeddingJobId::new(),
            2 => other.intent_id = MaterialKeyCreationIntentId::new(),
            3 => other.material_id = ContentMaterialId::new(),
            4 => other.key_id = MaterialKeyId::new(),
            5 => other.nonce = IntentNonce::new(),
            _ => other.output_ordinal += 1,
        }
        assert!(
            f.vault()
                .with_bound_embedding_output_key(&other, p, receipt, &mut |_| calls += 1)
                .is_err()
        );
        assert!(f.vault().bind_embedding_output(&other, p).is_err());
    }
    assert!(
        f.vault()
            .with_embedding_output_key(&b, &mut |_| calls += 1)
            .is_err()
    );
    assert_eq!(
        f.vault().unwrap(b.key_id, &mut |_| calls += 1),
        Err(VaultError::Provisional)
    );
    assert_eq!(
        f.vault().prepare_erasure(b.key_id),
        Err(VaultError::Provisional)
    );
    assert_eq!(f.vault().erase(b.key_id), Err(VaultError::Provisional));
    assert!(f.vault().retire_embedding_output(&b).is_err());
    assert_eq!(calls, 1);
    let dir = f.vault.path().join(b.key_id.to_string());
    assert!(dir.join("active/envelope").is_file());
    assert!(!dir.join("fence").exists());
    assert!(!dir.join("erased").exists());
}

// A failing mutation assertion must not leave a paused child holding the
// Windows test executable open across the mandatory restore/GREEN build.
struct OutputDecisionChild(Option<std::process::Child>);
impl Drop for OutputDecisionChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn output_disposition_process_winners_survive_death_and_refuse_the_loser() {
    use vestrace_application::EmbeddingResultPreparationId;
    use vestrace_domain::MaterialKeyBindingReceipt;
    for bind_wins in [true, false] {
        let f = Fixture::new();
        let b = f.binding();
        let p = EmbeddingResultPreparationId::new();
        f.vault().create_embedding_output_if_absent(&b).unwrap();
        let artifacts = TempDir::new().unwrap();
        let (winner, loser, checkpoint) = if bind_wins {
            ("output_bind", "output_retire", "output_bind_decision")
        } else {
            ("output_retire", "output_bind", "output_retire_decision")
        };
        // Admit the losing process before the winning immutable decision.
        // It must rejoin the same pathname arbitration after its stale reads.
        let loser_result = artifacts.path().join("loser-result");
        let loser_marker = artifacts.path().join("loser-marker");
        let loser_checkpoint = if bind_wins {
            "before_output_retire_decision"
        } else {
            "before_output_bind_decision"
        };
        let mut loser_command = fault_child_command(
            &f,
            &b,
            loser,
            loser_checkpoint,
            &loser_marker,
            &loser_result,
        );
        loser_command.env("VESTRACE_OUTPUT_PREPARATION_ID", p.as_uuid().to_string());
        let mut loser_child = OutputDecisionChild(Some(loser_command.spawn().unwrap()));
        wait_for_checkpoint(loser_child.0.as_mut().unwrap(), &loser_marker);
        let marker = artifacts.path().join("winner-marker");
        let result = artifacts.path().join("winner-result");
        let mut command = fault_child_command(&f, &b, winner, checkpoint, &marker, &result);
        command.env("VESTRACE_OUTPUT_PREPARATION_ID", p.as_uuid().to_string());
        let mut child = OutputDecisionChild(Some(command.spawn().unwrap()));
        wait_for_checkpoint(child.0.as_mut().unwrap(), &marker);
        let dir = f.vault.path().join(b.key_id.to_string());
        let decision_before = fs::read(dir.join("output-disposition")).unwrap();
        let decision: serde_json::Value = serde_json::from_slice(&decision_before).unwrap();
        assert_eq!(decision["kind"], if bind_wins { "bound" } else { "retire" });
        assert!(dir.join("active/envelope").exists());
        assert!(!dir.join("fence").exists());
        fs::write(loser_marker.with_extension("release"), b"release").unwrap();
        let rejected = finish_child(loser_child.0.take().unwrap(), &loser_result);
        assert!(
            rejected.starts_with("err:"),
            "loser unexpectedly succeeded: {rejected}"
        );
        assert!(dir.join("active/envelope").exists());
        assert!(!dir.join("fence").exists());
        child.0.as_mut().unwrap().kill().unwrap();
        assert!(!child.0.as_mut().unwrap().wait().unwrap().success());
        let replay_result = artifacts.path().join("replay-result");
        let mut command = child_command(&f, &b, winner, &replay_result);
        command.env("VESTRACE_OUTPUT_PREPARATION_ID", p.as_uuid().to_string());
        let witness = parse_child_uuid(&finish_child(command.spawn().unwrap(), &replay_result));
        assert_eq!(
            fs::read(dir.join("output-disposition")).unwrap(),
            decision_before
        );
        let mut calls = 0;
        if bind_wins {
            assert_eq!(
                decision["binding_receipt"].as_str().unwrap(),
                witness.to_string()
            );
            f.vault()
                .with_bound_embedding_output_key(
                    &b,
                    p,
                    MaterialKeyBindingReceipt::from_uuid(witness),
                    &mut |_| calls += 1,
                )
                .unwrap();
            assert_eq!(calls, 1);
            assert!(dir.join("active/envelope").exists());
            assert!(!dir.join("fence").exists());
        } else {
            assert!(!dir.join("active/envelope").exists());
            assert!(!dir.join("retired-active/envelope").exists());
            let fence: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.join("fence")).unwrap()).unwrap();
            assert_eq!(fence["receipt"], decision["fence_receipt"]);
            assert!(
                f.vault()
                    .with_bound_embedding_output_key(
                        &b,
                        p,
                        MaterialKeyBindingReceipt::new(),
                        &mut |_| calls += 1
                    )
                    .is_err()
            );
            assert!(
                f.vault()
                    .with_embedding_output_key(&b, &mut |_| calls += 1)
                    .is_err()
            );
            assert_eq!(calls, 0);
            assert_eq!(
                f.vault().retire_embedding_output(&b).unwrap().as_uuid(),
                witness
            );
        }
    }
}

#[test]
fn output_disposition_legacy_retirement_and_corrupt_records_fail_closed() {
    use vestrace_application::EmbeddingResultPreparationId;
    let f = Fixture::new();
    let b = f.binding();
    let p = EmbeddingResultPreparationId::new();
    f.vault().create_embedding_output_if_absent(&b).unwrap();
    let erased = f.vault().retire_embedding_output(&b).unwrap();
    let dir = f.vault.path().join(b.key_id.to_string());
    // 14C persisted these exact stages but had no output-disposition.
    fs::remove_file(dir.join("output-disposition")).unwrap();
    let fence = fs::read(dir.join("fence")).unwrap();
    assert!(f.vault().bind_embedding_output(&b, p).is_err());
    assert_eq!(f.vault().retire_embedding_output(&b).unwrap(), erased);
    assert_eq!(fs::read(dir.join("fence")).unwrap(), fence);
    for corruption in 0..5 {
        let f = Fixture::new();
        let b = f.binding();
        f.vault().create_embedding_output_if_absent(&b).unwrap();
        let receipt = f.vault().bind_embedding_output(&b, p).unwrap();
        let dir = f.vault.path().join(b.key_id.to_string());
        let mut decision: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("output-disposition")).unwrap()).unwrap();
        match corruption {
            0 => {
                decision["claim_receipt"] = serde_json::json!(uuid::Uuid::now_v7());
            }
            1 => {
                decision["authority"]["job_id"] = serde_json::json!(uuid::Uuid::now_v7());
            }
            2 => {
                decision["version"] = serde_json::json!(99);
            }
            3 => {
                decision.as_object_mut().unwrap().remove("binding_receipt");
            }
            _ => {
                let claim: serde_json::Value =
                    serde_json::from_slice(&fs::read(dir.join("claim")).unwrap()).unwrap();
                fs::write(dir.join("fence"),serde_json::to_vec(&serde_json::json!({"version":1,"claim_receipt":claim["receipt"],"receipt":uuid::Uuid::now_v7()})).unwrap()).unwrap();
            }
        }
        fs::write(
            dir.join("output-disposition"),
            serde_json::to_vec(&decision).unwrap(),
        )
        .unwrap();
        let original = fs::read(dir.join("output-disposition")).unwrap();
        let envelope = fs::read(dir.join("active/envelope")).unwrap();
        assert!(f.vault().bind_embedding_output(&b, p).is_err());
        assert!(f.vault().retire_embedding_output(&b).is_err());
        let mut calls = 0;
        assert!(
            f.vault()
                .with_bound_embedding_output_key(&b, p, receipt, &mut |_| calls += 1)
                .is_err()
        );
        assert_eq!(calls, 0);
        assert_eq!(fs::read(dir.join("output-disposition")).unwrap(), original);
        assert_eq!(fs::read(dir.join("active/envelope")).unwrap(), envelope);
    }
}

#[test]
fn bound_creation_replay_never_regenerates_a_missing_envelope() {
    let f = Fixture::new();
    let b = f.binding();
    let p = vestrace_application::EmbeddingResultPreparationId::new();
    let created = f.vault().create_embedding_output_if_absent(&b).unwrap();
    f.vault().bind_embedding_output(&b, p).unwrap();
    assert_eq!(
        f.vault().create_embedding_output_if_absent(&b).unwrap(),
        created
    );
    let dir = f.vault.path().join(b.key_id.to_string());
    let decision = fs::read(dir.join("output-disposition")).unwrap();
    fs::remove_file(dir.join("active/envelope")).unwrap();
    assert!(f.vault().create_embedding_output_if_absent(&b).is_err());
    assert!(!dir.join("active/envelope").exists());
    assert_eq!(fs::read(dir.join("output-disposition")).unwrap(), decision);
}
