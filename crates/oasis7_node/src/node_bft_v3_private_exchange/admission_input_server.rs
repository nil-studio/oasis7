use super::super::wire::{MAX_FRAME, decode_frame, digest, encode, encode_frame, verify_signature};
use super::input_wire::*;
use super::*;
use oasis7_distfs::protected_bft_v3::{BftSignerErrorV3, BftSignerServiceV3};
use std::collections::BTreeSet;

pub struct NodeBftAdmissionInputServerConfigV1 {
    pub binding: BftSignerBindingV3,
    pub peers: Vec<String>,
}
pub struct NodeBftLibp2pAdmissionInputServerV1 {
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
    gate: Arc<dyn NodeBftAdmissionInputReadGateV1>,
    alive: Arc<AtomicBool>,
}
impl NodeBftLibp2pAdmissionInputServerV1 {
    pub fn new<T: NodeBftAdmissionInputTransportV1 + 'static>(
        network: Arc<Libp2pNetwork>,
        config: NodeBftAdmissionInputServerConfigV1,
        state: Arc<NodeBftPeerExecutionStateV3>,
        private: Arc<T>,
        source: Arc<dyn NodeBftAdmissionInputSourceV1>,
        gate: Arc<dyn NodeBftAdmissionInputReadGateV1>,
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
        config: NodeBftAdmissionInputServerConfigV1,
        service: Arc<BftSignerServiceV3>,
        private: NodeBftPrivateTransportScopeV1,
        source: Arc<dyn NodeBftAdmissionInputSourceV1>,
        gate: Arc<dyn NodeBftAdmissionInputReadGateV1>,
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
        config: NodeBftAdmissionInputServerConfigV1,
        view: ViewOwner,
        private: NodeBftPrivateTransportScopeV1,
        transport_owner: Arc<dyn NodeBftAdmissionInputTransportV1>,
        source: Arc<dyn NodeBftAdmissionInputSourceV1>,
        gate: Arc<dyn NodeBftAdmissionInputReadGateV1>,
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
                PROTOCOL,
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
impl Drop for NodeBftLibp2pAdmissionInputServerV1 {
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
    fn handle(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        raw: &[u8],
    ) -> Result<Vec<u8>, String> {
        transport
            .request_context()
            .check_cancelled(|| "admission request cancelled".to_string())?;
        self.common_guards(transport)?;
        if let Ok(hint) = decode_frame::<super::input_hint::HintRequest>(raw) {
            return self.hint(transport, hint);
        }
        let request: Request = decode_frame(raw)?;
        request.validate()?;
        if request.read.requester_peer_id != transport.peer_id().to_string()
            || request.read.serving_peer_id != self.local_peer
            || request.read.reference.source.binding_digest
                != self
                    .binding
                    .digest()
                    .map_err(|_| "admission binding digest")?
        {
            return Err("admission original source/Noise endpoints differ".into());
        }
        let authorization = &request.authorization;
        let body = &authorization.grant.body;
        body.validate()
            .map_err(|_| "admission issuer grant shape rejected")?;
        if body.reference != request.read.reference
            || body.requester_peer_id != request.read.requester_peer_id
            || body.serving_peer_id != request.read.serving_peer_id
            || body.request_nonce != request.read.nonce
            || body.subject_public_key != authorization.subject_public_key
        {
            return Err("admission original issuer request differs".into());
        }
        verify_signature(
            &authorization.subject_public_key,
            &request.read.signing_bytes(&authorization.grant)?,
            &authorization.signature_hex,
        )?;
        self.gate.validate_identity(authorization)?;
        match &self.view {
            ViewOwner::Node(state) => {
                let view = self.node_view(transport, state)?;
                self.authorized_material(
                    transport,
                    &request,
                    NodeBftAdmissionInputReadViewV1::Node(&view),
                    None,
                )
            }
            ViewOwner::Provider(service) => service
                .with_observed_original_input_view(transport.request_context().deadline(), |view| {
                    // No outer service/backend lock is held while the independent
                    // issuer or immutable source builds this original response.
                    self.authorized_material(
                        transport,
                        &request,
                        NodeBftAdmissionInputReadViewV1::Provider(view),
                        Some(service),
                    )
                    .map_err(BftSignerErrorV3)
                })
                .map_err(|error| error.to_string()),
        }
    }
    fn hint(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        request: super::input_hint::HintRequest,
    ) -> Result<Vec<u8>, String> {
        request.validate()?;
        if request.requester_peer_id != transport.peer_id().to_string()
            || request.serving_peer_id != self.local_peer
        {
            return Err("admission hint Noise endpoints differ".into());
        }
        let deadline = self
            .private
            .original_deadline(
                request.requester_peer_id.clone(),
                request.nonce.clone(),
                request.digest()?,
                digest(
                    b"oasis7.bft.admission-input.hint-identity.v1",
                    &encode(&(&request.requester_peer_id, &request.serving_peer_id))?,
                ),
                true,
            )?
            .min(transport.request_context().deadline());
        transport
            .restrict_reply_until(deadline)
            .map_err(|_| "admission hint deadline expired")?;
        match &self.view {
            ViewOwner::Node(state) => {
                let view = self.node_view(transport, state)?;
                self.hint_bytes(&request, NodeBftAdmissionInputReadViewV1::Node(&view))
            }
            ViewOwner::Provider(service) => service
                .with_charged_original_input_response(deadline, |view| {
                    self.provider_guard(transport, view)
                        .map_err(BftSignerErrorV3)?;
                    self.hint_bytes(&request, NodeBftAdmissionInputReadViewV1::Provider(view))
                        .map_err(BftSignerErrorV3)
                })
                .map_err(|error| error.to_string()),
        }
    }
    fn hint_bytes(
        &self,
        request: &super::input_hint::HintRequest,
        view: NodeBftAdmissionInputReadViewV1<'_>,
    ) -> Result<Vec<u8>, String> {
        // No Ref argument, source lookup, CAS or hook lock: selection only.
        encode_frame(&super::input_hint::HintResponse {
            domain: super::input_hint::HINT_DOMAIN.into(),
            request_digest: request.digest()?,
            view: super::input_hint::NodeBftAdmissionInputCurrentViewV1 {
                binding_digest: self
                    .binding
                    .digest()
                    .map_err(|_| "admission hint binding digest")?,
                scope: view.schedule().scope().clone(),
                schedule_digest: view.schedule().digest()?,
                generation: view.generation(),
            },
        })
    }
    fn authorized_material(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        request: &Request,
        view: NodeBftAdmissionInputReadViewV1<'_>,
        provider: Option<&BftSignerServiceV3>,
    ) -> Result<Vec<u8>, String> {
        let read = &request.read;
        let authorization = &request.authorization;
        let body = &authorization.grant.body;
        body.validate()
            .map_err(|_| "admission issuer grant shape rejected")?;
        if body.scope != *view.schedule().scope()
            || body.reference != read.reference
            || body.requester_peer_id != read.requester_peer_id
            || body.serving_peer_id != read.serving_peer_id
            || body.request_nonce != read.nonce
            || body.subject_public_key != authorization.subject_public_key
            || body.schedule_digest != view.schedule().digest()?
            || body.view_generation != view.generation()
        {
            return Err("admission permission differs from original current-view request".into());
        }
        verify_signature(
            &authorization.subject_public_key,
            &read.signing_bytes(&authorization.grant)?,
            &authorization.signature_hex,
        )?;
        let scope = NodeBftAdmissionInputReadScopeV1 {
            view,
            reference: read.reference.clone(),
            requester_peer_id: read.requester_peer_id.clone(),
            serving_peer_id: read.serving_peer_id.clone(),
            subject_public_key: authorization.subject_public_key.clone(),
        };
        let mut make_response = |authorized_until: Instant| {
            let deadline = self
                .private
                .input_deadline(
                    read.requester_peer_id.clone(),
                    read.nonce.clone(),
                    request.base()?,
                    digest(
                        b"oasis7.bft.admission-input.identity.v1",
                        &encode(&authorization.grant)?,
                    ),
                    request.chunk_index.is_none(),
                )?
                .min(authorized_until)
                .min(transport.request_context().deadline());
            transport
                .restrict_reply_until(deadline)
                .map_err(|_| "admission original deadline expired")?;
            if let Some(provider) = provider {
                provider
                    .with_charged_original_input_response(deadline, |current| {
                        if current.schedule() != view.schedule()
                            || current.generation() != view.generation()
                        {
                            return Err(BftSignerErrorV3("admission provider view changed".into()));
                        }
                        self.provider_guard(transport, current)
                            .map_err(BftSignerErrorV3)?;
                        self.material_bytes(transport, request, deadline)
                            .map_err(BftSignerErrorV3)
                    })
                    .map_err(|error| error.to_string())
            } else {
                self.material_bytes(transport, request, deadline)
            }
        };
        self.gate
            .authorize(transport, authorization, &scope, &mut make_response)
    }
    fn material_bytes(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        request: &Request,
        deadline: Instant,
    ) -> Result<Vec<u8>, String> {
        transport
            .request_context()
            .check_cancelled(|| "admission request cancelled".to_string())?;
        if Instant::now() >= deadline {
            return Err("admission original deadline expired".into());
        }
        let read = &request.read;
        let original = self.private.admission_original(
            &read.requester_peer_id,
            &read.nonce,
            &read.reference,
            self.source.as_ref(),
            deadline,
        )?;
        if original.reference != read.reference
            || !original.bytes.belongs_to(&self.private.buffers())
        {
            return Err("admission immutable source/buffer scope differs".into());
        }
        let chunk_hex = match request.chunk_index {
            Some(index) => {
                let (offset, length) = read.operation.chunk_range(index)?;
                let start = usize::try_from(offset).map_err(|_| "admission chunk offset")?;
                let end = start
                    .checked_add(length as usize)
                    .ok_or("admission chunk end")?;
                Some(hex::encode(
                    original
                        .bytes
                        .get(start..end)
                        .ok_or("admission original chunk missing")?,
                ))
            }
            None => None,
        };
        encode_frame(&Response {
            domain: DOMAIN.into(),
            request_digest: request.digest()?,
            reference: read.reference.clone(),
            chunk_index: request.chunk_index,
            chunk_hex,
        })
    }
}
