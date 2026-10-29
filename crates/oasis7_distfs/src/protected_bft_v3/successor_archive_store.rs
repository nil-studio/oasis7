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
        let root = std::path::absolute(store.root()).map_err(io_error)?;
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
        use std::os::unix::fs::OpenOptionsExt;
        let _ = for_sync; // Unix permits fsync on the existing read descriptor.
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(io_error)?;
        if !file.metadata().map_err(io_error)?.is_file() {
            return Err(invalid(
                "successor archive publication is not a regular file",
            ));
        }
        Ok(file)
    }
    #[cfg(windows)]
    {
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
    let directory = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(io_error)?
    };
    #[cfg(windows)]
    let directory = {
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
