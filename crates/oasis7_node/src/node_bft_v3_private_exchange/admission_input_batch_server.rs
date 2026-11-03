use super::super::wire::{MAX_FRAME, decode_frame, digest, encode, encode_frame};
use super::batch_wire::*;
use super::*;
use oasis7_distfs::protected_bft_v3::{BftSignerErrorV3, BftSignerServiceV3};
use std::collections::BTreeSet;

pub struct NodeBftAdmissionInputServerConfigV2 {
    pub binding: BftSignerBindingV3,
    pub peers: Vec<String>,
}
pub struct NodeBftLibp2pAdmissionInputServerV2 {
    _registration: AuthenticatedHandlerRegistration,
    alive: Arc<AtomicBool>,
    _network: Arc<Libp2pNetwork>,
    _transport_owner: Arc<dyn NodeBftAdmissionInputTransportV1>,
}
enum ViewOwner {
    Node(Arc<NodeBftPeerExecutionStateV3>),
    Provider(Arc<BftSignerServiceV3>),
}
struct Server {
    binding: BftSignerBindingV3,
    local_peer: String,
    view: ViewOwner,
    private: NodeBftPrivateTransportScopeV1,
    source: Arc<dyn NodeBftAdmissionInputSourceV1>,
    gate: Arc<dyn NodeBftAdmissionInputBatchReadGateV2>,
    alive: Arc<AtomicBool>,
}
impl NodeBftLibp2pAdmissionInputServerV2 {
    pub fn new<T: NodeBftAdmissionInputTransportV1 + 'static>(
        network: Arc<Libp2pNetwork>,
        config: NodeBftAdmissionInputServerConfigV2,
        state: Arc<NodeBftPeerExecutionStateV3>,
        private: Arc<T>,
        source: Arc<dyn NodeBftAdmissionInputSourceV1>,
        gate: Arc<dyn NodeBftAdmissionInputBatchReadGateV2>,
    ) -> Result<Self, String> {
        let scope = private.admission_transport();
        Self::install(
            network,
            config,
            ViewOwner::Node(state),
            scope,
            private,
            source,
            gate,
        )
    }
    pub fn new_for_provider(
        network: Arc<Libp2pNetwork>,
        config: NodeBftAdmissionInputServerConfigV2,
        service: Arc<BftSignerServiceV3>,
        private: NodeBftPrivateTransportScopeV1,
        source: Arc<dyn NodeBftAdmissionInputSourceV1>,
        gate: Arc<dyn NodeBftAdmissionInputBatchReadGateV2>,
    ) -> Result<Self, String> {
        Self::install(
            network,
            config,
            ViewOwner::Provider(service),
            private.clone(),
            Arc::new(private),
            source,
            gate,
        )
    }
    fn install(
        network: Arc<Libp2pNetwork>,
        config: NodeBftAdmissionInputServerConfigV2,
        view: ViewOwner,
        private: NodeBftPrivateTransportScopeV1,
        transport_owner: Arc<dyn NodeBftAdmissionInputTransportV1>,
        source: Arc<dyn NodeBftAdmissionInputSourceV1>,
        gate: Arc<dyn NodeBftAdmissionInputBatchReadGateV2>,
    ) -> Result<Self, String> {
        config
            .binding
            .validate()
            .map_err(|_| "admission binding rejected")?;
        let peers = config
            .peers
            .iter()
            .map(|peer| {
                peer.parse::<libp2p::PeerId>()
                    .map_err(|_| "admission configured peer identity".to_string())
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if peers.is_empty()
            || peers.len() != config.peers.len()
            || peers.len() > 256
            || peers.contains(&network.peer_id())
            || peers.iter().any(|peer| !private.contains(peer))
        {
            return Err("admission peer routes duplicate/self/capacity".into());
        }
        let alive = Arc::new(AtomicBool::new(true));
        let server = Arc::new(Server {
            binding: config.binding,
            local_peer: network.peer_id().to_string(),
            view,
            private,
            source,
            gate,
            alive: alive.clone(),
        });
        let registration = network
            .register_authenticated_context_handler_with_admission(
                PROTOCOL_V2,
                MAX_FRAME,
                Box::new(move |peer, bytes| {
                    if peers.contains(peer) && bytes.len() <= MAX_FRAME {
                        Ok(())
                    } else {
                        Err(super::super::rejected("admission route rejected", false))
                    }
                }),
                Box::new(move |transport, bytes| {
                    server.handle(transport, bytes).map_err(|_| {
                        super::super::rejected("admission original request rejected", false)
                    })
                }),
            )
            .map_err(|_| "admission handler registration rejected")?;
        Ok(Self {
            _registration: registration,
            alive,
            _network: network,
            _transport_owner: transport_owner,
        })
    }
}
impl Drop for NodeBftLibp2pAdmissionInputServerV2 {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
    }
}
impl Server {
    fn common_guards(&self, transport: &AuthenticatedPeerRequestContext) -> Result<(), String> {
        if !self.alive.load(Ordering::Acquire) {
            return Err("admission stopped".into());
        }
        transport
            .restrict_reply_while(self.alive.clone())
            .map_err(|_| "admission stopped")?;
        transport
            .restrict_reply_while(self.private.release_guard())
            .map_err(|_| "admission transport stopped".to_owned())
    }
    fn node_view(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        state: &NodeBftPeerExecutionStateV3,
    ) -> Result<Arc<NodeBftPeerExecutionViewV3>, String> {
        if !state.is_serving() {
            return Err("admission current Node view unavailable".into());
        }
        let view = state
            .view()?
            .ok_or("admission authenticated current view unavailable")?;
        transport
            .restrict_reply_to_generation(state.generation_guard(), view.generation)
            .map_err(|_| "admission parent changed")?;
        transport
            .restrict_reply_while(state.serving_guard())
            .map_err(|_| "admission stopped")?;
        Ok(view)
    }
    fn provider_guard(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        view: &oasis7_distfs::protected_bft_v3::ObservedBftOriginalInputServingViewV1,
    ) -> Result<(), String> {
        if !view.is_current()
            || view.binding_digest()
                != self
                    .binding
                    .digest()
                    .map_err(|_| "admission binding digest")?
        {
            return Err("admission provider view differs".into());
        }
        transport
            .restrict_reply_while(view.release_guard())
            .map_err(|_| "admission provider view revoked")?;
        transport
            .restrict_reply_until(view.deadline())
            .map_err(|_| "admission provider deadline expired".to_owned())
    }
}
#[path = "admission_input_batch_server_flow.rs"]
mod flow;
#[path = "admission_input_batch_server_parts.rs"]
mod parts;

// Debug-only, globally bounded phase markers. Never print material identities,
// grants, payloads or errors; these observations cannot authorize any operation.
fn debug_phase(label: &'static str, started: Instant) {
    #[cfg(debug_assertions)]
    {
        static EVENTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        if EVENTS.fetch_add(1, Ordering::Relaxed) < 128 {
            eprintln!(
                "admission-v2 phase={label} elapsed_us={}",
                started.elapsed().as_micros()
            );
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = (label, started);
}
