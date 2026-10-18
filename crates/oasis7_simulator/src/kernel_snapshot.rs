//! Native kernel persistence values, independent of host archive extensions.
use super::kernel::{ChunkRuntimeConfig, WorldEventKind};
use super::types::{
    ActionEnvelope, ActionId, CHUNK_GENERATION_SCHEMA_VERSION, JOURNAL_VERSION, SNAPSHOT_VERSION,
    WorldEventId, WorldTime,
};
use super::world_model::{WorldConfig, WorldModel};
use serde::{Deserialize, Serialize};

fn snapshot_version() -> u32 {
    SNAPSHOT_VERSION
}
fn journal_version() -> u32 {
    JOURNAL_VERSION
}
fn generation_version() -> u32 {
    CHUNK_GENERATION_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelSnapshot {
    #[serde(default = "snapshot_version")]
    pub version: u32,
    #[serde(default = "generation_version")]
    pub chunk_generation_schema_version: u32,
    pub time: WorldTime,
    pub config: WorldConfig,
    pub model: WorldModel,
    #[serde(default)]
    pub chunk_runtime: ChunkRuntimeConfig,
    #[serde(default)]
    pub intel_ttl_ticks: WorldTime,
    pub next_event_id: WorldEventId,
    pub next_action_id: ActionId,
    pub pending_actions: Vec<ActionEnvelope>,
    pub journal_len: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelEvent {
    pub id: WorldEventId,
    pub time: WorldTime,
    pub kind: WorldEventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelJournal {
    #[serde(default = "journal_version")]
    pub version: u32,
    pub events: Vec<KernelEvent>,
}

impl KernelSnapshot {
    pub fn validate_version(&self) -> Result<(), super::persist_error::PersistError> {
        validate_version("snapshot", self.version, SNAPSHOT_VERSION)
    }
}
impl KernelJournal {
    pub fn validate_version(&self) -> Result<(), super::persist_error::PersistError> {
        validate_version("journal", self.version, JOURNAL_VERSION)
    }
}
fn validate_version(
    kind: &str,
    version: u32,
    expected: u32,
) -> Result<(), super::persist_error::PersistError> {
    if version == expected || version == expected.saturating_sub(1) {
        Ok(())
    } else {
        Err(super::persist_error::PersistError::UnsupportedVersion {
            kind: kind.into(),
            version,
            expected,
        })
    }
}
