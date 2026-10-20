//! Process-local execution driver state; no serialized authority.
use super::*;

pub(crate) struct NodeRuntimeExecutionDriver {
    pub(crate) state_path: std::path::PathBuf,
    pub(crate) world_dir: std::path::PathBuf,
    pub(crate) records_dir: std::path::PathBuf,
    pub(crate) simulator_world_dir: std::path::PathBuf,
    pub(crate) execution_store: LocalCasStore,
    pub(crate) state: ExecutionBridgeState,
    pub(crate) execution_world: RuntimeWorld,
    pub(crate) simulator_mirror: WorldKernel,
    pub(crate) execution_sandbox: Box<dyn ModuleSandbox + Send>,
    pub(crate) hot_window_heights: u64,
    pub(crate) checkpoint_interval_heights: u64,
    pub(crate) checkpoint_keep_latest: usize,
    pub(crate) retention_reconcile_pending: bool,
    pub(crate) retention_reconcile_next_height: Option<u64>,
    /// Host-side continuation marker for a staged product-validation intent.
    /// It is deliberately outside the runtime state root and is cleared after
    /// the authoritative per-height record is published.
    pub(in super::super) pending_product_validation_intent: Option<ProductValidationIntentMarkerV1>,
    pub(crate) local_execution_bootstrap: Option<NodeExecutionBootstrap>,
    pub(crate) controlled_capture_enabled: bool,
    /// Enabled only by the formal executor after qualified-parent attachment.
    pub(crate) formal_retention_enabled: bool,
    pub(crate) formal_retention_delta:
        Option<oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexDeltaV1>,
    pub(crate) formal_new_results: Vec<oasis7::world_service::CanonicalIntentResultV1>,
    pub(crate) formal_event_root: Option<String>,
    pub(crate) formal_unpruned_journal: Option<oasis7_runtime::Journal>,
    pub(crate) formal_journal_cursors: Vec<(u64, u64)>,
    /// Diagnostic resource identity only; this field grants no write authority.
    pub(crate) resource_identity: Option<ExecutionResourceIdentity>,
}

pub(crate) struct ExecutionResourceIdentity {
    pub chain_id: String,
    pub genesis_ref: String,
}
