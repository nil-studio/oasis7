//! Resource manifest contracts and canonical encoding.
use super::FragmentElementKind;
use crate::world_action::{MaterialLedgerId, ResourceKind};
use crate::world_geometry::ChunkCoord;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
type WorldTime = u64;
type WorldEventId = u64;
type ActionId = u64;
pub const CHAIN_RESOURCE_MANIFEST_SCHEMA_V1: &str = "oasis7.world_resource_manifest.v1";
pub const CHAIN_RESOURCE_DELTA_SCHEMA_V1: &str = "oasis7.world_resource_delta.v1";
pub const CHUNK_GENERATION_SCHEMA_V1: &str = "oasis7.chunk_generation.v1";

pub(crate) fn hash_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(value)?;
    Ok(sha256_hex(&bytes))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainResourceManifest {
    pub schema_version: String,
    pub world_id: String,
    pub chain_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genesis_ref: Option<String>,
    pub world_seed: u64,
    pub chunk_generation_schema_version: String,
    #[serde(default)]
    pub world_config_hash: String,
    #[serde(default)]
    pub generation_algorithm_id: String,
    #[serde(default)]
    pub generation_algorithm_hash: String,
    pub created_at_height: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at_block_hash: Option<String>,
    pub manifest_height: u64,
    pub manifest_hash: String,
    pub resource_balances: BTreeMap<ResourceKind, i64>,
    pub material_ledgers: BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    #[serde(default)]
    pub generated_chunks: BTreeMap<String, ChainChunkResourceManifestEntry>,
}

impl ChainResourceManifest {
    pub fn empty_at_height(world_id: impl Into<String>, world_seed: u64, height: u64) -> Self {
        let mut manifest = Self {
            schema_version: CHAIN_RESOURCE_MANIFEST_SCHEMA_V1.to_string(),
            world_id: world_id.into(),
            chain_id: "unbound".to_string(),
            genesis_ref: None,
            world_seed,
            chunk_generation_schema_version: CHUNK_GENERATION_SCHEMA_V1.to_string(),
            world_config_hash: String::new(),
            generation_algorithm_id: "runtime_ledger_v1".to_string(),
            generation_algorithm_hash: String::new(),
            created_at_height: height,
            created_at_block_hash: None,
            manifest_height: height,
            manifest_hash: String::new(),
            resource_balances: BTreeMap::new(),
            material_ledgers: BTreeMap::new(),
            generated_chunks: BTreeMap::new(),
        };
        manifest.manifest_hash = manifest.canonical_hash();
        manifest
    }

    pub fn canonical_hash(&self) -> String {
        let mut clone = self.clone();
        clone.manifest_hash.clear();
        hash_json(&clone).unwrap_or_else(|_| String::new())
    }

    pub fn refresh_hashes(&mut self) {
        for entry in self.generated_chunks.values_mut() {
            entry.manifest_hash = entry.canonical_hash();
        }
        self.manifest_hash = self.canonical_hash();
    }

    pub fn is_schema_current(&self) -> bool {
        self.schema_version == CHAIN_RESOURCE_MANIFEST_SCHEMA_V1
            && self.manifest_hash == self.canonical_hash()
    }
}

impl Default for ChainResourceManifest {
    fn default() -> Self {
        Self::empty_at_height("unbound", 0, 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainChunkResourceManifestEntry {
    pub schema_version: String,
    pub world_id: String,
    pub chain_id: String,
    pub world_seed: u64,
    pub chunk_generation_schema_version: String,
    pub coord: ChunkCoord,
    pub seed: u64,
    pub chunk_status: ChainChunkResourceStatus,
    pub fragment_count: u32,
    pub block_count: u32,
    #[serde(default)]
    pub fragment_refs: Vec<ChainFragmentResourceRef>,
    #[serde(default)]
    pub chunk_budget_total_hash: String,
    #[serde(default)]
    pub chunk_budget_remaining_hash: String,
    pub total_by_element_g: BTreeMap<FragmentElementKind, i64>,
    pub remaining_by_element_g: BTreeMap<FragmentElementKind, i64>,
    #[serde(default)]
    pub commit_ref: ChainResourceCommitRef,
    #[serde(default)]
    pub manifest_hash: String,
}

impl ChainChunkResourceManifestEntry {
    pub fn canonical_hash(&self) -> String {
        let mut clone = self.clone();
        clone.manifest_hash.clear();
        hash_json(&clone).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainChunkResourceStatus {
    #[default]
    Committed,
    ChainPending,
    Provisional,
    Exhausted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChainFragmentResourceRef {
    pub fragment_id: String,
    pub location_id: String,
    pub profile_hash: String,
    pub budget_total_hash: String,
    pub budget_remaining_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChainResourceCommitRef {
    pub height: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<WorldEventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<ActionId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainResourceDelta {
    pub schema_version: String,
    pub world_id: String,
    pub chain_id: String,
    pub delta_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<ActionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<WorldEventId>,
    pub ordering_key: ChainResourceOrderingKey,
    pub base_manifest_hash: String,
    pub resulting_manifest_hash: String,
    pub block_height: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_block_hash: Option<String>,
    pub tick: WorldTime,
    pub source: ChainResourceDeltaSource,
    pub replay_status: ChainResourceReplayStatus,
    pub entries: Vec<ChainResourceDeltaEntry>,
}

impl ChainResourceDelta {
    pub fn empty_for_manifest(
        world_id: impl Into<String>,
        manifest_hash: impl Into<String>,
        block_height: u64,
        tick: WorldTime,
    ) -> Self {
        let manifest_hash = manifest_hash.into();
        Self {
            schema_version: CHAIN_RESOURCE_DELTA_SCHEMA_V1.to_string(),
            world_id: world_id.into(),
            chain_id: "unbound".to_string(),
            delta_id: format!("resource-delta-{block_height}-{tick}"),
            action_id: None,
            event_id: None,
            ordering_key: ChainResourceOrderingKey {
                height: block_height,
                event_sequence: 0,
                action_sequence: 0,
            },
            base_manifest_hash: manifest_hash.clone(),
            resulting_manifest_hash: manifest_hash,
            block_height,
            commit_block_hash: None,
            tick,
            source: ChainResourceDeltaSource::Genesis,
            replay_status: ChainResourceReplayStatus::Committed,
            entries: Vec::new(),
        }
    }

    pub fn is_schema_current(&self) -> bool {
        self.schema_version == CHAIN_RESOURCE_DELTA_SCHEMA_V1
            && !self.base_manifest_hash.trim().is_empty()
            && !self.resulting_manifest_hash.trim().is_empty()
    }
}

impl Default for ChainResourceDelta {
    fn default() -> Self {
        Self::empty_for_manifest(
            "unbound",
            ChainResourceManifest::default().manifest_hash,
            0,
            0,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChainResourceDerivationContext<'a> {
    pub world_id: &'a str,
    pub chain_id: &'a str,
    pub genesis_ref: Option<&'a str>,
    pub created_at_height: u64,
    pub manifest_height: u64,
    pub commit_block_hash: Option<&'a str>,
    pub tick: WorldTime,
}

impl<'a> ChainResourceDerivationContext<'a> {
    pub fn simulator_default(tick: WorldTime) -> Self {
        Self {
            world_id: "simulator-world",
            chain_id: "simulator-chain",
            genesis_ref: None,
            created_at_height: 0,
            manifest_height: tick,
            commit_block_hash: None,
            tick,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainResourceOrderingKey {
    pub height: u64,
    pub event_sequence: u64,
    pub action_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainResourceDeltaSource {
    ChunkGenerated,
    MineCompound,
    Replenish,
    #[default]
    Genesis,
    Migration,
}

pub fn chunk_key(coord: ChunkCoord) -> String {
    format!("chunk:{}:{}:{}", coord.x, coord.y, coord.z)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainResourceReplayStatus {
    #[default]
    Committed,
    Rejected,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum ChainResourceDeltaEntry {
    RuntimeResource {
        kind: ResourceKind,
        delta: i64,
        resulting_balance: i64,
    },
    MaterialLedger {
        ledger_id: MaterialLedgerId,
        material_kind: String,
        delta: i64,
        resulting_balance: i64,
    },
    ChunkResource {
        coord: ChunkCoord,
        element: FragmentElementKind,
        total_delta_g: i64,
        remaining_delta_g: i64,
        resulting_remaining_g: i64,
        remaining_after_hash: String,
        chunk_remaining_after_hash: String,
    },
}
