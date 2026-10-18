use super::super::types::{AgentId, ModuleInstallTarget, ResourceKind, WorldTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleArtifactState {
    pub wasm_hash: String,
    pub publisher_agent_id: AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_id_hint: Option<String>,
    #[serde(default)]
    pub wasm_bytes: Vec<u8>,
    #[serde(default)]
    pub deployed_at_tick: WorldTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledModuleState {
    pub module_id: String,
    pub module_version: String,
    pub wasm_hash: String,
    pub installer_agent_id: AgentId,
    #[serde(default)]
    pub install_target: ModuleInstallTarget,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub installed_at_tick: WorldTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleArtifactListingState {
    pub order_id: u64,
    pub wasm_hash: String,
    pub seller_agent_id: AgentId,
    pub price_kind: ResourceKind,
    pub price_amount: i64,
    pub listed_at_tick: WorldTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleArtifactBidState {
    pub order_id: u64,
    pub wasm_hash: String,
    pub bidder_agent_id: AgentId,
    pub price_kind: ResourceKind,
    pub price_amount: i64,
    pub placed_at_tick: WorldTime,
}

pub(super) fn default_next_module_market_order_id() -> u64 {
    1
}

pub(super) fn default_next_module_market_sale_id() -> u64 {
    1
}
