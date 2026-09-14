//! Canonical P05-C restore and activation values.

use std::path::{Component, Path, PathBuf};

use crate::{
    BackupSetId, DatabaseGenerationId, RestoreHoldId, RestoreHoldReleaseReason, SafetyJournalDigest,
};

const RESTORE_ATTEMPT_PROGRESS_DOMAIN: &[u8] = b"vestrace-restore-attempt-progress-v1";
const SOURCE_FREEZE_POINT_DOMAIN: &[u8] = b"vestrace-source-freeze-point-v1";
const TARGET_ACTIVATION_PLAN_DOMAIN: &[u8] = b"vestrace-target-activation-plan-v1";
const RESTORE_TERMINAL_RECEIPT_DOMAIN: &[u8] = b"vestrace-restore-terminal-receipt-v1";

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RestoreCutoverError {
    #[error("restore/cutover value is malformed")]
    Malformed,
    #[error("restore source and target roots overlap")]
    OverlappingRoots,
    #[error("restore root must be absolute")]
    RelativeRoot,
    #[error("source freeze point is invalid")]
    InvalidFreezePoint,
    #[error("target generation must differ from source generation")]
    TargetGenerationMatchesSource,
    #[error("activation plan freeze generation does not match the source generation")]
    FreezeGenerationMismatch,
    #[error("restore attempt transition is invalid")]
    InvalidAttemptTransition,
}

macro_rules! restore_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(uuid::Uuid);

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }

            pub const fn from_uuid(value: uuid::Uuid) -> Self {
                Self(value)
            }

            pub const fn as_uuid(self) -> uuid::Uuid {
                self.0
            }
        }
    };
}

restore_id!(RestoreAttemptId);
restore_id!(RestoreTargetId);

/// Lexical absolute roots supplied to the host-only restore custody boundary.
/// The filesystem adapter must additionally reject aliases and existing targets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreTargetRoots {
    source_root: PathBuf,
    target_root: PathBuf,
}

impl RestoreTargetRoots {
    pub fn new(source_root: PathBuf, target_root: PathBuf) -> Result<Self, RestoreCutoverError> {
        if !source_root.is_absolute() || !target_root.is_absolute() {
            return Err(RestoreCutoverError::RelativeRoot);
        }
        let source_root = lexical_normalize(&source_root);
        let target_root = lexical_normalize(&target_root);
        if source_root.starts_with(&target_root) || target_root.starts_with(&source_root) {
            return Err(RestoreCutoverError::OverlappingRoots);
        }
        Ok(Self {
            source_root,
            target_root,
        })
    }

    pub fn source_root(&self) -> &Path {
        &self.source_root
    }

    pub fn target_root(&self) -> &Path {
        &self.target_root
    }
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }
    normalized
}

/// The one captured point through which all required archived WAL must pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFreezePoint {
    generation_id: DatabaseGenerationId,
    timeline: u32,
    lsn: u64,
    mutation_watermark: u64,
}

impl SourceFreezePoint {
    pub fn new(
        generation_id: DatabaseGenerationId,
        timeline: u32,
        lsn: u64,
        mutation_watermark: u64,
    ) -> Result<Self, RestoreCutoverError> {
        if timeline == 0 || lsn == 0 {
            return Err(RestoreCutoverError::InvalidFreezePoint);
        }
        Ok(Self {
            generation_id,
            timeline,
            lsn,
            mutation_watermark,
        })
    }

    pub const fn generation_id(self) -> DatabaseGenerationId {
        self.generation_id
    }

    pub const fn timeline(self) -> u32 {
        self.timeline
    }

    pub const fn lsn(self) -> u64 {
        self.lsn
    }

    pub const fn mutation_watermark(self) -> u64 {
        self.mutation_watermark
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        field(&mut out, SOURCE_FREEZE_POINT_DOMAIN);
        field(&mut out, self.generation_id.as_uuid().as_bytes());
        out.extend_from_slice(&self.timeline.to_be_bytes());
        out.extend_from_slice(&self.lsn.to_be_bytes());
        out.extend_from_slice(&self.mutation_watermark.to_be_bytes());
        out
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, RestoreCutoverError> {
        let mut decoder = CanonicalDecoder::new(bytes);
        let value = Self::decode(&mut decoder)?;
        decoder.finish()?;
        Ok(value)
    }

    fn decode(decoder: &mut CanonicalDecoder<'_>) -> Result<Self, RestoreCutoverError> {
        decoder.domain(SOURCE_FREEZE_POINT_DOMAIN)?;
        Self::new(
            DatabaseGenerationId::from_uuid(decoder.uuid()?),
            decoder.u32()?,
            decoder.u64()?,
            decoder.u64()?,
        )
    }
}

/// The witnessed, one-way restore-attempt progress preceding target activation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreAttemptProgress {
    attempt_id: RestoreAttemptId,
    target_id: RestoreTargetId,
    backup_set_id: BackupSetId,
    hold_id: RestoreHoldId,
    source_generation_id: DatabaseGenerationId,
    target_generation_id: DatabaseGenerationId,
    source_freeze: Option<SourceFreezePoint>,
    terminal_receipt: Option<RestoreTerminalReceipt>,
}

impl RestoreAttemptProgress {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        attempt_id: RestoreAttemptId,
        target_id: RestoreTargetId,
        backup_set_id: BackupSetId,
        hold_id: RestoreHoldId,
        source_generation_id: DatabaseGenerationId,
        target_generation_id: DatabaseGenerationId,
    ) -> Result<Self, RestoreCutoverError> {
        if source_generation_id == target_generation_id {
            return Err(RestoreCutoverError::TargetGenerationMatchesSource);
        }
        Ok(Self {
            attempt_id,
            target_id,
            backup_set_id,
            hold_id,
            source_generation_id,
            target_generation_id,
            source_freeze: None,
            terminal_receipt: None,
        })
    }

    pub fn record_source_freeze(
        &self,
        source_freeze: SourceFreezePoint,
    ) -> Result<Self, RestoreCutoverError> {
        if self.source_freeze.is_some()
            || source_freeze.generation_id() != self.source_generation_id
        {
            return Err(RestoreCutoverError::InvalidAttemptTransition);
        }
        let mut next = self.clone();
        next.source_freeze = Some(source_freeze);
        Ok(next)
    }

    pub fn record_terminal_receipt(
        &self,
        receipt: RestoreTerminalReceipt,
    ) -> Result<Self, RestoreCutoverError> {
        if self.source_freeze.is_none()
            || self.terminal_receipt.is_some()
            || receipt.attempt_id() != self.attempt_id
            || receipt.target_id() != self.target_id
        {
            return Err(RestoreCutoverError::InvalidAttemptTransition);
        }
        let mut next = self.clone();
        next.terminal_receipt = Some(receipt);
        Ok(next)
    }

    pub const fn attempt_id(&self) -> RestoreAttemptId {
        self.attempt_id
    }
    pub const fn target_id(&self) -> RestoreTargetId {
        self.target_id
    }
    pub const fn backup_set_id(&self) -> BackupSetId {
        self.backup_set_id
    }
    pub const fn hold_id(&self) -> RestoreHoldId {
        self.hold_id
    }
    pub const fn source_generation_id(&self) -> DatabaseGenerationId {
        self.source_generation_id
    }
    pub const fn target_generation_id(&self) -> DatabaseGenerationId {
        self.target_generation_id
    }
    pub const fn source_freeze(&self) -> Option<SourceFreezePoint> {
        self.source_freeze
    }
    pub fn terminal_receipt(&self) -> Option<&RestoreTerminalReceipt> {
        self.terminal_receipt.as_ref()
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        field(&mut out, RESTORE_ATTEMPT_PROGRESS_DOMAIN);
        field(&mut out, self.attempt_id.as_uuid().as_bytes());
        field(&mut out, self.target_id.as_uuid().as_bytes());
        field(&mut out, self.backup_set_id.as_uuid().as_bytes());
        field(&mut out, self.hold_id.as_uuid().as_bytes());
        field(&mut out, self.source_generation_id.as_uuid().as_bytes());
        field(&mut out, self.target_generation_id.as_uuid().as_bytes());
        match self.source_freeze {
            Some(source_freeze) => {
                out.push(1);
                field(&mut out, &source_freeze.canonical_bytes());
            }
            None => out.push(0),
        }
        match &self.terminal_receipt {
            Some(receipt) => {
                out.push(1);
                field(&mut out, &receipt.canonical_bytes());
            }
            None => out.push(0),
        }
        out
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, RestoreCutoverError> {
        let mut decoder = CanonicalDecoder::new(bytes);
        decoder.domain(RESTORE_ATTEMPT_PROGRESS_DOMAIN)?;
        let prepared = Self::prepare(
            RestoreAttemptId::from_uuid(decoder.uuid()?),
            RestoreTargetId::from_uuid(decoder.uuid()?),
            BackupSetId::from_uuid(decoder.uuid()?),
            RestoreHoldId::from_uuid(decoder.uuid()?),
            DatabaseGenerationId::from_uuid(decoder.uuid()?),
            DatabaseGenerationId::from_uuid(decoder.uuid()?),
        )?;
        let frozen = match decoder.u8()? {
            0 => prepared,
            1 => prepared
                .record_source_freeze(SourceFreezePoint::from_canonical_bytes(decoder.field()?)?)?,
            _ => return Err(RestoreCutoverError::Malformed),
        };
        let value = match decoder.u8()? {
            0 => frozen,
            1 => frozen.record_terminal_receipt(RestoreTerminalReceipt::from_canonical_bytes(
                decoder.field()?,
            )?)?,
            _ => return Err(RestoreCutoverError::Malformed),
        };
        decoder.finish()?;
        Ok(value)
    }
}

/// Immutable successor binding for exactly one target activation CAS.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetActivationPlan {
    attempt_id: RestoreAttemptId,
    target_id: RestoreTargetId,
    backup_set_id: BackupSetId,
    source_generation_id: DatabaseGenerationId,
    target_generation_id: DatabaseGenerationId,
    source_freeze: SourceFreezePoint,
    archive_head_digest: SafetyJournalDigest,
}

impl TargetActivationPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        attempt_id: RestoreAttemptId,
        target_id: RestoreTargetId,
        backup_set_id: BackupSetId,
        source_generation_id: DatabaseGenerationId,
        target_generation_id: DatabaseGenerationId,
        source_freeze: SourceFreezePoint,
        archive_head_digest: SafetyJournalDigest,
    ) -> Result<Self, RestoreCutoverError> {
        if source_generation_id == target_generation_id {
            return Err(RestoreCutoverError::TargetGenerationMatchesSource);
        }
        if source_freeze.generation_id() != source_generation_id {
            return Err(RestoreCutoverError::FreezeGenerationMismatch);
        }
        Ok(Self {
            attempt_id,
            target_id,
            backup_set_id,
            source_generation_id,
            target_generation_id,
            source_freeze,
            archive_head_digest,
        })
    }

    pub const fn attempt_id(&self) -> RestoreAttemptId {
        self.attempt_id
    }

    pub const fn target_id(&self) -> RestoreTargetId {
        self.target_id
    }

    pub const fn backup_set_id(&self) -> BackupSetId {
        self.backup_set_id
    }

    pub const fn source_generation_id(&self) -> DatabaseGenerationId {
        self.source_generation_id
    }

    pub const fn target_generation_id(&self) -> DatabaseGenerationId {
        self.target_generation_id
    }

    pub const fn source_freeze(&self) -> SourceFreezePoint {
        self.source_freeze
    }

    pub const fn archive_head_digest(&self) -> SafetyJournalDigest {
        self.archive_head_digest
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        field(&mut out, TARGET_ACTIVATION_PLAN_DOMAIN);
        field(&mut out, self.attempt_id.as_uuid().as_bytes());
        field(&mut out, self.target_id.as_uuid().as_bytes());
        field(&mut out, self.backup_set_id.as_uuid().as_bytes());
        field(&mut out, self.source_generation_id.as_uuid().as_bytes());
        field(&mut out, self.target_generation_id.as_uuid().as_bytes());
        field(&mut out, &self.source_freeze.canonical_bytes());
        field(&mut out, self.archive_head_digest.as_bytes());
        out
    }

    pub fn digest(&self) -> SafetyJournalDigest {
        SafetyJournalDigest::of(&self.canonical_bytes())
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, RestoreCutoverError> {
        let mut decoder = CanonicalDecoder::new(bytes);
        decoder.domain(TARGET_ACTIVATION_PLAN_DOMAIN)?;
        let attempt_id = RestoreAttemptId::from_uuid(decoder.uuid()?);
        let target_id = RestoreTargetId::from_uuid(decoder.uuid()?);
        let backup_set_id = BackupSetId::from_uuid(decoder.uuid()?);
        let source_generation_id = DatabaseGenerationId::from_uuid(decoder.uuid()?);
        let target_generation_id = DatabaseGenerationId::from_uuid(decoder.uuid()?);
        let source_freeze = SourceFreezePoint::from_canonical_bytes(decoder.field()?)?;
        let archive_head_digest = decoder.digest()?;
        decoder.finish()?;
        Self::new(
            attempt_id,
            target_id,
            backup_set_id,
            source_generation_id,
            target_generation_id,
            source_freeze,
            archive_head_digest,
        )
    }
}

/// A reason-bearing witness receipt required to release a P05-B restore hold.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreTerminalReceipt {
    attempt_id: RestoreAttemptId,
    target_id: RestoreTargetId,
    release_reason: RestoreHoldReleaseReason,
}

impl RestoreTerminalReceipt {
    pub const fn new(
        attempt_id: RestoreAttemptId,
        target_id: RestoreTargetId,
        release_reason: RestoreHoldReleaseReason,
    ) -> Self {
        Self {
            attempt_id,
            target_id,
            release_reason,
        }
    }

    pub const fn attempt_id(&self) -> RestoreAttemptId {
        self.attempt_id
    }

    pub const fn target_id(&self) -> RestoreTargetId {
        self.target_id
    }

    pub const fn release_reason(&self) -> &RestoreHoldReleaseReason {
        &self.release_reason
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        field(&mut out, RESTORE_TERMINAL_RECEIPT_DOMAIN);
        field(&mut out, self.attempt_id.as_uuid().as_bytes());
        field(&mut out, self.target_id.as_uuid().as_bytes());
        let (kind, receipt) = match &self.release_reason {
            RestoreHoldReleaseReason::TargetInitializationComplete { receipt } => (0, receipt),
            RestoreHoldReleaseReason::SourceResumePrepared { receipt } => (1, receipt),
            RestoreHoldReleaseReason::RefusedTargetDestroyed { receipt } => (2, receipt),
            RestoreHoldReleaseReason::SalvageInstallationCompleted { receipt } => (3, receipt),
        };
        out.push(kind);
        field(&mut out, receipt.as_bytes());
        out
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, RestoreCutoverError> {
        let mut decoder = CanonicalDecoder::new(bytes);
        decoder.domain(RESTORE_TERMINAL_RECEIPT_DOMAIN)?;
        let attempt_id = RestoreAttemptId::from_uuid(decoder.uuid()?);
        let target_id = RestoreTargetId::from_uuid(decoder.uuid()?);
        let kind = decoder.u8()?;
        let receipt = decoder.digest()?;
        decoder.finish()?;
        let release_reason = match kind {
            0 => RestoreHoldReleaseReason::TargetInitializationComplete { receipt },
            1 => RestoreHoldReleaseReason::SourceResumePrepared { receipt },
            2 => RestoreHoldReleaseReason::RefusedTargetDestroyed { receipt },
            3 => RestoreHoldReleaseReason::SalvageInstallationCompleted { receipt },
            _ => return Err(RestoreCutoverError::Malformed),
        };
        Ok(Self::new(attempt_id, target_id, release_reason))
    }
}

fn field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

struct CanonicalDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> CanonicalDecoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], RestoreCutoverError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(RestoreCutoverError::Malformed)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(RestoreCutoverError::Malformed)?;
        self.offset = end;
        Ok(value)
    }

    fn field(&mut self) -> Result<&'a [u8], RestoreCutoverError> {
        let length = u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| RestoreCutoverError::Malformed)?,
        );
        self.take(usize::try_from(length).map_err(|_| RestoreCutoverError::Malformed)?)
    }

    fn uuid(&mut self) -> Result<uuid::Uuid, RestoreCutoverError> {
        Ok(uuid::Uuid::from_bytes(
            self.field()?
                .try_into()
                .map_err(|_| RestoreCutoverError::Malformed)?,
        ))
    }

    fn digest(&mut self) -> Result<SafetyJournalDigest, RestoreCutoverError> {
        Ok(SafetyJournalDigest::from_bytes(
            self.field()?
                .try_into()
                .map_err(|_| RestoreCutoverError::Malformed)?,
        ))
    }

    fn u8(&mut self) -> Result<u8, RestoreCutoverError> {
        Ok(*self
            .take(1)?
            .first()
            .ok_or(RestoreCutoverError::Malformed)?)
    }

    fn u32(&mut self) -> Result<u32, RestoreCutoverError> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| RestoreCutoverError::Malformed)?,
        ))
    }

    fn u64(&mut self) -> Result<u64, RestoreCutoverError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| RestoreCutoverError::Malformed)?,
        ))
    }

    fn domain(&mut self, expected: &[u8]) -> Result<(), RestoreCutoverError> {
        if self.field()? == expected {
            Ok(())
        } else {
            Err(RestoreCutoverError::Malformed)
        }
    }

    fn finish(self) -> Result<(), RestoreCutoverError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(RestoreCutoverError::Malformed)
        }
    }
}
