//! Runtime error translation for the host-side artifact materializer.
use std::path::{Path, PathBuf};

use super::WorldError;
use oasis7_wasm_materializer::MaterializeError;

pub(crate) fn builtin_wasm_distfs_root() -> PathBuf {
    oasis7_wasm_materializer::builtin_wasm_distfs_root(&repo_root())
}

pub(crate) fn load_builtin_wasm_with_fetch_fallback(
    module_id: &str,
    expected_hashes: &[&str],
    distfs_root: &Path,
) -> Result<Vec<u8>, WorldError> {
    oasis7_wasm_materializer::load_builtin_wasm_with_fetch_fallback(
        module_id,
        expected_hashes,
        distfs_root,
        &repo_root(),
    )
    .map_err(materialize_world_error)
}

fn materialize_world_error(error: MaterializeError) -> WorldError {
    match error {
        MaterializeError::ModuleChangeInvalid { reason } => {
            WorldError::ModuleChangeInvalid { reason }
        }
        MaterializeError::Io(message) => WorldError::Io(message),
        MaterializeError::Store(error) => WorldError::from(error),
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_failures_preserve_runtime_error_contracts() {
        assert!(
            matches!(materialize_world_error(MaterializeError::ModuleChangeInvalid { reason: "invalid".into() }),
            WorldError::ModuleChangeInvalid { reason } if reason == "invalid")
        );
        assert!(
            matches!(materialize_world_error(MaterializeError::Io("io".into())),
            WorldError::Io(message) if message == "io")
        );
        assert!(matches!(materialize_world_error(MaterializeError::Store(
            oasis7_proto::world_error::WorldError::BlobHashMismatch { expected: "expected".into(), actual: "actual".into() })),
            WorldError::BlobHashMismatch { expected, actual } if expected == "expected" && actual == "actual"));
    }
}
