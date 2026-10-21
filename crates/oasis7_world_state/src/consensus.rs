//! Tick-level execution consensus records.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::types::{ActionId, WorldEventId, WorldTime};
use super::util::sha256_hex;

pub const DEFAULT_TICK_CONSENSUS_AUTHORITY_SOURCE: &str = "builtin.module.release.signer";
pub const TICK_BLOCK_HEADER_SCHEMA_V1: u16 = 1;
pub const TICK_BLOCK_HEADER_SCHEMA_V2: u16 = 2;

fn default_tick_consensus_authority_source() -> String {
    DEFAULT_TICK_CONSENSUS_AUTHORITY_SOURCE.to_string()
}

fn default_tick_block_header_schema_version() -> u16 {
    TICK_BLOCK_HEADER_SCHEMA_V1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TickConsensusSubmissionRole {
    Propagation,
    #[default]
    Authority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickBlockHeader {
    #[serde(default = "default_tick_block_header_schema_version")]
    pub schema_version: u16,
    pub epoch: u64,
    pub tick: WorldTime,
    pub parent_hash: String,
    pub events_hash: String,
    pub state_root: String,
    pub executor_version: String,
    pub randomness_seed: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_height: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_slot: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_block_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCommittedTickContext {
    pub height: u64,
    pub slot: u64,
    pub epoch: u64,
    pub node_block_hash: String,
    pub action_root: String,
    pub authority_node_id: String,
    pub committed_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickExecutionDigest {
    pub action_batch_hash: String,
    pub domain_events_hash: String,
    pub state_projection_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickBlock {
    pub header: TickBlockHeader,
    #[serde(default)]
    pub ordered_action_ids: Vec<ActionId>,
    #[serde(default)]
    pub ordered_event_ids: Vec<WorldEventId>,
    pub event_count: u32,
    pub execution_digest: TickExecutionDigest,
}

impl TickBlock {
    pub fn block_hash(&self) -> String {
        let payload = if self.header.schema_version >= TICK_BLOCK_HEADER_SCHEMA_V2 {
            format!(
                "tickblock:v2|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                self.header.parent_hash,
                self.header.tick,
                self.header.events_hash,
                self.header.state_root,
                self.header.executor_version,
                self.header.chain_height.unwrap_or_default(),
                self.header.chain_slot.unwrap_or_default(),
                self.header.chain_epoch.unwrap_or_default(),
                self.header.node_block_hash.as_deref().unwrap_or_default(),
                self.header.action_root.as_deref().unwrap_or_default(),
                self.header.committed_at_unix_ms.unwrap_or_default()
            )
        } else {
            format!(
                "tickblock:v1|{}|{}|{}|{}|{}",
                self.header.parent_hash,
                self.header.tick,
                self.header.events_hash,
                self.header.state_root,
                self.header.executor_version
            )
        };
        sha256_hex(payload.as_bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickCertificate {
    pub block_hash: String,
    pub consensus_height: u64,
    pub threshold: u16,
    #[serde(default = "default_tick_consensus_authority_source")]
    pub authority_source: String,
    #[serde(default)]
    pub submission_role: TickConsensusSubmissionRole,
    #[serde(default)]
    pub signatures: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickConsensusRecord {
    pub block: TickBlock,
    pub certificate: TickCertificate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickConsensusRejectionAuditEvent {
    pub recorded_at_tick: WorldTime,
    pub tick: WorldTime,
    pub consensus_height: u64,
    pub attempted_source: String,
    pub attempted_role: TickConsensusSubmissionRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_role: Option<TickConsensusSubmissionRole>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickConsensusDriftReport {
    pub tick: WorldTime,
    pub reason: String,
}
