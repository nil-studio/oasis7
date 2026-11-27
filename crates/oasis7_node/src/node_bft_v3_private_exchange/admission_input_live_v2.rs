//! Explicit, native Noise service for original admission-input V2 frames.
//! No default registration: only a separately provisioned protected authority
//! may supply fresh issuer state, exact material, budget and reply fencing.
//! Subject PoP and transport identity alone never grant original-byte access.
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use libp2p::request_response::Codec as _;
use oasis7_net::{
    AuthenticatedHandlerRegistration, AuthenticatedPeerRequestContext, Libp2pNetwork, WorldError,
};
use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};
use super::admission_input_request_v2::{
    AdmissionBatchOperationV2, AdmissionBatchRequestV2, AdmissionInputBoundContextV2,
    VerifiedAdmissionBatchSubjectV2, PROTOCOL_V2,
};
use super::admission_input_response_v2::{
    OriginalAdmissionBatchResponseV2, OriginalAdmissionPartBytesV2, OriginalAdmissionPartV2,
    RESPONSE_DOMAIN,
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
/// and exact readback. It is not a claim that an arbitrary local buffer is safe
/// to release without the authority's independent source verification.
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

/// Registered ONLY by an explicitly configured protected service. Merely
/// constructing an ordinary Node cannot register an original-input responder.
pub struct NodeBftOriginalAdmissionNoiseV2 {
    _registration: AuthenticatedHandlerRegistration,
}

impl NodeBftOriginalAdmissionNoiseV2 {
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
    let mut codec = libp2p::request_response::cbor::codec::Codec::<
        NetworkRequest,
        NetworkResponse,
    >::default();
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
    transport
        .restrict_reply_until(context.original_attempt_deadline)
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
    let original = authority.read_authorized_parts(&verified, context.original_attempt_deadline)?;
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
    if Instant::now() >= context.original_attempt_deadline
        || context.current_generation.load(Ordering::Acquire) != context.expected_generation
        || !context.reply_permitted.load(Ordering::Acquire)
    {
        return Err("original admission authority changed before reply".into());
    }
    let _hold_until_verified_charge = original.transport_hold;
    Ok(encoded)
}
