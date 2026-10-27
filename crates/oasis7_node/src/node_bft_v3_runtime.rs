//! Explicit single-validator qualified-H wire-3 runtime. No peer ingress.
use crate::runtime_util::lock_state;
use crate::{NodeError, NodeRuntime};
use oasis7_distfs::protected_bft_v3::{BftSignerBindingV3, RemoteBftSignerProviderV3};
use std::sync::{Arc, atomic::Ordering};
#[path = "node_bft_v3_actions.rs"]
mod actions;
#[path = "node_bft_v3_completions.rs"]
mod completions;
pub(super) fn validate_single_action(action: &crate::NodeConsensusAction) -> Result<(), NodeError> {
    actions::validate_single_action(action)
}
#[path = "node_bft_v3_startup.rs"]
mod startup;
#[path = "node_bft_v3_worker.rs"]
mod worker;
pub(super) fn start(runtime: &mut NodeRuntime) -> Result<(), NodeError> {
    startup::start(runtime)
}
#[derive(Clone)]
pub(crate) struct SelectedNodeBftConfigV3 {
    pub binding: BftSignerBindingV3,
    pub provider: Arc<RemoteBftSignerProviderV3>,
    pub capacity_exhausted: Arc<std::sync::atomic::AtomicBool>,
}
#[derive(Debug, Clone, Default)]
pub struct NodeBftStatusV3 {
    pub running: bool,
    pub height: u64,
    pub round: u64,
    pub committed_height: u64,
    pub durable_event_count: usize,
    pub blocked_reason: Option<String>,
}
pub(super) fn error(reason: impl Into<String>) -> NodeError {
    NodeError::Consensus {
        reason: reason.into(),
    }
}
impl NodeRuntime {
    pub fn with_tendermint_v3(
        mut self,
        binding: BftSignerBindingV3,
        provider: Arc<RemoteBftSignerProviderV3>,
    ) -> Result<Self, NodeError> {
        if self.running.load(Ordering::SeqCst) || self.worker.is_some() {
            return Err(NodeError::AlreadyRunning {
                node_id: self.config.node_id.clone(),
            });
        }
        if self.bft_config.is_some() || self.bft_v2_config.is_some() || self.bft_v3_config.is_some()
        {
            return Err(error("another consensus profile is already selected"));
        }
        let selected = SelectedNodeBftConfigV3 {
            binding,
            provider,
            capacity_exhausted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        startup::validate_runtime(&self, &selected)?;
        self.bft_v3_config = Some(selected);
        Ok(self)
    }
    pub fn bft_v3_status(&self) -> Option<NodeBftStatusV3> {
        self.bft_v3_config.as_ref()?;
        let state = lock_state(&self.state);
        let mut status = state.bft_v3_status.clone().unwrap_or_default();
        status.running = self.running.load(Ordering::SeqCst);
        status.blocked_reason = state
            .last_error
            .clone()
            .or_else(|| (!status.running).then(|| "wire-3 runtime is stopped".into()));
        Some(status)
    }
    pub(super) fn validate_bft_v3_payload_admission(
        &self,
        payload: &[u8],
    ) -> Result<(), NodeError> {
        let hook = self
            .execution_hook
            .as_ref()
            .ok_or_else(|| error("wire-3 requires an execution hook"))?;
        startup::lock_hook(hook)?
            .validate_bft_v3_action_payload(payload)
            .map_err(error)
    }
}
