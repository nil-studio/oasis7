use super::wire::*;
use super::*;
use std::{sync::mpsc, thread, time::Duration};
pub(super) struct ClientCore {
    pub network: Arc<Libp2pNetwork>,
    pub binding: BftSignerBindingV3,
    pub state: Arc<NodeBftPeerExecutionStateV3>,
    pub policy: String,
    pub grant_generation: u64,
    pub timeout: Duration,
    pub alive: Arc<AtomicBool>,
    pub credentials: Option<Arc<dyn NodeBftCommittedReadCredentialsV3>>,
    pub active: Arc<std::sync::atomic::AtomicUsize>,
}
pub(super) struct Reservation(Arc<std::sync::atomic::AtomicUsize>);
impl Drop for Reservation {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(super) fn reserve(core: &ClientCore) -> Result<Arc<Reservation>, String> {
    core.active
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            if n < 2 { Some(n + 1) } else { None }
        })
        .map_err(|_| "private fetch global count/132MiB budget")?;
    Ok(Arc::new(Reservation(core.active.clone())))
}
pub(super) struct Fetch {
    pub receiver: mpsc::Receiver<Result<NodeBftPrivateCandidatePackageV3, String>>,
    pub cancelled: Arc<AtomicBool>,
    pub alive: Arc<AtomicBool>,
    pub deadline: Instant,
    pub reservation: Option<Arc<Reservation>>,
    pub done: bool,
}
impl NodeBftPrivateCandidateFetchV3 for Fetch {
    fn poll(&mut self) -> Result<Option<NodeBftPrivateCandidatePackageV3>, String> {
        if self.done {
            return Err("private fetch already consumed".into());
        }
        if self.cancelled.load(Ordering::Acquire)
            || !self.alive.load(Ordering::Acquire)
            || Instant::now() >= self.deadline
        {
            self.cancel();
            return Err("private original fetch cancelled/deadline".into());
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                self.done = true;
                self.reservation = None;
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(_) => {
                self.done = true;
                self.reservation = None;
                Err("private fetch worker disconnected".into())
            }
        }
    }
    fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl Drop for Fetch {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn live(core: &ClientCore, cancel: &AtomicBool, deadline: Instant) -> Result<(), String> {
    if !core.alive.load(Ordering::Acquire)
        || cancel.load(Ordering::Acquire)
        || Instant::now() >= deadline
    {
        return Err("private original fetch cancelled/deadline".into());
    }
    Ok(())
}
fn authorization(
    core: &ClientCore,
    target: &Target,
    signer: &NodeBftSharedSignerV3,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<NodeBftPrivateReadAuthorizationV3, String> {
    live(core, cancel, deadline)?;
    match target.kind {
        NodeBftPrivateFetchKindV3::PendingCandidate => {
            let view = core
                .state
                .view()?
                .ok_or("private local verified view unavailable")?;
            target.value.validate_for_schedule(&view.schedule)?;
            let key = view
                .schedule
                .active_set()
                .validator(&core.binding.local_validator_id)
                .ok_or("private reader is not current active validator")?;
            let body = target.pending_body(
                key.validator_id.clone(),
                key.public_key_hex.clone(),
                core.policy.clone(),
                core.grant_generation,
            );
            // Try-lock polling is cancellable. A held signer mutex never blocks
            // the consensus event loop or silently renews this operation.
            loop {
                live(core, cancel, deadline)?;
                match signer.try_lock() {
                    Ok(mut signer) => {
                        let proof = signer
                            .sign_candidate_read_request(&body)
                            .map_err(|_| "native typed candidate read key proof unavailable")?;
                        live(core, cancel, deadline)?;
                        proof.verify_for(&body, &key.public_key_hex)?;
                        return Ok(NodeBftPrivateReadAuthorizationV3::Pending(Box::new(proof)));
                    }
                    Err(std::sync::TryLockError::WouldBlock) => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => return Err("private read signer poisoned".into()),
                }
            }
        }
        NodeBftPrivateFetchKindV3::CommittedState => {
            let credentials = core
                .credentials
                .as_ref()
                .ok_or("committed private read requires explicit V1 read grant/key credentials")?;
            let grant = credentials.grant();
            let subject = grant.body.node_public_key.clone();
            let preimage = committed_payload(target, &grant, &subject)?;
            let signature_hex = credentials.sign_request(&preimage)?;
            verify_signature(&subject, &preimage, &signature_hex)?;
            live(core, cancel, deadline)?;
            Ok(NodeBftPrivateReadAuthorizationV3::Committed(Box::new(
                NodeBftCommittedReadAuthorizationV3 {
                    grant,
                    subject_public_key: subject,
                    signature_hex,
                },
            )))
        }
    }
}
fn rpc(
    core: &ClientCore,
    peer: libp2p::PeerId,
    request: &Request,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<Response, String> {
    request.validate()?;
    let bytes = encode_frame(request)?;
    let expected = request.request_digest()?;
    loop {
        live(core, cancel, deadline)?;
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(1));
        match core
            .network
            .request_authenticated_to_peer_with_timeout(PROTOCOL, &bytes, peer, timeout)
        {
            Ok(raw) => {
                live(core, cancel, deadline)?;
                let reply: Response = decode_frame(&raw)?;
                if reply.domain != DOMAIN
                    || reply.request_digest != expected
                    || reply.chunk_index != request.chunk_index
                {
                    return Err("private reply does not echo original request".into());
                }
                reply.manifest.operation().validate()?;
                return Ok(reply);
            }
            Err(WorldError::NetworkRequestFailed {
                retryable: false, ..
            }) => return Err("private peer denied original read request".into()),
            Err(_) => {
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}
pub(super) fn fetch(
    core: &ClientCore,
    request: NodeBftPrivateFetchRequestV3,
    signer: NodeBftSharedSignerV3,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<NodeBftPrivateCandidatePackageV3, String> {
    let peer = request
        .peer_id
        .parse::<libp2p::PeerId>()
        .map_err(|_| "private fetch peer identity")?;
    let nonce = {
        let mut h = blake3::Hasher::new();
        h.update(b"oasis7.bft.private-fetch.nonce.v1");
        let mut random = [0u8; 32];
        getrandom::fill(&mut random).map_err(|_| "private fetch CSPRNG unavailable")?;
        h.update(&random);
        h.finalize().to_hex().to_string()
    };
    let mut target = Target {
        context: oasis7_consensus::tendermint_v3::BftPeerContextV3::from_handoff(
            core.binding.handoff(),
        ),
        value: request.value.clone(),
        kind: request.kind,
        requester_peer_id: core.network.peer_id().to_string(),
        serving_peer_id: peer.to_string(),
        nonce,
        operation: BftCandidateReadOperationV3::Manifest,
    };
    target.validate()?;
    let auth = authorization(core, &target, &signer, cancel, deadline)?;
    let reply = rpc(
        core,
        peer,
        &Request {
            domain: DOMAIN.into(),
            schema_version: 1,
            target: target.clone(),
            authorization: auth,
            chunk_index: None,
        },
        cancel,
        deadline,
    )?;
    if reply.chunk_hex.is_some() {
        return Err("private manifest response included data".into());
    }
    let manifest = reply.manifest;
    target.operation = manifest.operation();
    let auth = authorization(core, &target, &signer, cancel, deadline)?;
    let total = usize::try_from(manifest.total_bytes).map_err(|_| "private package total size")?;
    if total > MAX_PACKAGE {
        return Err("private package capacity".into());
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(total)
        .map_err(|_| "private fetch allocation")?;
    for index in 0..manifest.total_chunks {
        // Shared Net admission is authoritative. Pacing and all retries consume
        // the one original absolute deadline; no chunk gets a fresh operation.
        live(core, cancel, deadline)?;
        thread::sleep(Duration::from_millis(75));
        let reply = rpc(
            core,
            peer,
            &Request {
                domain: DOMAIN.into(),
                schema_version: 1,
                target: target.clone(),
                authorization: auth.clone(),
                chunk_index: Some(index),
            },
            cancel,
            deadline,
        )?;
        if reply.manifest != manifest {
            return Err("private package manifest changed across chunks".into());
        }
        let chunk = hex::decode(reply.chunk_hex.as_ref().ok_or("private chunk missing")?)
            .map_err(|_| "private chunk hex")?;
        let (offset, len) = manifest.operation().chunk_range(index)?;
        if offset != bytes.len() as u64
            || chunk.len() != len as usize
            || hex::encode(&chunk) != reply.chunk_hex.unwrap_or_default()
        {
            return Err("private chunk exact range/encoding".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    live(core, cancel, deadline)?;
    if bytes.len() != total || super::package::package_digest(&bytes) != manifest.package_digest {
        return Err("private package original digest/length mismatch".into());
    }
    let package = super::package::decode_package(&bytes)?;
    if package.candidate.public_value != request.value {
        return Err("private package differs from exact requested public value".into());
    }
    live(core, cancel, deadline)?;
    Ok(package)
}
