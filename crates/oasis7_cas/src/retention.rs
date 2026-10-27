//! Cooperative local retention transactions. The stable carrier must never be
//! unlinked or replaced while the CAS is in use; a PID is not lock ownership.
use super::{LocalCasStore, PinFile, read_json_from_path, validate_hash, write_json_atomic};
use oasis7_proto::world_error::WorldError;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};

const RETENTION_LOCK: &str = ".retention.lock";

/// Excludes cooperative GC while a specialized durable publisher updates CAS
/// reference files. Dropping the guard releases the OS lock, including unwind.
#[must_use]
pub struct CasRetentionGuard {
    _lock: File,
    store: LocalCasStore,
}

impl CasRetentionGuard {
    /// The owner of a private scope may retire these exact references after
    /// independently proving their replacement. Other scopes/global pins are
    /// untouched. Keep this guard through the following sweep; this is not
    /// authorization to retire world history. Changed shards and their parent
    /// are synced before returning, so failure must preserve source material.
    pub fn retire_pin_scope_references(
        &self,
        scope: &str,
        references: &BTreeSet<String>,
    ) -> Result<(), WorldError> {
        for reference in references {
            validate_hash(reference)?;
        }
        let mut changed = false;
        for shard in self.store.list_pin_scope_shards(scope)? {
            let path = self.store.pin_scope_shard_path(scope, &shard)?;
            let mut pins: PinFile = read_json_from_path(&path)?;
            let original_len = pins.pins.len();
            pins.pins.retain(|hash| !references.contains(hash));
            if pins.pins.len() != original_len {
                write_json_atomic(&pins, &path)?;
                File::open(&path)?.sync_all()?;
                changed = true;
            }
        }
        if changed {
            File::open(self.store.pin_scope_dir(scope)?)?.sync_all()?;
        }
        Ok(())
    }
}

impl LocalCasStore {
    /// Acquire the same lock used by pin, index and GC operations. This is for
    /// specialized durable reference publishers that write files themselves.
    /// Do not call another locking mutation on this store while holding it;
    /// the lock is not reentrant. Bare blob reads do not reacquire it; puts do.
    pub fn acquire_retention_guard(&self) -> Result<CasRetentionGuard, WorldError> {
        Ok(CasRetentionGuard {
            _lock: self.lock_retention()?,
            store: self.clone(),
        })
    }

    /// Hold through the complete pin/index read-modify-write or sweep. Internal
    /// helpers do not reacquire it: independent opens are not reentrant locks.
    pub(super) fn lock_retention(&self) -> Result<File, WorldError> {
        self.ensure_dirs()?;
        let path = self.root.join(RETENTION_LOCK);
        let invalid = || WorldError::DistributedValidationFailed {
            reason: "CAS retention lock must be a stable empty regular file".into(),
        };
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_file() || metadata.len() != 0 => {
                return Err(invalid());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        file.lock()?;
        let opened = file.metadata()?;
        let current = fs::symlink_metadata(&path)?;
        if !opened.is_file()
            || !current.file_type().is_file()
            || opened.len() != 0
            || current.len() != 0
        {
            return Err(invalid());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if opened.dev() != current.dev() || opened.ino() != current.ino() || opened.nlink() != 1
            {
                return Err(invalid());
            }
        }
        Ok(file)
    }
}
