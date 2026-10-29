//! Confidential libp2p candidate data. Authorization remains an explicit host gate.
use super::*;
use oasis7_client_api::world_service_full_sync::FormalNodeReadGrantV1;
use oasis7_net::{AuthenticatedPeerRequestContext, Libp2pNetwork, WorldError};
use oasis7_proto::distributed_bft::v3::{
    BftCandidateReadBodyV3, BftCandidateReadOperationV3, BftCandidateReadProofV3,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeBftCommittedReadAuthorizationV3 {
    pub grant: FormalNodeReadGrantV1,
    pub subject_public_key: String,
    pub signature_hex: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NodeBftPrivateReadAuthorizationV3 {
    Pending(Box<BftCandidateReadProofV3>),
    Committed(Box<NodeBftCommittedReadAuthorizationV3>),
}
pub struct NodeBftPrivateReadScopeV3 {
    pub view: Arc<NodeBftPeerExecutionViewV3>,
    pub value: BftHeaderValueV3,
    pub kind: NodeBftPrivateFetchKindV3,
    pub requester_peer_id: String,
    pub serving_peer_id: String,
    pub subject_public_key: String,
}
pub trait NodeBftPrivateReadGateV3: Send + Sync {
    /// Check fresh permissions before and after immutable response creation.
    /// The callback may only narrow the Net release guard and produce bytes.
    fn authorize(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        authorization: &NodeBftPrivateReadAuthorizationV3,
        scope: &NodeBftPrivateReadScopeV3,
        make_response: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String>;
}
pub trait NodeBftCommittedReadCredentialsV3: Send + Sync {
    fn grant(&self) -> FormalNodeReadGrantV1;
    /// Sign only the supplied domain-framed typed read request. This is an
    /// independently configured reader key, never the pending validator proof.
    fn sign_request(&self, canonical_payload: &[u8]) -> Result<String, String>;
}

#[path = "client.rs"]
mod client;
#[path = "package.rs"]
mod package;
#[path = "server.rs"]
mod server;
#[path = "wire.rs"]
mod wire;
use oasis7_net::AuthenticatedHandlerRegistration;
use std::{collections::BTreeSet, thread::JoinHandle, time::Duration};

pub struct NodeBftPrivateExchangeConfigV3 {
    pub binding: BftSignerBindingV3,
    pub candidate_policy_digest: String,
    pub candidate_grant_generation: u64,
    pub peers: Vec<String>,
    pub operation_timeout: Duration,
}
/// Native encrypted transport only. The explicit host gate supplies fresh
/// independently issued read authorization; configured routes supply none.
pub struct NodeBftLibp2pPrivateCandidateExchangeV3 {
    core: Arc<client::ClientCore>,
    server: Arc<server::Server>,
    peers: BTreeSet<libp2p::PeerId>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    registration: Option<AuthenticatedHandlerRegistration>,
}
impl NodeBftLibp2pPrivateCandidateExchangeV3 {
    /// Narrows only resource admission for real-wire tests. It supplies no
    /// identity, permission, driver state or authenticated transport context.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn restrict_nonce_budget_for_testing(&self, limit: usize) -> Result<(), String> {
        self.server.restrict_nonce_budget_for_testing(limit)
    }
    /// Read-only admission evidence for real-wire tests; never alters quota.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn nonce_count_for_testing(&self, peer: &str) -> Result<usize, String> {
        self.server.nonce_count_for_testing(peer)
    }
    pub fn new(
        network: Arc<Libp2pNetwork>,
        config: NodeBftPrivateExchangeConfigV3,
        state: Arc<NodeBftPeerExecutionStateV3>,
        hook: NodeBftSharedExecutionHookV3,
        gate: Arc<dyn NodeBftPrivateReadGateV3>,
        committed_credentials: Option<Arc<dyn NodeBftCommittedReadCredentialsV3>>,
    ) -> Result<Self, String> {
        config
            .binding
            .validate()
            .map_err(|_| "private exchange binding rejected")?;
        wire::hash(&config.candidate_policy_digest)?;
        if config.candidate_grant_generation == 0
            || config.peers.len() > 256
            || config.operation_timeout.is_zero()
            || config.operation_timeout > Duration::from_secs(600)
        {
            return Err("private exchange configuration capacity".into());
        }
        let peers: BTreeSet<libp2p::PeerId> = config
            .peers
            .iter()
            .map(|p| {
                p.parse()
                    .map_err(|_| "private configured peer identity".to_string())
            })
            .collect::<Result<_, _>>()?;
        if peers.len() != config.peers.len() || peers.contains(&network.peer_id()) {
            return Err("private peer route duplicate/self".into());
        }
        let mut config = config;
        config.peers = peers.iter().map(ToString::to_string).collect();
        let alive = Arc::new(AtomicBool::new(true));
        let server = Arc::new(server::Server::new(
            &config,
            network.peer_id().to_string(),
            state.clone(),
            hook,
            gate,
            alive.clone(),
        ));
        let core = Arc::new(client::ClientCore {
            network: network.clone(),
            binding: config.binding,
            policy: config.candidate_policy_digest,
            grant_generation: config.candidate_grant_generation,
            state,
            timeout: config.operation_timeout,
            alive,
            credentials: committed_credentials,
            active: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        });
        let mut owner = Self {
            core,
            server: server.clone(),
            peers: peers.clone(),
            workers: Mutex::new(Vec::new()),
            registration: None,
        };
        owner.registration = Some(
            network
                .register_authenticated_context_handler_with_admission(
                    wire::PROTOCOL,
                    wire::MAX_FRAME,
                    Box::new(move |peer, _| {
                        if peers.contains(peer) {
                            Ok(())
                        } else {
                            Err(rejected("unconfigured private peer route", false))
                        }
                    }),
                    Box::new(move |transport, bytes| {
                        server.handle(transport, bytes).map_err(|e| {
                            if e == "private cache build in progress"
                                || e.contains("private committed parent advancement")
                            {
                                rejected("private original response is being prepared", true)
                            } else {
                                rejected("private candidate request rejected", false)
                            }
                        })
                    }),
                )
                .map_err(|_| "private authenticated handler registration rejected")?,
        );
        Ok(owner)
    }
}
fn rejected(message: &str, retryable: bool) -> WorldError {
    WorldError::NetworkRequestFailed {
        code: if retryable {
            oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited
        } else {
            oasis7_proto::distributed::DistributedErrorCode::ErrBadRequest
        },
        message: message.into(),
        retryable,
    }
}
impl NodeBftPrivateCandidateExchangeV3 for NodeBftLibp2pPrivateCandidateExchangeV3 {
    fn validate_binding(&self, binding: &BftSignerBindingV3) -> Result<(), String> {
        if binding.digest().map_err(|_| "private runtime binding")?
            != self
                .core
                .binding
                .digest()
                .map_err(|_| "private configured binding")?
        {
            return Err("private exchange differs from exact runtime binding".into());
        }
        Ok(())
    }
    fn ensure_local_ready(&self, schedule: &BftHeaderHeightScheduleV3) -> Result<(), String> {
        if !self.core.alive.load(Ordering::Acquire)
            || schedule.scope() != &self.core.binding.scope
            || schedule.activation_digest() != self.core.binding.activation_digest
        {
            return Err("private local H scope unavailable".into());
        }
        if !schedule
            .active_set()
            .validator(&self.core.binding.local_validator_id)
            .is_some_and(|v| {
                self.core.binding.provisioned_signing_keys.iter().any(|k| {
                    k.validator_id == v.validator_id && k.public_key_hex == v.public_key_hex
                })
            })
        {
            return Err("private local validator read key is not provisioned".into());
        }
        Ok(())
    }
    fn start_fetch(
        &self,
        request: NodeBftPrivateFetchRequestV3,
        signer: NodeBftSharedSignerV3,
    ) -> Result<Box<dyn NodeBftPrivateCandidateFetchV3>, String> {
        self.start_fetch_bounded(request, signer, self.core.timeout)
    }
}
impl NodeBftLibp2pPrivateCandidateExchangeV3 {
    /// Only narrows a fresh real operation. No existing nonce, worker, polling
    /// deadline or server-side original layout can be changed by this hook.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn start_fetch_with_timeout_for_testing(
        &self,
        request: NodeBftPrivateFetchRequestV3,
        signer: NodeBftSharedSignerV3,
        timeout: Duration,
    ) -> Result<Box<dyn NodeBftPrivateCandidateFetchV3>, String> {
        self.start_fetch_bounded(request, signer, timeout)
    }
    fn start_fetch_bounded(
        &self,
        request: NodeBftPrivateFetchRequestV3,
        signer: NodeBftSharedSignerV3,
        timeout: Duration,
    ) -> Result<Box<dyn NodeBftPrivateCandidateFetchV3>, String> {
        if timeout.is_zero() || timeout > self.core.timeout {
            return Err(
                "private test operation timeout may only narrow the configured bound".into(),
            );
        }
        let peer = request
            .peer_id
            .parse::<libp2p::PeerId>()
            .map_err(|_| "private fetch peer identity")?;
        if !self.peers.contains(&peer) || !self.core.alive.load(Ordering::Acquire) {
            return Err("private fetch route unavailable".into());
        }
        request.value.validate_binding()?;
        if request.context.scope != self.core.binding.scope
            || request.context.activation_digest != self.core.binding.activation_digest
            || request.context.height != request.value.execution_commitment.height
            || request.context.parent_commit_ref
                != request.value.execution_commitment.parent_commit_ref
            || request.context.parent_state_root
                != request
                    .value
                    .execution_commitment
                    .parent_semantic_state_root
            || request.context.manifest_hash != request.value.execution_commitment.manifest_hash
        {
            return Err("private fetch exact execution context differs".into());
        }
        if request.kind == NodeBftPrivateFetchKindV3::CommittedState
            && self.core.credentials.is_none()
        {
            return Err("committed private fetch requires explicit V1 grant credentials".into());
        }
        let reservation = client::reserve(&self.core)?;
        let deadline = Instant::now() + timeout;
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = cancelled.clone();
        let core = self.core.clone();
        let worker_reservation = reservation.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut workers = self
            .workers
            .lock()
            .map_err(|_| "private fetch workers poisoned")?;
        let mut pending = Vec::new();
        for worker in workers.drain(..) {
            if worker.is_finished() {
                let _ = worker.join();
            } else {
                pending.push(worker);
            }
        }
        *workers = pending;
        let worker = std::thread::Builder::new()
            .name("bft-private-fetch".into())
            .spawn(move || {
                let _reservation = worker_reservation;
                let result = client::fetch(&core, request, signer, &cancel, deadline);
                let _ = tx.send(result);
            })
            .map_err(|_| "private fetch worker spawn failed")?;
        workers.push(worker);
        Ok(Box::new(client::Fetch {
            receiver: rx,
            cancelled,
            alive: self.core.alive.clone(),
            deadline,
            reservation: Some(reservation),
            done: false,
        }))
    }
}
impl Drop for NodeBftLibp2pPrivateCandidateExchangeV3 {
    fn drop(&mut self) {
        self.core.alive.store(false, Ordering::Release);
        self.registration.take();
        self.server.clear();
        if let Ok(mut workers) = self.workers.lock() {
            for worker in workers.drain(..) {
                let _ = worker.join();
            }
        }
    }
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
