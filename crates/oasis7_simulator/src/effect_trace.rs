//! Recorded effect values; execution and provider wiring belong to the host.
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmEffectIntentTrace {
    pub intent_id: String,
    pub kind: String,
    pub params: JsonValue,
    pub cap_ref: String,
    pub origin: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmEffectReceiptTrace {
    pub intent_id: String,
    pub status: String,
    pub payload: JsonValue,
    pub cost_cents: Option<u64>,
}
