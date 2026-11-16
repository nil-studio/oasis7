//! Local identity of the executable actually loaded by this process.
//!
//! This is neither remote attestation nor a custody authorization token.
//! Keep this facade stable: host consumers need no Runtime or network dependency.
use std::fs::{File, Metadata};

mod digest;
// Keep the platform source at its already trusted path for the first policy
// transition. Only this crate compiles it; Chain Runtime uses the facade below.
#[path = "../../oasis7_chain_runtime/src/host/executable_image/mod.rs"]
mod executable_image;

/// Open a descriptor bound to the loaded executable, rejecting unsupported hosts.
pub fn open_loaded_image() -> Result<File, String> {
    executable_image::open_loaded_image()
}

/// Verify that a descriptor still identifies this process's loaded executable.
pub fn validate_loaded_file(file: &File) -> Result<(), String> {
    executable_image::validate_loaded_file(file)
}

/// Compare file identity, size and change timestamps around a protected read.
pub fn unchanged(before: &Metadata, after: &Metadata) -> bool {
    executable_image::unchanged(before, after)
}

/// Hash the loaded executable once per process, retaining validation failures.
pub fn executable_digest() -> Result<String, String> {
    digest::executable_digest()
}

// The unchanged copied-process fixture uses the original internal policy path.
#[cfg(test)]
mod controlled_execution_policy {
    pub(crate) use crate::executable_digest;
}
