//! Read-only kernel queries, independent of Agent execution and providers.
use super::kernel::{
    MicroDepotActionKind, MicroDepotInstallQuote, MicroDepotPressureClass, MicroDepotQuotePreview,
};
use super::types::Action;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A read-only request an Agent can make against the current world snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentQuery {
    EvaluateMicroDepotQuote(MicroDepotQuoteRequest),
    /// Preview a prospective micro-depot installation without submitting its
    /// action or reserving any resources.
    QuoteMicroDepotInstall(Action),
}

/// Arguments needed to preview a micro-depot service without creating an intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroDepotQuoteRequest {
    pub agent_id: String,
    pub facility_id: String,
    pub target_id: String,
    pub action_kind: MicroDepotActionKind,
    pub base_cost_class: MicroDepotPressureClass,
    pub base_risk_class: MicroDepotPressureClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker_type: Option<String>,
}

/// The structured outcome of a read-only Agent query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentQueryResult {
    EvaluateMicroDepotQuote {
        request: MicroDepotQuoteRequest,
        /// Facility stock copied from the same snapshot used for the preview.
        available_units_by_kind: Option<BTreeMap<String, i64>>,
        result: Result<MicroDepotQuotePreview, String>,
    },
    QuoteMicroDepotInstall {
        action: Action,
        result: Result<MicroDepotInstallQuote, String>,
    },
}
