use std::{
    env,
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::Mutex,
};

use ring::rand::{SecureRandom as _, SystemRandom};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use vestrace_application::{ApplicationError, HealthRepository};
use vestrace_domain::installation::{
    FingerprintKey, FingerprintKeyId, FingerprintKeyVersion, InstallationFingerprintKey,
    InstallationId,
};
use zeroize::Zeroize as _;

use super::PgStore;

/// Required host-owned directory for the one create-only installation key.
/// It is deliberately separate from the mounted bootstrap store: this record
/// contains fingerprint-key material and never a material-key envelope.
pub const INSTALLATION_FINGERPRINT_VAULT_ROOT_ENV: &str =
    "VESTRACE_INSTALLATION_FINGERPRINT_VAULT_ROOT";
pub const INSTALLATION_FINGERPRINT_RECORD_FILE: &str = "installation-fingerprint-v1.json";

/// Internal diagnosis for a fail-closed installation fingerprint readiness
/// refusal. It intentionally excludes paths, key material, and database
/// details so it can be logged without changing the opaque health response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallationFingerprintReadinessCause {
    VaultRootAbsent,
    VaultRootUnavailable,
    HostRecordUnavailable,
    CreateOnlyRecordRejected,
    DatabaseUnavailable,
    ContinuityProofRejected,
}

impl InstallationFingerprintReadinessCause {
    fn as_str(self) -> &'static str {
        match self {
            Self::VaultRootAbsent => "vault_root_absent",
            Self::VaultRootUnavailable => "vault_root_unavailable",
            Self::HostRecordUnavailable => "host_record_unavailable",
            Self::CreateOnlyRecordRejected => "create_only_record_rejected",
            Self::DatabaseUnavailable => "database_unavailable",
            Self::ContinuityProofRejected => "continuity_proof_rejected",
        }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[derive(Deserialize, Serialize)]
struct StoredInstallationFingerprintKey {
    installation_id: uuid::Uuid,
    fingerprint_key_id: uuid::Uuid,
    version: i32,
    key: Vec<u8>,
}

enum HostFingerprintVaultError {
    NotFound,
    Unavailable,
}

/// A create-only host-vault record for the installation fingerprint key.
/// PostgreSQL never receives `StoredInstallationFingerprintKey::key`.
pub struct HostInstallationFingerprintVault {
    root: PathBuf,
}

impl HostInstallationFingerprintVault {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, ApplicationError> {
        fs::create_dir_all(root.as_ref()).map_err(|_| unavailable())?;
        let root = fs::canonicalize(root.as_ref()).map_err(|_| unavailable())?;
        Ok(Self { root })
    }

    pub fn from_environment() -> Result<Self, ApplicationError> {
        let root = env::var_os(INSTALLATION_FINGERPRINT_VAULT_ROOT_ENV).ok_or_else(unavailable)?;
        Self::open(PathBuf::from(root))
    }

    fn record_path(&self) -> PathBuf {
        self.root.join(INSTALLATION_FINGERPRINT_RECORD_FILE)
    }

    fn read(&self) -> Result<StoredInstallationFingerprintKey, HostFingerprintVaultError> {
        let bytes = fs::read(self.record_path()).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => HostFingerprintVaultError::NotFound,
            _ => HostFingerprintVaultError::Unavailable,
        })?;
        serde_json::from_slice(&bytes).map_err(|_| HostFingerprintVaultError::Unavailable)
    }

    fn decode(
        &self,
        mut stored: StoredInstallationFingerprintKey,
    ) -> Result<InstallationFingerprintKey, HostFingerprintVaultError> {
        if stored.version != FingerprintKeyVersion::V1.as_i32() {
            stored.key.zeroize();
            return Err(HostFingerprintVaultError::Unavailable);
        }
        let key_bytes: Result<[u8; 32], _> = stored.key.as_slice().try_into();
        stored.key.zeroize();
        let key_bytes = key_bytes.map_err(|_| HostFingerprintVaultError::Unavailable)?;
        Ok(InstallationFingerprintKey::new(
            InstallationId::from_uuid(stored.installation_id),
            FingerprintKeyId::from_uuid(stored.fingerprint_key_id),
            FingerprintKeyVersion::V1,
            FingerprintKey::from_bytes(key_bytes),
        ))
    }

    fn load_existing(&self) -> Result<InstallationFingerprintKey, HostFingerprintVaultError> {
        self.read().and_then(|stored| self.decode(stored))
    }

    fn write_new(
        &self,
        record: &StoredInstallationFingerprintKey,
    ) -> Result<(), HostFingerprintVaultError> {
        let mut bytes =
            serde_json::to_vec(record).map_err(|_| HostFingerprintVaultError::Unavailable)?;
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.record_path())
                .map_err(|error| match error.kind() {
                    std::io::ErrorKind::AlreadyExists => HostFingerprintVaultError::NotFound,
                    _ => HostFingerprintVaultError::Unavailable,
                })?;
            file.write_all(&bytes)
                .map_err(|_| HostFingerprintVaultError::Unavailable)?;
            file.sync_all()
                .map_err(|_| HostFingerprintVaultError::Unavailable)
        })();
        bytes.zeroize();
        result
    }

    /// Opens the host record or creates it once. A concurrent creator reloads
    /// the winner's record; it never replaces it with a second key.
    pub fn load_or_create(&self) -> Result<InstallationFingerprintKey, ApplicationError> {
        match self.load_existing() {
            Ok(key) => Ok(key),
            Err(HostFingerprintVaultError::Unavailable) => Err(unavailable()),
            Err(HostFingerprintVaultError::NotFound) => {
                let mut key_bytes = [0_u8; 32];
                SystemRandom::new()
                    .fill(&mut key_bytes)
                    .map_err(|_| unavailable())?;
                let record = StoredInstallationFingerprintKey {
                    installation_id: InstallationId::new().as_uuid(),
                    fingerprint_key_id: FingerprintKeyId::new().as_uuid(),
                    version: FingerprintKeyVersion::V1.as_i32(),
                    key: key_bytes.to_vec(),
                };
                key_bytes.zeroize();
                match self.write_new(&record) {
                    Ok(()) => self.decode(record).map_err(|_| unavailable()),
                    Err(HostFingerprintVaultError::NotFound) => {
                        self.load_existing().map_err(|_| unavailable())
                    }
                    Err(HostFingerprintVaultError::Unavailable) => Err(unavailable()),
                }
            }
        }
    }
}

fn unavailable() -> ApplicationError {
    ApplicationError::Unavailable("installation fingerprint continuity is not ready".to_owned())
}

fn update_retained_cause(
    retained: &mut Option<InstallationFingerprintReadinessCause>,
    cause: InstallationFingerprintReadinessCause,
) -> bool {
    if *retained == Some(cause) {
        false
    } else {
        *retained = Some(cause);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{InstallationFingerprintReadinessCause, update_retained_cause};

    #[test]
    fn readiness_diagnostic_logs_only_cause_transitions() {
        let mut retained = None;

        assert!(update_retained_cause(
            &mut retained,
            InstallationFingerprintReadinessCause::VaultRootAbsent,
        ));
        assert!(!update_retained_cause(
            &mut retained,
            InstallationFingerprintReadinessCause::VaultRootAbsent,
        ));
        assert!(update_retained_cause(
            &mut retained,
            InstallationFingerprintReadinessCause::DatabaseUnavailable,
        ));
        assert!(!update_retained_cause(
            &mut retained,
            InstallationFingerprintReadinessCause::DatabaseUnavailable,
        ));
    }
}

/// Persists only the public continuity binding and checks it against the
/// create-only key record held by the host vault caller.
#[derive(Clone, Debug)]
pub struct PgInstallationFingerprintSupervisor {
    pool: PgPool,
}

impl PgInstallationFingerprintSupervisor {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn record_create_only(
        &self,
        key: &InstallationFingerprintKey,
    ) -> Result<(), ApplicationError> {
        let proof = key.continuity_proof();
        let recorded: bool = sqlx::query_scalar(
            "SELECT vestrace_record_installation_fingerprint_continuity($1, $2, $3, $4)",
        )
        .bind(key.installation_id().as_uuid())
        .bind(key.fingerprint_key_id().as_uuid())
        .bind(key.version().as_i32())
        .bind(proof.as_bytes().as_slice())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        if recorded {
            Ok(())
        } else {
            Err(ApplicationError::Conflict(
                "installation fingerprint already exists".to_owned(),
            ))
        }
    }

    /// Fails closed whenever the host-vault record cannot reproduce the exact
    /// PostgreSQL continuity binding. Neither the key nor its bytes are read
    /// back from PostgreSQL.
    pub async fn ensure_ready(
        &self,
        key: &InstallationFingerprintKey,
    ) -> Result<(), ApplicationError> {
        let proof = key.continuity_proof();
        let matches: bool = sqlx::query_scalar(
            "SELECT vestrace_installation_fingerprint_continuity_matches($1, $2, $3, $4)",
        )
        .bind(key.installation_id().as_uuid())
        .bind(key.fingerprint_key_id().as_uuid())
        .bind(key.version().as_i32())
        .bind(proof.as_bytes().as_slice())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        if matches {
            Ok(())
        } else {
            Err(ApplicationError::Unavailable(
                "installation fingerprint continuity is not ready".to_owned(),
            ))
        }
    }
}

/// Production readiness composition. It records the host-vault key's public
/// continuity binding during initialization, then reopens that host record and
/// recomputes its proof on every readiness probe.
pub struct PgInstallationFingerprintReadiness {
    store: PgStore,
    supervisor: PgInstallationFingerprintSupervisor,
    vault: Option<HostInstallationFingerprintVault>,
    retained_cause: Mutex<Option<InstallationFingerprintReadinessCause>>,
}

impl PgInstallationFingerprintReadiness {
    pub async fn initialize(store: PgStore, root: impl AsRef<Path>) -> Self {
        let supervisor = PgInstallationFingerprintSupervisor::new(store.pool().clone());
        Self::initialize_with_vault(
            store,
            supervisor,
            HostInstallationFingerprintVault::open(root).map_err(|error| {
                (
                    InstallationFingerprintReadinessCause::VaultRootUnavailable,
                    error,
                )
            }),
        )
        .await
    }

    pub async fn initialize_from_environment(store: PgStore) -> Self {
        let supervisor = PgInstallationFingerprintSupervisor::new(store.pool().clone());
        let vault = env::var_os(INSTALLATION_FINGERPRINT_VAULT_ROOT_ENV)
            .ok_or_else(|| {
                (
                    InstallationFingerprintReadinessCause::VaultRootAbsent,
                    unavailable(),
                )
            })
            .and_then(|root| {
                HostInstallationFingerprintVault::open(PathBuf::from(root)).map_err(|error| {
                    (
                        InstallationFingerprintReadinessCause::VaultRootUnavailable,
                        error,
                    )
                })
            });
        Self::initialize_with_vault(store, supervisor, vault).await
    }

    async fn initialize_with_vault(
        store: PgStore,
        supervisor: PgInstallationFingerprintSupervisor,
        vault: Result<
            HostInstallationFingerprintVault,
            (InstallationFingerprintReadinessCause, ApplicationError),
        >,
    ) -> Self {
        let (vault, retained_cause) = match vault {
            Err((cause, error)) => {
                Self::log_failure(cause, &error);
                (None, Some(cause))
            }
            Ok(vault) => match vault.load_or_create() {
                Err(error) => {
                    let cause = InstallationFingerprintReadinessCause::HostRecordUnavailable;
                    Self::log_failure(cause, &error);
                    (None, Some(cause))
                }
                Ok(key) => match supervisor.record_create_only(&key).await {
                    Ok(()) => (Some(vault), None),
                    Err(error) => {
                        let cause = InstallationFingerprintReadinessCause::CreateOnlyRecordRejected;
                        Self::log_failure(cause, &error);
                        (None, Some(cause))
                    }
                },
            },
        };

        Self {
            store,
            supervisor,
            vault,
            retained_cause: Mutex::new(retained_cause),
        }
    }

    /// Returns the current internal failure classification for startup and
    /// readiness diagnostics. It is deliberately not exposed through HTTP.
    pub fn retained_cause(&self) -> Option<InstallationFingerprintReadinessCause> {
        *self
            .retained_cause
            .lock()
            .expect("installation fingerprint readiness cause lock must not be poisoned")
    }

    fn log_failure(cause: InstallationFingerprintReadinessCause, error: &ApplicationError) {
        eprintln!(
            "installation fingerprint readiness is unavailable: cause={}, error={error}",
            cause.as_str()
        );
    }

    fn refuse(
        &self,
        cause: InstallationFingerprintReadinessCause,
        error: &ApplicationError,
    ) -> ApplicationError {
        let mut retained = self
            .retained_cause
            .lock()
            .expect("installation fingerprint readiness cause lock must not be poisoned");
        if update_retained_cause(&mut retained, cause) {
            Self::log_failure(cause, error);
        }
        unavailable()
    }

    fn clear_retained_cause(&self) {
        *self
            .retained_cause
            .lock()
            .expect("installation fingerprint readiness cause lock must not be poisoned") = None;
    }
}

#[async_trait::async_trait]
impl HealthRepository for PgInstallationFingerprintReadiness {
    async fn check(&self) -> Result<(), ApplicationError> {
        if let Err(error) = HealthRepository::check(&self.store).await {
            return Err(self.refuse(
                InstallationFingerprintReadinessCause::DatabaseUnavailable,
                &error,
            ));
        }
        let vault = match self.vault.as_ref() {
            Some(vault) => vault,
            None => {
                let cause = self
                    .retained_cause()
                    .unwrap_or(InstallationFingerprintReadinessCause::VaultRootUnavailable);
                return Err(self.refuse(cause, &unavailable()));
            }
        };
        let key = vault.load_existing().map_err(|_| {
            self.refuse(
                InstallationFingerprintReadinessCause::HostRecordUnavailable,
                &unavailable(),
            )
        })?;
        match self.supervisor.ensure_ready(&key).await {
            Ok(()) => {
                self.clear_retained_cause();
                Ok(())
            }
            Err(error) => {
                let cause = match error {
                    ApplicationError::Unavailable(_) => {
                        InstallationFingerprintReadinessCause::ContinuityProofRejected
                    }
                    _ => InstallationFingerprintReadinessCause::DatabaseUnavailable,
                };
                Err(self.refuse(cause, &error))
            }
        }
    }
}
