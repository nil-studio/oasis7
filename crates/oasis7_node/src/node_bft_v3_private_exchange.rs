//! Private acquisition is a separately authorized data path, never public gossip.
use crate::{
    NodeBftExecutionContextV3, NodeBftPrivateCandidatePackageV3, NodeError, NodeExecutionHook,
    NodeRuntime,
};
use oasis7_consensus::tendermint_v3::{
    BftHeaderHeightScheduleV3, BftHeaderValueV3, FormalBftCertifiedHeaderV3,
};
use oasis7_distfs::protected_bft_v3::{BftSignerBindingV3, BftSignerLeaseV3};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

pub type NodeBftSharedSignerV3 = Arc<Mutex<Box<dyn BftSignerLeaseV3>>>;
pub type NodeBftSharedExecutionHookV3 = Arc<Mutex<Box<dyn NodeExecutionHook>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NodeBftPrivateFetchKindV3 {
    PendingCandidate,
    CommittedState,
}
#[derive(Clone)]
pub struct NodeBftPrivateFetchRequestV3 {
    pub peer_id: String,
    pub context: NodeBftExecutionContextV3,
    pub value: BftHeaderValueV3,
    pub kind: NodeBftPrivateFetchKindV3,
}
/// A bounded, cancellable operation. Polling must never wait for network I/O.
pub trait NodeBftPrivateCandidateFetchV3: Send {
    fn poll(&mut self) -> Result<Option<NodeBftPrivateCandidatePackageV3>, String>;
    fn cancel(&mut self);
}
/// Receiving data supplies no authority. The Node independently validates the
/// original parent, exact payload, actual execution and durable retention.
pub trait NodeBftPrivateCandidateExchangeV3: Send + Sync {
    fn validate_binding(&self, binding: &BftSignerBindingV3) -> Result<(), String>;
    fn ensure_local_ready(&self, schedule: &BftHeaderHeightScheduleV3) -> Result<(), String>;
    fn start_fetch(
        &self,
        request: NodeBftPrivateFetchRequestV3,
        signer: NodeBftSharedSignerV3,
    ) -> Result<Box<dyn NodeBftPrivateCandidateFetchV3>, String>;
}
/// Only the runtime publishes a view after verified replay and applied install.
pub struct NodeBftPeerExecutionViewV3 {
    pub schedule: BftHeaderHeightScheduleV3,
    pub committed_history: Vec<FormalBftCertifiedHeaderV3>,
    pub generation: u64,
}
pub struct NodeBftPeerExecutionStateV3 {
    serving: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    view: Mutex<Option<Arc<NodeBftPeerExecutionViewV3>>>,
}
impl Default for NodeBftPeerExecutionStateV3 {
    fn default() -> Self {
        Self {
            serving: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
            view: Mutex::new(None),
        }
    }
}
impl NodeBftPeerExecutionStateV3 {
    pub fn view(&self) -> Result<Option<Arc<NodeBftPeerExecutionViewV3>>, String> {
        self.view
            .lock()
            .map(|v| v.clone())
            .map_err(|_| "wire-3 peer view poisoned".into())
    }
    /// These handles only narrow an already authorized transport response.
    pub(super) fn serving_guard(&self) -> Arc<AtomicBool> {
        self.serving.clone()
    }
    pub(super) fn generation_guard(&self) -> Arc<AtomicU64> {
        self.generation.clone()
    }
    pub fn is_serving(&self) -> bool {
        self.serving.load(Ordering::Acquire)
    }
    pub(super) fn stop_serving(&self) {
        self.serving.store(false, Ordering::Release);
    }
    pub(super) fn invalidate_parent(&self) -> Result<(), String> {
        self.stop_serving();
        self.generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map(|_| ())
            .map_err(|_| "wire-3 peer generation exhausted".into())
    }
    pub(super) fn publish(
        &self,
        schedule: BftHeaderHeightScheduleV3,
        committed_history: Vec<FormalBftCertifiedHeaderV3>,
    ) -> Result<(), String> {
        if committed_history.len() > 64 {
            return Err("wire-3 peer history capacity".into());
        }
        let generation = self.generation.load(Ordering::Acquire);
        *self.view.lock().map_err(|_| "wire-3 peer view poisoned")? =
            Some(Arc::new(NodeBftPeerExecutionViewV3 {
                schedule,
                committed_history,
                generation,
            }));
        self.serving.store(true, Ordering::Release);
        Ok(())
    }
}
impl NodeRuntime {
    pub fn try_with_shared_execution_hook(
        mut self,
        hook: NodeBftSharedExecutionHookV3,
    ) -> Result<Self, NodeError> {
        if self.running.load(Ordering::SeqCst) || self.worker.is_some() {
            return Err(NodeError::AlreadyRunning {
                node_id: self.config.node_id.clone(),
            });
        }
        self.execution_hook = Some(hook);
        Ok(self)
    }
}

#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
#[path = "node_bft_v3_private_exchange/native.rs"]
mod native;
#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
pub use native::*;
