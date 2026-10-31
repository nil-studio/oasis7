use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

use super::provider_original_store_codec::{MAX_HEAD_BYTES, OwnerHead, decode_head, encode_head};

const FORMAT: &[u8] = b"oasis7.iws.provider-pending-store.v1\n";
const LOCK_NAME: &str = "owner.lock";
const HEAD_NAME: &str = "HEAD";
const FORMAT_NAME: &str = "format";

pub(super) struct OwnerFiles {
    root: PathBuf,
    _lock: File,
}

impl OwnerFiles {
    pub(super) fn create(root: &Path) -> Result<Self, String> {
        validate_path(root)?;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder
            .create(root)
            .map_err(|error| format!("provider owner directory create: {error}"))?;
        let lock = create_lock(root)?;
        create_regular(root.join(FORMAT_NAME).as_path(), FORMAT)?;
        sync_directory(root)?;
        sync_parent_directory(root)?;
        Ok(Self {
            root: root.to_owned(),
            _lock: lock,
        })
    }

    pub(super) fn open(root: &Path) -> Result<Self, String> {
        validate_path(root)?;
        validate_directory(root)?;
        let lock = open_lock(root)?;
        let format = read_small(root.join(FORMAT_NAME).as_path(), 64)?;
        if format.as_slice() != FORMAT {
            return Err("provider owner format capability".into());
        }
        sync_file(root.join(FORMAT_NAME).as_path())?;
        sync_directory(root)?;
        Ok(Self {
            root: root.to_owned(),
            _lock: lock,
        })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn sync_root(&self) -> Result<(), String> {
        sync_directory(&self.root)
    }

    pub(super) fn read_head(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<ReservedValue<Vec<u8>>, String> {
        let path = self.root.join(HEAD_NAME);
        let mut file = open_regular(&path, false)?;
        let length = usize::try_from(
            file.metadata()
                .map_err(|error| format!("provider HEAD metadata: {error}"))?
                .len(),
        )
        .map_err(|_| "provider HEAD length")?;
        if length == 0 || length > MAX_HEAD_BYTES {
            return Err("provider HEAD bound".into());
        }
        let charge = memory
            // The caller retains the encoded bytes while decode_head builds
            // and compares a second bounded canonical representation.
            .reserve(MAX_HEAD_BYTES.saturating_mul(2), class)
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::with_capacity(length);
        file.read_to_end(&mut bytes)
            .map_err(|error| format!("provider HEAD read: {error}"))?;
        if bytes.len() != length || bytes.capacity() > MAX_HEAD_BYTES {
            return Err("provider HEAD read length".into());
        }
        Ok(charge.retain(bytes))
    }

    pub(super) fn publish_head(
        &self,
        head: &OwnerHead,
        memory: &MemoryBudget,
    ) -> Result<(), String> {
        // Encoding, readback and decode/re-encode coexist during the durable
        // publication check. Admit their fixed maximum before any allocation.
        let _workspace = memory
            .reserve(MAX_HEAD_BYTES.saturating_mul(3), MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let encoded = encode_head(head)?;
        let mut random = [0u8; 12];
        getrandom::fill(&mut random).map_err(|error| format!("provider HEAD nonce: {error}"))?;
        let temporary = self.root.join(format!(
            ".head-{}-{}",
            std::process::id(),
            hex::encode(random)
        ));
        let mut file = create_new_regular(&temporary)?;
        file.write_all(&encoded)
            .map_err(|error| format!("provider HEAD write: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("provider HEAD fsync: {error}"))?;
        fs::rename(&temporary, self.root.join(HEAD_NAME))
            .map_err(|error| format!("provider HEAD atomic replace: {error}"))?;
        sync_directory(&self.root)?;
        let readback = read_small(&self.root.join(HEAD_NAME), MAX_HEAD_BYTES)?;
        if readback != encoded || decode_head(&readback)? != *head {
            return Err("provider HEAD durable readback mismatch".into());
        }
        Ok(())
    }
}

fn validate_path(root: &Path) -> Result<(), String> {
    if !root.is_absolute() || root.as_os_str().len() > 4096 {
        return Err("provider owner path must be absolute and bounded".into());
    }
    Ok(())
}

fn validate_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("provider owner directory metadata: {error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("provider owner path is not a regular directory".into());
    }
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
        return Err("provider owner directory owner/mode".into());
    }
    Ok(())
}

fn open_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    options
}

fn open_regular(path: &Path, write: bool) -> Result<File, String> {
    let before =
        fs::symlink_metadata(path).map_err(|error| format!("provider file metadata: {error}"))?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err("provider path is not a regular file".into());
    }
    if before.uid() != unsafe { libc::geteuid() } || before.mode() & 0o022 != 0 {
        return Err("provider file owner/mode".into());
    }
    let mut options = open_options();
    options.read(true).write(write);
    let file = options
        .open(path)
        .map_err(|error| format!("provider file open: {error}"))?;
    let after = file
        .metadata()
        .map_err(|error| format!("provider file handle metadata: {error}"))?;
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err("provider file changed during open".into());
    }
    Ok(file)
}

fn create_new_regular(path: &Path) -> Result<File, String> {
    open_options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("provider exclusive file create: {error}"))
}

fn create_regular(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = create_new_regular(path)?;
    file.write_all(bytes)
        .map_err(|error| format!("provider fixed file write: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("provider fixed file fsync: {error}"))
}

fn create_lock(root: &Path) -> Result<File, String> {
    let path = root.join(LOCK_NAME);
    let file = create_new_regular(&path)?;
    file.try_lock()
        .map_err(|error| format!("provider owner lock: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("provider lock fsync: {error}"))?;
    sync_directory(root)?;
    Ok(file)
}

fn open_lock(root: &Path) -> Result<File, String> {
    let file = open_regular(&root.join(LOCK_NAME), true)?;
    file.try_lock()
        .map_err(|error| format!("provider owner lock: {error}"))?;
    Ok(file)
}

fn sync_file(path: &Path) -> Result<(), String> {
    let file = open_regular(path, false)?;
    file.sync_all()
        .map_err(|error| format!("provider file sync: {error}"))
}

fn sync_directory(path: &Path) -> Result<(), String> {
    validate_directory(path)?;
    let mut options = OpenOptions::new();
    options.read(true).custom_flags(libc::O_NOFOLLOW);
    let file = options
        .open(path)
        .map_err(|error| format!("provider directory open: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("provider directory fsync: {error}"))
}

fn sync_parent_directory(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("provider owner parent directory missing")?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY);
    let file = options
        .open(parent)
        .map_err(|error| format!("provider owner parent open: {error}"))?;
    if !file
        .metadata()
        .map_err(|error| format!("provider owner parent metadata: {error}"))?
        .is_dir()
    {
        return Err("provider owner parent is not a directory".into());
    }
    file.sync_all()
        .map_err(|error| format!("provider owner parent fsync: {error}"))
}

fn read_small(path: &Path, bound: usize) -> Result<Vec<u8>, String> {
    let mut file = open_regular(path, false)?;
    let length = usize::try_from(
        file.metadata()
            .map_err(|error| format!("provider file length: {error}"))?
            .len(),
    )
    .map_err(|_| "provider file address space")?;
    if length == 0 || length > bound {
        return Err("provider file bound".into());
    }
    let mut bytes = Vec::with_capacity(length);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("provider bounded read: {error}"))?;
    if bytes.len() != length || bytes.capacity() > bound {
        return Err("provider file read length".into());
    }
    Ok(bytes)
}
