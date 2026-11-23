//! Native lifetime ownership and its exact durable operation binding.
use super::io;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

/// Lifetime ownership of one exact local directory and lock inode. Only the
/// native lock acquisition path can construct this value; arbitrary files and
/// persisted PID/incarnation strings are not ownership capabilities.
/// There is deliberately no unlock, raw-file conversion, or Clone operation.
/// Path checks detect existing mismatches; they do not pin future path lookups
/// against concurrent directory replacement or authorize operation recovery.
pub(in super::super) struct ProcessOwnership {
    pub(super) file: File,
    directory: PathBuf,
    directory_identity: (u64, u64),
}
impl ProcessOwnership {
    pub(in super::super) fn require_directory(&self, data: &Path) -> Result<(), String> {
        io::reject_symlinks(data)?;
        let directory = fs::canonicalize(data).map_err(|e| e.to_string())?;
        let metadata = fs::metadata(&directory).map_err(|e| e.to_string())?;
        let path = directory.join("process.lock");
        io::regular(&path)?;
        let current = fs::metadata(&path).map_err(|e| e.to_string())?;
        let held = self.file.metadata().map_err(|e| e.to_string())?;
        if directory != self.directory
            || (metadata.dev(), metadata.ino()) != self.directory_identity
            || (current.dev(), current.ino()) != (held.dev(), held.ino())
        {
            return Err("native ownership does not bind this exact data directory".into());
        }
        Ok(())
    }
}
pub(in super::super) fn lock(data: &Path) -> Result<ProcessOwnership, String> {
    let path = data.join("process.lock");
    io::regular(&path)?;
    super::super::super::controlled_live_config::require_private_owned(&path, false)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.try_lock()
        .map_err(|_| "another process retains this exact local node ownership lock".to_string())?;
    let directory = fs::canonicalize(data).map_err(|e| e.to_string())?;
    let metadata = fs::metadata(&directory).map_err(|e| e.to_string())?;
    let ownership = ProcessOwnership {
        file,
        directory,
        directory_identity: (metadata.dev(), metadata.ino()),
    };
    ownership.require_directory(data)?;
    Ok(ownership)
}

impl ProcessOwnership {
    #[cfg(test)]
    pub(in super::super) fn spawn_bound_worker<T, F>(
        self: std::sync::Arc<Self>,
        work: F,
    ) -> std::io::Result<std::thread::JoinHandle<T>>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        super::formal_full_sync_runtime::spawn_owned_worker(self, work)
    }

    pub(in super::super) fn binding_digest(&self) -> Result<String, String> {
        self.require_directory(&self.directory)?;
        let held = self.file.metadata().map_err(|error| error.to_string())?;
        io::digest(&(
            "oasis7.native-owner-binding/v1",
            &self.directory,
            self.directory_identity,
            held.dev(),
            held.ino(),
        ))
    }
}
