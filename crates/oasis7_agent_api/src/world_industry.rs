//! Shared industrial settlement values; no authoritative World state.
use oasis7_wasm_abi::MaterialStack;
use serde::{Deserialize, Serialize};

/// Actual verified recipe commitment retained with the first settlement.
/// Older jobs keep this absent; this is neither a quote nor a cash charge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterIndustrialSettlementSummaryV1 {
    pub requester_agent_id: String,
    pub accepted_batches: u32,
    pub consume: Vec<MaterialStack>,
    pub power_required: i64,
    pub produce: Vec<MaterialStack>,
}
