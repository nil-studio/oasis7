use super::*;

#[derive(Debug, Serialize)]
pub(crate) struct ChainLivenessStatus {
    pub(crate) status: String,
    pub(crate) running: bool,
    pub(crate) runtime_last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainReadinessStatus {
    pub(crate) status: String,
    pub(crate) ready: bool,
    pub(crate) failed_gates: Vec<String>,
    pub(crate) policy: ChainReadinessPolicyStatus,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainSyncStatus {
    pub(crate) status: String,
    pub(crate) network_head_source: String,
    pub(crate) network_height_lag: u64,
    pub(crate) fresh_peer_count: usize,
    pub(crate) stale_peer_count: usize,
    pub(crate) conflicting_peer_count: usize,
}
