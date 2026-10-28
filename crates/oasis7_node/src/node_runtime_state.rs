use crate::{NodeConsensusProgressObserverError, NodeConsensusSnapshot};

#[derive(Debug, Clone, Default)]
pub(super) struct RuntimeState {
    pub(super) generation: u64,
    pub(super) tick_count: u64,
    pub(super) last_tick_unix_ms: Option<i64>,
    pub(super) replica_maintenance_last_polled_at_ms: Option<i64>,
    pub(super) consensus: NodeConsensusSnapshot,
    pub(super) consensus_progress_observer_error: Option<NodeConsensusProgressObserverError>,
    pub(super) last_error: Option<String>,
    pub(super) bft_status: Option<crate::node_bft_runtime::NodeBftStatusV1>,
    pub(super) bft_v3_status: Option<crate::node_bft_v3_runtime::NodeBftStatusV3>,
    pub(super) bft_v2_status: Option<crate::node_bft_v2_runtime::NodeBftStatusV2>,
}
