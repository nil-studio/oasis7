use super::*;
use libp2p::PeerId;
use oasis7_consensus::tendermint_v3::{
    BFT_PUBLIC_PEER_MAX_BYTES_V3, decode_bft_public_peer_message_v3,
    encode_bft_public_peer_message_v3,
};
use oasis7_net::{Libp2pNetwork, WorldError};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
#[path = "assembly.rs"]
mod assembly;
use assembly::{CHUNK, Frame, Inbox, MEMORY, TTL};
const PROTOCOL: &str = "/oasis7/bft/header-peer/3.0.0";
fn rejected(reason: &str) -> WorldError {
    WorldError::NetworkRequestFailed {
        code: oasis7_proto::distributed::DistributedErrorCode::ErrBadRequest,
        message: reason.into(),
        retryable: false,
    }
}
fn context_digest(c: &BftPeerContextV3) -> Result<String, String> {
    let bytes = serde_cbor::to_vec(c).map_err(|_| "peer context encode")?;
    let mut h = blake3::Hasher::new();
    h.update(b"oasis7.bft.public-peer.context.v1\0");
    h.update(&bytes);
    Ok(h.finalize().to_hex().to_string())
}
fn message_digest(context: &str, bytes: &[u8]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"oasis7.bft.public-peer.message.v1\0");
    h.update(context.as_bytes());
    h.update(&(bytes.len() as u64).to_be_bytes());
    h.update(bytes);
    h.finalize().to_hex().to_string()
}
struct Counters {
    bytes: AtomicUsize,
    count: AtomicUsize,
}
struct Parcel {
    bytes: Vec<u8>,
    digest: String,
    deadline: Instant,
    counters: Arc<Counters>,
}
impl Drop for Parcel {
    fn drop(&mut self) {
        self.counters
            .bytes
            .fetch_sub(self.bytes.len(), Ordering::AcqRel);
        self.counters.count.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Outbound {
    queues: BTreeMap<PeerId, VecDeque<Arc<Parcel>>>,
    stop: bool,
}
/// Public plane only. Configured peer IDs are routes, not voting authority.
pub struct NodeBftLibp2pPeerTransportV3 {
    context: BftPeerContextV3,
    context_digest: String,
    peers: Vec<PeerId>,
    inbox: Arc<Mutex<Inbox>>,
    outbound: Arc<(Mutex<Outbound>, Condvar)>,
    counters: Arc<Counters>,
    workers: Vec<JoinHandle<()>>,
    errors: Arc<Mutex<VecDeque<String>>>,
}
impl NodeBftLibp2pPeerTransportV3 {
    pub fn new(
        network: Arc<Libp2pNetwork>,
        context: BftPeerContextV3,
        peers: Vec<String>,
    ) -> Result<Self, String> {
        if peers.len() > 256 {
            return Err("public peer route capacity".into());
        }
        let peers = peers
            .iter()
            .map(|p| {
                p.parse::<PeerId>()
                    .map_err(|_| "invalid configured peer ID".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let allowed: BTreeSet<_> = peers.iter().copied().collect();
        if allowed.len() != peers.len() || allowed.contains(&network.peer_id()) {
            return Err("duplicate/self peer route".into());
        }
        let digest = context_digest(&context)?;
        let inbox = Arc::new(Mutex::new(Inbox::new()));
        let outbound = Arc::new((
            Mutex::new(Outbound {
                queues: peers.iter().map(|p| (*p, VecDeque::new())).collect(),
                stop: false,
            }),
            Condvar::new(),
        ));
        let counters = Arc::new(Counters {
            bytes: AtomicUsize::new(0),
            count: AtomicUsize::new(0),
        });
        let errors = Arc::new(Mutex::new(VecDeque::new()));
        let mut owner = Self {
            context: context.clone(),
            context_digest: digest.clone(),
            peers: peers.clone(),
            inbox: inbox.clone(),
            outbound: outbound.clone(),
            counters,
            workers: Vec::new(),
            errors: errors.clone(),
        };
        // Resources exist before the immutable handler is published. Any failure
        // drops the owner and stops/join all successfully prepared workers.
        for peer in peers {
            let network = network.clone();
            let outbound = outbound.clone();
            let errors = errors.clone();
            let digest = digest.clone();
            let worker = std::thread::Builder::new()
                .name("bft-public-peer-send".into())
                .spawn(move || sender(network, peer, outbound, errors, digest))
                .map_err(|_| "public sender spawn failed")?;
            owner.workers.push(worker);
        }
        let admission_allowed = allowed.clone();
        let callback_outbound = outbound.clone();
        network
            .register_authenticated_context_handler_with_admission(
                PROTOCOL,
                64 * 1024,
                Box::new(move |peer, _| {
                    if admission_allowed.contains(peer) {
                        Ok(())
                    } else {
                        Err(rejected("unconfigured public peer route"))
                    }
                }),
                Box::new(move |transport, bytes| {
                    transport
                        .request_context()
                        .check_cancelled(|| rejected("public chunk cancelled"))?;
                    let frame = Frame::decode(bytes, &digest)
                        .map_err(|_| rejected("public chunk rejected"))?;
                    let mut state = inbox
                        .lock()
                        .map_err(|_| rejected("public inbox poisoned"))?;
                    if stopped(&callback_outbound) {
                        return Err(rejected("public transport stopped"));
                    }
                    transport
                        .request_context()
                        .check_cancelled(|| rejected("public chunk cancelled"))?;
                    state
                        .receive(transport.peer_id(), frame, &context)
                        .map_err(|_| rejected("public assembly rejected"))
                }),
            )
            .map_err(|_| "public authenticated handler registration rejected")?;
        Ok(owner)
    }
    fn enqueue(&self, targets: &[PeerId], message: &BftPublicPeerMessageV3) -> Result<(), String> {
        if targets.is_empty() {
            return Ok(());
        }
        let bytes = encode_bft_public_peer_message_v3(&self.context, message)?;
        let mut q = self
            .outbound
            .0
            .lock()
            .map_err(|_| "public send queue poisoned")?;
        if q.stop
            || self.counters.count.load(Ordering::Acquire) >= 128
            || bytes.len() > MEMORY.saturating_sub(self.counters.bytes.load(Ordering::Acquire))
        {
            return Err("public outbound capacity".into());
        }
        if targets.iter().any(|p| !q.queues.contains_key(p)) {
            return Err("public peer route unavailable".into());
        }
        self.counters.bytes.fetch_add(bytes.len(), Ordering::AcqRel);
        self.counters.count.fetch_add(1, Ordering::AcqRel);
        let parcel = Arc::new(Parcel {
            digest: message_digest(&self.context_digest, &bytes),
            bytes,
            deadline: Instant::now() + TTL,
            counters: self.counters.clone(),
        });
        for p in targets {
            q.queues
                .get_mut(p)
                .ok_or("public route missing")?
                .push_back(parcel.clone());
        }
        self.outbound.1.notify_all();
        Ok(())
    }
    pub fn drain_errors(&self) -> Vec<String> {
        self.errors
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    }
}
impl NodeBftPeerTransportV3 for NodeBftLibp2pPeerTransportV3 {
    fn context(&self) -> &BftPeerContextV3 {
        &self.context
    }
    fn peers(&self) -> Vec<String> {
        self.peers.iter().map(ToString::to_string).collect()
    }
    fn broadcast_public(&self, m: &BftPublicPeerMessageV3) -> Result<(), String> {
        self.enqueue(&self.peers, m)
    }
    fn send_public(&self, p: &str, m: &BftPublicPeerMessageV3) -> Result<(), String> {
        let p = p.parse().map_err(|_| "invalid public peer ID")?;
        self.enqueue(&[p], m)
    }
    fn drain_public(&self, max: usize) -> Result<Vec<NodeBftReceivedPublicV3>, String> {
        Ok(self
            .inbox
            .lock()
            .map_err(|_| "public inbox poisoned")?
            .drain(max))
    }
}
impl Drop for NodeBftLibp2pPeerTransportV3 {
    fn drop(&mut self) {
        if let Ok(mut q) = self.outbound.0.lock() {
            q.stop = true;
            q.queues.clear();
        }
        self.outbound.1.notify_all();
        if let Ok(mut inbox) = self.inbox.lock() {
            *inbox = Inbox::new();
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
fn stopped(q: &Arc<(Mutex<Outbound>, Condvar)>) -> bool {
    q.0.lock().map_or(true, |q| q.stop)
}
fn sender(
    network: Arc<Libp2pNetwork>,
    peer: PeerId,
    q: Arc<(Mutex<Outbound>, Condvar)>,
    errors: Arc<Mutex<VecDeque<String>>>,
    context: String,
) {
    loop {
        let parcel = {
            let Ok(mut state) = q.0.lock() else { return };
            loop {
                if state.stop {
                    return;
                };
                if let Some(p) = state.queues.get_mut(&peer).and_then(VecDeque::pop_front) {
                    break p;
                };
                state = match q.1.wait(state) {
                    Ok(s) => s,
                    Err(_) => return,
                };
            }
        };
        if let Err(reason) = send(&network, peer, &parcel, &q, &context) {
            if let Ok(mut e) = errors.lock() {
                if e.len() >= 128 {
                    e.pop_front();
                }
                e.push_back(reason);
            }
        }
    }
}
fn send(
    network: &Libp2pNetwork,
    peer: PeerId,
    p: &Parcel,
    q: &Arc<(Mutex<Outbound>, Condvar)>,
    context: &str,
) -> Result<(), String> {
    let mut offset = 0;
    while offset < p.bytes.len() {
        if stopped(q) || Instant::now() >= p.deadline {
            return Err("public send cancelled/deadline".into());
        }
        let frame = Frame::new(context, &p.bytes, &p.digest, offset)?;
        let next = p.bytes.len().min(offset + CHUNK);
        loop {
            if stopped(q) || Instant::now() >= p.deadline {
                return Err("public send cancelled/deadline".into());
            }
            let timeout = p
                .deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(1));
            match network
                .request_authenticated_to_peer_with_timeout(PROTOCOL, &frame, peer, timeout)
            {
                Ok(reply) => {
                    assembly::check_ack(&reply, context, &p.digest, next)?;
                    break;
                }
                Err(WorldError::NetworkRequestFailed {
                    code: oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited,
                    ..
                }) => std::thread::sleep(Duration::from_millis(100)),
                Err(_) => return Err("public peer request failed".into()),
            }
        }
        offset = next;
        // At most13.3 frames/s; the Net budget remains authoritative and shared
        // with private traffic. Backoff never changes the original deadline.
        std::thread::sleep(Duration::from_millis(75));
    }
    Ok(())
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
