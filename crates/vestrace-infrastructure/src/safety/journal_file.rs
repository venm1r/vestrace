use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(not(windows))]
use std::fs::File;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

use async_trait::async_trait;
use vestrace_application::{ApplicationError, SafetyJournal};
use vestrace_domain::{SafetyJournalDigest, SignedJournalEntry, WitnessError};

const ENTRIES_DIRECTORY: &str = "entries";
const INITIALIZER_RECEIPT: &str = ".initializer-receipt";
const INITIALIZER_RECEIPT_CONTENT: &[u8] = b"vestrace-safety-journal-initialized-v1\n";

/// The dedicated-volume immutable P05 journal. Its caller supplies an already
/// signed entry; this adapter only accepts the exact durable representation.
#[derive(Clone, Debug)]
pub struct FileSafetyJournal {
    entries: PathBuf,
}

impl FileSafetyJournal {
    /// Opens a journal initialized by the one-shot Compose initializer. It
    /// deliberately does not create the directory: an absent volume receipt is
    /// a readiness failure, not an opportunity to synthesize one.
    pub fn open(root: &Path) -> Result<Self, WitnessError> {
        let entries = root.join(ENTRIES_DIRECTORY);
        if !entries.is_dir()
            || fs::read(root.join(INITIALIZER_RECEIPT))
                .map_err(|error| unavailable(error.to_string()))?
                != INITIALIZER_RECEIPT_CONTENT
        {
            return Err(unavailable(
                "journal initializer receipt is absent or invalid",
            ));
        }
        Ok(Self { entries })
    }

    pub fn entry_path(&self, entry: &SignedJournalEntry) -> PathBuf {
        self.entries
            .join(format!("{}-{}.cbor", entry.sequence(), entry.digest()))
    }

    pub fn read_exact(
        &self,
        sequence: u64,
        digest: SafetyJournalDigest,
    ) -> Result<SignedJournalEntry, WitnessError> {
        let path = self.entries.join(format!("{sequence}-{digest}.cbor"));
        let bytes = fs::read(&path).map_err(|error| unavailable(error.to_string()))?;
        let (payload, signature) = decode_entry(&bytes)?;
        let entry = SignedJournalEntry::from_canonical_bytes_and_signature(payload, signature)
            .map_err(|error| unavailable(error.to_string()))?;
        if entry.sequence() != sequence || entry.digest() != digest {
            return Err(unavailable(
                "journal entry path does not match signed content",
            ));
        }
        Ok(entry)
    }

    /// Reads the immediate signed predecessor named by an entry. This is used
    /// only during crash recovery to derive the one state change the durable
    /// entry introduced; directory scans would permit unrelated entries to
    /// influence reconciliation.
    pub fn read_predecessor(
        &self,
        entry: &SignedJournalEntry,
    ) -> Result<SignedJournalEntry, WitnessError> {
        let sequence = entry.sequence().checked_sub(1).ok_or_else(|| {
            WitnessError::Unavailable("journal entry has no predecessor".to_owned())
        })?;
        self.read_exact(sequence, entry.previous_digest())
    }

    fn append_blocking(&self, entry: &SignedJournalEntry) -> Result<(), WitnessError> {
        entry
            .verify()
            .map_err(|error| unavailable(error.to_string()))?;
        let path = self.entry_path(entry);
        let encoded = encode_entry(entry);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(&encoded)
                    .and_then(|()| file.sync_all())
                    .map_err(|error| unavailable(error.to_string()))?;
                sync_directory(&self.entries)?;
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                // A crash after fsync but before the database commit retries
                // only this exact signed entry; a sibling at the same path is
                // never accepted.
                if fs::read(&path).map_err(|read| unavailable(read.to_string()))? == encoded {
                    self.read_exact(entry.sequence(), entry.digest())?;
                    Ok(())
                } else {
                    Err(unavailable("immutable journal entry already differs"))
                }
            }
            Err(error) => Err(unavailable(error.to_string())),
        }
    }
}

#[async_trait]
impl SafetyJournal for FileSafetyJournal {
    async fn append(&self, entry: &SignedJournalEntry) -> Result<(), ApplicationError> {
        self.append_blocking(entry)
            .map_err(|error| ApplicationError::Unavailable(error.to_string()))
    }
}

fn encode_entry(entry: &SignedJournalEntry) -> Vec<u8> {
    let payload = entry.canonical_bytes();
    let mut encoded = Vec::with_capacity(payload.len() + entry.signature().len() + 16);
    encoded.push(0x82); // canonical CBOR array(2)
    cbor_bytes(&mut encoded, &payload);
    cbor_bytes(&mut encoded, entry.signature());
    encoded
}

fn decode_entry(bytes: &[u8]) -> Result<(&[u8], [u8; 64]), WitnessError> {
    let mut decoder = CborDecoder::new(bytes);
    if decoder.byte()? != 0x82 {
        return Err(unavailable(
            "journal record is not a two-element CBOR array",
        ));
    }
    let payload = decoder.bytes()?;
    let signature = decoder
        .bytes()?
        .try_into()
        .map_err(|_| unavailable("journal signature has the wrong length"))?;
    decoder.finish()?;
    Ok((payload, signature))
}

fn cbor_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    match bytes.len() {
        0..=23 => out.push(0x40 | bytes.len() as u8),
        24..=255 => {
            out.extend_from_slice(&[0x58, bytes.len() as u8]);
        }
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
        let byte = *self
            .bytes
            .get(self.offset)
            .ok_or_else(|| unavailable("journal record is truncated"))?;
        self.offset += 1;
        Ok(byte)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], WitnessError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| unavailable("journal record length overflows"))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| unavailable("journal record is truncated"))?;
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
                    .map_err(|_| unavailable("journal CBOR length is malformed"))?,
            )),
            0x5a => usize::try_from(u32::from_be_bytes(
                self.take(4)?
                    .try_into()
                    .map_err(|_| unavailable("journal CBOR length is malformed"))?,
            ))
            .map_err(|_| unavailable("journal CBOR length overflows"))?,
            _ => {
                return Err(unavailable(
                    "journal record has a non-canonical byte string",
                ));
            }
        };
        self.take(length)
    }

    fn finish(self) -> Result<(), WitnessError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(unavailable("journal record has trailing bytes"))
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
