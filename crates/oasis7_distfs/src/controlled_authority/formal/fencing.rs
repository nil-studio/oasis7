//! Operational fencing adapters; all wire types and checked evidence are shared.
use crate::controlled_authority::replicated_protocol::EndpointRole;
use ed25519_dalek::SigningKey;
pub use oasis7_formal_protocol::controlled_authority::fencing::*;
use oasis7_formal_protocol::controlled_authority::fencing::{
    encoding_bytes as bytes, sign_fencing_body as sign,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[path = "fencing_provider.rs"]
mod provider;
pub use provider::ControlledFileProvider;
#[path = "fencing_remote.rs"]
mod remote;
pub use remote::*;

pub trait EndpointInstanceBindingDirectoryExt {
    fn verify_directory(&self, path: &Path) -> Result<(), FencingError>;
}
impl EndpointInstanceBindingDirectoryExt for EndpointInstanceBindingV1 {
    fn verify_directory(&self, path: &Path) -> Result<(), FencingError> {
        if self.storage_fingerprint != directory_fingerprint(path)? {
            return Err(invalid("endpoint storage instance mismatch"));
        }
        Ok(())
    }
}
/// Controlled resource locator only. Device/inode/path checks detect copies,
/// not rollback within the same directory or cloned private keys.
pub fn directory_fingerprint(path: &Path) -> Result<String, FencingError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| FencingError::Io(e.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("endpoint instance directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        hash_value(&(
            std::fs::canonicalize(path)
                .map_err(|e| FencingError::Io(e.to_string()))?
                .to_string_lossy(),
            metadata.dev(),
            metadata.ino(),
        ))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Err(FencingError::FencingUnproven)
    }
}
pub(crate) fn fresh_challenge() -> Result<String, FencingError> {
    #[cfg(unix)]
    {
        use std::io::Read;
        fresh_challenge_with_reader(|bytes| {
            std::fs::File::open("/dev/urandom")
                .and_then(|mut file| file.read_exact(bytes))
                .map_err(|error| FencingError::Io(error.to_string()))
        })
    }
    #[cfg(not(unix))]
    {
        Err(FencingError::FencingUnproven)
    }
}

/// Keep the original 32-byte challenge encoding while the caller accounts for
/// actual source operations. This helper supplies no fencing evidence.
pub(super) fn fresh_challenge_with_reader<E>(
    read: impl FnOnce(&mut [u8; 32]) -> Result<(), E>,
) -> Result<String, E> {
    let mut bytes = [0; 32];
    read(&mut bytes)?;
    Ok(hex::encode(bytes))
}

#[cfg(test)]
#[path = "fencing_tests.rs"]
mod tests;
