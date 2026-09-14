//! Immutable P05-A safety-journal values and Ed25519 receipt verification.

use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

#[cfg(not(windows))]
use std::fs::File;

use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use sha2::{Digest, Sha256};

use crate::{
    BackupArchiveStateV1, BackupSetLifecycle, FingerprintKeyContinuityProof, FingerprintKeyId,
    InstallationId, RequestId, RestoreAttemptProgress, RestoreHoldReleaseReason,
    RestoreTerminalReceipt, SourceFreezePoint, TargetActivationPlan,
};

pub const JOURNAL_ENTRY_DOMAIN: &[u8] = b"vestrace-installation-safety-journal-v1";
pub const WITNESS_RECEIPT_DOMAIN: &[u8] = b"vestrace-installation-witness-receipt-v1";
const BOOTSTRAP_RECORD_NAME: &str = "installation-safety-bootstrap-v1.cbor";
const BOOTSTRAP_RECORD_DOMAIN: &[u8] = b"vestrace-installation-safety-bootstrap-v1";

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum InstallationSafetyError {
    #[error("installation safety payload is malformed")]
    Malformed,
    #[error("installation safety signature is invalid")]
    InvalidSignature,
    #[error("installation safety identity does not match")]
    IdentityMismatch,
    #[error("installation safety sequence is not the next sequence")]
    SequenceConflict,
    #[error("installation safety witness state is not monotonic")]
    InvalidStateTransition,
}

/// Failures returned by a durable compare-and-advance witness.
#[derive(Debug, thiserror::Error)]
pub enum WitnessError {
    #[error("witness record conflicts with the expected head")]
    Conflict,
    #[error(transparent)]
    Invalid(#[from] InstallationSafetyError),
    #[error("witness storage is unavailable: {0}")]
    Unavailable(String),
}

/// Failures while creating or checking the create-only bootstrap binding.
#[derive(Debug, thiserror::Error)]
pub enum SafetyBootstrapError {
    #[error("bootstrap record differs from the first enrolled binding")]
    BindingMismatch,
    #[error("bootstrap storage is unavailable: {0}")]
    Unavailable(#[source] io::Error),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DatabaseGenerationId(uuid::Uuid);

impl DatabaseGenerationId {
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

impl Default for DatabaseGenerationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SafetyJournalDigest([u8; 32]);

impl SafetyJournalDigest {
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WitnessPublicKey([u8; 32]);

impl WitnessPublicKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The separately pinned key used to verify immutable journal entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalPublicKey([u8; 32]);

impl JournalPublicKey {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WitnessStateV1 {
    generation_lineage_state: Vec<u8>,
    backup_archive_state: BackupArchiveStateV1,
    target_activation_plan: Option<TargetActivationPlan>,
    restore_attempt_progress: Option<RestoreAttemptProgress>,
    target_activation_started: Option<SafetyJournalDigest>,
}

impl WitnessStateV1 {
    pub fn genesis(generation: DatabaseGenerationId) -> Self {
        Self {
            generation_lineage_state: generation.as_uuid().as_bytes().to_vec(),
            backup_archive_state: BackupArchiveStateV1::empty(),
            target_activation_plan: None,
            restore_attempt_progress: None,
            target_activation_started: None,
        }
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        field(&mut out, &self.generation_lineage_state);
        field(&mut out, &self.backup_archive_state.canonical_bytes());
        // Kept empty at its frozen P05-A field position. The two former raw
        // archive slots now have one versioned, validated representation.
        field(&mut out, &[]);
        optional_field(
            &mut out,
            self.target_activation_plan
                .as_ref()
                .map(TargetActivationPlan::canonical_bytes)
                .as_deref(),
        );
        optional_field(
            &mut out,
            self.restore_attempt_progress
                .as_ref()
                .map(RestoreAttemptProgress::canonical_bytes)
                .as_deref(),
        );
        // Omit an absent P05-C activation marker so pre-marker states retain
        // their exact signed encoding when reconstructed from durable media.
        if let Some(digest) = self.target_activation_started {
            optional_field(&mut out, Some(digest.as_bytes()));
        }
        out
    }

    /// Decodes the fixed P05 witness state. The old all-empty P05-A archive
    /// slots reopen as the archive-free P05-B state; all nonempty states use
    /// the versioned archive encoding.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, InstallationSafetyError> {
        let mut decoder = CanonicalDecoder::new(bytes);
        let generation_lineage_state = decoder.field()?.to_vec();
        let archive_slot = decoder.field()?;
        let retired_archive_slot = decoder.field()?;
        let target_activation_plan = decoder
            .optional_field()?
            .map(TargetActivationPlan::from_canonical_bytes)
            .transpose()
            .map_err(|_| InstallationSafetyError::Malformed)?;
        let restore_attempt_progress = decoder
            .optional_field()?
            .map(RestoreAttemptProgress::from_canonical_bytes)
            .transpose()
            .map_err(|_| InstallationSafetyError::Malformed)?;
        let target_activation_started = if decoder.is_finished() {
            None
        } else {
            Some(
                decoder
                    .optional_field()?
                    .ok_or(InstallationSafetyError::Malformed)
                    .and_then(|bytes| {
                        bytes
                            .try_into()
                            .map(SafetyJournalDigest::from_bytes)
                            .map_err(|_| InstallationSafetyError::Malformed)
                    })?,
            )
        };
        decoder.finish()?;
        if generation_lineage_state.len() < 16 || generation_lineage_state.len() % 16 != 0 {
            return Err(InstallationSafetyError::Malformed);
        }
        let backup_archive_state = if archive_slot.is_empty() && retired_archive_slot.is_empty() {
            BackupArchiveStateV1::empty()
        } else if retired_archive_slot.is_empty() {
            BackupArchiveStateV1::from_canonical_bytes(archive_slot)
                .map_err(|_| InstallationSafetyError::Malformed)?
        } else {
            return Err(InstallationSafetyError::Malformed);
        };
        Ok(Self {
            generation_lineage_state,
            backup_archive_state,
            target_activation_plan,
            restore_attempt_progress,
            target_activation_started,
        })
    }

    /// Extends the fixed-width lineage without changing any other P05-A state
    /// section. A caller cannot rewrite or truncate existing ancestry.
    pub fn register_generation(
        &self,
        next_generation: DatabaseGenerationId,
    ) -> Result<Self, InstallationSafetyError> {
        let current = self
            .generation_lineage_state
            .get(self.generation_lineage_state.len().saturating_sub(16)..)
            .ok_or(InstallationSafetyError::Malformed)?;
        if current.len() != 16 || current == next_generation.as_uuid().as_bytes() {
            return Err(InstallationSafetyError::InvalidStateTransition);
        }
        let mut next = self.clone();
        next.generation_lineage_state
            .extend_from_slice(next_generation.as_uuid().as_bytes());
        Ok(next)
    }

    /// Produces a candidate archive state for a separately signed P05-B
    /// transition. Witness validation decides which event may accept it.
    pub fn with_backup_archive_state(&self, backup_archive_state: BackupArchiveStateV1) -> Self {
        Self {
            generation_lineage_state: self.generation_lineage_state.clone(),
            backup_archive_state,
            target_activation_plan: self.target_activation_plan.clone(),
            restore_attempt_progress: self.restore_attempt_progress.clone(),
            target_activation_started: self.target_activation_started,
        }
    }

    pub fn backup_archive_state(&self) -> &BackupArchiveStateV1 {
        &self.backup_archive_state
    }

    /// Pins exactly one immutable activation plan into the frozen P05-A slot.
    pub fn with_target_activation_plan(
        &self,
        plan: TargetActivationPlan,
    ) -> Result<Self, InstallationSafetyError> {
        let Some(progress) = &self.restore_attempt_progress else {
            return Err(InstallationSafetyError::InvalidStateTransition);
        };
        if self.target_activation_plan.is_some()
            || progress.terminal_receipt().is_some()
            || progress.source_freeze() != Some(plan.source_freeze())
            || progress.attempt_id() != plan.attempt_id()
            || progress.target_id() != plan.target_id()
            || progress.backup_set_id() != plan.backup_set_id()
            || progress.source_generation_id() != plan.source_generation_id()
            || progress.target_generation_id() != plan.target_generation_id()
        {
            return Err(InstallationSafetyError::InvalidStateTransition);
        }
        let mut next = self.clone();
        next.target_activation_plan = Some(plan);
        Ok(next)
    }

    /// Starts one witnessed restore attempt before any source freeze is recorded.
    pub fn with_restore_attempt_progress(
        &self,
        progress: RestoreAttemptProgress,
    ) -> Result<Self, InstallationSafetyError> {
        if self.restore_attempt_progress.is_some()
            || progress.source_freeze().is_some()
            || progress.terminal_receipt().is_some()
        {
            return Err(InstallationSafetyError::InvalidStateTransition);
        }
        let mut next = self.clone();
        next.restore_attempt_progress = Some(progress);
        Ok(next)
    }

    pub fn with_source_freeze(
        &self,
        source_freeze: SourceFreezePoint,
    ) -> Result<Self, InstallationSafetyError> {
        let progress = self
            .restore_attempt_progress
            .as_ref()
            .ok_or(InstallationSafetyError::InvalidStateTransition)?
            .record_source_freeze(source_freeze)
            .map_err(|_| InstallationSafetyError::InvalidStateTransition)?;
        let mut next = self.clone();
        next.restore_attempt_progress = Some(progress);
        Ok(next)
    }

    /// Records one terminal receipt only for the already frozen restore attempt.
    pub fn with_restore_terminal_receipt(
        &self,
        receipt: RestoreTerminalReceipt,
    ) -> Result<Self, InstallationSafetyError> {
        let progress = self
            .restore_attempt_progress
            .as_ref()
            .ok_or(InstallationSafetyError::InvalidStateTransition)?
            .record_terminal_receipt(receipt)
            .map_err(|_| InstallationSafetyError::InvalidStateTransition)?;
        let mut next = self.clone();
        next.restore_attempt_progress = Some(progress);
        Ok(next)
    }

    /// Marks the only activation CAS attempt for the immutable target plan.
    /// The optional marker is persisted only once a target receipt has already
    /// bound the plan, making a restart before the SQL CAS distinguishable.
    pub fn with_target_activation_started(&self) -> Result<Self, InstallationSafetyError> {
        let plan = self
            .target_activation_plan
            .as_ref()
            .ok_or(InstallationSafetyError::InvalidStateTransition)?;
        let terminal = self
            .restore_terminal_receipt()
            .ok_or(InstallationSafetyError::InvalidStateTransition)?;
        if self.target_activation_started.is_some()
            || terminal.attempt_id() != plan.attempt_id()
            || terminal.target_id() != plan.target_id()
            || !matches!(
                terminal.release_reason(),
                RestoreHoldReleaseReason::TargetInitializationComplete { .. }
            )
        {
            return Err(InstallationSafetyError::InvalidStateTransition);
        }
        let mut next = self.clone();
        next.target_activation_started = Some(plan.digest());
        Ok(next)
    }

    pub fn target_activation_plan(&self) -> Option<&TargetActivationPlan> {
        self.target_activation_plan.as_ref()
    }

    pub const fn target_activation_started(&self) -> Option<SafetyJournalDigest> {
        self.target_activation_started
    }

    pub fn restore_terminal_receipt(&self) -> Option<&RestoreTerminalReceipt> {
        self.restore_attempt_progress
            .as_ref()
            .and_then(RestoreAttemptProgress::terminal_receipt)
    }

    pub fn restore_attempt_progress(&self) -> Option<&RestoreAttemptProgress> {
        self.restore_attempt_progress.as_ref()
    }

    fn is_exact_target_activation_plan_successor_of(&self, previous: &Self) -> bool {
        self.generation_lineage_state == previous.generation_lineage_state
            && self.backup_archive_state == previous.backup_archive_state
            && previous.target_activation_plan.is_none()
            && self.target_activation_plan.is_some()
            && self.restore_attempt_progress == previous.restore_attempt_progress
            && self.target_activation_started == previous.target_activation_started
    }

    fn is_exact_restore_terminal_successor_of(&self, previous: &Self) -> bool {
        self.generation_lineage_state == previous.generation_lineage_state
            && self.backup_archive_state == previous.backup_archive_state
            && self.target_activation_plan == previous.target_activation_plan
            && self.target_activation_started == previous.target_activation_started
            && matches!(
                (&previous.restore_attempt_progress, &self.restore_attempt_progress),
                (Some(previous), Some(next))
                    if previous.terminal_receipt().is_none()
                        && next.terminal_receipt().is_some()
                        && previous
                            .record_terminal_receipt(next.terminal_receipt().unwrap().clone())
                            .is_ok_and(|expected| expected == *next)
            )
    }

    fn is_exact_restore_attempt_prepared_successor_of(&self, previous: &Self) -> bool {
        self.generation_lineage_state == previous.generation_lineage_state
            && self.backup_archive_state == previous.backup_archive_state
            && self.target_activation_plan == previous.target_activation_plan
            && self.target_activation_started == previous.target_activation_started
            && previous.restore_attempt_progress.is_none()
            && self
                .restore_attempt_progress
                .as_ref()
                .is_some_and(|progress| {
                    progress.source_freeze().is_none() && progress.terminal_receipt().is_none()
                })
    }

    fn is_exact_source_freeze_successor_of(&self, previous: &Self) -> bool {
        self.generation_lineage_state == previous.generation_lineage_state
            && self.backup_archive_state == previous.backup_archive_state
            && self.target_activation_plan == previous.target_activation_plan
            && self.target_activation_started == previous.target_activation_started
            && matches!(
                (&previous.restore_attempt_progress, &self.restore_attempt_progress),
                (Some(previous), Some(next))
                    if previous.source_freeze().is_none()
                        && next.source_freeze().is_some()
                        && next.terminal_receipt().is_none()
                        && previous
                            .record_source_freeze(next.source_freeze().unwrap())
                            .is_ok_and(|expected| expected == *next)
            )
    }

    fn is_exact_target_activating_successor_of(&self, previous: &Self) -> bool {
        self.generation_lineage_state == previous.generation_lineage_state
            && self.backup_archive_state == previous.backup_archive_state
            && self.target_activation_plan == previous.target_activation_plan
            && self.restore_attempt_progress == previous.restore_attempt_progress
            && previous.target_activation_started.is_none()
            && self.target_activation_started
                == previous
                    .target_activation_plan
                    .as_ref()
                    .map(TargetActivationPlan::digest)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyEventKind {
    InstallationInitialized,
    GenerationRegistered,
    ArchiveCheckpointCommitted,
    BackupSetStarted,
    ArchiveRestoreHoldAcquired,
    ArchiveSealingStarted,
    ArchiveSealed,
    ManagedBackupDeletionPrepared,
    PreparedArchiveKeyErasure,
    ArchiveKeyErased,
    ManagedBackupDeleted,
    TargetActivationPlanned,
    RestoreTerminalRecorded,
    RestoreAttemptPrepared,
    SourceFreezeRecorded,
    TargetActivating,
}

#[derive(Clone, Eq, PartialEq)]
pub struct SignedJournalEntry {
    installation_id: InstallationId,
    fingerprint_key_id: FingerprintKeyId,
    continuity_proof: FingerprintKeyContinuityProof,
    request_id: RequestId,
    sequence: u64,
    previous_digest: SafetyJournalDigest,
    event_kind: SafetyEventKind,
    generation_id: DatabaseGenerationId,
    activation_epoch: u64,
    state: WitnessStateV1,
    signer_public_key: JournalPublicKey,
    signature: [u8; 64],
}

/// Complete immutable input for an Ed25519 journal entry signature.
///
/// Keeping the canonical fields together makes it explicit that signing does
/// not synthesize or omit a field from the durable journal representation.
#[derive(Clone, Eq, PartialEq)]
pub struct JournalEntryToSign {
    pub installation_id: InstallationId,
    pub fingerprint_key_id: FingerprintKeyId,
    pub continuity_proof: FingerprintKeyContinuityProof,
    pub request_id: RequestId,
    pub sequence: u64,
    pub previous_digest: SafetyJournalDigest,
    pub event_kind: SafetyEventKind,
    pub generation_id: DatabaseGenerationId,
    pub activation_epoch: u64,
    pub state: WitnessStateV1,
}

impl SignedJournalEntry {
    pub fn sign(signer: &Ed25519KeyPair, input: JournalEntryToSign) -> Self {
        let JournalEntryToSign {
            installation_id,
            fingerprint_key_id,
            continuity_proof,
            request_id,
            sequence,
            previous_digest,
            event_kind,
            generation_id,
            activation_epoch,
            state,
        } = input;
        let signer_public_key = JournalPublicKey::from_bytes(
            signer
                .public_key()
                .as_ref()
                .try_into()
                .expect("Ed25519 public key length"),
        );
        let mut entry = Self {
            installation_id,
            fingerprint_key_id,
            continuity_proof,
            request_id,
            sequence,
            previous_digest,
            event_kind,
            generation_id,
            activation_epoch,
            state,
            signer_public_key,
            signature: [0; 64],
        };
        entry.signature = signer
            .sign(&entry.canonical_bytes())
            .as_ref()
            .try_into()
            .expect("Ed25519 signature length");
        entry
    }

    pub fn verify(&self) -> Result<(), InstallationSafetyError> {
        UnparsedPublicKey::new(&ED25519, self.signer_public_key.as_bytes())
            .verify(&self.canonical_bytes(), &self.signature)
            .map_err(|_| InstallationSafetyError::InvalidSignature)
    }

    /// Reconstructs an immutable entry only after parsing its complete
    /// canonical payload and verifying its detached signature.
    pub fn from_canonical_bytes_and_signature(
        bytes: &[u8],
        signature: [u8; 64],
    ) -> Result<Self, InstallationSafetyError> {
        let mut decoder = CanonicalDecoder::new(bytes);
        decoder.domain(JOURNAL_ENTRY_DOMAIN)?;
        let installation_id = InstallationId::from_uuid(decoder.uuid()?);
        let fingerprint_key_id = FingerprintKeyId::from_uuid(decoder.uuid()?);
        let continuity_proof = FingerprintKeyContinuityProof::from_bytes(decoder.fixed()?);
        let request_id = RequestId::from_uuid(decoder.uuid()?);
        let sequence = decoder.u64()?;
        let previous_digest = SafetyJournalDigest(decoder.fixed()?);
        let event_kind = match decoder.take(1)? {
            [0] => SafetyEventKind::InstallationInitialized,
            [1] => SafetyEventKind::GenerationRegistered,
            [2] => SafetyEventKind::ArchiveCheckpointCommitted,
            [3] => SafetyEventKind::BackupSetStarted,
            [4] => SafetyEventKind::ArchiveRestoreHoldAcquired,
            [5] => SafetyEventKind::ArchiveSealingStarted,
            [6] => SafetyEventKind::ArchiveSealed,
            [7] => SafetyEventKind::ManagedBackupDeletionPrepared,
            [8] => SafetyEventKind::PreparedArchiveKeyErasure,
            [9] => SafetyEventKind::ArchiveKeyErased,
            [10] => SafetyEventKind::ManagedBackupDeleted,
            [11] => SafetyEventKind::TargetActivationPlanned,
            [12] => SafetyEventKind::RestoreTerminalRecorded,
            [13] => SafetyEventKind::RestoreAttemptPrepared,
            [14] => SafetyEventKind::SourceFreezeRecorded,
            [15] => SafetyEventKind::TargetActivating,
            _ => return Err(InstallationSafetyError::Malformed),
        };
        let generation_id = DatabaseGenerationId::from_uuid(decoder.uuid()?);
        let activation_epoch = decoder.u64()?;
        let state = WitnessStateV1::from_canonical_bytes(decoder.field()?)?;
        let signer_public_key = JournalPublicKey::from_bytes(decoder.fixed()?);
        decoder.finish()?;
        let entry = Self {
            installation_id,
            fingerprint_key_id,
            continuity_proof,
            request_id,
            sequence,
            previous_digest,
            event_kind,
            generation_id,
            activation_epoch,
            state,
            signer_public_key,
            signature,
        };
        entry.verify()?;
        Ok(entry)
    }

    pub fn digest(&self) -> SafetyJournalDigest {
        SafetyJournalDigest::of(&self.canonical_bytes())
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub const fn previous_digest(&self) -> SafetyJournalDigest {
        self.previous_digest
    }
    pub fn installation_id(&self) -> InstallationId {
        self.installation_id
    }
    pub fn fingerprint_key_id(&self) -> FingerprintKeyId {
        self.fingerprint_key_id
    }
    pub fn continuity_proof(&self) -> &FingerprintKeyContinuityProof {
        &self.continuity_proof
    }
    pub fn request_id(&self) -> RequestId {
        self.request_id
    }
    pub fn generation_id(&self) -> DatabaseGenerationId {
        self.generation_id
    }
    pub const fn event_kind(&self) -> SafetyEventKind {
        self.event_kind
    }
    pub fn activation_epoch(&self) -> u64 {
        self.activation_epoch
    }
    pub fn state(&self) -> &WitnessStateV1 {
        &self.state
    }
    pub fn signer_public_key(&self) -> &JournalPublicKey {
        &self.signer_public_key
    }
    pub const fn signature(&self) -> &[u8; 64] {
        &self.signature
    }
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        field(&mut out, JOURNAL_ENTRY_DOMAIN);
        field(&mut out, self.installation_id.as_uuid().as_bytes());
        field(&mut out, self.fingerprint_key_id.as_uuid().as_bytes());
        field(&mut out, self.continuity_proof.as_bytes());
        field(&mut out, self.request_id.as_uuid().as_bytes());
        out.extend_from_slice(&self.sequence.to_be_bytes());
        field(&mut out, self.previous_digest.as_bytes());
        out.push(match self.event_kind {
            SafetyEventKind::InstallationInitialized => 0,
            SafetyEventKind::GenerationRegistered => 1,
            SafetyEventKind::ArchiveCheckpointCommitted => 2,
            SafetyEventKind::BackupSetStarted => 3,
            SafetyEventKind::ArchiveRestoreHoldAcquired => 4,
            SafetyEventKind::ArchiveSealingStarted => 5,
            SafetyEventKind::ArchiveSealed => 6,
            SafetyEventKind::ManagedBackupDeletionPrepared => 7,
            SafetyEventKind::PreparedArchiveKeyErasure => 8,
            SafetyEventKind::ArchiveKeyErased => 9,
            SafetyEventKind::ManagedBackupDeleted => 10,
            SafetyEventKind::TargetActivationPlanned => 11,
            SafetyEventKind::RestoreTerminalRecorded => 12,
            SafetyEventKind::RestoreAttemptPrepared => 13,
            SafetyEventKind::SourceFreezeRecorded => 14,
            SafetyEventKind::TargetActivating => 15,
        });
        field(&mut out, self.generation_id.as_uuid().as_bytes());
        out.extend_from_slice(&self.activation_epoch.to_be_bytes());
        field(&mut out, &self.state.canonical_bytes());
        field(&mut out, self.signer_public_key.as_bytes());
        out
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct WitnessHead {
    installation_id: InstallationId,
    fingerprint_key_id: FingerprintKeyId,
    continuity_proof: FingerprintKeyContinuityProof,
    sequence: u64,
    chain_digest: SafetyJournalDigest,
    active_generation_id: DatabaseGenerationId,
    activation_epoch: u64,
    state: WitnessStateV1,
    journal_public_key: JournalPublicKey,
}

impl WitnessHead {
    pub fn genesis(
        installation_id: InstallationId,
        fingerprint_key_id: FingerprintKeyId,
        continuity_proof: FingerprintKeyContinuityProof,
        generation_id: DatabaseGenerationId,
        journal_public_key: JournalPublicKey,
    ) -> Self {
        Self {
            installation_id,
            fingerprint_key_id,
            continuity_proof,
            sequence: 0,
            chain_digest: SafetyJournalDigest::of(b"vestrace-installation-safety-genesis-v1"),
            active_generation_id: generation_id,
            activation_epoch: 0,
            state: WitnessStateV1::genesis(generation_id),
            journal_public_key,
        }
    }

    pub fn accept_signed(
        &self,
        entry: &SignedJournalEntry,
    ) -> Result<WitnessAdvance, InstallationSafetyError> {
        entry.verify()?;
        if entry.sequence
            != self
                .sequence
                .checked_add(1)
                .ok_or(InstallationSafetyError::SequenceConflict)?
            || entry.installation_id != self.installation_id
            || entry.fingerprint_key_id != self.fingerprint_key_id
            || entry.continuity_proof != self.continuity_proof
            || entry.previous_digest != self.chain_digest
            || entry.signer_public_key != self.journal_public_key
        {
            return Err(InstallationSafetyError::IdentityMismatch);
        }
        match entry.event_kind {
            SafetyEventKind::InstallationInitialized
                if entry.state != self.state
                    || entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::GenerationRegistered
                if entry.generation_id == self.active_generation_id
                    || entry.activation_epoch
                        != self
                            .activation_epoch
                            .checked_add(1)
                            .ok_or(InstallationSafetyError::InvalidStateTransition)?
                    || entry.state != self.state.register_generation(entry.generation_id)? =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::GenerationRegistered
                if self
                    .state
                    .restore_attempt_progress()
                    .is_some_and(|attempt| {
                        attempt.source_freeze().is_some() && attempt.terminal_receipt().is_none()
                    })
                    && self.state.target_activation_plan().is_none() =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::GenerationRegistered
                if self.state.target_activation_plan().is_some_and(|plan| {
                    !matches!(
                        self.state.restore_terminal_receipt(),
                        Some(receipt)
                            if receipt.attempt_id() == plan.attempt_id()
                                && receipt.target_id() == plan.target_id()
                                && matches!(
                                    receipt.release_reason(),
                                    RestoreHoldReleaseReason::TargetInitializationComplete { .. }
                                )
                    ) || entry.generation_id != plan.target_generation_id()
                        || entry.state.target_activation_plan() != Some(plan)
                        || self.state.target_activation_started() != Some(plan.digest())
                }) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::ArchiveCheckpointCommitted
                if entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch
                    || !entry
                        .state
                        .backup_archive_state()
                        .is_exact_checkpoint_successor_of(self.state.backup_archive_state()) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::BackupSetStarted
                if entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch
                    || !entry
                        .state
                        .backup_archive_state()
                        .is_exact_started_successor_of(self.state.backup_archive_state()) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::ArchiveRestoreHoldAcquired
                if entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch
                    || !entry
                        .state
                        .backup_archive_state()
                        .is_exact_hold_successor_of(self.state.backup_archive_state()) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::ArchiveSealingStarted
                if !exact_archive_lifecycle_transition(
                    self,
                    entry,
                    BackupSetLifecycle::Streaming,
                    BackupSetLifecycle::Sealing,
                ) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::ArchiveSealed
                if !exact_archive_lifecycle_transition(
                    self,
                    entry,
                    BackupSetLifecycle::Sealing,
                    BackupSetLifecycle::Sealed,
                ) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::ManagedBackupDeletionPrepared
                if entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch
                    || !entry
                        .state
                        .backup_archive_state()
                        .is_exact_prepared_deletion_successor_of(
                            self.state.backup_archive_state(),
                        ) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::PreparedArchiveKeyErasure
                if entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch
                    || !entry
                        .state
                        .backup_archive_state()
                        .is_exact_key_erasure_prepared_successor_of(
                            self.state.backup_archive_state(),
                        ) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::ArchiveKeyErased
                if !exact_archive_lifecycle_transition(
                    self,
                    entry,
                    BackupSetLifecycle::DeletionPrepared,
                    BackupSetLifecycle::ArchiveKeyErased,
                ) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::ManagedBackupDeleted
                if !exact_archive_lifecycle_transition(
                    self,
                    entry,
                    BackupSetLifecycle::ArchiveKeyErased,
                    BackupSetLifecycle::Deleted,
                ) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::TargetActivationPlanned
                if entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch
                    || !entry
                        .state
                        .is_exact_target_activation_plan_successor_of(&self.state) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::RestoreTerminalRecorded
                if entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch
                    || !entry
                        .state
                        .is_exact_restore_terminal_successor_of(&self.state) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::RestoreAttemptPrepared
                if entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch
                    || !entry
                        .state
                        .is_exact_restore_attempt_prepared_successor_of(&self.state) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::SourceFreezeRecorded
                if entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch
                    || !entry.state.is_exact_source_freeze_successor_of(&self.state) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            SafetyEventKind::TargetActivating
                if entry.generation_id != self.active_generation_id
                    || entry.activation_epoch != self.activation_epoch
                    || !entry
                        .state
                        .is_exact_target_activating_successor_of(&self.state) =>
            {
                return Err(InstallationSafetyError::InvalidStateTransition);
            }
            _ => {}
        }
        Ok(WitnessAdvance {
            event_kind: entry.event_kind,
            generation_id: entry.generation_id,
            activation_epoch: entry.activation_epoch,
            signed_entry: entry.clone(),
            next_state: entry.state.clone(),
        })
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn chain_digest(&self) -> SafetyJournalDigest {
        self.chain_digest
    }
    pub fn journal_public_key(&self) -> &JournalPublicKey {
        &self.journal_public_key
    }
    pub fn state(&self) -> &WitnessStateV1 {
        &self.state
    }

    pub const fn installation_id(&self) -> InstallationId {
        self.installation_id
    }
    pub const fn fingerprint_key_id(&self) -> FingerprintKeyId {
        self.fingerprint_key_id
    }
    pub fn continuity_proof(&self) -> &FingerprintKeyContinuityProof {
        &self.continuity_proof
    }
    pub const fn active_generation_id(&self) -> DatabaseGenerationId {
        self.active_generation_id
    }
    pub const fn activation_epoch(&self) -> u64 {
        self.activation_epoch
    }

    /// Reopens a durable witness only from its create-only binding and a
    /// receipt signed by the binding's pinned witness key.
    pub fn from_durable_receipt(
        binding: &SafetyBootstrapBinding,
        receipt: &WitnessReceipt,
    ) -> Result<Self, InstallationSafetyError> {
        receipt.verify_against(binding.witness_public_key())?;
        if receipt.sequence == 0
            || receipt.installation_id != binding.installation_id
            || receipt.fingerprint_key_id != binding.fingerprint_key_id
            || receipt.continuity_proof != binding.continuity_proof
        {
            return Err(InstallationSafetyError::IdentityMismatch);
        }
        Ok(Self {
            installation_id: receipt.installation_id,
            fingerprint_key_id: receipt.fingerprint_key_id,
            continuity_proof: receipt.continuity_proof.clone(),
            sequence: receipt.sequence,
            chain_digest: receipt.journal_digest,
            active_generation_id: receipt.generation_id,
            activation_epoch: receipt.activation_epoch,
            state: receipt.state.clone(),
            journal_public_key: binding.journal_public_key.clone(),
        })
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct WitnessAdvance {
    event_kind: SafetyEventKind,
    generation_id: DatabaseGenerationId,
    activation_epoch: u64,
    signed_entry: SignedJournalEntry,
    next_state: WitnessStateV1,
}

impl WitnessAdvance {
    pub fn signed_entry(&self) -> &SignedJournalEntry {
        &self.signed_entry
    }
}

fn exact_archive_lifecycle_transition(
    head: &WitnessHead,
    entry: &SignedJournalEntry,
    from: BackupSetLifecycle,
    to: BackupSetLifecycle,
) -> bool {
    entry.generation_id == head.active_generation_id
        && entry.activation_epoch == head.activation_epoch
        && entry
            .state
            .backup_archive_state()
            .is_exact_lifecycle_successor_of(head.state.backup_archive_state(), from, to)
}

#[derive(Clone, Eq, PartialEq)]
pub struct WitnessReceipt {
    installation_id: InstallationId,
    fingerprint_key_id: FingerprintKeyId,
    continuity_proof: FingerprintKeyContinuityProof,
    sequence: u64,
    journal_digest: SafetyJournalDigest,
    generation_id: DatabaseGenerationId,
    activation_epoch: u64,
    state: WitnessStateV1,
    witness_public_key: WitnessPublicKey,
    signature: [u8; 64],
}

impl WitnessReceipt {
    pub fn sign(
        witness: &Ed25519KeyPair,
        head: &WitnessHead,
        advance: WitnessAdvance,
    ) -> Result<Self, InstallationSafetyError> {
        if advance.signed_entry.sequence
            != head
                .sequence
                .checked_add(1)
                .ok_or(InstallationSafetyError::SequenceConflict)?
        {
            return Err(InstallationSafetyError::SequenceConflict);
        }
        let witness_public_key = WitnessPublicKey::from_bytes(
            witness
                .public_key()
                .as_ref()
                .try_into()
                .expect("Ed25519 public key length"),
        );
        let journal_digest = advance.signed_entry.digest();
        let mut receipt = Self {
            installation_id: head.installation_id,
            fingerprint_key_id: head.fingerprint_key_id,
            continuity_proof: head.continuity_proof.clone(),
            sequence: advance.signed_entry.sequence,
            journal_digest,
            generation_id: advance.generation_id,
            activation_epoch: advance.activation_epoch,
            state: advance.next_state,
            witness_public_key,
            signature: [0; 64],
        };
        receipt.signature = witness
            .sign(&receipt.canonical_bytes())
            .as_ref()
            .try_into()
            .expect("Ed25519 signature length");
        Ok(receipt)
    }

    pub fn verify_against(
        &self,
        expected: &WitnessPublicKey,
    ) -> Result<(), InstallationSafetyError> {
        if &self.witness_public_key != expected {
            return Err(InstallationSafetyError::IdentityMismatch);
        }
        UnparsedPublicKey::new(&ED25519, expected.as_bytes())
            .verify(&self.canonical_bytes(), &self.signature)
            .map_err(|_| InstallationSafetyError::InvalidSignature)
    }

    /// Reconstructs a receipt only after its complete canonical payload and
    /// detached signature are well formed and self-verifying. Callers that
    /// persist it must additionally pin it with `verify_against`.
    pub fn from_canonical_bytes_and_signature(
        bytes: &[u8],
        signature: [u8; 64],
    ) -> Result<Self, InstallationSafetyError> {
        let mut decoder = CanonicalDecoder::new(bytes);
        decoder.domain(WITNESS_RECEIPT_DOMAIN)?;
        let installation_id = InstallationId::from_uuid(decoder.uuid()?);
        let fingerprint_key_id = FingerprintKeyId::from_uuid(decoder.uuid()?);
        let continuity_proof = FingerprintKeyContinuityProof::from_bytes(decoder.fixed()?);
        let sequence = decoder.u64()?;
        let journal_digest = SafetyJournalDigest(decoder.fixed()?);
        let generation_id = DatabaseGenerationId::from_uuid(decoder.uuid()?);
        let activation_epoch = decoder.u64()?;
        let state = WitnessStateV1::from_canonical_bytes(decoder.field()?)?;
        let witness_public_key = WitnessPublicKey::from_bytes(decoder.fixed()?);
        decoder.finish()?;
        let receipt = Self {
            installation_id,
            fingerprint_key_id,
            continuity_proof,
            sequence,
            journal_digest,
            generation_id,
            activation_epoch,
            state,
            witness_public_key,
            signature,
        };
        receipt.verify_against(&receipt.witness_public_key)?;
        Ok(receipt)
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        field(&mut out, WITNESS_RECEIPT_DOMAIN);
        field(&mut out, self.installation_id.as_uuid().as_bytes());
        field(&mut out, self.fingerprint_key_id.as_uuid().as_bytes());
        field(&mut out, self.continuity_proof.as_bytes());
        out.extend_from_slice(&self.sequence.to_be_bytes());
        field(&mut out, self.journal_digest.as_bytes());
        field(&mut out, self.generation_id.as_uuid().as_bytes());
        out.extend_from_slice(&self.activation_epoch.to_be_bytes());
        field(&mut out, &self.state.canonical_bytes());
        field(&mut out, self.witness_public_key.as_bytes());
        out
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn journal_digest(&self) -> SafetyJournalDigest {
        self.journal_digest
    }
    pub fn installation_id(&self) -> InstallationId {
        self.installation_id
    }
    pub fn fingerprint_key_id(&self) -> FingerprintKeyId {
        self.fingerprint_key_id
    }
    pub fn continuity_proof(&self) -> &FingerprintKeyContinuityProof {
        &self.continuity_proof
    }
    pub fn generation_id(&self) -> DatabaseGenerationId {
        self.generation_id
    }
    pub fn activation_epoch(&self) -> u64 {
        self.activation_epoch
    }
    pub fn state(&self) -> &WitnessStateV1 {
        &self.state
    }
    pub fn signature(&self) -> &[u8; 64] {
        &self.signature
    }
    pub fn witness_public_key(&self) -> &WitnessPublicKey {
        &self.witness_public_key
    }
}

/// The values bound exactly once by the host-custody bootstrap record.
#[derive(Clone, Eq, PartialEq)]
pub struct SafetyBootstrapBinding {
    installation_id: InstallationId,
    fingerprint_key_id: FingerprintKeyId,
    continuity_proof: FingerprintKeyContinuityProof,
    journal_public_key: JournalPublicKey,
    witness_public_key: WitnessPublicKey,
}

impl SafetyBootstrapBinding {
    pub fn new(
        installation_id: InstallationId,
        fingerprint_key_id: FingerprintKeyId,
        continuity_proof: FingerprintKeyContinuityProof,
        journal_public_key: JournalPublicKey,
        witness_public_key: WitnessPublicKey,
    ) -> Self {
        Self {
            installation_id,
            fingerprint_key_id,
            continuity_proof,
            journal_public_key,
            witness_public_key,
        }
    }

    pub const fn installation_id(&self) -> InstallationId {
        self.installation_id
    }

    pub const fn fingerprint_key_id(&self) -> FingerprintKeyId {
        self.fingerprint_key_id
    }

    pub fn continuity_proof(&self) -> &FingerprintKeyContinuityProof {
        &self.continuity_proof
    }

    pub fn journal_public_key(&self) -> &JournalPublicKey {
        &self.journal_public_key
    }

    pub fn witness_public_key(&self) -> &WitnessPublicKey {
        &self.witness_public_key
    }

    fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        field(&mut bytes, BOOTSTRAP_RECORD_DOMAIN);
        field(&mut bytes, self.installation_id.as_uuid().as_bytes());
        field(&mut bytes, self.fingerprint_key_id.as_uuid().as_bytes());
        field(&mut bytes, self.continuity_proof.as_bytes());
        field(&mut bytes, self.journal_public_key.as_bytes());
        field(&mut bytes, self.witness_public_key.as_bytes());
        bytes
    }

    /// Reconstructs only the fixed canonical create-only binding representation.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, InstallationSafetyError> {
        let mut decoder = CanonicalDecoder::new(bytes);
        decoder.domain(BOOTSTRAP_RECORD_DOMAIN)?;
        let installation_id = InstallationId::from_uuid(decoder.uuid()?);
        let fingerprint_key_id = FingerprintKeyId::from_uuid(decoder.uuid()?);
        let continuity_proof = FingerprintKeyContinuityProof::from_bytes(decoder.fixed()?);
        let journal_public_key = JournalPublicKey::from_bytes(decoder.fixed()?);
        let witness_public_key = WitnessPublicKey::from_bytes(decoder.fixed()?);
        decoder.finish()?;
        Ok(Self {
            installation_id,
            fingerprint_key_id,
            continuity_proof,
            journal_public_key,
            witness_public_key,
        })
    }

    pub fn canonical_record_bytes(&self) -> Vec<u8> {
        self.canonical_bytes()
    }

    pub fn digest(&self) -> SafetyJournalDigest {
        SafetyJournalDigest::of(&self.canonical_bytes())
    }
}

/// Create-only host-custody enrollment. Existing bytes must exactly match the
/// proposed first binding; reopening exposes only the resulting digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyBootstrapRecord {
    digest: SafetyJournalDigest,
}

impl SafetyBootstrapRecord {
    pub fn open_or_create(
        root: &Path,
        binding: &SafetyBootstrapBinding,
    ) -> Result<Self, SafetyBootstrapError> {
        fs::create_dir_all(root).map_err(SafetyBootstrapError::Unavailable)?;
        let path = Self::path(root);
        let expected = binding.canonical_bytes();
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(&expected)
                    .map_err(SafetyBootstrapError::Unavailable)?;
                file.sync_all().map_err(SafetyBootstrapError::Unavailable)?;
                sync_parent_directory(root)?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if fs::read(&path).map_err(SafetyBootstrapError::Unavailable)? != expected {
                    return Err(SafetyBootstrapError::BindingMismatch);
                }
            }
            Err(error) => return Err(SafetyBootstrapError::Unavailable(error)),
        }
        Ok(Self {
            digest: SafetyJournalDigest::of(&expected),
        })
    }

    pub const fn digest(&self) -> SafetyJournalDigest {
        self.digest
    }

    pub fn path(root: &Path) -> PathBuf {
        root.join(BOOTSTRAP_RECORD_NAME)
    }
}

fn sync_parent_directory(root: &Path) -> Result<(), SafetyBootstrapError> {
    #[cfg(not(windows))]
    {
        File::open(root)
            .and_then(|directory| directory.sync_all())
            .map_err(SafetyBootstrapError::Unavailable)
    }
    #[cfg(windows)]
    {
        // FILE_FLAG_BACKUP_SEMANTICS permits an ordinary Win32 file handle to
        // address a directory, which lets `sync_all` issue FlushFileBuffers.
        OpenOptions::new()
            .read(true)
            .custom_flags(0x0200_0000)
            .open(root)
            .and_then(|directory| directory.sync_all())
            .map_err(SafetyBootstrapError::Unavailable)
    }
}

fn field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}
fn optional_field(out: &mut Vec<u8>, bytes: Option<&[u8]>) {
    out.push(u8::from(bytes.is_some()));
    if let Some(bytes) = bytes {
        field(out, bytes);
    }
}

struct CanonicalDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> CanonicalDecoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], InstallationSafetyError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(InstallationSafetyError::Malformed)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(InstallationSafetyError::Malformed)?;
        self.offset = end;
        Ok(value)
    }

    fn field(&mut self) -> Result<&'a [u8], InstallationSafetyError> {
        let length = u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| InstallationSafetyError::Malformed)?,
        );
        self.take(usize::try_from(length).map_err(|_| InstallationSafetyError::Malformed)?)
    }

    fn optional_field(&mut self) -> Result<Option<&'a [u8]>, InstallationSafetyError> {
        match self.take(1)? {
            [0] => Ok(None),
            [1] => self.field().map(Some),
            _ => Err(InstallationSafetyError::Malformed),
        }
    }

    fn is_finished(&self) -> bool {
        self.offset == self.bytes.len()
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], InstallationSafetyError> {
        self.field()?
            .try_into()
            .map_err(|_| InstallationSafetyError::Malformed)
    }

    fn uuid(&mut self) -> Result<uuid::Uuid, InstallationSafetyError> {
        Ok(uuid::Uuid::from_bytes(self.fixed()?))
    }

    fn u64(&mut self) -> Result<u64, InstallationSafetyError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| InstallationSafetyError::Malformed)?,
        ))
    }

    fn domain(&mut self, expected: &[u8]) -> Result<(), InstallationSafetyError> {
        if self.field()? == expected {
            Ok(())
        } else {
            Err(InstallationSafetyError::Malformed)
        }
    }

    fn finish(self) -> Result<(), InstallationSafetyError> {
        if self.is_finished() {
            Ok(())
        } else {
            Err(InstallationSafetyError::Malformed)
        }
    }
}

impl fmt::Display for SafetyJournalDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
