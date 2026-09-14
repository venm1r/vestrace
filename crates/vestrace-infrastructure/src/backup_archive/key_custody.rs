use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use ring::{
    aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use sha2::{Digest as _, Sha256};
use vestrace_application::{
    ApplicationError, ArchiveKeyCustody, ArchiveKeyEnvelopeRef, ArchiveKeyErasureReceipt,
    ManagedBackupDeletionPrepared,
};
use vestrace_domain::{ArchiveObjectDescriptor, BackupSetId};
use zeroize::{Zeroize as _, Zeroizing};

use super::sync_parent;

const ENVELOPE_MAGIC: &[u8; 4] = b"VBAK";
const OBJECT_MAGIC: &[u8; 4] = b"VBAF";
const FRAME_VERSION: u8 = 1;
const STREAM_FRAME_VERSION: u8 = 2;
const STREAM_CHUNK_BYTES: usize = 64 * 1024;
const ARCHIVE_AAD_DOMAIN: &[u8] = b"vestrace-managed-backup-object-v1";
const AES_GCM_TAG_LENGTH: u64 = 16;

/// Host-only encrypted archive-key envelopes. The wrapping key remains inside
/// this root and is never represented by an application port value.
pub struct FileArchiveKeyCustody {
    root: PathBuf,
    wrapping_key: Zeroizing<[u8; 32]>,
    random: SystemRandom,
}

impl FileArchiveKeyCustody {
    pub fn open(root: PathBuf) -> Result<Self, ApplicationError> {
        fs::create_dir_all(root.join("envelopes"))
            .map_err(unavailable("create archive key root"))?;
        let wrapping_key_path = root.join("archive-envelope-wrapping-key-v1");
        let wrapping_key = read_or_create_key(&wrapping_key_path)?;
        sync_parent(&wrapping_key_path).map_err(unavailable("sync archive key root"))?;
        Ok(Self {
            root,
            wrapping_key: Zeroizing::new(wrapping_key),
            random: SystemRandom::new(),
        })
    }

    /// Encrypts one opaque archive payload with the per-set key. Its AAD binds
    /// the set/object identity and immutable PostgreSQL checkpoint metadata.
    pub fn seal_object(
        &self,
        set: BackupSetId,
        descriptor: &ArchiveObjectDescriptor,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, ApplicationError> {
        if descriptor.set_id() != set || plaintext.is_empty() {
            return Err(ApplicationError::Policy(
                "archive encryption requires the exact set descriptor and nonempty payload"
                    .to_owned(),
            ));
        }
        let key = self.open_set_key(set)?;
        let mut nonce = [0_u8; NONCE_LEN];
        self.random.fill(&mut nonce).map_err(|_| {
            ApplicationError::Unavailable("archive randomness is unavailable".to_owned())
        })?;
        let mut sealed = Zeroizing::new(plaintext.to_vec());
        let aad = object_aad(descriptor);
        seal(&key, nonce, &aad, &mut sealed)?;
        let mut frame = Vec::with_capacity(OBJECT_MAGIC.len() + 1 + NONCE_LEN + sealed.len());
        frame.extend_from_slice(OBJECT_MAGIC);
        frame.push(FRAME_VERSION);
        frame.extend_from_slice(&nonce);
        frame.extend_from_slice(&sealed);
        sealed.zeroize();
        Ok(frame)
    }

    /// The encrypted frame length is fixed by AES-GCM framing, allowing the
    /// immutable descriptor to bind its final size before host encryption.
    pub fn sealed_length_for_plaintext(plaintext_length: u64) -> Result<u64, ApplicationError> {
        let framing = u64::try_from(OBJECT_MAGIC.len() + 1 + NONCE_LEN)
            .expect("archive frame overhead fits into u64");
        plaintext_length
            .checked_add(framing)
            .and_then(|length| length.checked_add(AES_GCM_TAG_LENGTH))
            .ok_or_else(|| {
                ApplicationError::Policy("archive object length overflows u64".to_owned())
            })
    }

    /// Returns the final encrypted-frame length for chunked streaming AES-GCM
    /// without opening the plaintext file. Each chunk has its own nonce,
    /// authenticated length, and tag.
    pub fn sealed_stream_length_for_plaintext(
        plaintext_length: u64,
    ) -> Result<u64, ApplicationError> {
        if plaintext_length == 0 {
            return Err(ApplicationError::Policy(
                "archive streaming encryption requires a nonempty payload".to_owned(),
            ));
        }
        let chunk = STREAM_CHUNK_BYTES as u64;
        let chunks = plaintext_length
            .checked_add(chunk - 1)
            .and_then(|length| length.checked_div(chunk))
            .ok_or_else(|| {
                ApplicationError::Policy("archive object length overflows u64".to_owned())
            })?;
        let overhead = u64::try_from(NONCE_LEN + 4)
            .expect("archive streaming chunk overhead fits into u64")
            .checked_add(AES_GCM_TAG_LENGTH)
            .ok_or_else(|| {
                ApplicationError::Policy("archive object length overflows u64".to_owned())
            })?;
        let header =
            u64::try_from(OBJECT_MAGIC.len() + 1).expect("archive frame header fits into u64");
        plaintext_length
            .checked_add(chunks.checked_mul(overhead).ok_or_else(|| {
                ApplicationError::Policy("archive object length overflows u64".to_owned())
            })?)
            .and_then(|length| length.checked_add(header))
            .ok_or_else(|| {
                ApplicationError::Policy("archive object length overflows u64".to_owned())
            })
    }

    /// Hashes a local segment incrementally, returning its immutable digest
    /// and observed length without retaining plaintext in process memory.
    pub fn digest_file(
        path: &Path,
    ) -> Result<(vestrace_domain::SafetyJournalDigest, u64), ApplicationError> {
        let mut input = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(unavailable("open archive plaintext segment"))?;
        let mut digest = Sha256::new();
        let mut length = 0_u64;
        let mut buffer = [0_u8; STREAM_CHUNK_BYTES];
        loop {
            let read = input
                .read(&mut buffer)
                .map_err(unavailable("read archive plaintext segment"))?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
            length = length.checked_add(read as u64).ok_or_else(|| {
                ApplicationError::Policy("archive plaintext length overflows u64".to_owned())
            })?;
        }
        Ok((
            vestrace_domain::SafetyJournalDigest::from_bytes(digest.finalize().into()),
            length,
        ))
    }

    /// Encrypts a local segment as independently authenticated chunks into a
    /// create-only host spool. The returned digest covers exactly the spool
    /// bytes named by the final immutable descriptor.
    pub fn seal_file_to(
        &self,
        set: BackupSetId,
        descriptor: &ArchiveObjectDescriptor,
        plaintext_path: &Path,
        ciphertext_path: &Path,
    ) -> Result<vestrace_domain::SafetyJournalDigest, ApplicationError> {
        if descriptor.set_id() != set {
            return Err(ApplicationError::Policy(
                "archive streaming encryption requires the exact set descriptor".to_owned(),
            ));
        }
        let input_descriptor = descriptor.to_input();
        let expected_length = Self::sealed_stream_length_for_plaintext(
            fs::metadata(plaintext_path)
                .map_err(unavailable("stat archive plaintext segment"))?
                .len(),
        )?;
        if input_descriptor.length != expected_length {
            return Err(ApplicationError::Policy(
                "archive streaming descriptor length is not exact".to_owned(),
            ));
        }
        let parent = ciphertext_path.parent().ok_or_else(|| {
            ApplicationError::Unavailable("archive ciphertext spool has no parent".to_owned())
        })?;
        fs::create_dir_all(parent).map_err(unavailable("create archive ciphertext spool root"))?;
        let result = (|| {
            let mut plaintext = OpenOptions::new()
                .read(true)
                .open(plaintext_path)
                .map_err(unavailable("open archive plaintext segment"))?;
            let mut ciphertext = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(ciphertext_path)
                .map_err(unavailable("create archive ciphertext spool"))?;
            let mut digest = Sha256::new();
            let mut plaintext_digest = Sha256::new();
            let mut ciphertext_length = 0_u64;
            let mut write_ciphertext = |bytes: &[u8]| -> Result<(), ApplicationError> {
                ciphertext
                    .write_all(bytes)
                    .map_err(unavailable("write archive ciphertext spool"))?;
                digest.update(bytes);
                ciphertext_length = ciphertext_length
                    .checked_add(bytes.len() as u64)
                    .ok_or_else(|| {
                        ApplicationError::Policy(
                            "archive ciphertext length overflows u64".to_owned(),
                        )
                    })?;
                Ok(())
            };
            write_ciphertext(OBJECT_MAGIC)?;
            write_ciphertext(&[STREAM_FRAME_VERSION])?;
            let key = self.open_set_key(set)?;
            let mut index = 0_u64;
            let mut buffer = [0_u8; STREAM_CHUNK_BYTES];
            loop {
                let read = plaintext
                    .read(&mut buffer)
                    .map_err(unavailable("read archive plaintext segment"))?;
                if read == 0 {
                    break;
                }
                plaintext_digest.update(&buffer[..read]);
                let mut nonce = [0_u8; NONCE_LEN];
                self.random.fill(&mut nonce).map_err(|_| {
                    ApplicationError::Unavailable("archive randomness is unavailable".to_owned())
                })?;
                let mut sealed = Zeroizing::new(buffer[..read].to_vec());
                let aad = stream_chunk_aad(descriptor, index, read as u32);
                seal(&key, nonce, &aad, &mut sealed)?;
                write_ciphertext(&nonce)?;
                write_ciphertext(&(read as u32).to_be_bytes())?;
                write_ciphertext(&sealed)?;
                sealed.zeroize();
                index = index.checked_add(1).ok_or_else(|| {
                    ApplicationError::Policy("archive chunk ordinal overflows u64".to_owned())
                })?;
            }
            if index == 0
                || vestrace_domain::SafetyJournalDigest::from_bytes(
                    plaintext_digest.finalize().into(),
                ) != input_descriptor.plaintext_digest
                || ciphertext_length != input_descriptor.length
            {
                return Err(ApplicationError::Policy(
                    "archive plaintext changed during streaming encryption".to_owned(),
                ));
            }
            ciphertext
                .sync_all()
                .map_err(unavailable("sync archive ciphertext spool"))?;
            sync_parent(ciphertext_path)
                .map_err(unavailable("sync archive ciphertext spool directory"))?;
            Ok(vestrace_domain::SafetyJournalDigest::from_bytes(
                digest.finalize().into(),
            ))
        })();
        if result.is_err() {
            let _ = fs::remove_file(ciphertext_path);
        }
        result
    }

    pub fn open_object(
        &self,
        set: BackupSetId,
        descriptor: &ArchiveObjectDescriptor,
        frame: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, ApplicationError> {
        if descriptor.set_id() != set
            || frame.get(..OBJECT_MAGIC.len()) != Some(OBJECT_MAGIC.as_slice())
        {
            return Err(ApplicationError::Storage(
                "archive encrypted frame is malformed or has mismatched metadata".to_owned(),
            ));
        }
        if frame.get(OBJECT_MAGIC.len()).copied() == Some(STREAM_FRAME_VERSION) {
            return self.open_stream_object(set, descriptor, frame);
        }
        if frame.get(OBJECT_MAGIC.len()).copied() != Some(FRAME_VERSION) {
            return Err(ApplicationError::Storage(
                "archive encrypted frame has an unsupported version".to_owned(),
            ));
        }
        let nonce_start = OBJECT_MAGIC.len() + 1;
        let nonce_end = nonce_start + NONCE_LEN;
        let nonce: [u8; NONCE_LEN] = frame
            .get(nonce_start..nonce_end)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| {
                ApplicationError::Storage("archive encrypted frame is truncated".to_owned())
            })?;
        let mut encrypted = Zeroizing::new(
            frame
                .get(nonce_end..)
                .ok_or_else(|| {
                    ApplicationError::Storage("archive encrypted frame is truncated".to_owned())
                })?
                .to_vec(),
        );
        let key = self.open_set_key(set)?;
        let aad = object_aad(descriptor);
        let opened = open(&key, nonce, &aad, &mut encrypted)?;
        Ok(Zeroizing::new(opened.to_vec()))
    }

    fn open_stream_object(
        &self,
        set: BackupSetId,
        descriptor: &ArchiveObjectDescriptor,
        frame: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, ApplicationError> {
        let mut offset = OBJECT_MAGIC.len() + 1;
        let mut index = 0_u64;
        let mut plaintext = Zeroizing::new(Vec::new());
        let key = self.open_set_key(set)?;
        while offset < frame.len() {
            let nonce_end = offset.checked_add(NONCE_LEN).ok_or_else(|| {
                ApplicationError::Storage("archive streaming frame is malformed".to_owned())
            })?;
            let nonce: [u8; NONCE_LEN] = frame
                .get(offset..nonce_end)
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(|| {
                    ApplicationError::Storage("archive streaming frame is truncated".to_owned())
                })?;
            offset = nonce_end;
            let length_end = offset.checked_add(4).ok_or_else(|| {
                ApplicationError::Storage("archive streaming frame is malformed".to_owned())
            })?;
            let length = u32::from_be_bytes(
                frame
                    .get(offset..length_end)
                    .ok_or_else(|| {
                        ApplicationError::Storage("archive streaming frame is truncated".to_owned())
                    })?
                    .try_into()
                    .map_err(|_| {
                        ApplicationError::Storage(
                            "archive streaming chunk length is malformed".to_owned(),
                        )
                    })?,
            );
            if length == 0 || length as usize > STREAM_CHUNK_BYTES {
                return Err(ApplicationError::Storage(
                    "archive streaming chunk length is invalid".to_owned(),
                ));
            }
            offset = length_end;
            let sealed_end = offset
                .checked_add(length as usize)
                .and_then(|value| value.checked_add(AES_GCM_TAG_LENGTH as usize))
                .ok_or_else(|| {
                    ApplicationError::Storage("archive streaming frame is malformed".to_owned())
                })?;
            let mut sealed = Zeroizing::new(
                frame
                    .get(offset..sealed_end)
                    .ok_or_else(|| {
                        ApplicationError::Storage("archive streaming frame is truncated".to_owned())
                    })?
                    .to_vec(),
            );
            let aad = stream_chunk_aad(descriptor, index, length);
            let opened = open(&key, nonce, &aad, &mut sealed)?;
            plaintext.extend_from_slice(opened);
            sealed.zeroize();
            offset = sealed_end;
            index = index.checked_add(1).ok_or_else(|| {
                ApplicationError::Storage("archive streaming chunk ordinal overflows".to_owned())
            })?;
        }
        if index == 0 || plaintext.is_empty() {
            return Err(ApplicationError::Storage(
                "archive streaming frame has no ciphertext chunks".to_owned(),
            ));
        }
        Ok(plaintext)
    }

    fn envelope_path(&self, set: BackupSetId) -> PathBuf {
        self.root
            .join("envelopes")
            .join(format!("{}.envelope", set.as_uuid()))
    }

    fn open_set_key(&self, set: BackupSetId) -> Result<Zeroizing<[u8; 32]>, ApplicationError> {
        let path = self.envelope_path(set);
        let mut bytes = Vec::new();
        OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(unavailable("open archive key envelope"))?
            .read_to_end(&mut bytes)
            .map_err(unavailable("read archive key envelope"))?;
        let nonce_start = ENVELOPE_MAGIC.len() + 1;
        let nonce_end = nonce_start + NONCE_LEN;
        if bytes.get(..ENVELOPE_MAGIC.len()) != Some(ENVELOPE_MAGIC.as_slice())
            || bytes.get(ENVELOPE_MAGIC.len()).copied() != Some(FRAME_VERSION)
        {
            return Err(ApplicationError::Storage(
                "archive key envelope is malformed".to_owned(),
            ));
        }
        let nonce: [u8; NONCE_LEN] = bytes
            .get(nonce_start..nonce_end)
            .and_then(|value| value.try_into().ok())
            .ok_or_else(|| {
                ApplicationError::Storage("archive key envelope is truncated".to_owned())
            })?;
        let mut encrypted = Zeroizing::new(
            bytes
                .get(nonce_end..)
                .ok_or_else(|| {
                    ApplicationError::Storage("archive key envelope is truncated".to_owned())
                })?
                .to_vec(),
        );
        let aad = envelope_aad(set);
        let opened = open(&self.wrapping_key, nonce, &aad, &mut encrypted)?;
        let key: [u8; 32] = opened.try_into().map_err(|_| {
            ApplicationError::Storage("archive key envelope length is invalid".to_owned())
        })?;
        Ok(Zeroizing::new(key))
    }

    /// Verifies that the exact pre-created envelope is readable before a
    /// journalled backup-start transition is reconciled into PostgreSQL.
    pub fn ensure_set_key(&self, set: BackupSetId) -> Result<(), ApplicationError> {
        self.open_set_key(set).map(|_| ())
    }
}

#[async_trait]
impl ArchiveKeyCustody for FileArchiveKeyCustody {
    async fn create_set_key(
        &self,
        set: BackupSetId,
    ) -> Result<ArchiveKeyEnvelopeRef, ApplicationError> {
        let path = self.envelope_path(set);
        if path.exists() {
            return Err(ApplicationError::Conflict(
                "archive key envelope already exists for backup set".to_owned(),
            ));
        }
        let parent = path.parent().ok_or_else(|| {
            ApplicationError::Unavailable("archive envelope path has no parent".to_owned())
        })?;
        fs::create_dir_all(parent).map_err(unavailable("create archive envelope directory"))?;
        let mut set_key = Zeroizing::new([0_u8; 32]);
        self.random.fill(&mut *set_key).map_err(|_| {
            ApplicationError::Unavailable("archive randomness is unavailable".to_owned())
        })?;
        let mut nonce = [0_u8; NONCE_LEN];
        self.random.fill(&mut nonce).map_err(|_| {
            ApplicationError::Unavailable("archive randomness is unavailable".to_owned())
        })?;
        let aad = envelope_aad(set);
        let mut encrypted = Zeroizing::new(set_key.to_vec());
        seal(&self.wrapping_key, nonce, &aad, &mut encrypted)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(unavailable("create archive key envelope"))?;
        file.write_all(ENVELOPE_MAGIC)
            .and_then(|()| file.write_all(&[FRAME_VERSION]))
            .and_then(|()| file.write_all(&nonce))
            .and_then(|()| file.write_all(&encrypted))
            .and_then(|()| file.sync_all())
            .map_err(unavailable("durably write archive key envelope"))?;
        sync_parent(&path).map_err(unavailable("sync archive envelope directory"))?;
        encrypted.zeroize();
        Ok(ArchiveKeyEnvelopeRef::from_uuid(set.as_uuid()))
    }

    async fn discard_uncommitted_set_key(&self, set: BackupSetId) -> Result<(), ApplicationError> {
        let path = self.envelope_path(set);
        match fs::remove_file(&path) {
            Ok(()) => sync_parent(&path).map_err(unavailable("sync discarded archive envelope")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(unavailable("discard uncommitted archive key envelope")(
                error,
            )),
        }
    }

    async fn erase_prepared_key(
        &self,
        prepared: ManagedBackupDeletionPrepared,
    ) -> Result<ArchiveKeyErasureReceipt, ApplicationError> {
        let path = self.envelope_path(prepared.set_id());
        match fs::remove_file(&path) {
            Ok(()) => sync_parent(&path).map_err(unavailable("sync archive key erasure"))?,
            // The signed erasure intent is durable before this call. A crash
            // after unlinking but before event 9 must resume with the same
            // preparation, not turn an already-erased envelope into a new
            // unsafe failure mode.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(unavailable("erase prepared archive key envelope")(error)),
        }
        Ok(ArchiveKeyErasureReceipt::new(prepared.preparation_digest()))
    }
}

fn read_or_create_key(path: &Path) -> Result<[u8; 32], ApplicationError> {
    match OpenOptions::new().read(true).open(path) {
        Ok(mut file) => {
            let mut key = [0_u8; 32];
            file.read_exact(&mut key)
                .map_err(unavailable("read archive wrapping key"))?;
            if file
                .read(&mut [0_u8; 1])
                .map_err(unavailable("validate archive wrapping key"))?
                != 0
            {
                return Err(ApplicationError::Storage(
                    "archive wrapping key length is invalid".to_owned(),
                ));
            }
            Ok(key)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let random = SystemRandom::new();
            let mut key = [0_u8; 32];
            random.fill(&mut key).map_err(|_| {
                ApplicationError::Unavailable("archive randomness is unavailable".to_owned())
            })?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(unavailable("create archive wrapping key"))?;
            file.write_all(&key)
                .and_then(|()| file.sync_all())
                .map_err(unavailable("durably write archive wrapping key"))?;
            Ok(key)
        }
        Err(error) => Err(unavailable("open archive wrapping key")(error)),
    }
}

fn seal(
    key: &[u8; 32],
    nonce: [u8; NONCE_LEN],
    aad: &[u8],
    bytes: &mut Vec<u8>,
) -> Result<(), ApplicationError> {
    let key = LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, key)
            .map_err(|_| ApplicationError::Storage("archive AES key is invalid".to_owned()))?,
    );
    key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(aad), bytes)
        .map_err(|_| ApplicationError::Storage("archive encryption failed".to_owned()))
}

fn open<'a>(
    key: &[u8; 32],
    nonce: [u8; NONCE_LEN],
    aad: &[u8],
    bytes: &'a mut [u8],
) -> Result<&'a [u8], ApplicationError> {
    let key = LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, key)
            .map_err(|_| ApplicationError::Storage("archive AES key is invalid".to_owned()))?,
    );
    key.open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(aad), bytes)
        .map(|plaintext| &*plaintext)
        .map_err(|_| {
            ApplicationError::Storage("archive ciphertext authentication failed".to_owned())
        })
}

fn envelope_aad(set: BackupSetId) -> Vec<u8> {
    [ARCHIVE_AAD_DOMAIN, b"|envelope|", set.as_uuid().as_bytes()].concat()
}

fn object_aad(descriptor: &ArchiveObjectDescriptor) -> Vec<u8> {
    let input = descriptor.to_input();
    let mut out = Vec::new();
    out.extend_from_slice(ARCHIVE_AAD_DOMAIN);
    out.extend_from_slice(input.set_id.as_uuid().as_bytes());
    out.extend_from_slice(input.object_id.as_uuid().as_bytes());
    out.push(match input.kind {
        vestrace_domain::ArchiveObjectKind::BaseChunk => 0,
        vestrace_domain::ArchiveObjectKind::WalSegment => 1,
        vestrace_domain::ArchiveObjectKind::TimelineHistory => 2,
    });
    out.extend_from_slice(&input.ordinal.to_be_bytes());
    out.extend_from_slice(&input.timeline.to_be_bytes());
    out.extend_from_slice(&input.start_lsn.to_be_bytes());
    out.extend_from_slice(&input.end_lsn.to_be_bytes());
    out.extend_from_slice(input.plaintext_digest.as_bytes());
    out.extend_from_slice(&input.length.to_be_bytes());
    out.extend_from_slice(input.predecessor_head_digest.as_bytes());
    out
}

fn stream_chunk_aad(
    descriptor: &ArchiveObjectDescriptor,
    index: u64,
    plaintext_length: u32,
) -> Vec<u8> {
    let mut aad = object_aad(descriptor);
    aad.extend_from_slice(b"|stream-chunk-v2|");
    aad.extend_from_slice(&index.to_be_bytes());
    aad.extend_from_slice(&plaintext_length.to_be_bytes());
    aad
}

fn unavailable(operation: &'static str) -> impl FnOnce(std::io::Error) -> ApplicationError {
    move |error| ApplicationError::Unavailable(format!("{operation}: {error}"))
}
