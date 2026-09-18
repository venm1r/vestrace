use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
};

use async_trait::async_trait;
use sha2::{Digest as _, Sha256};
use vestrace_application::{ApplicationError, FreshRestoreTargetCustody};
use vestrace_domain::{
    ArchiveObjectDescriptor, ArchiveObjectKind, BackupSetId, SafetyJournalDigest,
};

use crate::backup_archive::{FileArchiveKeyCustody, FileBackupObjectStore};

/// Absolute PostgreSQL program selected by deployment configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreTool(PathBuf);

impl RestoreTool {
    pub fn new(path: PathBuf) -> Result<Self, RestoreTargetError> {
        if !path.is_absolute() || !path.is_file() {
            return Err(RestoreTargetError::ToolIsNotAbsoluteFile);
        }
        Ok(Self(path))
    }
}

/// Host roots which must never be shared with a new target directory.
#[derive(Clone, Debug)]
pub struct RestoreTargetCustody {
    protected_roots: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RestoreTargetError {
    #[error("restore custody root is not absolute")]
    RelativeRoot,
    #[error("restore custody roots overlap")]
    OverlappingRoots,
    #[error("restore target already exists")]
    TargetExists,
    #[error("restore target is missing")]
    TargetMissing,
    #[error("restore target tool must be an absolute existing file")]
    ToolIsNotAbsoluteFile,
    #[error("restore target tool failed")]
    ToolFailed,
    #[error("restore archive manifest is invalid")]
    InvalidManifest,
    #[error("restore target filesystem operation failed")]
    Filesystem,
}

impl RestoreTargetCustody {
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Result<Self, RestoreTargetError> {
        let roots: Vec<_> = roots
            .into_iter()
            .map(|root| lexical_normalize(&root))
            .collect();
        if roots.iter().any(|root| !root.is_absolute()) {
            return Err(RestoreTargetError::RelativeRoot);
        }
        for (index, root) in roots.iter().enumerate() {
            if roots
                .iter()
                .skip(index + 1)
                .any(|other| overlaps(root, other))
            {
                return Err(RestoreTargetError::OverlappingRoots);
            }
        }
        Ok(Self {
            protected_roots: roots,
        })
    }

    /// Creates exactly one formerly absent, disjoint target directory.
    pub fn prepare_fresh(&self, target: PathBuf) -> Result<PathBuf, RestoreTargetError> {
        let target = lexical_normalize(&target);
        if !target.is_absolute() {
            return Err(RestoreTargetError::RelativeRoot);
        }
        if self
            .protected_roots
            .iter()
            .any(|root| overlaps(root, &target))
        {
            return Err(RestoreTargetError::OverlappingRoots);
        }
        if target.exists() {
            return Err(RestoreTargetError::TargetExists);
        }
        fs::create_dir(&target).map_err(|_| RestoreTargetError::Filesystem)?;
        Ok(target)
    }

    /// Removes only a previously prepared target after the caller has bound
    /// that path to an exact restore attempt. Protected roots and a missing
    /// target are refusals, so a retry can never widen this operation.
    pub fn destroy_refused_target(&self, target: &Path) -> Result<(), RestoreTargetError> {
        let target = lexical_normalize(target);
        if !target.is_absolute()
            || self
                .protected_roots
                .iter()
                .any(|root| overlaps(root, &target))
        {
            return Err(RestoreTargetError::OverlappingRoots);
        }
        if !target.is_dir() {
            return Err(RestoreTargetError::TargetMissing);
        }
        fs::remove_dir_all(target).map_err(|_| RestoreTargetError::Filesystem)
    }

    /// Opens exactly the descriptor-bound encrypted objects selected by the
    /// caller and writes their authenticated plaintext into the new target.
    /// It deliberately retains an incomplete target for explicit operator
    /// refusal handling after any failure.
    pub fn materialize_manifest(
        &self,
        target: &Path,
        set: BackupSetId,
        manifest: &[ArchiveObjectDescriptor],
        archive: &FileBackupObjectStore,
        keys: &FileArchiveKeyCustody,
    ) -> Result<Vec<PathBuf>, RestoreTargetError> {
        if !target.is_absolute()
            || !target.is_dir()
            || self
                .protected_roots
                .iter()
                .any(|root| overlaps(root, target))
        {
            return Err(RestoreTargetError::OverlappingRoots);
        }
        validate_manifest(set, manifest)?;
        let input_root = target.join("restore-input");
        fs::create_dir(&input_root).map_err(|_| RestoreTargetError::Filesystem)?;
        let mut written = Vec::with_capacity(manifest.len());
        for descriptor in manifest {
            let ciphertext = archive
                .read_committed_frame(descriptor)
                .map_err(|_| RestoreTargetError::Filesystem)?;
            let plaintext = keys
                .open_object(set, descriptor, &ciphertext)
                .map_err(|_| RestoreTargetError::Filesystem)?;
            let input = descriptor.to_input();
            if SafetyJournalDigest::from_bytes(Sha256::digest(&*plaintext).into())
                != input.plaintext_digest
            {
                return Err(RestoreTargetError::InvalidManifest);
            }
            let label = match input.kind {
                ArchiveObjectKind::BaseChunk => "base.tar",
                ArchiveObjectKind::WalSegment => "wal.segment",
                ArchiveObjectKind::TimelineHistory => "timeline.history",
            };
            let path = input_root.join(format!("{:020}-{label}", input.ordinal));
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|_| RestoreTargetError::Filesystem)?;
            output
                .write_all(&plaintext)
                .and_then(|()| output.sync_all())
                .map_err(|_| RestoreTargetError::Filesystem)?;
            written.push(path);
        }
        Ok(written)
    }

    /// Runs a configured physical restore tool without a shell or source-root argument.
    pub fn run_tool(
        &self,
        tool: &RestoreTool,
        target: &Path,
        arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    ) -> Result<(), RestoreTargetError> {
        if !target.is_absolute()
            || self
                .protected_roots
                .iter()
                .any(|root| overlaps(root, target))
        {
            return Err(RestoreTargetError::OverlappingRoots);
        }
        let arguments: Vec<_> = arguments.into_iter().collect();
        if arguments.iter().any(|argument| {
            let path = Path::new(argument.as_ref());
            path.is_absolute() && self.protected_roots.iter().any(|root| overlaps(root, path))
        }) {
            return Err(RestoreTargetError::OverlappingRoots);
        }
        let status = Command::new(&tool.0)
            .args(&arguments)
            .arg("--pgdata")
            .arg(target)
            .status()
            .map_err(|_| RestoreTargetError::Filesystem)?;
        if status.success() {
            Ok(())
        } else {
            Err(RestoreTargetError::ToolFailed)
        }
    }
}

fn validate_manifest(
    set: BackupSetId,
    manifest: &[ArchiveObjectDescriptor],
) -> Result<(), RestoreTargetError> {
    let Some((first, rest)) = manifest.split_first() else {
        return Err(RestoreTargetError::InvalidManifest);
    };
    let first_input = first.to_input();
    if first_input.set_id != set || first_input.kind != ArchiveObjectKind::BaseChunk {
        return Err(RestoreTargetError::InvalidManifest);
    }
    let mut expected_ordinal = first_input.ordinal;
    let mut previous_timeline = first_input.timeline;
    let mut previous_end_lsn = first_input.end_lsn;
    for descriptor in rest {
        let input = descriptor.to_input();
        expected_ordinal = expected_ordinal
            .checked_add(1)
            .ok_or(RestoreTargetError::InvalidManifest)?;
        if input.set_id != set
            || input.ordinal != expected_ordinal
            || matches!(input.kind, ArchiveObjectKind::BaseChunk)
            || (input.kind == ArchiveObjectKind::WalSegment
                && (input.timeline != previous_timeline || input.start_lsn > previous_end_lsn))
        {
            return Err(RestoreTargetError::InvalidManifest);
        }
        previous_timeline = input.timeline;
        previous_end_lsn = input.end_lsn;
    }
    Ok(())
}

#[async_trait]
impl FreshRestoreTargetCustody for RestoreTargetCustody {
    async fn prepare_fresh(&self, target: PathBuf) -> Result<(), ApplicationError> {
        Self::prepare_fresh(self, target)
            .map(|_| ())
            .map_err(|error| ApplicationError::Unavailable(error.to_string()))
    }
}

fn overlaps(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
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
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_existing_and_overlapping_targets_before_creating_them() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let archive = root.path().join("archive");
        let existing = root.path().join("already-created");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&archive).unwrap();
        fs::create_dir(&existing).unwrap();
        let custody = RestoreTargetCustody::new([source.clone(), archive.clone()]).unwrap();
        assert_eq!(
            custody.prepare_fresh(source.join("target")),
            Err(RestoreTargetError::OverlappingRoots)
        );
        assert_eq!(
            custody.prepare_fresh(existing),
            Err(RestoreTargetError::TargetExists)
        );
        let target = custody.prepare_fresh(root.path().join("target")).unwrap();
        assert!(target.is_dir());
    }

    #[test]
    fn destroys_only_an_existing_disjoint_target() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let target = root.path().join("target");
        fs::create_dir(&source).unwrap();
        fs::create_dir_all(target.join("partial")).unwrap();
        let custody = RestoreTargetCustody::new([source.clone()]).unwrap();

        assert_eq!(
            custody.destroy_refused_target(&source),
            Err(RestoreTargetError::OverlappingRoots)
        );
        custody.destroy_refused_target(&target).unwrap();
        assert!(!target.exists());
        assert_eq!(
            custody.destroy_refused_target(&target),
            Err(RestoreTargetError::TargetMissing)
        );
    }
}
