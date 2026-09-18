use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[cfg(not(windows))]
use std::fs::File;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

use async_trait::async_trait;
use ring::signature::{Ed25519KeyPair, KeyPair};
use vestrace_application::InstallationSafetyWitness;
use vestrace_domain::{
    DatabaseGenerationId, SafetyBootstrapBinding, WitnessAdvance, WitnessError, WitnessHead,
    WitnessPublicKey, WitnessReceipt,
};

const WITNESS_RECORD_NAME: &str = "installation-safety-witness-v1.cbor";
const WITNESS_KEY_NAME: &str = "installation-safety-witness.pk8";
const WITNESS_RECORD_DOMAIN: &[u8] = b"vestrace-installation-safety-witness-v1";

/// Host-only durable witness. The signing key is loaded from a protected host
/// directory and is never exposed through this adapter's API.
pub struct FileInstallationSafetyWitness {
    root: PathBuf,
    binding: SafetyBootstrapBinding,
    signing_key: Ed25519KeyPair,
    head: Mutex<WitnessHead>,
}

impl FileInstallationSafetyWitness {
    /// Opens an existing durable record and refuses absent, malformed,
    /// truncated, signature-invalid, or identity-mismatched state.
    pub fn open(root: &Path) -> Result<Self, WitnessError> {
        let signing_key = load_signing_key(root)?;
        let record = fs::read(record_path(root)).map_err(|error| unavailable(error.to_string()))?;
        let stored = decode_record(&record)?;
        let expected_key = WitnessPublicKey::from_bytes(
            signing_key
                .public_key()
                .as_ref()
                .try_into()
                .map_err(|_| unavailable("witness public key has the wrong length"))?,
        );
        if stored.binding.witness_public_key() != &expected_key {
            return Err(unavailable(
                "witness signing key does not match pinned binding",
            ));
        }
        let head = match stored.receipt {
            Some(receipt) => WitnessHead::from_durable_receipt(&stored.binding, &receipt)
                .map_err(|error| unavailable(error.to_string()))?,
            None => WitnessHead::genesis(
                stored.binding.installation_id(),
                stored.binding.fingerprint_key_id(),
                stored.binding.continuity_proof().clone(),
                stored
                    .genesis_generation
                    .ok_or_else(|| unavailable("genesis witness is missing generation"))?,
                stored.binding.journal_public_key().clone(),
            ),
        };
        Ok(Self {
            root: root.to_path_buf(),
            binding: stored.binding,
            signing_key,
            head: Mutex::new(head),
        })
    }

    /// Creates the first, unsigned genesis record only when no record exists.
    /// The caller must already have created the host-only signing-key file.
    pub fn open_or_create(
        root: &Path,
        binding: SafetyBootstrapBinding,
        generation: DatabaseGenerationId,
    ) -> Result<Self, WitnessError> {
        match Self::open(root) {
            Ok(witness) => {
                if witness.binding != binding
                    || witness.head()?.active_generation_id() != generation
                {
                    return Err(unavailable(
                        "existing witness binding or genesis generation differs",
                    ));
                }
                Ok(witness)
            }
            Err(WitnessError::Unavailable(_)) if !record_path(root).exists() => {
                fs::create_dir_all(root).map_err(|error| unavailable(error.to_string()))?;
                let signing_key = load_signing_key(root)?;
                let expected_key = WitnessPublicKey::from_bytes(
                    signing_key
                        .public_key()
                        .as_ref()
                        .try_into()
                        .map_err(|_| unavailable("witness public key has the wrong length"))?,
                );
                if binding.witness_public_key() != &expected_key {
                    return Err(unavailable(
                        "witness signing key does not match bootstrap binding",
                    ));
                }
                let head = WitnessHead::genesis(
                    binding.installation_id(),
                    binding.fingerprint_key_id(),
                    binding.continuity_proof().clone(),
                    generation,
                    binding.journal_public_key().clone(),
                );
                let record = StoredWitnessRecord {
                    binding: binding.clone(),
                    genesis_generation: Some(generation),
                    receipt: None,
                };
                write_initial_record(root, &encode_record(&record))?;
                Ok(Self {
                    root: root.to_path_buf(),
                    binding,
                    signing_key,
                    head: Mutex::new(head),
                })
            }
            Err(error) => Err(error),
        }
    }

    fn head(&self) -> Result<std::sync::MutexGuard<'_, WitnessHead>, WitnessError> {
        self.head
            .lock()
            .map_err(|_| unavailable("witness head lock is poisoned"))
    }

    pub fn binding(&self) -> &SafetyBootstrapBinding {
        &self.binding
    }

    /// Returns only the existing signed receipt used for an exact database
    /// reconciliation. It never issues a new receipt or advances the head.
    pub fn durable_receipt(&self) -> Result<WitnessReceipt, WitnessError> {
        let stored = decode_record(
            &fs::read(record_path(&self.root)).map_err(|error| unavailable(error.to_string()))?,
        )?;
        if stored.binding != self.binding {
            return Err(unavailable("durable witness binding changed on disk"));
        }
        stored
            .receipt
            .ok_or_else(|| unavailable("witness has no durable receipt to reconcile"))
    }

    fn replace_record(&self, receipt: &WitnessReceipt) -> Result<(), WitnessError> {
        let record = StoredWitnessRecord {
            binding: self.binding.clone(),
            genesis_generation: None,
            receipt: Some(receipt.clone()),
        };
        let temporary = self.root.join(format!(
            ".{WITNESS_RECORD_NAME}.{}.tmp",
            uuid::Uuid::now_v7()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| unavailable(error.to_string()))?;
        file.write_all(&encode_record(&record))
            .and_then(|()| file.sync_all())
            .map_err(|error| unavailable(error.to_string()))?;
        fs::rename(&temporary, record_path(&self.root))
            .map_err(|error| unavailable(error.to_string()))?;
        sync_directory(&self.root)
    }
}

#[async_trait]
impl InstallationSafetyWitness for FileInstallationSafetyWitness {
    async fn read_head(&self) -> Result<WitnessHead, WitnessError> {
        Ok(self.head()?.clone())
    }

    async fn compare_and_advance(
        &self,
        expected: WitnessHead,
        next: WitnessAdvance,
    ) -> Result<WitnessReceipt, WitnessError> {
        let mut current = self.head()?;
        if *current != expected {
            return Err(WitnessError::Conflict);
        }
        let receipt = WitnessReceipt::sign(&self.signing_key, &current, next)?;
        let replacement = WitnessHead::from_durable_receipt(&self.binding, &receipt)?;
        // The replacement is durable, including its parent directory, before
        // the signed receipt can escape to the guarded database call.
        self.replace_record(&receipt)?;
        *current = replacement;
        Ok(receipt)
    }
}

#[derive(Clone)]
struct StoredWitnessRecord {
    binding: SafetyBootstrapBinding,
    genesis_generation: Option<DatabaseGenerationId>,
    receipt: Option<WitnessReceipt>,
}

fn load_signing_key(root: &Path) -> Result<Ed25519KeyPair, WitnessError> {
    let bytes =
        fs::read(root.join(WITNESS_KEY_NAME)).map_err(|error| unavailable(error.to_string()))?;
    Ed25519KeyPair::from_pkcs8(&bytes)
        .map_err(|_| unavailable("witness signing key is not valid PKCS#8"))
}

fn record_path(root: &Path) -> PathBuf {
    root.join(WITNESS_RECORD_NAME)
}

fn write_initial_record(root: &Path, bytes: &[u8]) -> Result<(), WitnessError> {
    let path = record_path(root);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| unavailable(error.to_string()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| unavailable(error.to_string()))?;
    sync_directory(root)
}

fn encode_record(record: &StoredWitnessRecord) -> Vec<u8> {
    let binding = record.binding.canonical_record_bytes();
    let (generation, receipt, signature) = match &record.receipt {
        Some(receipt) => (
            Vec::new(),
            receipt.canonical_bytes(),
            receipt.signature().to_vec(),
        ),
        None => (
            record
                .genesis_generation
                .expect("genesis witness record has a generation")
                .as_uuid()
                .as_bytes()
                .to_vec(),
            Vec::new(),
            Vec::new(),
        ),
    };
    let mut encoded = Vec::new();
    encoded.push(0x85); // canonical CBOR array(5)
    cbor_bytes(&mut encoded, WITNESS_RECORD_DOMAIN);
    cbor_bytes(&mut encoded, &binding);
    cbor_bytes(&mut encoded, &generation);
    cbor_bytes(&mut encoded, &receipt);
    cbor_bytes(&mut encoded, &signature);
    encoded
}

fn decode_record(bytes: &[u8]) -> Result<StoredWitnessRecord, WitnessError> {
    let mut decoder = CborDecoder::new(bytes);
    if decoder.byte()? != 0x85 || decoder.bytes()? != WITNESS_RECORD_DOMAIN {
        return Err(unavailable("witness record has the wrong domain"));
    }
    let binding = SafetyBootstrapBinding::from_canonical_bytes(decoder.bytes()?)
        .map_err(|error| unavailable(error.to_string()))?;
    let generation = decoder.bytes()?;
    let receipt_payload = decoder.bytes()?;
    let signature = decoder.bytes()?;
    decoder.finish()?;
    match (generation, receipt_payload, signature) {
        ([a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p], [], []) => Ok(StoredWitnessRecord {
            binding,
            genesis_generation: Some(DatabaseGenerationId::from_uuid(uuid::Uuid::from_bytes([
                *a, *b, *c, *d, *e, *f, *g, *h, *i, *j, *k, *l, *m, *n, *o, *p,
            ]))),
            receipt: None,
        }),
        ([], payload, signature) => Ok(StoredWitnessRecord {
            binding,
            genesis_generation: None,
            receipt: Some(
                WitnessReceipt::from_canonical_bytes_and_signature(
                    payload,
                    signature.try_into().map_err(|_| {
                        unavailable("witness receipt signature has the wrong length")
                    })?,
                )
                .map_err(|error| unavailable(error.to_string()))?,
            ),
        }),
        _ => Err(unavailable(
            "witness record mixes genesis and receipt state",
        )),
    }
}

fn cbor_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    match bytes.len() {
        0..=23 => out.push(0x40 | bytes.len() as u8),
        24..=255 => out.extend_from_slice(&[0x58, bytes.len() as u8]),
        256..=65_535 => {
            out.push(0x59);
            out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        }
        _ => {
            out.push(0x5a);
            out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        }
    }
    out.extend_from_slice(bytes);
}

struct CborDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> CborDecoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn byte(&mut self) -> Result<u8, WitnessError> {
        let value = *self
            .bytes
            .get(self.offset)
            .ok_or_else(|| unavailable("witness record is truncated"))?;
        self.offset += 1;
        Ok(value)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], WitnessError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| unavailable("witness record length overflows"))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| unavailable("witness record is truncated"))?;
        self.offset = end;
        Ok(slice)
    }

    fn bytes(&mut self) -> Result<&'a [u8], WitnessError> {
        let initial = self.byte()?;
        let length = match initial {
            0x40..=0x57 => usize::from(initial & 0x1f),
            0x58 => usize::from(self.byte()?),
            0x59 => usize::from(u16::from_be_bytes(
                self.take(2)?
                    .try_into()
                    .map_err(|_| unavailable("witness CBOR length is malformed"))?,
            )),
            0x5a => usize::try_from(u32::from_be_bytes(
                self.take(4)?
                    .try_into()
                    .map_err(|_| unavailable("witness CBOR length is malformed"))?,
            ))
            .map_err(|_| unavailable("witness CBOR length overflows"))?,
            _ => {
                return Err(unavailable(
                    "witness record has a non-canonical byte string",
                ));
            }
        };
        self.take(length)
    }

    fn finish(self) -> Result<(), WitnessError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(unavailable("witness record has trailing bytes"))
        }
    }
}

fn sync_directory(path: &Path) -> Result<(), WitnessError> {
    #[cfg(not(windows))]
    {
        File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| unavailable(error.to_string()))
    }
    #[cfg(windows)]
    {
        OpenOptions::new()
            .read(true)
            .custom_flags(0x0200_0000)
            .open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| unavailable(error.to_string()))
    }
}

fn unavailable(message: impl Into<String>) -> WitnessError {
    WitnessError::Unavailable(message.into())
}
