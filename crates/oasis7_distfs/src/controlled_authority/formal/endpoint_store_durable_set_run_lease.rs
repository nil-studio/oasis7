//! A timed-out provider may still be walking after the endpoint owner releases
//! its native lock. Scratch lifetime therefore has its OWN kernel lease.
use super::*;

const GUARD: &str = ".run-creation-guard-v1";
const LEASE: &str = ".run-lease-v1";
// Bound the short shared namespace critical section, not world history. An
// incomplete scan rejects this attempt; it never reclaims an unprobed live run.
const MAX_SCAN_ENTRIES: usize = 256;
struct Lease(File);
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = File::unlock(&self.0);
    }
}
fn private_metadata(metadata: &fs::Metadata, is_directory: bool) -> Result<(), FormalError> {
    if (is_directory && !metadata.is_dir()) || (!is_directory && !metadata.is_file()) {
        return Err(invalid("healthy copy scratch type/symlink"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid only reads this process's ownership identity.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(invalid("healthy copy scratch private owner/mode"));
        }
        if !is_directory && metadata.len() != 0 {
            return Err(invalid("healthy copy scratch lease payload"));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    Err(invalid(
        "healthy copy scratch requires Unix private storage",
    ))
}
fn open_lease(path: &Path, create: bool, exclusive_create: bool) -> Result<File, FormalError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => private_metadata(&metadata, false)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(create)
        .create_new(exclusive_create);
    #[cfg(unix)]
    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options.open(path).map_err(io)?;
    private_metadata(&file.metadata().map_err(io)?, false)?;
    Ok(file)
}
fn parent_guard(staging: &Path) -> Result<Lease, FormalError> {
    private_metadata(&fs::symlink_metadata(staging).map_err(io)?, true)?;
    let file = open_lease(&staging.join(GUARD), true, false)?;
    file.try_lock().map_err(|error| match error {
        fs::TryLockError::WouldBlock => {
            FormalError::Conflict("healthy scratch creation busy".into())
        }
        fs::TryLockError::Error(error) => io(error),
    })?;
    Ok(Lease(file))
}
fn missing_is_ok(result: std::io::Result<()>) -> Result<(), FormalError> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io(error)),
    }
}
struct Orphan {
    path: PathBuf,
    _lease: Lease,
}
struct Scan {
    orphan: Option<Orphan>,
    claimed: bool,
    complete: bool,
}
// The parent guard covers BOTH this scan and mkdir/lease acquisition. A run
// without a lease can only be an interrupted creation/retirement and must be empty.
fn scan_orphans(staging: &Path) -> Result<Scan, FormalError> {
    let mut scan = Scan {
        orphan: None,
        claimed: false,
        complete: false,
    };
    let mut entries = fs::read_dir(staging).map_err(io)?;
    for _ in 0..MAX_SCAN_ENTRIES {
        let Some(entry) = entries.next() else {
            scan.complete = true;
            break;
        };
        let entry = entry.map_err(io)?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| invalid("healthy copy staging name"))?;
        if name == GUARD {
            continue;
        }
        let id = name
            .strip_prefix("run-")
            .ok_or_else(|| invalid("healthy copy foreign staging entry"))?;
        hash_hex(id)?;
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io(error)),
        };
        private_metadata(&metadata, true)?;
        let lease_path = path.join(LEASE);
        match fs::symlink_metadata(&lease_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Only an empty interrupted creation/retirement is provable
                // without a lease. Keep the per-call one-orphan cleanup bound.
                if fs::read_dir(&path).map_err(io)?.next().is_some() {
                    return Err(invalid("healthy copy nonempty scratch without lease"));
                }
                if !scan.claimed {
                    missing_is_ok(fs::remove_dir(&path))?;
                    scan.claimed = true;
                }
                continue;
            }
            Err(error) => return Err(io(error)),
            Ok(metadata) => private_metadata(&metadata, false)?,
        }
        // Claim at most one orphan. Further entries still undergo name/type
        // checks, but no additional lease descriptors or cleanup are retained.
        if scan.claimed {
            continue;
        }
        let file = open_lease(&lease_path, false, false)?;
        match file.try_lock() {
            Ok(()) => {
                scan.claimed = true;
                scan.orphan = Some(Orphan {
                    path,
                    _lease: Lease(file),
                })
            }
            Err(fs::TryLockError::WouldBlock) => {} // A live walk owns its markers.
            Err(fs::TryLockError::Error(error)) => return Err(io(error)),
        }
    }
    Ok(scan)
}
fn retire_run(path: &Path, staging: &Path) -> Result<(), FormalError> {
    // Caller holds this run's lease throughout. Marker cleanup retains the
    // original full-walk cleanup cost; it is NOT a paged global-GC operation.
    // Crucially, that work does not hold the shared namespace creation guard.
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io(error)),
    };
    for entry in entries {
        let entry = entry.map_err(io)?;
        if entry.file_name() == LEASE {
            continue;
        }
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io(error)),
        };
        if !metadata.is_file() {
            return Err(invalid("healthy copy scratch marker type/symlink"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            // SAFETY: geteuid only reads this process's ownership identity.
            if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
                return Err(invalid("healthy copy scratch marker private owner/mode"));
            }
        }
        missing_is_ok(fs::remove_file(entry.path()))?;
    }
    // Persist marker removal before retiring the lease: a crash that loses the
    // lease must leave an empty, safely reclaimable run, not unknown work.
    sync(path)?;
    // Keep the lease filename visible while deletion is in progress. The last
    // two unlinks are atomic with respect to creator/scanner inspection.
    let _creation = parent_guard(staging)?;
    missing_is_ok(fs::remove_file(path.join(LEASE)))?;
    missing_is_ok(fs::remove_dir(path))?;
    sync(staging)
}
pub(super) struct RunLease {
    pub(super) path: PathBuf,
    staging: PathBuf,
    _lease: Lease,
    trace: bool,
}
impl RunLease {
    pub(super) fn start(target: &Path, trace: bool) -> Result<Self, FormalError> {
        let staging = target.join(".formal-durable-set-staging");
        directory(&staging)?;
        let mut creation = parent_guard(&staging)?;
        let scan = scan_orphans(&staging)?;
        if let Some(orphan) = scan.orphan {
            drop(creation);
            retire_run(&orphan.path, &staging)?;
            drop(orphan);
            creation = parent_guard(&staging)?;
        }
        if !scan.complete {
            return Err(FormalError::Conflict(
                "healthy scratch scan page exhausted".into(),
            ));
        }
        sync(&staging)?;
        let path = staging.join(format!(
            "run-{}",
            fencing::fresh_challenge().map_err(fencing_error)?
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(&path).map_err(io)?;
        // The creation guard prevents a scanner from observing a live creator's
        // directory before its private lease exists and is locked.
        let file = open_lease(&path.join(LEASE), false, true)?;
        file.try_lock().map_err(io)?;
        // Publish durable ownership before a walk may persist any markers.
        file.sync_all().map_err(io)?;
        sync(&path)?;
        sync(&staging)?;
        drop(creation);
        Ok(Self {
            path,
            staging,
            _lease: Lease(file),
            trace,
        })
    }
}
impl Drop for RunLease {
    fn drop(&mut self) {
        let started = self.trace.then(std::time::Instant::now);
        // Cleanup owns only this run, keeps its lease locked and visible, and
        // never holds the namespace guard across the full marker walk. If the
        // final short guard is busy, a later scan can reclaim the orphan.
        let _ = retire_run(&self.path, &self.staging);
        if self.trace {
            eprintln!("healthy-native-cleanup cleanup_us={}", elapsed_us(started));
        }
    }
}

#[cfg(all(test, unix))]
#[path = "endpoint_store_durable_set_run_lease_tests.rs"]
mod tests;
