//! Capability available only on authenticated encrypted libp2p inbound streams.
use super::*;
use oasis7_proto::distributed_net::NetworkRequestContext;
use std::time::{Duration, Instant};
/// This context is not serializable and cannot be constructed by callers.
/// It authenticates transport identity, not validator membership or authority.
pub struct AuthenticatedPeerRequestContext {
    base: NetworkRequestContext,
    peer: libp2p::PeerId,
}
impl AuthenticatedPeerRequestContext {
    pub fn request_context(&self) -> &NetworkRequestContext {
        &self.base
    }
    pub fn peer_id(&self) -> libp2p::PeerId {
        self.peer
    }
    pub(super) fn new(base: NetworkRequestContext, peer: libp2p::PeerId) -> Self {
        Self { base, peer }
    }
}
pub type AuthenticatedPeerHandler = Box<
    dyn Fn(&AuthenticatedPeerRequestContext, &[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync,
>;
pub type AuthenticatedPeerAdmission =
    Box<dyn Fn(&libp2p::PeerId, &[u8]) -> Result<(), WorldError> + Send + Sync>;
pub(super) struct Registration {
    pub max_bytes: usize,
    pub admission: AuthenticatedPeerAdmission,
    pub handler: AuthenticatedPeerHandler,
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
    ) -> Result<(), WorldError> {
        if max_request_bytes == 0 || max_request_bytes > AUTHENTICATED_REQUEST_MAX_BYTES {
            return Err(rejected("authenticated request bound outside hard limit"));
        }
        if !matches!(
            protocol,
            "/oasis7/bft/header-peer/3.0.0" | "/oasis7/bft/private-candidate/1.0.0"
        ) {
            return Err(rejected(
                "authenticated handler requires dedicated BFT namespace",
            ));
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.enqueue_command(super::Command::RegisterAuthenticatedHandler {
            protocol: protocol.into(),
            registration: Registration {
                max_bytes: max_request_bytes,
                admission,
                handler,
            },
            response: sender,
        })?;
        super::api::block_on_command_response(
            receiver,
            "register_authenticated_context_handler_with_admission",
        )
    }
}

pub(super) fn reserved(protocol: &str) -> bool {
    matches!(
        protocol,
        "/oasis7/bft/header-peer/3.0.0" | "/oasis7/bft/private-candidate/1.0.0"
    )
}
pub(super) fn install(
    handlers: &mut HashMap<String, super::runtime_loop::HandlerRegistration>,
    protocol: String,
    registration: Registration,
) -> Result<(), WorldError> {
    if !reserved(&protocol) || handlers.contains_key(&protocol) {
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
