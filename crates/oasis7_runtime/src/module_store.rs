//! Module storage persistence for artifacts and registry.

use oasis7_wasm_store::{ModuleStore as InnerModuleStore, ModuleStoreError};
use std::path::{Path, PathBuf};

use super::error::WorldError;
use super::modules::{ModuleManifest, ModuleRegistry};

/// File-based module store for artifacts, meta, and registry.
#[derive(Debug, Clone)]
pub struct ModuleStore {
    inner: InnerModuleStore,
}

impl ModuleStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            inner: InnerModuleStore::new(root),
        }
    }

    pub fn root(&self) -> &Path {
        self.inner.root()
    }

    pub fn registry_path(&self) -> &Path {
        self.inner.registry_path()
    }

    pub fn modules_dir(&self) -> &Path {
        self.inner.modules_dir()
    }

    pub fn write_artifact(&self, wasm_hash: &str, bytes: &[u8]) -> Result<PathBuf, WorldError> {
        self.inner
            .write_artifact(wasm_hash, bytes)
            .map_err(map_store_error)
    }

    pub fn read_artifact(&self, wasm_hash: &str) -> Result<Vec<u8>, WorldError> {
        self.inner.read_artifact(wasm_hash).map_err(map_store_error)
    }

    pub fn write_meta(&self, manifest: &ModuleManifest) -> Result<PathBuf, WorldError> {
        self.inner.write_meta(manifest).map_err(map_store_error)
    }

    pub fn read_meta(&self, wasm_hash: &str) -> Result<ModuleManifest, WorldError> {
        self.inner.read_meta(wasm_hash).map_err(map_store_error)
    }

    pub fn save_registry(&self, registry: &ModuleRegistry) -> Result<(), WorldError> {
        self.inner.save_registry(registry).map_err(map_store_error)
    }

    pub fn load_registry(&self) -> Result<ModuleRegistry, WorldError> {
        self.inner.load_registry().map_err(map_store_error)
    }
}

fn map_store_error(error: ModuleStoreError) -> WorldError {
    match error {
        ModuleStoreError::VersionMismatch { expected, found } => {
            WorldError::ModuleStoreVersionMismatch { expected, found }
        }
        ModuleStoreError::InvalidArtifactKey(key) => WorldError::ModuleChangeInvalid {
            reason: format!("invalid module artifact key: {key}"),
        },
        ModuleStoreError::Io(message) => WorldError::Io(message),
        ModuleStoreError::Serde(message) => WorldError::Serde(message),
    }
}
