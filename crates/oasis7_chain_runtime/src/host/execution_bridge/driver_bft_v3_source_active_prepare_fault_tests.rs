//! Fault only local authenticated metadata under the original real active lease.
use super::*;
use std::fs::{File, OpenOptions, Permissions};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{FileExt, MetadataExt, PermissionsExt};

struct RestoreBase {
    original_file: File,
    writable: Option<File>,
    bytes: Vec<u8>,
    mode: u32,
    device: u64,
    inode: u64,
    restored: bool,
}
impl RestoreBase {
    fn corrupt(path: &Path) -> std::io::Result<Self> {
        let metadata = std::fs::symlink_metadata(path)?;
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        let mut original_file = File::open(path)?;
        let actual = original_file.metadata()?;
        assert_eq!(
            (actual.dev(), actual.ino()),
            (metadata.dev(), metadata.ino())
        );
        let mut bytes = Vec::new();
        original_file.read_to_end(&mut bytes)?;
        assert!(!bytes.is_empty());
        // Arm cleanup before even changing mode. This is fault injection only;
        // production readonly/ownership rules remain untouched.
        let mut guard = Self {
            original_file,
            writable: None,
            bytes,
            mode: actual.mode(),
            device: actual.dev(),
            inode: actual.ino(),
            restored: false,
        };
        guard
            .original_file
            .set_permissions(Permissions::from_mode(guard.mode | 0o200))?;
        let writable = OpenOptions::new().read(true).write(true).open(path)?;
        let opened = writable.metadata()?;
        assert_eq!((opened.dev(), opened.ino()), (guard.device, guard.inode));
        guard.writable = Some(writable);
        // Existing fd writes remain possible; the verifier sees the ORIGINAL
        // readonly mode, so it must reject bytes rather than a permission fault.
        guard
            .original_file
            .set_permissions(Permissions::from_mode(guard.mode))?;
        let file = guard.writable.as_ref().unwrap();
        file.write_all_at(&[guard.bytes[0] ^ 1], 0)?;
        file.sync_all()?;
        assert_eq!(file.metadata()?.mode(), guard.mode);
        Ok(guard)
    }
    fn restore(&mut self) -> std::io::Result<()> {
        if let Some(file) = &self.writable {
            file.write_all_at(&self.bytes, 0)?;
            file.sync_all()?;
        }
        self.original_file
            .set_permissions(Permissions::from_mode(self.mode))?;
        self.original_file.sync_all()?;
        let metadata = self.original_file.metadata()?;
        assert_eq!((metadata.dev(), metadata.ino()), (self.device, self.inode));
        assert_eq!(metadata.mode(), self.mode);
        assert_eq!(metadata.len(), self.bytes.len() as u64);
        self.original_file.seek(SeekFrom::Start(0))?;
        let mut restored = Vec::new();
        self.original_file.read_to_end(&mut restored)?;
        assert_eq!(restored, self.bytes);
        self.restored = true;
        Ok(())
    }
}
impl Drop for RestoreBase {
    fn drop(&mut self) {
        if !self.restored {
            // Never assert/panic during unwinding. Attempt byte restoration and
            // original mode independently, even if either IO operation fails.
            if let Some(file) = &self.writable {
                let _ = file.write_all_at(&self.bytes, 0);
                let _ = file.sync_all();
            }
            let _ = self
                .original_file
                .set_permissions(Permissions::from_mode(self.mode));
            let _ = self.original_file.sync_all();
        }
    }
}

pub(super) fn check(
    hook: &oasis7_node::NodeBftSharedExecutionHookV3,
    binding: &BftSignerBindingV3,
    lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
    backend: &mut BftPrivateRetentionMemoryBackendV1,
    base: &Path,
    signatures: &std::sync::atomic::AtomicUsize,
    deadline: Instant,
) {
    let digest = binding.digest().unwrap();
    let before = backend.load_private_retention_current(&digest).unwrap();
    let signed_before = signatures.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(signed_before, 0);
    let mut restoration = RestoreBase::corrupt(base).unwrap();
    let outcome = hook
        .lock()
        .unwrap()
        .prepare_bft_v3_execution(binding, lease, deadline);
    // Restore exact bytes and original mode on the same inode BEFORE asserting
    // either the expected denial or the backend invariants.
    restoration.restore().unwrap();
    assert_eq!(
        outcome.unwrap_err(),
        "active private base changed after open"
    );
    let after = backend.load_private_retention_current(&digest).unwrap();
    assert_eq!(
        before, after,
        "prepare denial changed the actual backend snapshot"
    );
    for (digest, original) in &before.inputs {
        assert!(Arc::ptr_eq(original, after.inputs.get(digest).unwrap()));
    }
    assert_eq!(
        signatures.load(std::sync::atomic::Ordering::SeqCst),
        signed_before
    );
    // This performs real checkpoint RPCs/provider signatures; it is NOT a
    // zero-RPC assertion. No validator sign/admission/Acquire operation occurs.
}
