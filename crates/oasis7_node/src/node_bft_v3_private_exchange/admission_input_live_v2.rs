//! Explicit, native Noise service for original admission-input V2 frames.
//! No default registration: only a separately provisioned protected authority
//! may supply fresh issuer state, exact material, budget and reply fencing.
//! Subject PoP and transport identity alone never grant original-byte access.
use super::admission_input_request_v2::{
    AdmissionBatchOperationV2, AdmissionBatchRequestV2, AdmissionInputBoundContextV2, PROTOCOL_V2,
    VerifiedAdmissionBatchSubjectV2,
};
use super::admission_input_response_v2::{
    OriginalAdmissionBatchResponseV2, OriginalAdmissionPartBytesV2, OriginalAdmissionPartV2,
    RESPONSE_DOMAIN,
};
use libp2p::request_response::Codec as _;
use oasis7_distfs::protected_bft_v3::BftSignerBackendV3;
use oasis7_net::{
    AuthenticatedHandlerRegistration, AuthenticatedPeerRequestContext, Libp2pNetwork, WorldError,
};
use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// These fields are supplied from separately verified protected issuer and
/// original-attempt state. In-memory gates only narrow the Net reply guard;
/// they cannot replace protected fencing, an independent time source, or CAS.
pub struct ProtectedOriginalAdmissionContextV2 {
    pub bound: AdmissionInputBoundContextV2,
    pub original_attempt_deadline: Instant,
    pub current_generation: Arc<AtomicU64>,
    pub expected_generation: u64,
    pub reply_permitted: Arc<AtomicBool>,
}

/// The hold remains alive throughout binary encoding, protected byte-spend CAS
/// and exact readback, then through the actual Net response write/close. This
/// does not authorize releasing a local buffer without the authority's
/// independent source verification.
pub struct ProtectedOriginalAdmissionPartsV2 {
    pub parts: Vec<Vec<u8>>,
    pub transport_hold: Box<dyn Send + Sync>,
}

/// An actual deployed implementation must verify current issuer/witness,
/// source possession, non-revocation, original material and same-attempt limits
/// from independently protected storage. It must charge the complete encoded
/// NetworkResponse in the same protected namespace before returning success,
/// fail closed on unknown commits and NEVER refund accepted reservations.
/// The Node/Net adapter cannot fabricate these protected prerequisites.
pub trait ProtectedOriginalAdmissionAuthorityV2: Send + Sync {
    fn current_context(
        &self,
        request: &AdmissionBatchRequestV2,
    ) -> Result<ProtectedOriginalAdmissionContextV2, String>;

    fn read_authorized_parts(
        &self,
        subject: &VerifiedAdmissionBatchSubjectV2,
        deadline: Instant,
    ) -> Result<ProtectedOriginalAdmissionPartsV2, String>;

    fn reserve_encoded_response_and_readback(
        &self,
        subject: &VerifiedAdmissionBatchSubjectV2,
        exact_outer_bytes: u64,
    ) -> Result<(), String>;
}

/// Whole-tuple, nonrefundable outer Noise response charging over a REAL
/// independently protected backend selected by the owner. There is no local
/// counter, cache-only readback, extra WAL, or legacy Fresh-only CAS fallback.
/// The injected backend must enforce a single writer, exact leased token,
/// monotonic protected time and one atomic Fresh/FirstReserve/original-ops CAS.
/// An unknown commit/readback poisons this adapter until the owner explicitly
/// cold-recovers under a new externally fenced lease.
pub struct ProtectedOriginalResponseChargeV2 {
    backend: Mutex<Box<dyn BftSignerBackendV3>>,
    lease_generation: u64,
    lease_token_digest: String,
    poisoned: AtomicBool,
}

impl ProtectedOriginalResponseChargeV2 {
    pub fn new(
        backend: Box<dyn BftSignerBackendV3>,
        lease_generation: u64,
        lease_token_digest: String,
    ) -> Result<Self, String> {
        if lease_generation == 0
            || lease_token_digest.len() != 64
            || !lease_token_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("original response requires an exact protected lease".into());
        }
        Ok(Self {
            backend: Mutex::new(backend),
            lease_generation,
            lease_token_digest,
            poisoned: AtomicBool::new(false),
        })
    }

    /// Called only after verified Noise subject, protected current context,
    /// exact source read and full outer NetworkResponse encoding. This method
    /// charges the *same original retention namespace* once before release.
    /// Error or uncertain commit never means the charge can be refunded.
    pub fn reserve_encoded_response_and_readback(
        &self,
        subject: &VerifiedAdmissionBatchSubjectV2,
        exact_outer_bytes: u64,
    ) -> Result<(), String> {
        if self.poisoned.load(Ordering::Acquire) {
            return Err("protected original response charge requires cold recovery".into());
        }
        let mut backend = self
            .backend
            .lock()
            .map_err(|_| "protected original response backend lock poisoned")?;
        if self.poisoned.load(Ordering::Acquire) {
            return Err("protected original response charge requires cold recovery".into());
        }
        let result = (|| {
            let expected = backend
                .read_original_operation_envelope_for_lease(
                    self.lease_generation,
                    &self.lease_token_digest,
                )
                .map_err(|error| error.to_string())?;
            let reference = &subject.read().reference;
            let marker = &expected.current.current.marker;
            if reference.source != marker.source
                || reference.key.ledger_origin_digest != marker.ledger_origin_digest
                || reference.key.height != marker.initial_height
            {
                return Err("original response differs from protected H source".into());
            }
            let next = expected
                .prepare_original_response_charge(
                    self.lease_generation,
                    &self.lease_token_digest,
                    exact_outer_bytes,
                )
                .map_err(|error| error.to_string())?;
            backend
                .compare_and_commit_original_operation_envelope(&expected, &next)
                .map_err(|error| error.to_string())?;
            let actual = backend
                .read_original_operation_envelope_for_lease(
                    self.lease_generation,
                    &self.lease_token_digest,
                )
                .map_err(|error| error.to_string())?;
            if actual != next {
                return Err("original response whole-tuple CAS readback uncertain".into());
            }
            Ok(())
        })();
        if result.is_err() {
            // Even a byte/revision mismatch cannot be assumed refundable.
            // The active owner must cold-reopen exact backend state before any
            // further release; never retry an ambiguous debit internally.
            self.poisoned.store(true, Ordering::Release);
        }
        result
    }
}

/// A separately authenticated material/issuer source. Unlike the broad
/// responder trait, it CANNOT implement a hidden second response-spend ledger:
/// the sole charge path is the same fenced original-retention backend below.
/// Implementations must independently verify current lease, issuer/grant,
/// source possession and protected time before returning original material.
pub trait ProtectedOriginalAdmissionSourceV2: Send + Sync {
    fn current_context(
        &self,
        request: &AdmissionBatchRequestV2,
    ) -> Result<ProtectedOriginalAdmissionContextV2, String>;

    fn read_authorized_parts(
        &self,
        subject: &VerifiedAdmissionBatchSubjectV2,
        deadline: Instant,
    ) -> Result<ProtectedOriginalAdmissionPartsV2, String>;
}

/// The actual registered Noise responder uses BOTH independent gates: its
/// material/issuer source and the original full-tuple response-spend CAS.
/// Never allow a separate memory counter, generic State CAS, second WAL or
/// successful fallback when the protected backend is unprovisioned.
pub struct ProtectedOriginalAdmissionRetainedAuthorityV2 {
    source: Arc<dyn ProtectedOriginalAdmissionSourceV2>,
    response_charge: ProtectedOriginalResponseChargeV2,
}

impl ProtectedOriginalAdmissionRetainedAuthorityV2 {
    pub fn new(
        source: Arc<dyn ProtectedOriginalAdmissionSourceV2>,
        response_charge: ProtectedOriginalResponseChargeV2,
    ) -> Self {
        Self {
            source,
            response_charge,
        }
    }
}

impl ProtectedOriginalAdmissionAuthorityV2 for ProtectedOriginalAdmissionRetainedAuthorityV2 {
    fn current_context(
        &self,
        request: &AdmissionBatchRequestV2,
    ) -> Result<ProtectedOriginalAdmissionContextV2, String> {
        self.source.current_context(request)
    }

    fn read_authorized_parts(
        &self,
        subject: &VerifiedAdmissionBatchSubjectV2,
        deadline: Instant,
    ) -> Result<ProtectedOriginalAdmissionPartsV2, String> {
        self.source.read_authorized_parts(subject, deadline)
    }

    fn reserve_encoded_response_and_readback(
        &self,
        subject: &VerifiedAdmissionBatchSubjectV2,
        exact_outer_bytes: u64,
    ) -> Result<(), String> {
        self.response_charge
            .reserve_encoded_response_and_readback(subject, exact_outer_bytes)
    }
}

/// Registered ONLY by an explicitly configured protected service. Merely
/// constructing an ordinary Node cannot register an original-input responder.
pub struct NodeBftOriginalAdmissionNoiseV2 {
    _registration: AuthenticatedHandlerRegistration,
}

impl NodeBftOriginalAdmissionNoiseV2 {
    /// Explicit production combination: the same independently protected
    /// current source + one full original-retention transaction backend.
    /// A missing backend primitive fails closed at actual response charging;
    /// construction never creates or resets the protected namespace.
    pub fn new_with_retained_charge(
        network: Arc<Libp2pNetwork>,
        peer_ids: Vec<String>,
        source: Arc<dyn ProtectedOriginalAdmissionSourceV2>,
        backend: Box<dyn BftSignerBackendV3>,
        lease_generation: u64,
        lease_token_digest: String,
    ) -> Result<Self, String> {
        let response_charge =
            ProtectedOriginalResponseChargeV2::new(backend, lease_generation, lease_token_digest)?;
        let authority = Arc::new(ProtectedOriginalAdmissionRetainedAuthorityV2::new(
            source,
            response_charge,
        ));
        Self::new(network, peer_ids, authority)
    }

    pub fn new(
        network: Arc<Libp2pNetwork>,
        peer_ids: Vec<String>,
        authority: Arc<dyn ProtectedOriginalAdmissionAuthorityV2>,
    ) -> Result<Self, String> {
        if peer_ids.is_empty() || peer_ids.len() > 256 {
            return Err("original admission configured peer capacity".into());
        }
        let peers: BTreeSet<libp2p::PeerId> = peer_ids
            .iter()
            .map(|peer| {
                peer.parse()
                    .map_err(|_| "original admission configured Noise peer invalid".to_owned())
            })
            .collect::<Result<_, _>>()?;
        if peers.len() != peer_ids.len() || peers.contains(&network.peer_id()) {
            return Err("original admission duplicate or self Noise peer".into());
        }
        let local_peer = network.peer_id().to_string();
        let registration = network
            .register_authenticated_context_handler_with_admission(
                PROTOCOL_V2,
                super::wire::MAX_FRAME,
                Box::new(move |peer, _| {
                    if peers.contains(peer) {
                        Ok(())
                    } else {
                        Err(rejected())
                    }
                }),
                Box::new(move |transport, bytes| {
                    handle(transport, bytes, &local_peer, authority.as_ref())
                        .map_err(|_| rejected())
                }),
            )
            .map_err(|_| "original admission Noise registration rejected")?;
        Ok(Self {
            _registration: registration,
        })
    }
}

fn rejected() -> WorldError {
    WorldError::NetworkRequestFailed {
        code: oasis7_proto::distributed::DistributedErrorCode::ErrBadRequest,
        message: "original admission authorization or response rejected".into(),
        retryable: false,
    }
}

fn actual_outer_response_bytes(payload: &[u8]) -> Result<u64, String> {
    let mut codec =
        libp2p::request_response::cbor::codec::Codec::<NetworkRequest, NetworkResponse>::default();
    let mut writer = futures::io::Cursor::new(Vec::new());
    futures::executor::block_on(codec.write_response(
        &libp2p::StreamProtocol::new(PROTOCOL_V2),
        &mut writer,
        NetworkResponse {
            payload: payload.to_vec(),
        },
    ))
    .map_err(|_| "original admission actual NetworkResponse encode failed")?;
    u64::try_from(writer.into_inner().len())
        .map_err(|_| "original admission response outer byte count overflow".into())
}

fn handle(
    transport: &AuthenticatedPeerRequestContext,
    bytes: &[u8],
    local_peer: &str,
    authority: &dyn ProtectedOriginalAdmissionAuthorityV2,
) -> Result<Vec<u8>, String> {
    let request = AdmissionBatchRequestV2::decode(bytes)?;
    let mut context = authority.current_context(&request)?;
    // These two IDs come from real Noise/QUIC connection state, never payload
    // or self-reported protected context.
    context.bound.authenticated_requester_peer_id = transport.peer_id().to_string();
    context.bound.authenticated_serving_peer_id = local_peer.to_owned();
    let verified = request.verify_bound_subject(&context.bound)?;
    let now = Instant::now();
    if context.original_attempt_deadline <= now
        || context.original_attempt_deadline
            > now
                .checked_add(Duration::from_secs(120))
                .ok_or("original admission 120s deadline overflow")?
        || context.expected_generation == 0
        || context.current_generation.load(Ordering::Acquire) != context.expected_generation
        || !context.reply_permitted.load(Ordering::Acquire)
    {
        return Err("original admission protected attempt is unavailable".into());
    }
    // The entire original attempt is at most 120 seconds, and each actual
    // authenticated RPC must finish within a separate 10-second budget.
    let rpc_deadline = now
        .checked_add(Duration::from_secs(10))
        .ok_or("original admission per-RPC deadline overflow")?;
    let response_deadline = rpc_deadline.min(context.original_attempt_deadline);
    transport
        .restrict_reply_until(response_deadline)
        .map_err(|_| "original admission deadline guard rejected")?;
    transport
        .restrict_reply_to_generation(
            context.current_generation.clone(),
            context.expected_generation,
        )
        .map_err(|_| "original admission generation guard rejected")?;
    transport
        .restrict_reply_while(context.reply_permitted.clone())
        .map_err(|_| "original admission release guard rejected")?;
    let original = authority.read_authorized_parts(&verified, response_deadline)?;
    let ranges = request.read.ranges()?;
    if original.parts.len() != ranges.len() {
        return Err("original admission exact material part count invalid".into());
    }
    let mut parts = Vec::with_capacity(ranges.len());
    for (bytes, range) in original.parts.iter().zip(ranges.iter()) {
        if bytes.len() != range.length as usize {
            return Err("original admission exact material part length invalid".into());
        }
        parts.push(OriginalAdmissionPartV2 {
            index: range.index,
            bytes: OriginalAdmissionPartBytesV2::new(bytes.clone())?,
        });
    }
    let offset_bytes = match request.read.operation {
        AdmissionBatchOperationV2::Manifest => 0,
        AdmissionBatchOperationV2::Batch { offset_bytes, .. } => offset_bytes,
    };
    let response = OriginalAdmissionBatchResponseV2 {
        domain: RESPONSE_DOMAIN.into(),
        request_digest: request.request_digest()?,
        reference: request.read.reference.clone(),
        offset_bytes,
        parts,
    };
    let encoded = response.encode_for(&request)?;
    let encoded_outer_bytes = actual_outer_response_bytes(&encoded)?;
    // Charge only after the complete verified frame exists, but BEFORE the
    // reply can be released. An unknown/missing backend readback denies it.
    authority.reserve_encoded_response_and_readback(&verified, encoded_outer_bytes)?;
    if Instant::now() >= response_deadline
        || context.current_generation.load(Ordering::Acquire) != context.expected_generation
        || !context.reply_permitted.load(Ordering::Acquire)
    {
        return Err("original admission authority changed before reply".into());
    }
    transport
        .retain_reply_resource(original.transport_hold)
        .map_err(|_| "original admission response hold handoff rejected")?;
    Ok(encoded)
}
