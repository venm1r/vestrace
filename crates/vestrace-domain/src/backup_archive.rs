//! Closed P05-B backup archive lifecycle values.

use std::collections::BTreeMap;

use crate::{DatabaseGenerationId, InstallationId, SafetyJournalDigest};

const ARCHIVE_STATE_VERSION: u8 = 3;
const CHECKPOINT_DOMAIN: &[u8] = b"vestrace-wal-archive-checkpoint-v1";

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum BackupArchiveError {
    #[error("backup archive value is malformed")]
    Malformed,
    #[error("backup set already exists")]
    DuplicateBackupSet,
    #[error("restore hold already exists")]
    DuplicateRestoreHold,
    #[error("backup set does not exist")]
    UnknownBackupSet,
    #[error("backup set is not streaming")]
    NotStreaming,
    #[error("backup set is not restorable")]
    NotRestorable,
    #[error("backup set requires a base checkpoint before this operation")]
    BaseCheckpointRequired,
    #[error("backup set already has its unique base checkpoint")]
    DuplicateBaseCheckpoint,
    #[error("the base checkpoint must be the first archive checkpoint")]
    BaseCheckpointMustBeFirst,
    #[error("WAL checkpoint does not descend from the recorded base checkpoint")]
    BaseCheckpointAncestryMismatch,
    #[error("backup set has an active restore hold")]
    RestoreHoldPresent,
    #[error("backup archive head does not match")]
    ArchiveHeadMismatch,
    #[error("backup archive lifecycle transition is invalid")]
    InvalidLifecycleTransition,
    #[error("restore hold is already released")]
    RestoreHoldAlreadyReleased,
}

macro_rules! archive_id {
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

archive_id!(BackupSetId);
archive_id!(BackupObjectId);
archive_id!(RestoreHoldId);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackupSetLifecycle {
    Streaming,
    Sealing,
    Sealed,
    DeletionPrepared,
    ArchiveKeyErased,
    Deleted,
}

impl BackupSetLifecycle {
    fn code(self) -> u8 {
        match self {
            Self::Streaming => 0,
            Self::Sealing => 1,
            Self::Sealed => 2,
            Self::DeletionPrepared => 3,
            Self::ArchiveKeyErased => 4,
            Self::Deleted => 5,
        }
    }

    fn from_code(value: u8) -> Result<Self, BackupArchiveError> {
        match value {
            0 => Ok(Self::Streaming),
            1 => Ok(Self::Sealing),
            2 => Ok(Self::Sealed),
            3 => Ok(Self::DeletionPrepared),
            4 => Ok(Self::ArchiveKeyErased),
            5 => Ok(Self::Deleted),
            _ => Err(BackupArchiveError::Malformed),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveObjectKind {
    BaseChunk,
    WalSegment,
    TimelineHistory,
}

impl ArchiveObjectKind {
    fn code(self) -> u8 {
        match self {
            Self::BaseChunk => 0,
            Self::WalSegment => 1,
            Self::TimelineHistory => 2,
        }
    }

    fn from_code(value: u8) -> Result<Self, BackupArchiveError> {
        match value {
            0 => Ok(Self::BaseChunk),
            1 => Ok(Self::WalSegment),
            2 => Ok(Self::TimelineHistory),
            _ => Err(BackupArchiveError::Malformed),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveHead {
    pub checkpoint_ordinal: u64,
    pub digest: SafetyJournalDigest,
}

impl ArchiveHead {
    pub const fn genesis(digest: SafetyJournalDigest) -> Self {
        Self {
            checkpoint_ordinal: 0,
            digest,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackupSetIdentity {
    set_id: BackupSetId,
    installation_id: InstallationId,
    generation_id: DatabaseGenerationId,
}

impl BackupSetIdentity {
    pub const fn new(
        set_id: BackupSetId,
        installation_id: InstallationId,
        generation_id: DatabaseGenerationId,
    ) -> Self {
        Self {
            set_id,
            installation_id,
            generation_id,
        }
    }

    pub const fn set_id(self) -> BackupSetId {
        self.set_id
    }

    pub const fn installation_id(self) -> InstallationId {
        self.installation_id
    }

    pub const fn generation_id(self) -> DatabaseGenerationId {
        self.generation_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveObjectDescriptorInput {
    pub set_id: BackupSetId,
    pub object_id: BackupObjectId,
    pub kind: ArchiveObjectKind,
    pub ordinal: u64,
    pub timeline: u32,
    pub start_lsn: u64,
    pub end_lsn: u64,
    pub ciphertext_digest: SafetyJournalDigest,
    pub plaintext_digest: SafetyJournalDigest,
    pub length: u64,
    pub predecessor_head_digest: SafetyJournalDigest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveObjectDescriptor(ArchiveObjectDescriptorInput);

impl ArchiveObjectDescriptor {
    pub fn new(input: ArchiveObjectDescriptorInput) -> Result<Self, BackupArchiveError> {
        if input.ordinal == 0
            || input.timeline == 0
            || input.end_lsn < input.start_lsn
            || input.length == 0
        {
            return Err(BackupArchiveError::Malformed);
        }
        Ok(Self(input))
    }

    pub fn to_input(&self) -> ArchiveObjectDescriptorInput {
        self.0.clone()
    }

    pub const fn set_id(&self) -> BackupSetId {
        self.0.set_id
    }

    pub const fn object_id(&self) -> BackupObjectId {
        self.0.object_id
    }

    pub const fn ordinal(&self) -> u64 {
        self.0.ordinal
    }

    pub const fn predecessor_head_digest(&self) -> SafetyJournalDigest {
        self.0.predecessor_head_digest
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.0.set_id.as_uuid().as_bytes());
        out.extend_from_slice(self.0.object_id.as_uuid().as_bytes());
        out.push(self.0.kind.code());
        out.extend_from_slice(&self.0.ordinal.to_be_bytes());
        out.extend_from_slice(&self.0.timeline.to_be_bytes());
        out.extend_from_slice(&self.0.start_lsn.to_be_bytes());
        out.extend_from_slice(&self.0.end_lsn.to_be_bytes());
        out.extend_from_slice(self.0.ciphertext_digest.as_bytes());
        out.extend_from_slice(self.0.plaintext_digest.as_bytes());
        out.extend_from_slice(&self.0.length.to_be_bytes());
        out.extend_from_slice(self.0.predecessor_head_digest.as_bytes());
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self, BackupArchiveError> {
        Self::new(ArchiveObjectDescriptorInput {
            set_id: BackupSetId::from_uuid(decoder.uuid()?),
            object_id: BackupObjectId::from_uuid(decoder.uuid()?),
            kind: ArchiveObjectKind::from_code(decoder.u8()?)?,
            ordinal: decoder.u64()?,
            timeline: decoder.u32()?,
            start_lsn: decoder.u64()?,
            end_lsn: decoder.u64()?,
            ciphertext_digest: decoder.digest()?,
            plaintext_digest: decoder.digest()?,
            length: decoder.u64()?,
            predecessor_head_digest: decoder.digest()?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveAppendReservation {
    set_id: BackupSetId,
    expected: ArchiveHead,
    object: ArchiveObjectDescriptor,
}

impl ArchiveAppendReservation {
    pub fn new(
        set_id: BackupSetId,
        expected: ArchiveHead,
        object: ArchiveObjectDescriptor,
    ) -> Result<Self, BackupArchiveError> {
        if object.set_id() != set_id
            || object.predecessor_head_digest() != expected.digest
            || expected
                .checkpoint_ordinal
                .checked_add(1)
                .is_none_or(|ordinal| object.ordinal() != ordinal)
        {
            return Err(BackupArchiveError::ArchiveHeadMismatch);
        }
        Ok(Self {
            set_id,
            expected,
            object,
        })
    }

    pub const fn set_id(&self) -> BackupSetId {
        self.set_id
    }

    pub const fn expected_head(&self) -> ArchiveHead {
        self.expected
    }

    pub fn object(&self) -> &ArchiveObjectDescriptor {
        &self.object
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WalArchiveCheckpoint {
    object: ArchiveObjectDescriptor,
}

impl WalArchiveCheckpoint {
    pub fn from_reservation(reservation: &ArchiveAppendReservation) -> Self {
        Self {
            object: reservation.object.clone(),
        }
    }

    pub fn object(&self) -> &ArchiveObjectDescriptor {
        &self.object
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        field(&mut bytes, CHECKPOINT_DOMAIN);
        self.object.encode(&mut bytes);
        bytes
    }

    pub fn digest(&self) -> SafetyJournalDigest {
        SafetyJournalDigest::of(&self.canonical_bytes())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self, BackupArchiveError> {
        decoder.domain(CHECKPOINT_DOMAIN)?;
        Ok(Self {
            object: ArchiveObjectDescriptor::decode(decoder)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RestoreHoldReleaseReason {
    TargetInitializationComplete { receipt: SafetyJournalDigest },
    SourceResumePrepared { receipt: SafetyJournalDigest },
    RefusedTargetDestroyed { receipt: SafetyJournalDigest },
    SalvageInstallationCompleted { receipt: SafetyJournalDigest },
}

impl RestoreHoldReleaseReason {
    pub const fn receipt(&self) -> SafetyJournalDigest {
        match self {
            Self::TargetInitializationComplete { receipt }
            | Self::SourceResumePrepared { receipt }
            | Self::RefusedTargetDestroyed { receipt }
            | Self::SalvageInstallationCompleted { receipt } => *receipt,
        }
    }

    fn encode(&self, out: &mut Vec<u8>) {
        let (kind, receipt) = match self {
            Self::TargetInitializationComplete { receipt } => (0, receipt),
            Self::SourceResumePrepared { receipt } => (1, receipt),
            Self::RefusedTargetDestroyed { receipt } => (2, receipt),
            Self::SalvageInstallationCompleted { receipt } => (3, receipt),
        };
        out.push(kind);
        out.extend_from_slice(receipt.as_bytes());
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self, BackupArchiveError> {
        let kind = decoder.u8()?;
        let receipt = decoder.digest()?;
        match kind {
            0 => Ok(Self::TargetInitializationComplete { receipt }),
            1 => Ok(Self::SourceResumePrepared { receipt }),
            2 => Ok(Self::RefusedTargetDestroyed { receipt }),
            3 => Ok(Self::SalvageInstallationCompleted { receipt }),
            _ => Err(BackupArchiveError::Malformed),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreHold {
    id: RestoreHoldId,
    release_reason: Option<RestoreHoldReleaseReason>,
}

impl RestoreHold {
    pub const fn id(&self) -> RestoreHoldId {
        self.id
    }

    pub fn release_reason(&self) -> Option<&RestoreHoldReleaseReason> {
        self.release_reason.as_ref()
    }

    fn active(&self) -> bool {
        self.release_reason.is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BackupSetState {
    identity: BackupSetIdentity,
    lifecycle: BackupSetLifecycle,
    deletion_requested: bool,
    deletion_preparation: Option<SafetyJournalDigest>,
    key_erasure_preparation: Option<SafetyJournalDigest>,
    head: ArchiveHead,
    checkpoints: Vec<WalArchiveCheckpoint>,
    holds: BTreeMap<RestoreHoldId, RestoreHold>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupArchiveStateV1 {
    sets: BTreeMap<BackupSetId, BackupSetState>,
}

impl BackupArchiveStateV1 {
    pub fn empty() -> Self {
        Self {
            sets: BTreeMap::new(),
        }
    }

    pub fn start_streaming(
        &self,
        identity: BackupSetIdentity,
        head: ArchiveHead,
    ) -> Result<Self, BackupArchiveError> {
        if self.sets.contains_key(&identity.set_id) {
            return Err(BackupArchiveError::DuplicateBackupSet);
        }
        let mut next = self.clone();
        next.sets.insert(
            identity.set_id,
            BackupSetState {
                identity,
                lifecycle: BackupSetLifecycle::Streaming,
                deletion_requested: false,
                deletion_preparation: None,
                key_erasure_preparation: None,
                head,
                checkpoints: Vec::new(),
                holds: BTreeMap::new(),
            },
        );
        Ok(next)
    }

    pub fn sets(&self) -> impl Iterator<Item = BackupSetId> + '_ {
        self.sets.keys().copied()
    }

    pub fn archive_head(&self, set: BackupSetId) -> Result<ArchiveHead, BackupArchiveError> {
        Ok(self.set(set)?.head)
    }

    pub fn lifecycle(&self, set: BackupSetId) -> Result<BackupSetLifecycle, BackupArchiveError> {
        Ok(self.set(set)?.lifecycle)
    }

    /// Returns every hold identity named by one set, including terminally
    /// released holds. Reconciliation uses this inventory only to identify
    /// the one new active hold carried by a signed successor state.
    pub fn restore_hold_ids(
        &self,
        set: BackupSetId,
    ) -> Result<Vec<RestoreHoldId>, BackupArchiveError> {
        Ok(self.set(set)?.holds.keys().copied().collect())
    }

    /// Exact witnessed identity bound to a prepared deletion and its later
    /// key-erasure receipt.
    pub fn deletion_preparation_digest(
        &self,
        set: BackupSetId,
    ) -> Result<Option<SafetyJournalDigest>, BackupArchiveError> {
        Ok(self.set(set)?.deletion_preparation)
    }

    /// Exact witnessed deletion preparation for which host key custody may be
    /// erased. This intent is separately signed before the host operation.
    pub fn key_erasure_preparation_digest(
        &self,
        set: BackupSetId,
    ) -> Result<Option<SafetyJournalDigest>, BackupArchiveError> {
        Ok(self.set(set)?.key_erasure_preparation)
    }

    /// Returns the exact descriptor named by the current signed state.  Host
    /// recovery uses this only for the checkpoint carried by its witnessed
    /// journal entry; callers cannot synthesize a staged member from a set
    /// prefix or an unpinned archive scan.
    pub fn latest_checkpoint(
        &self,
        set: BackupSetId,
    ) -> Result<Option<&WalArchiveCheckpoint>, BackupArchiveError> {
        Ok(self.set(set)?.checkpoints.last())
    }

    pub fn backup_set_identity(
        &self,
        set: BackupSetId,
    ) -> Result<BackupSetIdentity, BackupArchiveError> {
        Ok(self.set(set)?.identity)
    }

    pub fn reserve_append(
        &self,
        set: BackupSetId,
        expected: ArchiveHead,
        object: ArchiveObjectDescriptor,
    ) -> Result<ArchiveAppendReservation, BackupArchiveError> {
        let current = self.set(set)?;
        if current.lifecycle != BackupSetLifecycle::Streaming {
            return Err(BackupArchiveError::NotStreaming);
        }
        Self::validate_append_kind(current, &object)?;
        if current.head != expected
            || object.set_id() != set
            || object.predecessor_head_digest() != expected.digest
            || expected
                .checkpoint_ordinal
                .checked_add(1)
                .is_none_or(|ordinal| object.ordinal() != ordinal)
        {
            return Err(BackupArchiveError::ArchiveHeadMismatch);
        }
        ArchiveAppendReservation::new(set, expected, object)
    }

    pub fn commit_checkpoint(
        &self,
        reservation: ArchiveAppendReservation,
        checkpoint: WalArchiveCheckpoint,
    ) -> Result<Self, BackupArchiveError> {
        let current = self.set(reservation.set_id)?;
        if current.lifecycle != BackupSetLifecycle::Streaming
            || current.head != reservation.expected
        {
            return Err(BackupArchiveError::ArchiveHeadMismatch);
        }
        if checkpoint.object != reservation.object {
            return Err(BackupArchiveError::ArchiveHeadMismatch);
        }
        Self::validate_append_kind(current, checkpoint.object())?;
        let mut next = self.clone();
        let state = next
            .sets
            .get_mut(&reservation.set_id)
            .ok_or(BackupArchiveError::UnknownBackupSet)?;
        state.head = ArchiveHead {
            checkpoint_ordinal: checkpoint.object.ordinal(),
            digest: checkpoint.digest(),
        };
        state.checkpoints.push(checkpoint);
        Ok(next)
    }

    /// Returns true only when `next` preserves every archive set except for
    /// one streaming set receiving exactly one valid checkpoint successor.
    pub fn is_exact_checkpoint_successor_of(&self, previous: &Self) -> bool {
        if self.sets.len() != previous.sets.len() {
            return false;
        }
        let mut changed = false;
        for (set_id, before) in &previous.sets {
            let Some(after) = self.sets.get(set_id) else {
                return false;
            };
            if after == before {
                continue;
            }
            if changed
                || after.identity != before.identity
                || after.lifecycle != BackupSetLifecycle::Streaming
                || after.deletion_requested != before.deletion_requested
                || after.deletion_preparation != before.deletion_preparation
                || after.key_erasure_preparation != before.key_erasure_preparation
                || after.holds != before.holds
                || after.checkpoints.len() != before.checkpoints.len().saturating_add(1)
                || !after.checkpoints.starts_with(&before.checkpoints)
            {
                return false;
            }
            let Some(last) = after.checkpoints.last() else {
                return false;
            };
            if last.object.set_id() != *set_id
                || Self::validate_append_kind(before, last.object()).is_err()
                || last.object.ordinal()
                    != before
                        .head
                        .checkpoint_ordinal
                        .checked_add(1)
                        .unwrap_or(u64::MAX)
                || last.object.predecessor_head_digest() != before.head.digest
                || after.head
                    != (ArchiveHead {
                        checkpoint_ordinal: last.object.ordinal(),
                        digest: last.digest(),
                    })
            {
                return false;
            }
            changed = true;
        }
        changed
    }

    /// Accepts exactly one new, empty Streaming set and preserves every
    /// existing archive state byte-for-byte.
    pub fn is_exact_started_successor_of(&self, previous: &Self) -> bool {
        if self.sets.len() != previous.sets.len().saturating_add(1) {
            return false;
        }
        let mut added = false;
        for (set_id, after) in &self.sets {
            match previous.sets.get(set_id) {
                Some(before) if before == after => {}
                Some(_) => return false,
                None if !added
                    && after.lifecycle == BackupSetLifecycle::Streaming
                    && !after.deletion_requested
                    && after.deletion_preparation.is_none()
                    && after.key_erasure_preparation.is_none()
                    && after.checkpoints.is_empty()
                    && after.holds.is_empty()
                    && after.head.checkpoint_ordinal == 0 =>
                {
                    added = true;
                }
                None => return false,
            }
        }
        added
    }

    /// Accepts one non-expiring hold on one unchanged Streaming set.
    pub fn is_exact_hold_successor_of(&self, previous: &Self) -> bool {
        if self.sets.len() != previous.sets.len() {
            return false;
        }
        let mut changed = false;
        for (set_id, before) in &previous.sets {
            let Some(after) = self.sets.get(set_id) else {
                return false;
            };
            if after == before {
                continue;
            }
            if changed
                || after.identity != before.identity
                || after.lifecycle != BackupSetLifecycle::Streaming
                || after.deletion_requested != before.deletion_requested
                || after.deletion_preparation != before.deletion_preparation
                || after.key_erasure_preparation != before.key_erasure_preparation
                || after.head != before.head
                || after.checkpoints != before.checkpoints
                || !Self::has_base_checkpoint(before)
                || after.holds.len() != before.holds.len().saturating_add(1)
                || !before
                    .holds
                    .iter()
                    .all(|(id, hold)| after.holds.get(id) == Some(hold))
                || !after
                    .holds
                    .iter()
                    .any(|(id, hold)| !before.holds.contains_key(id) && hold.active())
            {
                return false;
            }
            changed = true;
        }
        changed
    }

    /// Accepts exactly one lifecycle step without changing head, checkpoint,
    /// hold, or immutable set identity data.
    pub fn is_exact_lifecycle_successor_of(
        &self,
        previous: &Self,
        from: BackupSetLifecycle,
        to: BackupSetLifecycle,
    ) -> bool {
        if self.sets.len() != previous.sets.len() {
            return false;
        }
        let mut changed = false;
        for (set_id, before) in &previous.sets {
            let Some(after) = self.sets.get(set_id) else {
                return false;
            };
            if after == before {
                continue;
            }
            if changed
                || before.lifecycle != from
                || after.lifecycle != to
                || after.identity != before.identity
                || after.deletion_requested != before.deletion_requested
                || after.deletion_preparation != before.deletion_preparation
                || after.key_erasure_preparation != before.key_erasure_preparation
                || after.head != before.head
                || after.checkpoints != before.checkpoints
                || after.holds != before.holds
                || !Self::has_base_checkpoint(before)
            {
                return false;
            }
            changed = true;
        }
        changed
    }

    /// The deletion-prepared edge is separate because it must bind the opaque
    /// preparation digest that later authorizes host key erasure.
    pub fn is_exact_prepared_deletion_successor_of(&self, previous: &Self) -> bool {
        if self.sets.len() != previous.sets.len() {
            return false;
        }
        let mut changed = false;
        for (set_id, before) in &previous.sets {
            let Some(after) = self.sets.get(set_id) else {
                return false;
            };
            if after == before {
                continue;
            }
            if changed
                || before.lifecycle != BackupSetLifecycle::Sealed
                || after.lifecycle != BackupSetLifecycle::DeletionPrepared
                || before.deletion_preparation.is_some()
                || after.deletion_preparation.is_none()
                || before.key_erasure_preparation.is_some()
                || after.key_erasure_preparation.is_some()
                || after.identity != before.identity
                || after.deletion_requested != before.deletion_requested
                || after.head != before.head
                || after.checkpoints != before.checkpoints
                || after.holds != before.holds
            {
                return false;
            }
            changed = true;
        }
        changed
    }

    /// Binds one host key-erasure intent to the preparation that already
    /// moved the set into `DeletionPrepared`. No lifecycle fact changes yet.
    pub fn is_exact_key_erasure_prepared_successor_of(&self, previous: &Self) -> bool {
        if self.sets.len() != previous.sets.len() {
            return false;
        }
        let mut changed = false;
        for (set_id, before) in &previous.sets {
            let Some(after) = self.sets.get(set_id) else {
                return false;
            };
            if after == before {
                continue;
            }
            if changed
                || before.lifecycle != BackupSetLifecycle::DeletionPrepared
                || after.lifecycle != BackupSetLifecycle::DeletionPrepared
                || before.key_erasure_preparation.is_some()
                || after.key_erasure_preparation != before.deletion_preparation
                || after.identity != before.identity
                || after.deletion_requested != before.deletion_requested
                || after.deletion_preparation != before.deletion_preparation
                || after.head != before.head
                || after.checkpoints != before.checkpoints
                || after.holds != before.holds
            {
                return false;
            }
            changed = true;
        }
        changed
    }

    pub fn acquire_restore_hold(
        &self,
        set: BackupSetId,
        hold_id: RestoreHoldId,
    ) -> Result<Self, BackupArchiveError> {
        let current = self.set(set)?;
        if current.lifecycle != BackupSetLifecycle::Streaming {
            return Err(BackupArchiveError::NotRestorable);
        }
        if !Self::has_base_checkpoint(current) {
            return Err(BackupArchiveError::NotRestorable);
        }
        if current.holds.contains_key(&hold_id) {
            return Err(BackupArchiveError::DuplicateRestoreHold);
        }
        let mut next = self.clone();
        next.sets
            .get_mut(&set)
            .ok_or(BackupArchiveError::UnknownBackupSet)?
            .holds
            .insert(
                hold_id,
                RestoreHold {
                    id: hold_id,
                    release_reason: None,
                },
            );
        Ok(next)
    }

    pub fn release_restore_hold(
        &self,
        set: BackupSetId,
        hold_id: RestoreHoldId,
        reason: RestoreHoldReleaseReason,
    ) -> Result<Self, BackupArchiveError> {
        let mut next = self.clone();
        let hold = next
            .sets
            .get_mut(&set)
            .ok_or(BackupArchiveError::UnknownBackupSet)?
            .holds
            .get_mut(&hold_id)
            .ok_or(BackupArchiveError::UnknownBackupSet)?;
        if !hold.active() {
            return Err(BackupArchiveError::RestoreHoldAlreadyReleased);
        }
        hold.release_reason = Some(reason);
        Ok(next)
    }

    pub fn begin_sealing(&self, set: BackupSetId) -> Result<Self, BackupArchiveError> {
        let current = self.set(set)?;
        if current.lifecycle != BackupSetLifecycle::Streaming {
            return Err(BackupArchiveError::InvalidLifecycleTransition);
        }
        if !Self::has_base_checkpoint(current) {
            return Err(BackupArchiveError::NotRestorable);
        }
        if current.holds.values().any(RestoreHold::active) {
            return Err(BackupArchiveError::RestoreHoldPresent);
        }
        self.transition(set, BackupSetLifecycle::Sealing)
    }

    pub fn commit_sealed(&self, set: BackupSetId) -> Result<Self, BackupArchiveError> {
        self.require_no_active_holds(set)?;
        self.transition_from(set, BackupSetLifecycle::Sealing, BackupSetLifecycle::Sealed)
    }

    pub fn prepare_deletion(
        &self,
        set: BackupSetId,
        preparation: SafetyJournalDigest,
    ) -> Result<Self, BackupArchiveError> {
        self.require_no_active_holds(set)?;
        let mut next = self.transition_from(
            set,
            BackupSetLifecycle::Sealed,
            BackupSetLifecycle::DeletionPrepared,
        )?;
        next.sets
            .get_mut(&set)
            .ok_or(BackupArchiveError::UnknownBackupSet)?
            .deletion_preparation = Some(preparation);
        Ok(next)
    }

    pub fn record_archive_key_erased(&self, set: BackupSetId) -> Result<Self, BackupArchiveError> {
        let current = self.set(set)?;
        if current.key_erasure_preparation.is_none()
            || current.key_erasure_preparation != current.deletion_preparation
        {
            return Err(BackupArchiveError::InvalidLifecycleTransition);
        }
        self.transition_from(
            set,
            BackupSetLifecycle::DeletionPrepared,
            BackupSetLifecycle::ArchiveKeyErased,
        )
    }

    pub fn prepare_archive_key_erasure(
        &self,
        set: BackupSetId,
        preparation: SafetyJournalDigest,
    ) -> Result<Self, BackupArchiveError> {
        let current = self.set(set)?;
        if current.lifecycle != BackupSetLifecycle::DeletionPrepared
            || current.deletion_preparation != Some(preparation)
            || current.key_erasure_preparation.is_some()
        {
            return Err(BackupArchiveError::InvalidLifecycleTransition);
        }
        let mut next = self.clone();
        next.sets
            .get_mut(&set)
            .ok_or(BackupArchiveError::UnknownBackupSet)?
            .key_erasure_preparation = Some(preparation);
        Ok(next)
    }

    pub fn finalize_deleted(&self, set: BackupSetId) -> Result<Self, BackupArchiveError> {
        self.transition_from(
            set,
            BackupSetLifecycle::ArchiveKeyErased,
            BackupSetLifecycle::Deleted,
        )
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(ARCHIVE_STATE_VERSION);
        out.extend_from_slice(&(self.sets.len() as u64).to_be_bytes());
        for state in self.sets.values() {
            out.extend_from_slice(state.identity.set_id.as_uuid().as_bytes());
            out.extend_from_slice(state.identity.installation_id.as_uuid().as_bytes());
            out.extend_from_slice(state.identity.generation_id.as_uuid().as_bytes());
            out.push(state.lifecycle.code());
            out.push(u8::from(state.deletion_requested));
            out.push(u8::from(state.deletion_preparation.is_some()));
            if let Some(preparation) = state.deletion_preparation {
                out.extend_from_slice(preparation.as_bytes());
            }
            out.push(u8::from(state.key_erasure_preparation.is_some()));
            if let Some(preparation) = state.key_erasure_preparation {
                out.extend_from_slice(preparation.as_bytes());
            }
            out.extend_from_slice(&state.head.checkpoint_ordinal.to_be_bytes());
            out.extend_from_slice(state.head.digest.as_bytes());
            out.extend_from_slice(&(state.checkpoints.len() as u64).to_be_bytes());
            for checkpoint in &state.checkpoints {
                field(&mut out, &checkpoint.canonical_bytes());
            }
            out.extend_from_slice(&(state.holds.len() as u64).to_be_bytes());
            for hold in state.holds.values() {
                out.extend_from_slice(hold.id.as_uuid().as_bytes());
                out.push(u8::from(hold.release_reason.is_some()));
                if let Some(reason) = &hold.release_reason {
                    reason.encode(&mut out);
                }
            }
        }
        out
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, BackupArchiveError> {
        if bytes.is_empty() {
            // P05-A wrote an explicitly empty slot. It represents the same
            // archive-free genesis state and is accepted only for reopening.
            return Ok(Self::empty());
        }
        let mut decoder = Decoder::new(bytes);
        if decoder.u8()? != ARCHIVE_STATE_VERSION {
            return Err(BackupArchiveError::Malformed);
        }
        let set_count = decoder.usize()?;
        let mut sets = BTreeMap::new();
        for _ in 0..set_count {
            let identity = BackupSetIdentity::new(
                BackupSetId::from_uuid(decoder.uuid()?),
                InstallationId::from_uuid(decoder.uuid()?),
                DatabaseGenerationId::from_uuid(decoder.uuid()?),
            );
            let lifecycle = BackupSetLifecycle::from_code(decoder.u8()?)?;
            let deletion_requested = decoder.bool()?;
            let deletion_preparation = if decoder.bool()? {
                Some(decoder.digest()?)
            } else {
                None
            };
            let key_erasure_preparation = if decoder.bool()? {
                Some(decoder.digest()?)
            } else {
                None
            };
            let head = ArchiveHead {
                checkpoint_ordinal: decoder.u64()?,
                digest: decoder.digest()?,
            };
            let checkpoint_count = decoder.usize()?;
            let mut checkpoints = Vec::with_capacity(checkpoint_count);
            let mut previous_ordinal = 0;
            for _ in 0..checkpoint_count {
                let mut checkpoint_decoder = Decoder::new(decoder.field()?);
                let checkpoint = WalArchiveCheckpoint::decode(&mut checkpoint_decoder)?;
                checkpoint_decoder.finish()?;
                if checkpoint.object.set_id() != identity.set_id
                    || checkpoint.object.ordinal() <= previous_ordinal
                {
                    return Err(BackupArchiveError::Malformed);
                }
                previous_ordinal = checkpoint.object.ordinal();
                checkpoints.push(checkpoint);
            }
            if let Some(first) = checkpoints.first() {
                let first_kind = first.object.to_input().kind;
                if first_kind == ArchiveObjectKind::BaseChunk && first.object.ordinal() != 1 {
                    return Err(BackupArchiveError::Malformed);
                }
                if checkpoints.iter().skip(1).any(|checkpoint| {
                    checkpoint.object.to_input().kind == ArchiveObjectKind::BaseChunk
                }) {
                    return Err(BackupArchiveError::Malformed);
                }
                if first_kind != ArchiveObjectKind::BaseChunk
                    && checkpoints.iter().any(|checkpoint| {
                        checkpoint.object.to_input().kind == ArchiveObjectKind::BaseChunk
                    })
                {
                    return Err(BackupArchiveError::Malformed);
                }
            }
            if let Some(last) = checkpoints.last() {
                if head.checkpoint_ordinal != last.object.ordinal() || head.digest != last.digest()
                {
                    return Err(BackupArchiveError::Malformed);
                }
            }
            if checkpoints.is_empty() && head.checkpoint_ordinal != 0 {
                return Err(BackupArchiveError::Malformed);
            }
            if matches!(
                lifecycle,
                BackupSetLifecycle::DeletionPrepared
                    | BackupSetLifecycle::ArchiveKeyErased
                    | BackupSetLifecycle::Deleted
            ) != deletion_preparation.is_some()
            {
                return Err(BackupArchiveError::Malformed);
            }
            if key_erasure_preparation.is_some() && key_erasure_preparation != deletion_preparation
            {
                return Err(BackupArchiveError::Malformed);
            }
            if matches!(
                lifecycle,
                BackupSetLifecycle::ArchiveKeyErased | BackupSetLifecycle::Deleted
            ) && key_erasure_preparation.is_none()
            {
                return Err(BackupArchiveError::Malformed);
            }
            let hold_count = decoder.usize()?;
            let mut holds = BTreeMap::new();
            for _ in 0..hold_count {
                let hold_id = RestoreHoldId::from_uuid(decoder.uuid()?);
                let release_reason = if decoder.bool()? {
                    Some(RestoreHoldReleaseReason::decode(&mut decoder)?)
                } else {
                    None
                };
                if holds
                    .insert(
                        hold_id,
                        RestoreHold {
                            id: hold_id,
                            release_reason,
                        },
                    )
                    .is_some()
                {
                    return Err(BackupArchiveError::Malformed);
                }
            }
            if sets
                .insert(
                    identity.set_id,
                    BackupSetState {
                        identity,
                        lifecycle,
                        deletion_requested,
                        deletion_preparation,
                        key_erasure_preparation,
                        head,
                        checkpoints,
                        holds,
                    },
                )
                .is_some()
            {
                return Err(BackupArchiveError::Malformed);
            }
        }
        decoder.finish()?;
        Ok(Self { sets })
    }

    fn set(&self, set: BackupSetId) -> Result<&BackupSetState, BackupArchiveError> {
        self.sets
            .get(&set)
            .ok_or(BackupArchiveError::UnknownBackupSet)
    }

    fn has_base_checkpoint(state: &BackupSetState) -> bool {
        state.checkpoints.first().is_some_and(|checkpoint| {
            checkpoint.object.to_input().kind == ArchiveObjectKind::BaseChunk
        })
    }

    fn validate_append_kind(
        state: &BackupSetState,
        object: &ArchiveObjectDescriptor,
    ) -> Result<(), BackupArchiveError> {
        match (Self::has_base_checkpoint(state), object.to_input().kind) {
            (false, ArchiveObjectKind::BaseChunk) if object.ordinal() == 1 => Ok(()),
            (false, ArchiveObjectKind::BaseChunk) => {
                Err(BackupArchiveError::BaseCheckpointMustBeFirst)
            }
            (false, _) => Err(BackupArchiveError::BaseCheckpointRequired),
            (true, ArchiveObjectKind::BaseChunk) => {
                Err(BackupArchiveError::DuplicateBaseCheckpoint)
            }
            (true, ArchiveObjectKind::WalSegment) => {
                let base = state
                    .checkpoints
                    .first()
                    .ok_or(BackupArchiveError::BaseCheckpointRequired)?
                    .object
                    .to_input();
                let wal = object.to_input();
                if wal.timeline != base.timeline || wal.start_lsn < base.start_lsn {
                    return Err(BackupArchiveError::BaseCheckpointAncestryMismatch);
                }
                Ok(())
            }
            (true, _) => Ok(()),
        }
    }

    fn require_no_active_holds(&self, set: BackupSetId) -> Result<(), BackupArchiveError> {
        if self.set(set)?.holds.values().any(RestoreHold::active) {
            return Err(BackupArchiveError::RestoreHoldPresent);
        }
        Ok(())
    }

    fn transition(
        &self,
        set: BackupSetId,
        next_lifecycle: BackupSetLifecycle,
    ) -> Result<Self, BackupArchiveError> {
        let mut next = self.clone();
        next.sets
            .get_mut(&set)
            .ok_or(BackupArchiveError::UnknownBackupSet)?
            .lifecycle = next_lifecycle;
        Ok(next)
    }

    fn transition_from(
        &self,
        set: BackupSetId,
        expected: BackupSetLifecycle,
        next_lifecycle: BackupSetLifecycle,
    ) -> Result<Self, BackupArchiveError> {
        if self.set(set)?.lifecycle != expected {
            return Err(BackupArchiveError::InvalidLifecycleTransition);
        }
        self.transition(set, next_lifecycle)
    }
}

fn field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], BackupArchiveError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(BackupArchiveError::Malformed)?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(BackupArchiveError::Malformed)?;
        self.offset = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, BackupArchiveError> {
        Ok(self.take(1)?[0])
    }

    fn bool(&mut self) -> Result<bool, BackupArchiveError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(BackupArchiveError::Malformed),
        }
    }

    fn u32(&mut self) -> Result<u32, BackupArchiveError> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| BackupArchiveError::Malformed)?,
        ))
    }

    fn u64(&mut self) -> Result<u64, BackupArchiveError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| BackupArchiveError::Malformed)?,
        ))
    }

    fn usize(&mut self) -> Result<usize, BackupArchiveError> {
        usize::try_from(self.u64()?).map_err(|_| BackupArchiveError::Malformed)
    }

    fn uuid(&mut self) -> Result<uuid::Uuid, BackupArchiveError> {
        Ok(uuid::Uuid::from_bytes(
            self.take(16)?
                .try_into()
                .map_err(|_| BackupArchiveError::Malformed)?,
        ))
    }

    fn digest(&mut self) -> Result<SafetyJournalDigest, BackupArchiveError> {
        Ok(SafetyJournalDigest::from_bytes(
            self.take(32)?
                .try_into()
                .map_err(|_| BackupArchiveError::Malformed)?,
        ))
    }

    fn field(&mut self) -> Result<&'a [u8], BackupArchiveError> {
        let length = self.usize()?;
        self.take(length)
    }

    fn domain(&mut self, expected: &[u8]) -> Result<(), BackupArchiveError> {
        if self.field()? == expected {
            Ok(())
        } else {
            Err(BackupArchiveError::Malformed)
        }
    }

    fn finish(self) -> Result<(), BackupArchiveError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(BackupArchiveError::Malformed)
        }
    }
}
