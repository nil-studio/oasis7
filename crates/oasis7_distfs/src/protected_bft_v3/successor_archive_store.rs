//! Durable, content-addressed public material for successor-window recovery.
//! Storage is not authority: the provider must verify the returned state from
//! its admitted H and protected predecessor before adopting a checkpoint.
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path};

use oasis7_cas::{BlobStore, HashAlgorithm, LocalCasStore, decode_local_blob_into};

use super::binding::{BftSignerErrorV3, invalid};
use super::crypto::{BFT_SIGNER_MAX_FRAME_BYTES_V3, decode_bounded, encode_bounded};
use super::state::BftSignerStateV3;
use super::successor_window::BftSuccessorWindowArchiveRefV1;

/// Implementations must publish immutable bytes durably and verify reads.
/// The independently protected minimum remains in the signer backend CAS;
/// possession of an archive, or its path, cannot replace that minimum.
pub trait BftSuccessorArchiveStoreV1: Send + Sync {
    fn persist_public_state(
        &self,
        state: &BftSignerStateV3,
    ) -> Result<BftSuccessorWindowArchiveRefV1, BftSignerErrorV3>;

    /// Returns decoded data, not a verified schedule or signing capability.
    fn load_public_state(
        &self,
        reference: &BftSuccessorWindowArchiveRefV1,
    ) -> Result<BftSignerStateV3, BftSignerErrorV3>;
}

/// Uses the existing CAS retention lock and compression format. This adapter
/// additionally fsyncs the material, pin and directory publication before a
/// provider may advance its independently protected minimum.
pub struct LocalBftSuccessorArchiveStoreV1 {
    store: LocalCasStore,
}

impl LocalBftSuccessorArchiveStoreV1 {
    /// Storage location only, for deployment path separation checks.
    pub fn root(&self) -> &Path {
        self.store.root()
    }

    pub fn new(store: LocalCasStore) -> Result<Self, BftSignerErrorV3> {
        if store.hash_algorithm() != HashAlgorithm::Blake3
            || store
                .root()
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(invalid(
                "successor archive requires a BLAKE3 CAS without parent traversal",
            ));
        }
        let absolute = std::path::absolute(store.root()).map_err(io_error)?;
        // Normalize a trusted configured path once (including OS aliases such
        // as /var on Darwin). Every later object open rejects link traversal.
        let root = physical_root(&absolute)?;
        Ok(Self {
            store: LocalCasStore::new(root),
        })
    }

    fn persist_bytes(
        &self,
        bytes: &[u8],
    ) -> Result<BftSuccessorWindowArchiveRefV1, BftSignerErrorV3> {
        self.persist_object(bytes, BFT_SIGNER_MAX_FRAME_BYTES_V3)
    }

    /// Durable raw material for a caller's separately configured archive.
    /// This grants no remote-read permission or checkpoint authority. The
    /// caller must choose the limit for the artifact's declared role.
    pub fn persist_object(
        &self,
        bytes: &[u8],
        maximum: usize,
    ) -> Result<BftSuccessorWindowArchiveRefV1, BftSignerErrorV3> {
        validate_object_limit(maximum)?;
        if bytes.is_empty() || bytes.len() > maximum {
            return Err(invalid("successor public archive length bound"));
        }
        reject_link_components(self.store.root())?;
        reject_link_components(self.store.blobs_dir())?;
        reject_link_components(self.store.pins_path())?;
        let mut directories = vec![self.store.blobs_dir().to_path_buf()];
        // Include the first existing ancestor: newly created directories must
        // themselves be durably published by their parent before CAS commits.
        for directory in self.store.root().ancestors() {
            directories.push(directory.to_path_buf());
            match fs::symlink_metadata(directory) {
                Ok(metadata) if metadata.is_dir() => break,
                Ok(_) => return Err(invalid("successor archive ancestor is not a directory")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error)),
            }
        }
        let reference = BftSuccessorWindowArchiveRefV1 {
            digest: blake3::hash(bytes).to_hex().to_string(),
            bytes: u64::try_from(bytes.len())
                .map_err(|_| invalid("successor archive length overflow"))?,
        };
        self.store
            .put(&reference.digest, bytes)
            .map_err(cas_error)?;
        self.store.pin(&reference.digest).map_err(cas_error)?;
        sync_regular_file(
            &self
                .store
                .blobs_dir()
                .join(format!("{}.blob", reference.digest)),
        )?;
        sync_regular_file(self.store.pins_path())?;
        for directory in directories {
            if !fs::symlink_metadata(&directory).map_err(io_error)?.is_dir() {
                return Err(invalid("successor archive directory publication changed"));
            }
            sync_directory(&directory)?;
        }
        // CAS put may encounter an existing object. It is not a successful
        // checkpoint preparation unless a fresh bounded reread is exact.
        if self.load_object(&reference, maximum)? != bytes {
            return Err(invalid(
                "successor archive reread differs from original material",
            ));
        }
        Ok(reference)
    }

    fn load_bytes(
        &self,
        reference: &BftSuccessorWindowArchiveRefV1,
    ) -> Result<Vec<u8>, BftSignerErrorV3> {
        self.load_object(reference, BFT_SIGNER_MAX_FRAME_BYTES_V3)
    }

    /// Fresh no-follow read of exact content-addressed bytes. It never returns
    /// a trusted Snapshot, schedule, or signing capability.
    pub fn load_object(
        &self,
        reference: &BftSuccessorWindowArchiveRefV1,
        maximum: usize,
    ) -> Result<Vec<u8>, BftSignerErrorV3> {
        validate_object_limit(maximum)?;
        reference.validate()?;
        let size = usize::try_from(reference.bytes)
            .map_err(|_| invalid("successor archive read length overflow"))?;
        if size > maximum {
            return Err(invalid("successor public archive read bound"));
        }
        let path = self
            .store
            .blobs_dir()
            .join(format!("{}.blob", reference.digest));
        let file = open_regular_file(&path, false)?;
        let encoded_limit = reference
            .bytes
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(4096))
            .ok_or_else(|| invalid("successor archive encoded length overflow"))?;
        if file.metadata().map_err(io_error)?.len() > encoded_limit {
            return Err(invalid("successor archive encoded length bound"));
        }
        // Read a fresh descriptor, bypassing CAS's decoded range cache. A
        // cached correct object cannot prove that today's disk publication
        // will survive the checkpoint/minimum transaction or a cold reopen.
        let mut bytes = Vec::new();
        decode_local_blob_into(
            HashAlgorithm::Blake3,
            &reference.digest,
            file.take(encoded_limit),
            &mut bytes,
            size,
        )
        .map_err(cas_error)?;
        reference.validate_bytes(&bytes)?;
        Ok(bytes)
    }
}

fn validate_object_limit(maximum: usize) -> Result<(), BftSignerErrorV3> {
    if maximum == 0 || maximum > 512 * 1024 * 1024 {
        return Err(invalid("successor object limit must be within 512 MiB"));
    }
    Ok(())
}

fn physical_root(path: &Path) -> Result<std::path::PathBuf, BftSignerErrorV3> {
    let mut existing = path;
    let mut suffix = Vec::new();
    loop {
        match fs::symlink_metadata(existing) {
            Ok(_) => {
                let mut root = existing.canonicalize().map_err(io_error)?;
                if !root.is_dir() {
                    return Err(invalid("successor archive ancestor is not a directory"));
                }
                for component in suffix.into_iter().rev() {
                    root.push(component);
                }
                return Ok(root);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(
                    existing
                        .file_name()
                        .ok_or_else(|| invalid("archive root name"))?
                        .to_os_string(),
                );
                existing = existing
                    .parent()
                    .ok_or_else(|| invalid("archive root parent"))?;
            }
            Err(error) => return Err(io_error(error)),
        }
    }
}

fn reject_link_components(path: &Path) -> Result<(), BftSignerErrorV3> {
    for component in path.ancestors() {
        let metadata = match fs::symlink_metadata(component) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io_error(error)),
        };
        if metadata.file_type().is_symlink() {
            return Err(invalid("successor archive path contains a link"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(invalid("successor archive path contains a reparse point"));
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn open_directory_nofollow(path: &Path) -> Result<File, BftSignerErrorV3> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    if !path.is_absolute() {
        return Err(invalid("successor archive directory is not absolute"));
    }
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    let mut directory = fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open("/")
        .map_err(io_error)?;
    for component in path.components() {
        let name = match component {
            Component::RootDir => continue,
            Component::Normal(name) => {
                CString::new(name.as_bytes()).map_err(|_| invalid("archive path contains NUL"))?
            }
            _ => return Err(invalid("successor archive path is not normalized")),
        };
        // SAFETY: the parent fd and NUL-terminated component remain live for
        // openat. Each owned result is checked before taking ownership.
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        // SAFETY: successful openat returned a new fd owned by this function.
        directory = unsafe { File::from_raw_fd(fd) };
    }
    Ok(directory)
}

impl BftSuccessorArchiveStoreV1 for LocalBftSuccessorArchiveStoreV1 {
    fn persist_public_state(
        &self,
        state: &BftSignerStateV3,
    ) -> Result<BftSuccessorWindowArchiveRefV1, BftSignerErrorV3> {
        self.persist_bytes(&encode_bounded(state, BFT_SIGNER_MAX_FRAME_BYTES_V3)?)
    }

    fn load_public_state(
        &self,
        reference: &BftSuccessorWindowArchiveRefV1,
    ) -> Result<BftSignerStateV3, BftSignerErrorV3> {
        decode_bounded(&self.load_bytes(reference)?, BFT_SIGNER_MAX_FRAME_BYTES_V3)
    }
}

fn sync_regular_file(path: &Path) -> Result<(), BftSignerErrorV3> {
    open_regular_file(path, true)?.sync_all().map_err(io_error)
}

fn open_regular_file(path: &Path, for_sync: bool) -> Result<File, BftSignerErrorV3> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::fd::{AsRawFd, FromRawFd};
        use std::os::unix::ffi::OsStrExt;
        let _ = for_sync; // Unix permits fsync on the existing read descriptor.
        let parent = open_directory_nofollow(
            path.parent()
                .ok_or_else(|| invalid("archive file parent"))?,
        )?;
        let name = CString::new(
            path.file_name()
                .ok_or_else(|| invalid("archive file name"))?
                .as_bytes(),
        )
        .map_err(|_| invalid("archive path contains NUL"))?;
        // SAFETY: parent and component remain live for the call. Relative
        // no-follow traversal prevents a replaced blobs directory alias.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        // SAFETY: the checked openat result is a new owned file descriptor.
        let file = unsafe { File::from_raw_fd(fd) };
        if !file.metadata().map_err(io_error)?.is_file() {
            return Err(invalid(
                "successor archive publication is not a regular file",
            ));
        }
        Ok(file)
    }
    #[cfg(windows)]
    {
        reject_link_components(path)?;
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        let file = fs::OpenOptions::new()
            .read(true)
            .write(for_sync) // FlushFileBuffers requires write access.
            .custom_flags(0x0020_0000) // FILE_FLAG_OPEN_REPARSE_POINT
            .open(path)
            .map_err(io_error)?;
        let metadata = file.metadata().map_err(io_error)?;
        if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid(
                "successor archive publication is not a regular file",
            ));
        }
        Ok(file)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, for_sync);
        Err(invalid("successor archive requires no-follow file handles"))
    }
}

fn sync_directory(path: &Path) -> Result<(), BftSignerErrorV3> {
    #[cfg(unix)]
    let directory = open_directory_nofollow(path)?;
    #[cfg(windows)]
    let directory = {
        reject_link_components(path)?;
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        // Match the durable-transaction directory handle contract; also refuse
        // reparse points instead of silently syncing their destination.
        let directory = fs::OpenOptions::new()
            .write(true)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .map_err(io_error)?;
        if directory.metadata().map_err(io_error)?.file_attributes() & 0x400 != 0 {
            return Err(invalid("successor archive directory is a reparse point"));
        }
        directory
    };
    #[cfg(any(unix, windows))]
    {
        if !directory.metadata().map_err(io_error)?.is_dir() {
            return Err(invalid("successor archive directory handle changed"));
        }
        directory.sync_all().map_err(io_error)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Err(invalid(
            "successor archive requires durable directory handles",
        ))
    }
}

fn io_error(error: std::io::Error) -> BftSignerErrorV3 {
    invalid(format!("successor archive I/O: {error}"))
}

fn cas_error(error: oasis7_proto::world_error::WorldError) -> BftSignerErrorV3 {
    invalid(format!("successor archive CAS: {error:?}"))
}

#[cfg(test)]
#[path = "successor_archive_store_tests.rs"]
mod tests;
