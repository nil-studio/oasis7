//! Capability available only on authenticated encrypted libp2p inbound streams.
use super::*;
use oasis7_proto::distributed_net::NetworkRequestContext;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
/// In-process callback owner only; not validator or persistent signer authority.
pub struct AuthenticatedHandlerRegistration {
    alive: Arc<AtomicBool>,
    callbacks: SharedCallbacks,
}
impl AuthenticatedHandlerRegistration {
    pub fn close(&self) {
        self.alive.store(false, Ordering::Release);
        // Revoke before detaching idle resources. Running jobs retain their
        // own Arc until completion and are still fenced by the alive flag.
        let callbacks = self
            .callbacks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        drop(callbacks);
    }
}
impl Drop for AuthenticatedHandlerRegistration {
    fn drop(&mut self) {
        self.close();
    }
}
/// This context is not serializable and cannot be constructed by callers.
/// It authenticates transport identity, not validator membership or authority.
pub struct AuthenticatedPeerRequestContext {
    base: NetworkRequestContext,
    peer: libp2p::PeerId,
    reply: super::authenticated_reply::ReplyGuard,
}
impl AuthenticatedPeerRequestContext {
    pub fn request_context(&self) -> &NetworkRequestContext {
        &self.base
    }
    pub fn peer_id(&self) -> libp2p::PeerId {
        self.peer
    }
    pub fn restrict_reply_until(&self, deadline: Instant) -> Result<(), WorldError> {
        self.reply.until(deadline)
    }
    pub fn restrict_reply_to_generation(
        &self,
        generation: Arc<std::sync::atomic::AtomicU64>,
        expected: u64,
    ) -> Result<(), WorldError> {
        self.reply.generation(generation, expected)
    }
    pub fn restrict_reply_while(&self, allowed: Arc<AtomicBool>) -> Result<(), WorldError> {
        self.reply.while_true(allowed)
    }
    pub(super) fn new(
        base: NetworkRequestContext,
        peer: libp2p::PeerId,
        reply: super::authenticated_reply::ReplyGuard,
    ) -> Self {
        Self { base, peer, reply }
    }
}
pub type AuthenticatedPeerHandler = Box<
    dyn Fn(&AuthenticatedPeerRequestContext, &[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync,
>;
/// Cheap routing/shape admission on the swarm loop, before shared quota.
/// It must not perform cryptography, business work, disk or network I/O.
/// Expensive authorization belongs in the bounded handler worker.
pub type AuthenticatedPeerAdmission =
    Box<dyn Fn(&libp2p::PeerId, &[u8]) -> Result<(), WorldError> + Send + Sync>;
type SharedCallbacks = Arc<Mutex<Option<Arc<Callbacks>>>>;
pub(super) struct Callbacks {
    pub admission: AuthenticatedPeerAdmission,
    pub handler: AuthenticatedPeerHandler,
}
pub(super) struct Registration {
    pub alive: Arc<AtomicBool>,
    pub max_bytes: usize,
    pub callbacks: SharedCallbacks,
}
impl Registration {
    pub fn callbacks(&self) -> Result<Arc<Callbacks>, WorldError> {
        self.callbacks
            .lock()
            .map_err(|_| rejected("authenticated callbacks poisoned"))?
            .as_ref()
            .cloned()
            .ok_or_else(|| rejected("authenticated handler owner closed"))
    }
}
/// Shared across authenticated protocols, keyed only by Noise/QUIC peer identity.
/// Fixed one-second monotonic window:16 requests/peer,128 total,256 peer slots.
pub(super) struct Budget {
    started: Instant,
    total: usize,
    peers: HashMap<libp2p::PeerId, usize>,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            total: 0,
            peers: HashMap::new(),
        }
    }
}
impl Budget {
    /// Reject unconfigured identities/cheap malformed frames without spending
    /// scarce authenticated-peer quota. Only eligible routes reach admit().
    pub fn admit_routed(
        &mut self,
        peer: libp2p::PeerId,
        payload: &[u8],
        admission: &AuthenticatedPeerAdmission,
    ) -> Result<(), WorldError> {
        admission(&peer, payload)?;
        self.admit(peer)
    }
    pub fn admit(&mut self, peer: libp2p::PeerId) -> Result<(), WorldError> {
        if self.started.elapsed() >= Duration::from_secs(1) {
            self.started = Instant::now();
            self.total = 0;
            self.peers.clear();
        }
        let count = self.peers.get(&peer).copied().unwrap_or(0);
        if count >= 16
            || self.total >= 128
            || (!self.peers.contains_key(&peer) && self.peers.len() >= 256)
        {
            return Err(WorldError::NetworkRequestFailed {
                code: oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited,
                message: "authenticated request admission exhausted".into(),
                retryable: true,
            });
        }
        self.total += 1;
        self.peers.insert(peer, count + 1);
        Ok(())
    }
}
pub const AUTHENTICATED_REQUEST_MAX_BYTES: usize = 64 * 1024;
pub const AUTHENTICATED_RESPONSE_MAX_BYTES: usize = 64 * 1024;
pub(super) fn rejected(message: &str) -> WorldError {
    WorldError::NetworkRequestFailed {
        code: oasis7_proto::distributed::DistributedErrorCode::ErrBadRequest,
        message: message.into(),
        retryable: false,
    }
}
impl Libp2pNetwork {
    /// Dedicated authenticated protocols return typed bounded errors; the
    /// legacy generic request API intentionally continues returning raw bytes.
    pub fn request_authenticated_to_peer_with_timeout(
        &self,
        protocol: &str,
        payload: &[u8],
        peer: libp2p::PeerId,
        timeout: Duration,
    ) -> Result<Vec<u8>, WorldError> {
        if !reserved(protocol) || payload.len() > AUTHENTICATED_REQUEST_MAX_BYTES {
            return Err(rejected("authenticated request namespace/byte bound"));
        }
        let bytes = self.request_to_peer_with_timeout(protocol, payload, peer, timeout)?;
        decode_reply(bytes)
    }
    /// Uses real Noise/QUIC connection identity and admission before reserving a
    /// response worker. Does not expose this capability through DistributedNetwork.
    pub fn register_authenticated_context_handler_with_admission(
        &self,
        protocol: &str,
        max_request_bytes: usize,
        admission: AuthenticatedPeerAdmission,
        handler: AuthenticatedPeerHandler,
    ) -> Result<AuthenticatedHandlerRegistration, WorldError> {
        if max_request_bytes == 0 || max_request_bytes > AUTHENTICATED_REQUEST_MAX_BYTES {
            return Err(rejected("authenticated request bound outside hard limit"));
        }
        if !matches!(
            protocol,
            "/oasis7/bft/header-peer/3.0.0"
                | "/oasis7/bft/private-candidate/1.0.0"
                | "/oasis7/bft/private-candidate/2.0.0"
        ) {
            return Err(rejected(
                "authenticated handler requires dedicated BFT namespace",
            ));
        }
        let token = AuthenticatedHandlerRegistration {
            alive: Arc::new(AtomicBool::new(true)),
            callbacks: Arc::new(Mutex::new(Some(Arc::new(Callbacks { admission, handler })))),
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        self.enqueue_command(super::Command::RegisterAuthenticatedHandler {
            protocol: protocol.into(),
            registration: Registration {
                alive: token.alive.clone(),
                max_bytes: max_request_bytes,
                callbacks: token.callbacks.clone(),
            },
            response: sender,
        })?;
        super::api::block_on_command_response(
            receiver,
            "register_authenticated_context_handler_with_admission",
        )?;
        Ok(token)
    }
}

pub(super) fn reserved(protocol: &str) -> bool {
    matches!(
        protocol,
        "/oasis7/bft/header-peer/3.0.0"
            | "/oasis7/bft/private-candidate/1.0.0"
            | "/oasis7/bft/private-candidate/2.0.0"
    )
}
pub(super) fn install(
    handlers: &mut HashMap<String, super::runtime_loop::HandlerRegistration>,
    protocol: String,
    registration: Registration,
) -> Result<(), WorldError> {
    if !reserved(&protocol)
        || handlers.get(&protocol).is_some_and(|old| {
            old.authenticated
                .as_ref()
                .is_none_or(|registration| registration.alive.load(Ordering::Acquire))
        })
    {
        return Err(rejected(
            "authenticated handler already installed or namespace invalid",
        ));
    }
    handlers.insert(
        protocol,
        super::runtime_loop::HandlerRegistration {
            handler: Arc::new(|_, _| Err(rejected("authenticated transport context required"))),
            admission: None,
            authenticated: Some(Arc::new(registration)),
        },
    );
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactError {
    code: oasis7_proto::distributed::DistributedErrorCode,
    message: String,
    retryable: bool,
}
fn decode_reply(bytes: Vec<u8>) -> Result<Vec<u8>, WorldError> {
    if bytes.len() > AUTHENTICATED_RESPONSE_MAX_BYTES {
        return Err(rejected("authenticated response byte bound"));
    }
    // No Vec/map field is allocated from a declared count; serde reads the
    // three fixed fields from this bounded slice. Exact canonical re-encoding
    // and deny_unknown_fields distinguish an error from a domain-tagged ACK.
    if let Ok(error) = serde_cbor::from_slice::<ExactError>(&bytes)
        && crate::util::to_canonical_cbor(&error).is_ok_and(|canonical| canonical == bytes)
    {
        return Err(WorldError::NetworkRequestFailed {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
        });
    }
    Ok(bytes)
}
#[cfg(test)]
mod reply_tests {
    use super::*;
    #[test]
    fn only_exact_canonical_error_shape_is_converted() {
        let error = oasis7_proto::distributed::ErrorResponse::from_code(
            oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited,
            "limited",
        );
        assert!(matches!(
            decode_reply(crate::util::to_canonical_cbor(&error).unwrap()),
            Err(WorldError::NetworkRequestFailed {
                code: oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited,
                ..
            })
        ));
        let ack=crate::util::to_canonical_cbor(&serde_json::json!({"domain":"ack.v1","code":"ERR_BUSY","message":"ok","retryable":true})).unwrap();
        assert_eq!(decode_reply(ack.clone()).unwrap(), ack);
    }
}

#[cfg(test)]
mod routing_budget_tests {
    use super::*;
    #[test]
    fn rejected_routes_cannot_consume_authenticated_global_or_peer_capacity() {
        let trusted = (0..8).map(|_| libp2p::PeerId::random()).collect::<Vec<_>>();
        let allow = trusted.clone();
        let admission: AuthenticatedPeerAdmission = Box::new(move |peer, _| {
            if allow.contains(peer) {
                Ok(())
            } else {
                Err(rejected("unconfigured Noise peer"))
            }
        });
        let mut budget = Budget {
            started: Instant::now() + Duration::from_secs(3600),
            ..Budget::default()
        };
        // Keep this routing-order test inside one fixed window regardless of
        // ordinary test scheduling; expiry behavior has separate coverage.
        for _ in 0..8 {
            let attacker = libp2p::PeerId::random();
            for _ in 0..16 {
                assert!(
                    budget
                        .admit_routed(attacker, b"claimed trusted", &admission)
                        .is_err()
                );
                assert_eq!(budget.total, 0);
                assert!(budget.peers.is_empty());
            }
        }
        for peer in trusted {
            for _ in 0..16 {
                budget
                    .admit_routed(peer, b"valid frame", &admission)
                    .unwrap();
            }
        }
        assert_eq!(budget.total, 128);
        assert_eq!(budget.peers.len(), 8);
        let first = *budget.peers.keys().next().unwrap();
        assert!(
            budget
                .admit_routed(first, b"valid frame", &admission)
                .is_err()
        );
    }
}
