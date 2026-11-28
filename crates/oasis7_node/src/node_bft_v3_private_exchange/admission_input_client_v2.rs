//! Real authenticated Noise client for a *single original* admission-input attempt.
//! It returns only BLAKE3/length-staged raw original bytes, never admission
//! authority, a CAS publication, a protected transfer receipt or Runtime root.
//! The caller must use a disposable/private sink, publish only after successful
//! canonical AdmissionInput validation and protected CAS exact readback.
//! A failed/unknown RPC is terminal for this invocation: no hidden retry/refund.
use super::admission_input_request_v2::{
    AdmissionBatchOperationV2, AdmissionBatchRequestV2, AdmissionInputBoundContextV2, PROTOCOL_V2,
};
use super::admission_input_response_v2::OriginalAdmissionBatchResponseV2;
use oasis7_distfs::protected_bft_v3::{
    BftAdmissionInputTransferPlanV2, BftAdmissionInputV2StreamStage,
    BftPrivateRetentionAdmissionInputV1, LocalBftSuccessorArchiveStoreV1,
};
use oasis7_net::Libp2pNetwork;
use std::{
    collections::VecDeque,
    io::Write,
    time::{Duration, Instant},
};

const ATTEMPT_LIMIT: Duration = Duration::from_secs(120);
const RPC_LIMIT: Duration = Duration::from_secs(10);
const PEER_WINDOW: Duration = Duration::from_secs(1);
const PEER_WINDOW_MAX: usize = 16;
const PARTS_PER_MESSAGE: u64 = 3;
const PART_BYTES: u64 = 16 * 1024;

/// Limit this caller's Noise requests to sixteen per rolling second, counting
/// Manifest too. Other callers still share the Net-level 16/peer, 128/global
/// budget; a remote denial is terminal, not a permission to retry for free.
fn admit_peer_request(
    recent: &mut VecDeque<Instant>,
    attempt_deadline: Instant,
) -> Result<(), String> {
    loop {
        let now = Instant::now();
        if now >= attempt_deadline {
            return Err("original admission attempt deadline exhausted".into());
        }
        while recent
            .front()
            .is_some_and(|start| now.saturating_duration_since(*start) >= PEER_WINDOW)
        {
            recent.pop_front();
        }
        if recent.len() < PEER_WINDOW_MAX {
            recent.push_back(now);
            return Ok(());
        }
        let next = recent[0]
            .checked_add(PEER_WINDOW)
            .ok_or("original admission rate window overflow")?;
        if next >= attempt_deadline {
            return Err("original admission per-peer rate exceeds deadline".into());
        }
        std::thread::sleep(next.saturating_duration_since(now));
    }
}

fn signed_operation<F>(
    template: &AdmissionBatchRequestV2,
    operation: AdmissionBatchOperationV2,
    context: &AdmissionInputBoundContextV2,
    sign_subject: &mut F,
) -> Result<AdmissionBatchRequestV2, String>
where
    F: FnMut(&[u8]) -> Result<String, String>,
{
    let mut request = template.clone();
    request.read.operation = operation;
    // The issuer's original dedicated grant/ref/nonce/Noise peers do not
    // change; only the subject re-signs this exact manifest/batch operation.
    request.authorization.signature_hex =
        sign_subject(&request.read.signing_bytes(&request.authorization.grant)?)?;
    request.verify_bound_subject(context)?;
    if request.original_attempt_digest()? != template.original_attempt_digest()? {
        return Err("original admission operation changed original attempt".into());
    }
    Ok(request)
}

fn query(
    network: &Libp2pNetwork,
    peer: libp2p::PeerId,
    request: &AdmissionBatchRequestV2,
    recent: &mut VecDeque<Instant>,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    admit_peer_request(recent, deadline)?;
    let payload = request.encode()?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("original admission attempt deadline exhausted".into());
    }
    network
        .request_authenticated_to_peer_with_timeout(
            PROTOCOL_V2,
            &payload,
            peer,
            remaining.min(RPC_LIMIT),
        )
        .map_err(|_| "original admission Noise request denied, lost or timed out".into())
}

/// Check the original absolute deadline both before and after completing the
/// digest/length proof and flushing the staging sink. A successful last RPC
/// does not extend the original attempt while final local work is running.
pub(super) fn finish_before_attempt_deadline<W: Write>(
    staging: BftAdmissionInputV2StreamStage<W>,
    deadline: Instant,
    now: impl Fn() -> Instant,
) -> Result<W, String> {
    if now() >= deadline {
        return Err("original admission attempt deadline exhausted".into());
    }
    let sink = staging.finish().map_err(|error| error.to_string())?;
    if now() >= deadline {
        return Err("original admission attempt deadline exhausted".into());
    }
    Ok(sink)
}

/// Fetch the original signed grant's Manifest followed by every exact 16-KiB
/// part over real authenticated Noise and stage the original into `sink`.
///
/// The already-issued dedicated grant and separately verified bound context
/// must refer to this actual local/remote Noise identity and original nonce.
/// `sign_subject` signs the existing subject key's operation preimage only;
/// it must not issue grants. The full attempt uses one non-resettable 120-second
/// deadline; each RPC has a separate <=10-second bound. No transport error
/// retries or budget refunds occur. Return value is raw original bytes ONLY:
/// the consumer still owes canonical decoding, original admission_digest,
/// immutable CAS exact readback, protected 4-GiB transfer ledger and Runtime
/// acceptance, and must not publish the sink from a failed attempt.
pub fn fetch_original_admission_input_v2<W, F>(
    network: &Libp2pNetwork,
    peer: libp2p::PeerId,
    template: &AdmissionBatchRequestV2,
    context: &AdmissionInputBoundContextV2,
    mut sign_subject: F,
    sink: W,
) -> Result<W, String>
where
    W: Write,
    F: FnMut(&[u8]) -> Result<String, String>,
{
    let deadline = Instant::now()
        .checked_add(ATTEMPT_LIMIT)
        .ok_or("original admission attempt deadline overflow")?;
    fetch_original_admission_input_v2_before_deadline(
        network, peer, template, context, &mut sign_subject, sink, deadline,
    )
}

/// A real authenticated Noise attempt, followed by canonical original input
/// validation and immutable CAS exact readback. External protected 4-GiB
/// transfer budget, independent issuer/custody, buffer holds and real Runtime
/// acceptance must already be provided by the operator: a local CAS object
/// does not install or authorize a BFT successor.
pub fn fetch_original_admission_input_v2_into_cas<F>(
    network: &Libp2pNetwork,
    peer: libp2p::PeerId,
    template: &AdmissionBatchRequestV2,
    context: &AdmissionInputBoundContextV2,
    mut sign_subject: F,
    archive: &LocalBftSuccessorArchiveStoreV1,
) -> Result<BftPrivateRetentionAdmissionInputV1, String>
where
    F: FnMut(&[u8]) -> Result<String, String>,
{
    let deadline = Instant::now()
        .checked_add(ATTEMPT_LIMIT)
        .ok_or("original admission attempt deadline overflow")?;
    let bytes = fetch_original_admission_input_v2_before_deadline(
        network, peer, template, context, &mut sign_subject, Vec::new(), deadline,
    )?;
    if Instant::now() >= deadline {
        return Err("original admission attempt deadline exhausted before CAS".into());
    }
    archive
        .persist_original_admission_input_raw(&template.read.reference, &bytes)
        .map_err(|error| error.to_string())?;
    if Instant::now() >= deadline {
        return Err("original admission attempt deadline exhausted after CAS".into());
    }
    // Release the original staging allocation before a separate cold read.
    drop(bytes);
    let readback = archive
        .load_original_admission_input(&template.read.reference)
        .map_err(|error| error.to_string())?;
    if Instant::now() >= deadline {
        return Err("original admission attempt deadline exhausted after readback".into());
    }
    Ok(readback)
}

fn fetch_original_admission_input_v2_before_deadline<W, F>(
    network: &Libp2pNetwork,
    peer: libp2p::PeerId,
    template: &AdmissionBatchRequestV2,
    context: &AdmissionInputBoundContextV2,
    sign_subject: &mut F,
    sink: W,
    deadline: Instant,
) -> Result<W, String>
where
    W: Write,
    F: FnMut(&[u8]) -> Result<String, String>,
{
    template.verify_bound_subject(context)?;
    if template.read.requester_peer_id != network.peer_id().to_string()
        || template.read.serving_peer_id != peer.to_string()
        || context.authenticated_requester_peer_id != network.peer_id().to_string()
        || context.authenticated_serving_peer_id != peer.to_string()
    {
        return Err("original admission signed grant mismatches actual Noise peers".into());
    }
    let plan = BftAdmissionInputTransferPlanV2::for_original(&template.read.reference)
        .map_err(|error| error.to_string())?;
    let mut recent = VecDeque::with_capacity(PEER_WINDOW_MAX);
    let manifest = signed_operation(
        template,
        AdmissionBatchOperationV2::Manifest,
        context,
        sign_subject,
    )?;
    let raw_manifest = query(network, peer, &manifest, &mut recent, deadline)?;
    OriginalAdmissionBatchResponseV2::decode_for(&raw_manifest, &manifest)?;
    let mut staging = BftAdmissionInputV2StreamStage::new(&template.read.reference, sink)
        .map_err(|error| error.to_string())?;

    for index in 0..plan.messages() {
        let parts = plan.message(index).map_err(|error| error.to_string())?;
        let part_count = parts.parts.iter().flatten().count() as u8;
        let offset_bytes = u64::from(index)
            .checked_mul(PARTS_PER_MESSAGE * PART_BYTES)
            .ok_or("original admission batch offset overflow")?;
        let request = signed_operation(
            template,
            AdmissionBatchOperationV2::Batch {
                offset_bytes,
                part_count,
            },
            context,
            sign_subject,
        )?;
        let raw_response = query(network, peer, &request, &mut recent, deadline)?;
        OriginalAdmissionBatchResponseV2::decode_and_stage(&raw_response, &request, &mut staging)?;
    }
    finish_before_attempt_deadline(staging, deadline, Instant::now)
}
