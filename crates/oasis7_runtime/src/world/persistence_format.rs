//! Selector for the durable World layout consumed by `World::load_from_dir`.
//!
//! The selector is deliberately not an authority or a claim about chain
//! compatibility. It makes the persistence decoder's supported layout
//! explicit so a runtime can reject an unknown on-disk version before it
//! attempts recovery or writes anything back.

use super::World;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

const SELECTOR_FILE: &str = "world-format.json";
const SELECTOR_API: &str = "oasis7.runtime.world-persistence-selector/v1";
const LEGACY_FORMAT: u32 = 0;
const CURRENT_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldPersistenceSelectorV1 {
    api_version: String,
    format_version: u32,
    snapshot_decoder: String,
    journal_decoder: String,
    sidecar_decoder: String,
    module_store_decoder: String,
}

impl WorldPersistenceSelectorV1 {
    fn current() -> Self {
        Self {
            api_version: SELECTOR_API.into(),
            format_version: CURRENT_FORMAT,
            snapshot_decoder: World::SNAPSHOT_PERSISTENCE_DECODER.into(),
            journal_decoder: World::JOURNAL_PERSISTENCE_DECODER.into(),
            sidecar_decoder: World::SIDECAR_PERSISTENCE_DECODER.into(),
            module_store_decoder: World::MODULE_STORE_PERSISTENCE_DECODER.into(),
        }
    }

    fn validate(&self) -> Result<(), String> {
        let expected = Self::current();
        if self != &expected {
            return Err(format!(
                "unsupported World persistence selector: version={} api_version={}",
                self.format_version, self.api_version
            ));
        }
        Ok(())
    }
}

impl World {
    pub const SNAPSHOT_PERSISTENCE_DECODER: &'static str = "oasis7.runtime.snapshot-json/v1";
    pub const JOURNAL_PERSISTENCE_DECODER: &'static str = "oasis7.runtime.journal-json/v1";
    pub const SIDECAR_PERSISTENCE_DECODER: &'static str = "oasis7.runtime.distfs-world-sidecar/v1";
    pub const MODULE_STORE_PERSISTENCE_DECODER: &'static str = "oasis7.runtime.module-store/v1";

    /// The selector version written before this decoder emits durable bytes.
    /// The runtime compatibility command reports this same value.
    pub const fn current_persistence_format_version() -> u32 {
        CURRENT_FORMAT
    }

    /// Unmarked snapshots predate the selector and are parsed by the legacy
    /// decoder path. The value is stable and is not a chain-authority claim.
    pub const fn legacy_persistence_format_version() -> u32 {
        LEGACY_FORMAT
    }

    /// Public identifier shared by the write selector and the executable's
    /// read-only compatibility descriptor.
    pub const fn persistence_selector_api() -> &'static str {
        SELECTOR_API
    }
}

pub(super) fn write_selector_before_payloads(dir: &Path) -> Result<(), super::WorldError> {
    validate_selector(dir).map_err(format_error)?;
    super::super::super::util::atomic_write_json_to_path(
        &WorldPersistenceSelectorV1::current(),
        dir.join(SELECTOR_FILE).as_path(),
    )
}

pub(super) fn validate_selector_before_load(dir: &Path) -> Result<(), super::WorldError> {
    validate_selector(dir).map_err(format_error)
}

fn validate_selector(dir: &Path) -> Result<(), String> {
    let selector_path = dir.join(SELECTOR_FILE);
    match fs::symlink_metadata(&selector_path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err("World persistence selector must be a regular file".into())
        }
        Ok(_) => {
            let selector: WorldPersistenceSelectorV1 =
                super::super::super::util::read_json_from_path(selector_path.as_path())
                    .map_err(|error| format!("read World persistence selector: {error:?}"))?;
            selector.validate()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect World persistence selector: {error}")),
    }
}

fn format_error(reason: String) -> super::WorldError {
    super::WorldError::DistributedValidationFailed { reason }
}

#[cfg(test)]
#[path = "persistence_format_tests.rs"]
mod tests;
