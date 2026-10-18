pub(in crate::viewer::runtime_live) const MAX_PROVIDER_STALE_REPLANS: u32 = 3;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(in crate::viewer::runtime_live) struct ProviderStaleReplanState {
    pub(in crate::viewer::runtime_live) count: u32,
    pub(in crate::viewer::runtime_live) pending_cause: Option<ProviderStaleReplanCause>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(in crate::viewer::runtime_live) struct ProviderStaleReplanCause {
    pub(in crate::viewer::runtime_live) parent_agent_turn_id: String,
    pub(in crate::viewer::runtime_live) parent_decision_request_id: String,
    pub(in crate::viewer::runtime_live) count: u32,
}
