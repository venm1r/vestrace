//! Host-only P05-B archive-object and archive-key custody adapters.

mod file_store;
mod key_custody;

pub use file_store::FileBackupObjectStore;
pub use key_custody::FileArchiveKeyCustody;

use std::{io, path::Path};

#[cfg(not(windows))]
use std::fs::File;

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

fn sync_parent(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    #[cfg(windows)]
    {
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x0200_0000)
            .open(parent)?
            .sync_all()
    }
    #[cfg(not(windows))]
    {
        File::open(parent)?.sync_all()
    }
}
