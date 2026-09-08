//! Host-owned encrypted envelopes for per-material data-encryption keys.
//!
//! New records are per-key directories with append-only fixed stages. A stage
//! is written to a unique same-directory temporary file, synced, then published
//! with a hard link; a competing process adopts the installed witness.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use vestrace_application::{
    EmbeddingOutputKeyBinding, EmbeddingResultPreparationId, FenceReceipt, MaterialKeyVault,
    VaultError,
};
use vestrace_domain::trust::{KeyProvider, KeyReference, SecretResolutionRequest};
use vestrace_domain::{
    ErasureReceipt, IntentNonce, MaterialKeyBindingReceipt, MaterialKeyId, VaultReceipt,
    ZeroizingDek,
};
use zeroize::Zeroize as _;

use super::{EnvelopeCipher, KEY_LENGTH, MountedSecretStoreKeyProvider, Sealed};

const AAD_DOMAIN: &str = "vestrace-material-vault-v1";
const VERSION: u8 = 1;

pub struct HostMaterialKeyVault {
    vault_root: PathBuf,
    bootstrap_provider: MountedSecretStoreKeyProvider,
    bootstrap_reference: KeyReference,
    bootstrap_request: SecretResolutionRequest,
    cipher: EnvelopeCipher,
    #[cfg(debug_assertions)]
    fault_checkpoint: Option<(String, PathBuf)>,
}

// Compatibility format for records written before the fixed-stage protocol.
#[derive(Deserialize, Serialize)]
struct Legacy {
    nonce: uuid::Uuid,
    receipt: uuid::Uuid,
    envelope: Option<Envelope>,
    fence_receipt: Option<uuid::Uuid>,
    erasure_receipt: Option<uuid::Uuid>,
}
#[derive(Clone, Deserialize, Serialize)]
struct Envelope {
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Authority {
    Ordinary {
        nonce: uuid::Uuid,
    },
    EmbeddingOutput {
        workspace_id: uuid::Uuid,
        job_id: uuid::Uuid,
        intent_id: uuid::Uuid,
        material_id: uuid::Uuid,
        key_id: uuid::Uuid,
        nonce: uuid::Uuid,
        output_ordinal: u64,
    },
}
#[derive(Clone, Deserialize, Serialize)]
struct Claim {
    version: u8,
    receipt: uuid::Uuid,
    authority: Authority,
}
#[derive(Deserialize, Serialize)]
struct EnvelopeStage {
    version: u8,
    claim_receipt: uuid::Uuid,
    envelope: Envelope,
}
#[derive(Deserialize, Serialize)]
struct WitnessStage {
    version: u8,
    claim_receipt: uuid::Uuid,
    receipt: uuid::Uuid,
}

/// The only decision pathname consulted by both specialized mutators.
/// Old processes that do not consult this stage must be stopped on upgrade.
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum OutputDisposition {
    Bound {
        version: u8,
        claim_receipt: uuid::Uuid,
        authority: Authority,
        preparation_id: uuid::Uuid,
        binding_receipt: uuid::Uuid,
    },
    Retire {
        version: u8,
        claim_receipt: uuid::Uuid,
        authority: Authority,
        fence_receipt: uuid::Uuid,
    },
}

impl Authority {
    fn ordinary(nonce: IntentNonce) -> Self {
        Self::Ordinary {
            nonce: nonce.as_uuid(),
        }
    }
    fn output(binding: &EmbeddingOutputKeyBinding) -> Self {
        Self::EmbeddingOutput {
            workspace_id: binding.workspace_id.as_uuid(),
            job_id: binding.job_id.as_uuid(),
            intent_id: binding.intent_id.as_uuid(),
            material_id: binding.material_id.as_uuid(),
            key_id: binding.key_id.as_uuid(),
            nonce: binding.nonce.as_uuid(),
            output_ordinal: binding.output_ordinal,
        }
    }
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Ordinary { nonce: left }, Self::Ordinary { nonce: right }) => left == right,
            (
                Self::EmbeddingOutput {
                    workspace_id: a,
                    job_id: b,
                    intent_id: c,
                    material_id: d,
                    key_id: e,
                    nonce: f,
                    output_ordinal: g,
                },
                Self::EmbeddingOutput {
                    workspace_id: h,
                    job_id: i,
                    intent_id: j,
                    material_id: k,
                    key_id: l,
                    nonce: m,
                    output_ordinal: n,
                },
            ) => a == h && b == i && c == j && d == k && e == l && f == m && g == n,
            _ => false,
        }
    }
    fn output_key(&self) -> bool {
        matches!(self, Self::EmbeddingOutput { .. })
    }
}

impl HostMaterialKeyVault {
    pub fn new(
        vault_root: impl Into<PathBuf>,
        bootstrap_root: impl AsRef<Path>,
        bootstrap_reference: KeyReference,
        bootstrap_request: SecretResolutionRequest,
    ) -> Result<Self, VaultError> {
        let root = vault_root.into();
        fs::create_dir_all(&root).map_err(|_| VaultError::Unavailable)?;
        let vault_root = fs::canonicalize(root).map_err(|_| VaultError::Unavailable)?;
        let bootstrap_root =
            fs::canonicalize(bootstrap_root.as_ref()).map_err(|_| VaultError::Unavailable)?;
        if vault_root.starts_with(&bootstrap_root) || bootstrap_root.starts_with(&vault_root) {
            return Err(VaultError::Unavailable);
        }
        Self::verify_hard_links(&vault_root)?;
        Ok(Self {
            vault_root,
            bootstrap_provider: MountedSecretStoreKeyProvider::new(bootstrap_root),
            bootstrap_reference,
            bootstrap_request,
            cipher: EnvelopeCipher::new(),
            #[cfg(debug_assertions)]
            fault_checkpoint: None,
        })
    }

    /// Installs an explicit debug-build-only process-death checkpoint. Normal
    /// constructors can never activate it, and release artifacts do not expose
    /// the hook at all.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn with_test_fault_checkpoint(
        mut self,
        checkpoint: impl Into<String>,
        marker: impl Into<PathBuf>,
    ) -> Self {
        self.fault_checkpoint = Some((checkpoint.into(), marker.into()));
        self
    }

    fn test_checkpoint(&self, checkpoint: &str) {
        #[cfg(debug_assertions)]
        if let Some((expected, marker)) = &self.fault_checkpoint {
            if expected == checkpoint {
                fs::write(marker, checkpoint).expect("test checkpoint marker must be writable");
                let release = marker.with_extension("release");
                while !release.exists() {
                    std::thread::yield_now();
                }
            }
        }
    }

    fn verify_hard_links(root: &Path) -> Result<(), VaultError> {
        let source = root.join(format!(".link-probe-{}", uuid::Uuid::now_v7()));
        let link = root.join(format!(".link-probe-link-{}", uuid::Uuid::now_v7()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&source)?;
            file.write_all(b"probe")?;
            file.sync_all()?;
            fs::hard_link(&source, &link)
        })();
        let _ = fs::remove_file(&source);
        let _ = fs::remove_file(&link);
        result.map(|_| ()).map_err(|_| VaultError::Unavailable)
    }
    fn legacy_path(&self, key: MaterialKeyId) -> PathBuf {
        self.vault_root.join(format!("{}.json", key.as_uuid()))
    }
    fn dir(&self, key: MaterialKeyId) -> PathBuf {
        self.vault_root.join(key.as_uuid().to_string())
    }
    fn stage(&self, key: MaterialKeyId, stage: &str) -> PathBuf {
        self.dir(key).join(stage)
    }
    fn active_dir(&self, key: MaterialKeyId) -> PathBuf {
        self.dir(key).join("active")
    }
    fn retired_active_dir(&self, key: MaterialKeyId) -> PathBuf {
        self.dir(key).join("retired-active")
    }
    fn active_stage(&self, key: MaterialKeyId, stage: &str) -> PathBuf {
        self.active_dir(key).join(stage)
    }
    fn read_legacy(&self, key: MaterialKeyId) -> Result<Option<Legacy>, VaultError> {
        let path = self.legacy_path(key);
        if !path.exists() {
            return Ok(None);
        }
        serde_json::from_slice(&fs::read(path).map_err(|_| VaultError::Unavailable)?)
            .map(Some)
            .map_err(|_| VaultError::Unavailable)
    }
    fn write_legacy(&self, key: MaterialKeyId, record: &Legacy) -> Result<(), VaultError> {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(self.legacy_path(key))
            .map_err(|_| VaultError::Unavailable)?;
        file.write_all(&serde_json::to_vec(record).map_err(|_| VaultError::Unavailable)?)
            .map_err(|_| VaultError::Unavailable)?;
        file.sync_all().map_err(|_| VaultError::Unavailable)
    }
    fn read<T: DeserializeOwned>(
        &self,
        key: MaterialKeyId,
        stage: &str,
    ) -> Result<Option<T>, VaultError> {
        match fs::read(self.stage(key, stage)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| VaultError::Unavailable),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(VaultError::Unavailable),
        }
    }
    /// `false` means an existing fixed stage won; it is then read and checked
    /// against caller-known authority, never against a losing random receipt.
    fn publish<T: Serialize>(
        &self,
        key: MaterialKeyId,
        stage: &str,
        value: &T,
    ) -> Result<bool, VaultError> {
        let dir = self.dir(key);
        fs::create_dir_all(&dir).map_err(|_| VaultError::Unavailable)?;
        let temporary = dir.join(format!(".{stage}.tmp-{}", uuid::Uuid::now_v7()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec(value).map_err(std::io::Error::other)?)?;
            file.sync_all()?;
            fs::hard_link(&temporary, dir.join(stage))
        })();
        let _ = fs::remove_file(&temporary);
        match result {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(_) => Err(VaultError::Unavailable),
        }
    }
    fn publish_active<T: Serialize>(
        &self,
        key: MaterialKeyId,
        stage: &str,
        value: &T,
    ) -> Result<bool, VaultError> {
        let active = self.active_dir(key);
        if !active.is_dir() || self.retired_active_dir(key).exists() {
            return Err(VaultError::ErasurePrepared);
        }
        let temporary = active.join(format!(".{stage}.tmp-{}", uuid::Uuid::now_v7()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec(value).map_err(std::io::Error::other)?)?;
            file.sync_all()?;
            fs::hard_link(&temporary, active.join(stage))
        })();
        let _ = fs::remove_file(&temporary);
        match result {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(VaultError::ErasurePrepared)
            }
            Err(_) => Err(VaultError::Unavailable),
        }
    }
    fn claim(&self, key: MaterialKeyId, authority: Authority) -> Result<Claim, VaultError> {
        if self.read_legacy(key)?.is_some() {
            return Err(if authority.output_key() {
                VaultError::BindingMismatch
            } else {
                VaultError::Unavailable
            });
        }
        let existing: Option<Claim> = self.read(key, "claim")?;
        if existing.is_none() {
            fs::create_dir_all(self.dir(key)).map_err(|_| VaultError::Unavailable)?;
            match fs::create_dir(self.active_dir(key)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(VaultError::Unavailable),
            }
        }
        let candidate = Claim {
            version: VERSION,
            receipt: VaultReceipt::new().as_uuid(),
            authority: authority.clone(),
        };
        let _ = self.publish(key, "claim", &candidate)?;
        let installed: Claim = self.read(key, "claim")?.ok_or(VaultError::Unavailable)?;
        if installed.version != VERSION || !installed.authority.same(&authority) {
            return Err(if installed.authority.output_key() {
                VaultError::BindingMismatch
            } else {
                VaultError::NonceMismatch
            });
        }
        if self.active_dir(key).is_dir() == self.retired_active_dir(key).is_dir() {
            return Err(VaultError::Unavailable);
        }
        self.test_checkpoint("claim");
        Ok(installed)
    }
    fn retired(&self, key: MaterialKeyId, claim: &Claim) -> Result<(), VaultError> {
        if let Some(erased) = self.read::<WitnessStage>(key, "erased")? {
            if erased.version != VERSION || erased.claim_receipt != claim.receipt {
                return Err(VaultError::Unavailable);
            }
            return Err(VaultError::Erased);
        }
        if let Some(fence) = self.read::<WitnessStage>(key, "fence")? {
            if fence.version != VERSION || fence.claim_receipt != claim.receipt {
                return Err(VaultError::Unavailable);
            }
            return Err(VaultError::ErasurePrepared);
        }
        Ok(())
    }
    fn bootstrap_key(&self) -> Result<[u8; KEY_LENGTH], VaultError> {
        let mut bytes = self
            .bootstrap_provider
            .resolve(&self.bootstrap_reference, &self.bootstrap_request)
            .map_err(|_| VaultError::Unavailable)?
            .into_bytes();
        let hash = digest(&SHA256, &bytes);
        let mut key = [0; KEY_LENGTH];
        key.copy_from_slice(hash.as_ref());
        bytes.zeroize();
        Ok(key)
    }
    fn aad(key: MaterialKeyId, nonce: IntentNonce) -> Vec<u8> {
        format!("{AAD_DOMAIN}|{}|{}", key.as_uuid(), nonce.as_uuid()).into_bytes()
    }
    fn envelope(&self, key: MaterialKeyId, nonce: IntentNonce) -> Result<Envelope, VaultError> {
        let mut bootstrap = self.bootstrap_key()?;
        let mut dek = self
            .cipher
            .generate_data_key()
            .map_err(|_| VaultError::Unavailable)?;
        let sealed = self
            .cipher
            .seal(&bootstrap, &Self::aad(key, nonce), &dek)
            .map_err(|_| VaultError::Unavailable);
        bootstrap.zeroize();
        dek.zeroize();
        let sealed = sealed?;
        Ok(Envelope {
            nonce: sealed.nonce,
            ciphertext: sealed.ciphertext,
        })
    }
    fn create_new(
        &self,
        key: MaterialKeyId,
        nonce: IntentNonce,
        authority: Authority,
    ) -> Result<VaultReceipt, VaultError> {
        let claim = self.claim(key, authority)?;
        if claim.authority.output_key() {
            match self.output_disposition(key, &claim)? {
                Some(OutputDisposition::Bound { .. }) => {
                    self.output_envelope(key, &claim)?;
                    return Ok(VaultReceipt::from_uuid(claim.receipt));
                }
                Some(OutputDisposition::Retire { .. }) => {
                    self.retired(key, &claim)?;
                    return Err(VaultError::ErasurePrepared);
                }
                None => {}
            }
        }
        self.retired(key, &claim)?;
        self.test_checkpoint("before_envelope");
        let candidate = EnvelopeStage {
            version: VERSION,
            claim_receipt: claim.receipt,
            envelope: self.envelope(key, nonce)?,
        };
        let _ = self.publish_active(key, "envelope", &candidate)?;
        let installed: EnvelopeStage = match fs::read(self.active_stage(key, "envelope")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| VaultError::Unavailable)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.retired(key, &claim)?;
                return Err(VaultError::Unavailable);
            }
            Err(_) => return Err(VaultError::Unavailable),
        };
        if installed.version != VERSION || installed.claim_receipt != claim.receipt {
            return Err(VaultError::Unavailable);
        }
        self.test_checkpoint("envelope");
        if let Err(error) = self.retired(key, &claim) {
            let _ = fs::remove_file(self.active_stage(key, "envelope"));
            let _ = fs::remove_file(self.retired_active_dir(key).join("envelope"));
            return Err(error);
        }
        Ok(VaultReceipt::from_uuid(claim.receipt))
    }
    fn open(
        &self,
        key: MaterialKeyId,
        nonce: IntentNonce,
        envelope: Envelope,
        callback: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        let mut bootstrap = self.bootstrap_key()?;
        let opened = self.cipher.open(
            &bootstrap,
            &Self::aad(key, nonce),
            &Sealed {
                nonce: envelope.nonce,
                ciphertext: envelope.ciphertext,
            },
        );
        bootstrap.zeroize();
        let mut opened = opened.map_err(|_| VaultError::Unavailable)?;
        let bytes: Result<[u8; KEY_LENGTH], _> = opened.as_slice().try_into();
        opened.zeroize();
        let mut bytes = bytes.map_err(|_| VaultError::Unavailable)?;
        let dek = ZeroizingDek::new(bytes);
        bytes.zeroize();
        callback(&dek);
        Ok(())
    }
    fn output_disposition(
        &self,
        key: MaterialKeyId,
        claim: &Claim,
    ) -> Result<Option<OutputDisposition>, VaultError> {
        if claim.version != VERSION || claim.receipt.is_nil() || !claim.authority.output_key() {
            return Err(VaultError::Unavailable);
        }
        let disposition = self.read::<OutputDisposition>(key, "output-disposition")?;
        let fence = self.read::<WitnessStage>(key, "fence")?;
        let erased = self.read::<WitnessStage>(key, "erased")?;
        for witness in [fence.as_ref(), erased.as_ref()].into_iter().flatten() {
            if witness.version != VERSION
                || witness.claim_receipt != claim.receipt
                || witness.receipt.is_nil()
            {
                return Err(VaultError::Unavailable);
            }
        }
        if erased.is_some() && fence.is_none() {
            return Err(VaultError::Unavailable);
        }
        if self.active_dir(key).is_dir() == self.retired_active_dir(key).is_dir()
            || (erased.is_some() && self.active_dir(key).exists())
            || (self.retired_active_dir(key).exists() && fence.is_none())
        {
            return Err(VaultError::Unavailable);
        }
        if let Some(ref value) = disposition {
            let (version, receipt, authority) = match value {
                OutputDisposition::Bound {
                    version,
                    claim_receipt,
                    authority,
                    preparation_id,
                    binding_receipt,
                } => {
                    if preparation_id.is_nil()
                        || binding_receipt.is_nil()
                        || fence.is_some()
                        || erased.is_some()
                        || !self.active_dir(key).is_dir()
                    {
                        return Err(VaultError::Unavailable);
                    }
                    (*version, *claim_receipt, authority)
                }
                OutputDisposition::Retire {
                    version,
                    claim_receipt,
                    authority,
                    fence_receipt,
                } => {
                    if fence_receipt.is_nil()
                        || fence.as_ref().is_some_and(|f| f.receipt != *fence_receipt)
                    {
                        return Err(VaultError::Unavailable);
                    }
                    (*version, *claim_receipt, authority)
                }
            };
            if version != VERSION || receipt != claim.receipt || !authority.same(&claim.authority) {
                return Err(VaultError::Unavailable);
            }
        }
        Ok(disposition)
    }

    fn output_claim(&self, binding: &EmbeddingOutputKeyBinding) -> Result<Claim, VaultError> {
        if self.read_legacy(binding.key_id)?.is_some() {
            return Err(VaultError::BindingMismatch);
        }
        let claim: Claim = self
            .read(binding.key_id, "claim")?
            .ok_or(VaultError::NotFound)?;
        if claim.version != VERSION
            || claim.receipt.is_nil()
            || !claim.authority.same(&Authority::output(binding))
        {
            return Err(VaultError::BindingMismatch);
        }
        Ok(claim)
    }

    fn output_envelope(
        &self,
        key: MaterialKeyId,
        claim: &Claim,
    ) -> Result<EnvelopeStage, VaultError> {
        let envelope: EnvelopeStage = serde_json::from_slice(
            &fs::read(self.active_stage(key, "envelope")).map_err(|_| VaultError::Unavailable)?,
        )
        .map_err(|_| VaultError::Unavailable)?;
        if envelope.version != VERSION || envelope.claim_receipt != claim.receipt {
            return Err(VaultError::Unavailable);
        }
        Ok(envelope)
    }

    fn output_retirement_decision(
        &self,
        key: MaterialKeyId,
        claim: &Claim,
    ) -> Result<uuid::Uuid, VaultError> {
        if let Some(OutputDisposition::Bound { .. }) = self.output_disposition(key, claim)? {
            return Err(VaultError::BindingMismatch);
        }
        // A completed old retirement keeps its fence. It never becomes Bound.
        let fence_receipt = self
            .read::<WitnessStage>(key, "fence")?
            .map(|f| f.receipt)
            .unwrap_or_else(|| FenceReceipt::new().as_uuid());
        let candidate = OutputDisposition::Retire {
            version: VERSION,
            claim_receipt: claim.receipt,
            authority: claim.authority.clone(),
            fence_receipt,
        };
        self.test_checkpoint("before_output_retire_decision");
        self.publish(key, "output-disposition", &candidate)?;
        match self
            .output_disposition(key, claim)?
            .ok_or(VaultError::Unavailable)?
        {
            OutputDisposition::Bound { .. } => Err(VaultError::BindingMismatch),
            OutputDisposition::Retire { fence_receipt, .. } => {
                self.test_checkpoint("output_retire_decision");
                Ok(fence_receipt)
            }
        }
    }

    fn retire_new(
        &self,
        key: MaterialKeyId,
        authority: Authority,
    ) -> Result<ErasureReceipt, VaultError> {
        let claim = self.claim(key, authority)?;
        let output_fence = if claim.authority.output_key() {
            Some(self.output_retirement_decision(key, &claim)?)
        } else {
            None
        };
        if let Some(erased) = self.read::<WitnessStage>(key, "erased")? {
            let _ = fs::remove_file(self.retired_active_dir(key).join("envelope"));
            if erased.version == VERSION && erased.claim_receipt == claim.receipt {
                return Ok(ErasureReceipt::from_uuid(erased.receipt));
            }
            return Err(VaultError::Unavailable);
        }
        let fence = WitnessStage {
            version: VERSION,
            claim_receipt: claim.receipt,
            receipt: output_fence.unwrap_or_else(|| FenceReceipt::new().as_uuid()),
        };
        let _ = self.publish(key, "fence", &fence)?;
        let installed_fence: WitnessStage =
            self.read(key, "fence")?.ok_or(VaultError::Unavailable)?;
        if installed_fence.version != VERSION || installed_fence.claim_receipt != claim.receipt {
            return Err(VaultError::Unavailable);
        }
        self.test_checkpoint("fence");
        let active = self.active_dir(key);
        let retired_active = self.retired_active_dir(key);
        match fs::rename(&active, &retired_active) {
            Ok(()) => {}
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && retired_active.is_dir() => {}
            Err(_) => return Err(VaultError::Unavailable),
        }
        if let Err(error) = fs::remove_file(retired_active.join("envelope")) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(VaultError::Unavailable);
            }
        }
        let erased = WitnessStage {
            version: VERSION,
            claim_receipt: claim.receipt,
            receipt: ErasureReceipt::new().as_uuid(),
        };
        let _ = self.publish(key, "erased", &erased)?;
        if let Err(error) = fs::remove_file(retired_active.join("envelope")) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(VaultError::Unavailable);
            }
        }
        let installed: WitnessStage = self.read(key, "erased")?.ok_or(VaultError::Unavailable)?;
        if installed.version != VERSION || installed.claim_receipt != claim.receipt {
            return Err(VaultError::Unavailable);
        }
        self.test_checkpoint("erased");
        Ok(ErasureReceipt::from_uuid(installed.receipt))
    }
}

impl MaterialKeyVault for HostMaterialKeyVault {
    fn bind_embedding_output(
        &self,
        binding: &EmbeddingOutputKeyBinding,
        preparation: EmbeddingResultPreparationId,
    ) -> Result<MaterialKeyBindingReceipt, VaultError> {
        if preparation.as_uuid().is_nil() {
            return Err(VaultError::BindingMismatch);
        }
        let key = binding.key_id;
        let claim = self.output_claim(binding)?;
        self.output_disposition(key, &claim)?;
        self.retired(key, &claim)?;
        // No decision may bless a missing or foreign envelope.
        self.output_envelope(key, &claim)?;
        let candidate = OutputDisposition::Bound {
            version: VERSION,
            claim_receipt: claim.receipt,
            authority: claim.authority.clone(),
            preparation_id: preparation.as_uuid(),
            binding_receipt: MaterialKeyBindingReceipt::new().as_uuid(),
        };
        self.test_checkpoint("before_output_bind_decision");
        self.publish(key, "output-disposition", &candidate)?;
        match self
            .output_disposition(key, &claim)?
            .ok_or(VaultError::Unavailable)?
        {
            OutputDisposition::Retire { .. } => Err(VaultError::ErasurePrepared),
            OutputDisposition::Bound {
                preparation_id,
                binding_receipt,
                ..
            } => {
                if preparation_id != preparation.as_uuid() {
                    return Err(VaultError::BindingMismatch);
                }
                self.test_checkpoint("output_bind_decision");
                Ok(MaterialKeyBindingReceipt::from_uuid(binding_receipt))
            }
        }
    }

    fn with_bound_embedding_output_key(
        &self,
        binding: &EmbeddingOutputKeyBinding,
        preparation: EmbeddingResultPreparationId,
        receipt: MaterialKeyBindingReceipt,
        callback: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        let claim = self.output_claim(binding)?;
        match self
            .output_disposition(binding.key_id, &claim)?
            .ok_or(VaultError::Provisional)?
        {
            OutputDisposition::Bound {
                preparation_id,
                binding_receipt,
                ..
            } if preparation_id == preparation.as_uuid()
                && binding_receipt == receipt.as_uuid() => {}
            _ => return Err(VaultError::BindingMismatch),
        }
        let envelope = self.output_envelope(binding.key_id, &claim)?;
        self.open(binding.key_id, binding.nonce, envelope.envelope, callback)
    }

    fn create_if_absent(
        &self,
        key: MaterialKeyId,
        nonce: IntentNonce,
    ) -> Result<VaultReceipt, VaultError> {
        if let Some(legacy) = self.read_legacy(key)? {
            if self.dir(key).exists() {
                return Err(VaultError::Unavailable);
            }
            return if legacy.nonce == nonce.as_uuid() {
                Ok(VaultReceipt::from_uuid(legacy.receipt))
            } else {
                Err(VaultError::NonceMismatch)
            };
        }
        self.create_new(key, nonce, Authority::ordinary(nonce))
    }
    fn create_embedding_output_if_absent(
        &self,
        binding: &EmbeddingOutputKeyBinding,
    ) -> Result<VaultReceipt, VaultError> {
        self.create_new(binding.key_id, binding.nonce, Authority::output(binding))
    }
    fn retire_embedding_output(
        &self,
        binding: &EmbeddingOutputKeyBinding,
    ) -> Result<ErasureReceipt, VaultError> {
        self.retire_new(binding.key_id, Authority::output(binding))
    }
    fn with_embedding_output_key(
        &self,
        binding: &EmbeddingOutputKeyBinding,
        callback: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        let claim = self.output_claim(binding)?;
        match self.output_disposition(binding.key_id, &claim)? {
            Some(OutputDisposition::Bound { .. }) => return Err(VaultError::BindingMismatch),
            Some(OutputDisposition::Retire { .. }) => return Err(VaultError::ErasurePrepared),
            None => {}
        }
        if self
            .read::<WitnessStage>(binding.key_id, "erased")?
            .is_some()
        {
            return Err(VaultError::Erased);
        }
        if self
            .read::<WitnessStage>(binding.key_id, "fence")?
            .is_some()
        {
            return Err(VaultError::ErasurePrepared);
        }
        let envelope: EnvelopeStage = serde_json::from_slice(
            &fs::read(self.active_stage(binding.key_id, "envelope"))
                .map_err(|_| VaultError::Unavailable)?,
        )
        .map_err(|_| VaultError::Unavailable)?;
        if envelope.version != VERSION || envelope.claim_receipt != claim.receipt {
            return Err(VaultError::Unavailable);
        }
        self.open(binding.key_id, binding.nonce, envelope.envelope, callback)
    }
    fn unwrap(
        &self,
        key: MaterialKeyId,
        callback: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        if let Some(legacy) = self.read_legacy(key)? {
            if self.dir(key).exists() {
                return Err(VaultError::Unavailable);
            }
            if legacy.erasure_receipt.is_some() {
                return Err(VaultError::Erased);
            }
            if legacy.fence_receipt.is_some() {
                return Err(VaultError::ErasurePrepared);
            }
            return self.open(
                key,
                IntentNonce::from_uuid(legacy.nonce),
                legacy.envelope.ok_or(VaultError::Erased)?,
                callback,
            );
        }
        let claim: Claim = self.read(key, "claim")?.ok_or(VaultError::NotFound)?;
        if claim.version != VERSION {
            return Err(VaultError::Unavailable);
        }
        if claim.authority.output_key() {
            return Err(VaultError::Provisional);
        }
        if let Some(erased) = self.read::<WitnessStage>(key, "erased")? {
            if erased.version != VERSION
                || erased.claim_receipt != claim.receipt
                || self.read::<EnvelopeStage>(key, "envelope")?.is_some()
            {
                return Err(VaultError::Unavailable);
            }
            return Err(VaultError::Erased);
        }
        if let Some(fence) = self.read::<WitnessStage>(key, "fence")? {
            if fence.version != VERSION || fence.claim_receipt != claim.receipt {
                return Err(VaultError::Unavailable);
            }
            return Err(VaultError::ErasurePrepared);
        }
        let envelope: EnvelopeStage = serde_json::from_slice(
            &fs::read(self.active_stage(key, "envelope")).map_err(|_| VaultError::Unavailable)?,
        )
        .map_err(|_| VaultError::Unavailable)?;
        if envelope.version != VERSION || envelope.claim_receipt != claim.receipt {
            return Err(VaultError::Unavailable);
        }
        let Authority::Ordinary { nonce } = claim.authority else {
            return Err(VaultError::Provisional);
        };
        self.open(
            key,
            IntentNonce::from_uuid(nonce),
            envelope.envelope,
            callback,
        )
    }
    fn prepare_erasure(&self, key: MaterialKeyId) -> Result<FenceReceipt, VaultError> {
        if let Some(mut legacy) = self.read_legacy(key)? {
            if self.dir(key).exists() {
                return Err(VaultError::Unavailable);
            }
            if let Some(receipt) = legacy.fence_receipt {
                return Ok(FenceReceipt::from_uuid(receipt));
            }
            let receipt = FenceReceipt::new();
            legacy.fence_receipt = Some(receipt.as_uuid());
            self.write_legacy(key, &legacy)?;
            return Ok(receipt);
        }
        let claim: Claim = self.read(key, "claim")?.ok_or(VaultError::NotFound)?;
        if claim.authority.output_key() {
            return Err(VaultError::Provisional);
        }
        let candidate = WitnessStage {
            version: VERSION,
            claim_receipt: claim.receipt,
            receipt: FenceReceipt::new().as_uuid(),
        };
        let _ = self.publish(key, "fence", &candidate)?;
        let installed: WitnessStage = self.read(key, "fence")?.ok_or(VaultError::Unavailable)?;
        if installed.version != VERSION || installed.claim_receipt != claim.receipt {
            return Err(VaultError::Unavailable);
        }
        Ok(FenceReceipt::from_uuid(installed.receipt))
    }
    fn erase(&self, key: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        if let Some(mut legacy) = self.read_legacy(key)? {
            if self.dir(key).exists() {
                return Err(VaultError::Unavailable);
            }
            if let Some(receipt) = legacy.erasure_receipt {
                return Ok(ErasureReceipt::from_uuid(receipt));
            }
            if legacy.fence_receipt.is_none() {
                return Err(VaultError::ErasureNotPrepared);
            }
            legacy.envelope = None;
            let receipt = ErasureReceipt::new();
            legacy.erasure_receipt = Some(receipt.as_uuid());
            self.write_legacy(key, &legacy)?;
            return Ok(receipt);
        }
        let claim: Claim = self.read(key, "claim")?.ok_or(VaultError::NotFound)?;
        if claim.authority.output_key() {
            return Err(VaultError::Provisional);
        }
        if self.read::<WitnessStage>(key, "fence")?.is_none() {
            return Err(VaultError::ErasureNotPrepared);
        }
        self.retire_new(key, claim.authority)
    }
}
