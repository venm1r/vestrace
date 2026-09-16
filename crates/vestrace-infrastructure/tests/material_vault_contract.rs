use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use sqlx::PgPool;
use tempfile::TempDir;
use vestrace_application::{MaterialKeyVault, VaultError};
use vestrace_domain::trust::{KeyPurpose, KeyReference, SecretResolutionRequest};
use vestrace_domain::{IntentNonce, MaterialKeyId, WorkspaceId};
use vestrace_infrastructure::crypto::{HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER};

const BOOTSTRAP_KEY_ID: &str = "material-vault-bootstrap";
const BOOTSTRAP_SCOPE: &str = "material-vault-bootstrap";
const BOOTSTRAP_ALGORITHM: &str = "aes-256-gcm-v1";

struct VaultFixture {
    bootstrap_root: TempDir,
    vault_root: TempDir,
    bootstrap_reference: KeyReference,
    bootstrap_request: SecretResolutionRequest,
}

impl VaultFixture {
    fn new() -> Self {
        let bootstrap_root = TempDir::new().expect("bootstrap mount");
        write_bootstrap_key(bootstrap_root.path());
        Self {
            vault_root: TempDir::new().expect("material vault"),
            bootstrap_reference: KeyReference::new(
                MOUNTED_SECRET_STORE_PROVIDER,
                BOOTSTRAP_KEY_ID,
                "v1",
                KeyPurpose::Storage,
                BOOTSTRAP_SCOPE,
                BOOTSTRAP_ALGORITHM,
            )
            .expect("bootstrap reference"),
            bootstrap_request: SecretResolutionRequest::new(
                WorkspaceId::new(),
                BOOTSTRAP_SCOPE,
                "test://material-vault",
            ),
            bootstrap_root,
        }
    }

    fn vault(&self) -> HostMaterialKeyVault {
        HostMaterialKeyVault::new(
            self.vault_root.path(),
            self.bootstrap_root.path(),
            self.bootstrap_reference.clone(),
            self.bootstrap_request.clone(),
        )
        .expect("separate host vault")
    }
}

fn write_bootstrap_key(root: &Path) {
    let key_root = root.join(BOOTSTRAP_KEY_ID);
    let version_root = key_root.join("v1");
    fs::create_dir_all(&version_root).expect("bootstrap key directory");
    fs::write(key_root.join("scope"), BOOTSTRAP_SCOPE).expect("bootstrap scope");
    fs::write(key_root.join("purpose"), "storage").expect("bootstrap purpose");
    fs::write(key_root.join("algorithm"), BOOTSTRAP_ALGORITHM).expect("bootstrap algorithm");
    fs::write(version_root.join("state"), "active").expect("bootstrap state");
    fs::write(version_root.join("private.pkcs8"), [0x5A; 32]).expect("bootstrap key material");
}

fn mounted_store_snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut snapshot = BTreeMap::new();
    snapshot_directory(root, root, &mut snapshot);
    snapshot
}

fn snapshot_directory(root: &Path, directory: &Path, snapshot: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in fs::read_dir(directory).expect("mounted store directory") {
        let entry = entry.expect("mounted store entry");
        let path = entry.path();
        if path.is_dir() {
            snapshot_directory(root, &path, snapshot);
        } else {
            snapshot.insert(
                path.strip_prefix(root)
                    .expect("mounted store relative path")
                    .to_path_buf(),
                fs::read(path).expect("mounted store file"),
            );
        }
    }
}

#[test]
fn create_if_absent_is_idempotent_for_the_same_nonce() {
    let fixture = VaultFixture::new();
    let vault = fixture.vault();
    let other_handle = fixture.vault();
    let key_id = MaterialKeyId::new();
    let nonce = IntentNonce::new();

    let first = vault.create_if_absent(key_id, nonce).expect("first create");
    let replay = vault
        .create_if_absent(key_id, nonce)
        .expect("replayed create");

    assert_eq!(replay, first);
    let mut identical = false;
    vault
        .unwrap(key_id, &mut |first_dek| {
            other_handle
                .unwrap(key_id, &mut |replayed_dek| {
                    identical =
                        first_dek.expose(|first| replayed_dek.expose(|replayed| first == replayed));
                })
                .expect("replayed unwrap");
        })
        .expect("first unwrap");
    assert!(identical, "a replayed create must retain the original DEK");
}

#[test]
fn create_if_absent_refuses_a_different_nonce_for_the_same_key_id() {
    let fixture = VaultFixture::new();
    let vault = fixture.vault();
    let key_id = MaterialKeyId::new();

    vault
        .create_if_absent(key_id, IntentNonce::new())
        .expect("first create");

    assert!(matches!(
        vault.create_if_absent(key_id, IntentNonce::new()),
        Err(VaultError::NonceMismatch)
    ));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn unwrap_refuses_after_prepare_erasure_without_consulting_postgres(pool: PgPool) {
    let fixture = VaultFixture::new();
    let vault = fixture.vault();
    let key_id = MaterialKeyId::new();

    vault
        .create_if_absent(key_id, IntentNonce::new())
        .expect("create key");
    vault.prepare_erasure(key_id).expect("prepare erasure");

    // The only PostgreSQL connection is deliberately closed before the refusal.
    // The adapter has no pool or database URL and must reject from its durable
    // host-side fence before resolving bootstrap material.
    pool.close().await;
    assert!(matches!(
        vault.unwrap(key_id, &mut |_| panic!("a fenced DEK must not be exposed")),
        Err(VaultError::ErasurePrepared)
    ));
}

#[test]
fn prepare_erasure_is_idempotent_and_witnessed() {
    let fixture = VaultFixture::new();
    let key_id = MaterialKeyId::new();

    fixture
        .vault()
        .create_if_absent(key_id, IntentNonce::new())
        .expect("create key");
    let first = fixture
        .vault()
        .prepare_erasure(key_id)
        .expect("first fence");
    let replay = fixture
        .vault()
        .prepare_erasure(key_id)
        .expect("replayed fence");

    assert_eq!(
        replay, first,
        "the durable fence keeps its original witness"
    );
    assert!(matches!(
        fixture
            .vault()
            .unwrap(key_id, &mut |_| panic!("a fenced DEK must not be exposed")),
        Err(VaultError::ErasurePrepared)
    ));
}

#[test]
fn erase_after_prepare_is_idempotent() {
    let fixture = VaultFixture::new();
    let vault = fixture.vault();
    let key_id = MaterialKeyId::new();

    vault
        .create_if_absent(key_id, IntentNonce::new())
        .expect("create key");
    vault.prepare_erasure(key_id).expect("prepare erasure");
    let first = vault.erase(key_id).expect("first erase");
    let replay = vault.erase(key_id).expect("replayed erase");

    assert_eq!(replay, first);
    assert!(matches!(
        vault.unwrap(key_id, &mut |_| panic!("an erased DEK must not be exposed")),
        Err(VaultError::Erased)
    ));
}

#[test]
fn erase_without_prepare_is_refused() {
    let fixture = VaultFixture::new();
    let vault = fixture.vault();
    let key_id = MaterialKeyId::new();

    vault
        .create_if_absent(key_id, IntentNonce::new())
        .expect("create key");

    assert!(matches!(
        vault.erase(key_id),
        Err(VaultError::ErasureNotPrepared)
    ));
}

#[test]
fn dek_is_zeroized_after_use() {
    let fixture = VaultFixture::new();
    let vault = fixture.vault();
    let key_id = MaterialKeyId::new();
    let mut used = false;

    vault
        .create_if_absent(key_id, IntentNonce::new())
        .expect("create key");
    vault
        .unwrap(key_id, &mut |dek| {
            used = dek.expose(|bytes| bytes.iter().any(|byte| *byte != 0));
        })
        .expect("unwrap key");

    assert!(used, "the callback receives the generated DEK");
}

#[test]
fn vault_never_writes_dek_envelopes_into_the_mounted_bootstrap_store() {
    let fixture = VaultFixture::new();
    let before = mounted_store_snapshot(fixture.bootstrap_root.path());
    let vault = fixture.vault();
    let key_id = MaterialKeyId::new();

    vault
        .create_if_absent(key_id, IntentNonce::new())
        .expect("create key");
    vault.unwrap(key_id, &mut |_| {}).expect("unwrap key");
    vault.prepare_erasure(key_id).expect("prepare erasure");
    vault.erase(key_id).expect("erase key");

    assert_eq!(
        mounted_store_snapshot(fixture.bootstrap_root.path()),
        before
    );
}
