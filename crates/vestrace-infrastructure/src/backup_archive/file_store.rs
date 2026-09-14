use std::{
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use sha2::{Digest as _, Sha256};
use vestrace_application::{
    ApplicationError, ArchiveObjectWrite, ArchiveStagingId, BackupObjectStore, StagedArchiveObject,
};
use vestrace_domain::{ArchiveObjectDescriptor, BackupObjectId, BackupSetId, SafetyJournalDigest};

use super::sync_parent;

const FRAME_MAGIC: &[u8; 4] = b"VBAO";
const FRAME_VERSION: u8 = 1;

/// Durable host-only staging store. The port receives ciphertext; set-key
/// custody performs the AES-GCM operation before this adapter sees a write.
pub struct FileBackupObjectStore {
    root: PathBuf,
}

impl FileBackupObjectStore {
    pub fn open(root: PathBuf) -> Result<Self, ApplicationError> {
        fs::create_dir_all(root.join("staging"))
            .map_err(unavailable("create archive staging root"))?;
        fs::create_dir_all(root.join("objects"))
            .map_err(unavailable("create archive object root"))?;
        sync_parent(&root.join("staging")).map_err(unavailable("sync archive root"))?;
        Ok(Self { root })
    }

    pub fn staging_path(&self, staged: &StagedArchiveObject) -> PathBuf {
        self.root
            .join("staging")
            .join(staged.descriptor().set_id().as_uuid().to_string())
            .join(format!("{}.frame", staged.staging_id().as_uuid()))
    }

    /// A create-only intermediate name for the encrypted stream. It is
    /// derived from the same immutable object UUID as the eventual staging
    /// frame and never names a set-wide directory scan.
    pub fn ciphertext_spool_path(&self, descriptor: &ArchiveObjectDescriptor) -> PathBuf {
        self.root
            .join("staging")
            .join(descriptor.set_id().as_uuid().to_string())
            .join(format!("{}.ciphertext", descriptor.object_id().as_uuid()))
    }

    fn final_path(&self, descriptor: &ArchiveObjectDescriptor) -> PathBuf {
        let input = descriptor.to_input();
        let kind = match input.kind {
            vestrace_domain::ArchiveObjectKind::BaseChunk => "base",
            vestrace_domain::ArchiveObjectKind::WalSegment => "wal",
            vestrace_domain::ArchiveObjectKind::TimelineHistory => "timeline",
        };
        let mut digest = String::with_capacity(64);
        for byte in input.ciphertext_digest.as_bytes() {
            write!(&mut digest, "{byte:02x}").expect("write to a string cannot fail");
        }
        self.root
            .join("objects")
            .join(input.set_id.as_uuid().to_string())
            .join(kind)
            .join(format!("{}-{digest}.frame", input.ordinal))
    }

    /// Reads one exact, committed immutable frame after checking the same
    /// descriptor-bound framing and ciphertext digest used at promotion.
    /// Callers receive only the encrypted object frame; key custody remains
    /// responsible for opening it.
    pub fn read_committed_frame(
        &self,
        descriptor: &ArchiveObjectDescriptor,
    ) -> Result<Vec<u8>, ApplicationError> {
        let path = self.final_path(descriptor);
        Self::verify_frame(&path, descriptor)?;
        let frame = fs::read(&path).map_err(unavailable("read committed archive object"))?;
        let payload = frame.get(FRAME_MAGIC.len() + 1..).ok_or_else(|| {
            ApplicationError::Storage("archive committed frame is malformed".to_owned())
        })?;
        Ok(payload.to_vec())
    }

    /// Removes one staging frame only after the guarded pending intent that
    /// names its set/object UUID has been cancelled. This deliberately has no
    /// directory scan or set-wide cleanup operation.
    pub fn remove_cancelled_pending_staging(
        &self,
        set: BackupSetId,
        object: BackupObjectId,
    ) -> Result<(), ApplicationError> {
        let path = self
            .root
            .join("staging")
            .join(set.as_uuid().to_string())
            .join(format!("{}.frame", object.as_uuid()));
        match fs::remove_file(&path) {
            Ok(()) => {
                sync_parent(&path).map_err(unavailable("sync cancelled archive staging directory"))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ApplicationError::Unavailable(format!(
                "remove cancelled archive staging object: {error}"
            ))),
        }
    }

    /// Promotes an already verified staging member to its immutable object
    /// name. Callers may invoke this only after the guarded checkpoint commit.
    pub fn promote_after_checkpoint(
        &self,
        staged: &StagedArchiveObject,
    ) -> Result<PathBuf, ApplicationError> {
        let staging = self.staging_path(staged);
        Self::verify_frame(&staging, staged.descriptor())?;
        let final_path = self.final_path(staged.descriptor());
        let parent = final_path.parent().ok_or_else(|| {
            ApplicationError::Unavailable("archive final path has no parent".to_owned())
        })?;
        fs::create_dir_all(parent).map_err(unavailable("create immutable archive object root"))?;
        let mut source = OpenOptions::new()
            .read(true)
            .open(&staging)
            .map_err(unavailable("open staged archive object for promotion"))?;
        let mut target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&final_path)
            .map_err(unavailable("create immutable archive object"))?;
        std::io::copy(&mut source, &mut target)
            .and_then(|_| target.sync_all())
            .map_err(unavailable("durably promote immutable archive object"))?;
        sync_parent(&final_path).map_err(unavailable("sync immutable archive object directory"))?;
        fs::remove_file(&staging).map_err(unavailable("remove promoted archive staging object"))?;
        sync_parent(&staging).map_err(unavailable("sync promoted archive staging directory"))?;
        self.remove_empty_staging_set_directory(&staging)?;
        Ok(final_path)
    }

    fn remove_empty_staging_set_directory(&self, staging: &Path) -> Result<(), ApplicationError> {
        let set_directory = staging.parent().ok_or_else(|| {
            ApplicationError::Unavailable("archive staging object has no set directory".to_owned())
        })?;
        match fs::remove_dir(set_directory) {
            Ok(()) => {
                sync_parent(set_directory).map_err(unavailable("sync emptied archive staging root"))
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(ApplicationError::Unavailable(format!(
                "remove empty exact archive staging directory: {error}"
            ))),
        }
    }

    fn write_path(&self, write: &ArchiveObjectWrite, staging: ArchiveStagingId) -> PathBuf {
        self.root
            .join("staging")
            .join(write.descriptor().set_id().as_uuid().to_string())
            .join(format!("{}.frame", staging.as_uuid()))
    }

    fn verify_frame(
        path: &Path,
        descriptor: &ArchiveObjectDescriptor,
    ) -> Result<(), ApplicationError> {
        let mut frame = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(unavailable("open staged archive object"))?;
        let mut header = [0_u8; 5];
        frame
            .read_exact(&mut header)
            .map_err(unavailable("read staged archive header"))?;
        if header[..FRAME_MAGIC.len()] != *FRAME_MAGIC || header[FRAME_MAGIC.len()] != FRAME_VERSION
        {
            return Err(ApplicationError::Storage(
                "archive staging frame is malformed".to_owned(),
            ));
        }
        let mut digest = Sha256::new();
        let mut length = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = frame
                .read(&mut buffer)
                .map_err(unavailable("read staged archive ciphertext"))?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
            length = length.checked_add(read as u64).ok_or_else(|| {
                ApplicationError::Storage("archive staging ciphertext length overflows".to_owned())
            })?;
        }
        let digest = SafetyJournalDigest::from_bytes(digest.finalize().into());
        let input = descriptor.to_input();
        if length != input.length || digest != input.ciphertext_digest {
            return Err(ApplicationError::Storage(
                "archive staging ciphertext differs from descriptor".to_owned(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl BackupObjectStore for FileBackupObjectStore {
    async fn stage_encrypted(
        &self,
        write: ArchiveObjectWrite,
    ) -> Result<StagedArchiveObject, ApplicationError> {
        // The guarded append intent uses the immutable object UUID.  Derive
        // the staged name from it, so recovery addresses one exact file
        // without ever scanning a backup-set-wide prefix.
        let staging = ArchiveStagingId::from_uuid(write.descriptor().object_id().as_uuid());
        let path = self.write_path(&write, staging);
        let parent = path.parent().ok_or_else(|| {
            ApplicationError::Unavailable("archive staging path has no parent".to_owned())
        })?;
        fs::create_dir_all(parent).map_err(unavailable("create archive set staging root"))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(unavailable("create archive staging object"))?;
        file.write_all(FRAME_MAGIC)
            .and_then(|()| file.write_all(&[FRAME_VERSION]))
            .map_err(unavailable("write archive staging header"))?;
        let copied = write.copy_ciphertext_to(&mut file)?;
        if copied != write.descriptor().to_input().length {
            return Err(ApplicationError::Storage(
                "archive ciphertext source length differs from descriptor".to_owned(),
            ));
        }
        file.sync_all()
            .map_err(unavailable("durably write archive staging object"))?;
        sync_parent(&path).map_err(unavailable("sync archive staging directory"))?;
        Ok(StagedArchiveObject::new(
            staging,
            write.descriptor().clone(),
        ))
    }

    async fn verify_durable(&self, staged: &StagedArchiveObject) -> Result<(), ApplicationError> {
        Self::verify_frame(&self.staging_path(staged), staged.descriptor())
    }

    async fn promote_after_checkpoint(
        &self,
        staged: &StagedArchiveObject,
    ) -> Result<(), ApplicationError> {
        FileBackupObjectStore::promote_after_checkpoint(self, staged).map(|_| ())
    }

    async fn remove_orphan(&self, staged: &StagedArchiveObject) -> Result<(), ApplicationError> {
        let path = self.staging_path(staged);
        match fs::remove_file(&path) {
            Ok(()) => sync_parent(&path).map_err(unavailable("sync orphan cleanup directory")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ApplicationError::Unavailable(format!(
                "remove exact archive orphan: {error}"
            ))),
        }
    }

    async fn remove_committed(&self, _: &ArchiveObjectDescriptor) -> Result<(), ApplicationError> {
        Err(ApplicationError::Policy(
            "committed archive deletion requires a guarded deletion preparation".to_owned(),
        ))
    }

    async fn remove_prepared_committed(
        &self,
        _: vestrace_application::ManagedBackupDeletionPrepared,
        object: &ArchiveObjectDescriptor,
    ) -> Result<(), ApplicationError> {
        let path = self.final_path(object);
        match fs::metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // The manifest and signed `ArchiveKeyErased` transition are
                // durable. A retry after a partial exact deletion must resume
                // this member rather than scanning or widening the target.
                return Ok(());
            }
            Err(error) => return Err(unavailable("inspect manifest archive object")(error)),
        }
        Self::verify_frame(&path, object)?;
        fs::remove_file(&path).map_err(unavailable("remove manifest-verified archive object"))?;
        sync_parent(&path).map_err(unavailable("sync deleted archive object directory"))
    }
}

fn unavailable(operation: &'static str) -> impl FnOnce(std::io::Error) -> ApplicationError {
    move |error| ApplicationError::Unavailable(format!("{operation}: {error}"))
}
