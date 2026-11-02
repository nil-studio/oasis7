//! Independent journal for the explicitly configured SQLite-only machine lane.
//! The path must be outside the database backup/restore unit. This does not
//! qualify rollback of the whole filesystem or VM; Account's external floor is
//! still mandatory. No missing journal is silently initialized on ordinary open.
use oasis7_custody_core::native_cleanup_v2::NativeStorageHeadV2;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    api_version: u8,
    stable: NativeStorageHeadV2,
    prepared: Option<NativeStorageHeadV2>,
}
pub(super) struct RestoreGuard {
    path: PathBuf,
}
const MAX_BYTES: u64 = 2048;
fn denied() -> String {
    "native machine restore guard unavailable".into()
}
/// The journal directory is a separate Native-owned restore unit. A model-
/// writable ancestor cannot substitute a journal even when the file is 0600.
#[cfg(unix)]
fn protected_path(path: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if !path.is_absolute() {
        return Err(denied());
    }
    let parent = path
        .parent()
        .ok_or_else(denied)?
        .canonicalize()
        .map_err(|_| denied())?;
    let mut at = PathBuf::from("/");
    // SAFETY: geteuid has no arguments and only reports this process credential.
    let uid = unsafe { libc::geteuid() };
    for component in parent.components().skip(1) {
        at.push(component.as_os_str());
        let m = fs::symlink_metadata(&at).map_err(|_| denied())?;
        let mode = m.permissions().mode();
        let trusted_sticky = m.uid() == 0 && mode & 0o1000 != 0;
        if !m.is_dir() || ![0, uid].contains(&m.uid()) || mode & 0o022 != 0 && !trusted_sticky {
            return Err(denied());
        }
    }
    let m = fs::symlink_metadata(&parent).map_err(|_| denied())?;
    if m.uid() != uid || m.permissions().mode() & 0o077 != 0 {
        return Err(denied());
    }
    Ok(parent.join(path.file_name().ok_or_else(denied)?))
}
#[cfg(not(unix))]
fn protected_path(_path: &Path) -> Result<PathBuf, String> {
    // This slice does not pretend POSIX permission checks qualify Windows ACLs.
    Err(denied())
}
impl RestoreGuard {
    /// Called only by explicit first-registration provisioning, never recovery.
    pub(super) fn create(path: &Path, initial: &NativeStorageHeadV2) -> Result<Self, String> {
        let path = protected_path(path)?;
        initial.validate()?;
        let journal = Journal {
            api_version: 2,
            stable: initial.clone(),
            prepared: None,
        };
        let bytes = serde_json::to_vec(&journal).map_err(|_| denied())?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|_| denied())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| denied())?;
        sync_parent(&path)?;
        Ok(Self { path })
    }
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        let guard = Self {
            path: protected_path(path)?,
        };
        guard.load()?;
        Ok(guard)
    }
    fn load(&self) -> Result<Journal, String> {
        if protected_path(&self.path)? != self.path {
            return Err(denied());
        }
        let metadata = fs::symlink_metadata(&self.path).map_err(|_| denied())?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_BYTES {
            return Err(denied());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            // SAFETY: geteuid only reports this process credential.
            let uid = unsafe { libc::geteuid() };
            if metadata.uid() != uid
                || metadata.nlink() != 1
                || metadata.permissions().mode() & 0o077 != 0
            {
                return Err(denied());
            }
        }
        let mut bytes = Vec::new();
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        options
            .open(&self.path)
            .map_err(|_| denied())?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| denied())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(denied());
        }
        let journal: Journal = serde_json::from_slice(&bytes).map_err(|_| denied())?;
        if journal.api_version != 2 {
            return Err(denied());
        }
        journal.stable.validate()?;
        if let Some(next) = &journal.prepared {
            validate_successor(&journal.stable, next)?;
        }
        Ok(journal)
    }
    fn replace(&self, journal: &Journal) -> Result<(), String> {
        if protected_path(&self.path)? != self.path {
            return Err(denied());
        }
        let temp = self.path.with_extension("machine-guard-next");
        // A leftover temp is not overwritten: startup must reconcile it explicitly.
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(|_| denied())?;
        let bytes = serde_json::to_vec(journal).map_err(|_| denied())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| denied())?;
        fs::rename(&temp, &self.path).map_err(|_| denied())?;
        sync_parent(&self.path)
    }
    /// Before the same SQLite transaction installs `next` and commits.
    pub(super) fn prepare(
        &self,
        current: &NativeStorageHeadV2,
        next: &NativeStorageHeadV2,
    ) -> Result<(), String> {
        let mut journal = self.load()?;
        if journal.prepared.is_some() || &journal.stable != current {
            return Err(denied());
        }
        validate_successor(current, next)?;
        journal.prepared = Some(next.clone());
        self.replace(&journal)
    }
    /// Must read the actual committed SQLite marker, not a caller's planned head.
    /// An exact prepared marker can finish the journal. A prepared journal with
    /// the old marker is ambiguous with database rollback and stays blocked; it
    /// is never silently reset as a supposedly aborted transaction.
    pub(super) fn reconcile(&self, committed: &NativeStorageHeadV2) -> Result<(), String> {
        let mut journal = self.load()?;
        if journal.prepared.as_ref() == Some(committed) {
            journal.stable = committed.clone();
        } else if journal.prepared.is_some() || &journal.stable != committed {
            return Err(denied());
        }
        if journal.prepared.take().is_some() {
            self.replace(&journal)?;
        }
        Ok(())
    }
}
fn validate_successor(old: &NativeStorageHeadV2, next: &NativeStorageHeadV2) -> Result<(), String> {
    old.validate()?;
    next.validate()?;
    if next.instance_id != old.instance_id
        || old.revision.0.checked_add(1) != Some(next.revision.0)
        || next.commitment == old.commitment
    {
        return Err(denied());
    }
    Ok(())
}
fn sync_parent(path: &Path) -> Result<(), String> {
    fs::File::open(path.parent().ok_or_else(denied)?)
        .and_then(|f| f.sync_all())
        .map_err(|_| denied())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use oasis7_custody_core::DecimalU64;
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("oasis7-native-guard-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            }
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn head(revision: u64, byte: char) -> NativeStorageHeadV2 {
        NativeStorageHeadV2 {
            instance_id: "a".repeat(64),
            revision: DecimalU64(revision),
            commitment: byte.to_string().repeat(64),
        }
    }
    #[test]
    fn independent_machine_guard_rejects_database_only_backup_rollback() {
        let temp = TempDir::new();
        let path = temp.path().join("outside-database-backup-unit");
        let first = head(0, 'b');
        let next = head(1, 'c');
        let guard = RestoreGuard::create(&path, &first).unwrap();
        guard.reconcile(&first).unwrap();
        guard.prepare(&first, &next).unwrap();
        // Actual transaction has committed next; cold open must retain it.
        RestoreGuard::open(&path).unwrap().reconcile(&next).unwrap();
        assert!(
            RestoreGuard::open(&path)
                .unwrap()
                .reconcile(&first)
                .is_err()
        );
        assert!(RestoreGuard::create(&path, &first).is_err());
    }
    #[test]
    fn ambiguous_old_marker_does_not_clear_prepared_guard() {
        let temp = TempDir::new();
        let path = temp.path().join("guard");
        let old = head(0, 'b');
        let next = head(1, 'c');
        let guard = RestoreGuard::create(&path, &old).unwrap();
        guard.prepare(&old, &next).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(guard.reconcile(&old).is_err());
        assert_eq!(before, fs::read(&path).unwrap());
        guard.reconcile(&next).unwrap();
        assert!(guard.prepare(&next, &head(u64::MAX, 'd')).is_err());
    }
}
