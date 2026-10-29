//! Real private Noise reads with a gate-only barrier and explicit public partition.
use super::*;
use oasis7_consensus::tendermint_v3::{
    BftPeerContextV3, BftProposalIntentV3, BftPublicPeerMessageV3, BftPublicProposalV3,
    BftVoteIntentV3,
};
use oasis7_node::{
    NodeBftPeerTransportV3, NodeBftPrivateCandidateExchangeV3, NodeBftPrivateCandidateFetchV3,
    NodeBftPrivateFetchKindV3, NodeBftPrivateFetchRequestV3, NodeBftPrivateReadAuthorizationV3,
    NodeBftPrivateReadGateV3, NodeBftPrivateReadScopeV3, NodeBftReceivedPublicV3,
    NodeBftSharedSignerV3,
};
use oasis7_proto::distributed_bft::v3::BftVoteV3;
use std::sync::{
    Condvar,
    atomic::{AtomicBool, Ordering},
};
pub(super) struct PartitionedPublic {
    pub inner: Arc<dyn NodeBftPeerTransportV3>,
    pub enabled: Arc<AtomicBool>,
    pub held: Mutex<Vec<(Option<String>, BftPublicPeerMessageV3)>>,
    pub drains: Arc<std::sync::atomic::AtomicU64>,
    pub received: Arc<std::sync::atomic::AtomicU64>,
}
impl NodeBftPeerTransportV3 for PartitionedPublic {
    fn context(&self) -> &BftPeerContextV3 {
        self.inner.context()
    }
    fn peers(&self) -> Vec<String> {
        self.inner.peers()
    }
    fn broadcast_public(&self, m: &BftPublicPeerMessageV3) -> Result<(), String> {
        if self.enabled.load(Ordering::Acquire) {
            self.inner.broadcast_public(m)
        } else {
            let mut held = self.held.lock().unwrap();
            if held.len() >= 128 {
                return Err("test paused delivery queue capacity".into());
            }
            if !held.iter().any(|(peer, msg)| peer.is_none() && msg == m) {
                held.push((None, m.clone()));
            }
            Ok(())
        }
    }
    fn send_public(&self, p: &str, m: &BftPublicPeerMessageV3) -> Result<(), String> {
        if self.enabled.load(Ordering::Acquire) {
            self.inner.send_public(p, m)
        } else {
            let mut held = self.held.lock().unwrap();
            if held.len() >= 128 {
                return Err("test paused delivery queue capacity".into());
            }
            if !held
                .iter()
                .any(|(peer, msg)| peer.as_deref() == Some(p) && msg == m)
            {
                held.push((Some(p.into()), m.clone()));
            }
            Ok(())
        }
    }
    fn drain_public(&self, max: usize) -> Result<Vec<NodeBftReceivedPublicV3>, String> {
        if self.enabled.load(Ordering::Acquire) {
            for (peer, message) in std::mem::take(&mut *self.held.lock().unwrap()) {
                if let Some(peer) = peer {
                    self.inner.send_public(&peer, &message)?;
                } else {
                    self.inner.broadcast_public(&message)?;
                }
            }
        }
        let messages = self.inner.drain_public(max)?;
        self.received
            .fetch_add(messages.len() as u64, Ordering::AcqRel);
        self.drains.fetch_add(1, Ordering::AcqRel);
        Ok(messages)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
struct Manifest {
    package_digest: String,
    total_bytes: u64,
    chunk_size: u32,
    total_chunks: u32,
}
#[derive(serde::Deserialize)]
struct Probe {
    request_digest: String,
    manifest: Manifest,
    chunk_index: Option<u32>,
}
struct Seen {
    probe: Probe,
    signature: String,
}
struct Barrier {
    revision: u64,
    header: Option<String>,
    released: bool,
    arrived: bool,
    release_allowed: Arc<AtomicBool>,
    seen: Vec<Seen>,
    ignored: std::collections::BTreeSet<String>,
}
pub(super) struct BarrierGate {
    inner: Arc<dyn NodeBftPrivateReadGateV3>,
    state: Mutex<Barrier>,
    wake: Condvar,
}
impl BarrierGate {
    pub fn new(inner: Arc<dyn NodeBftPrivateReadGateV3>) -> Self {
        Self {
            inner,
            state: Mutex::new(Barrier {
                revision: 0,
                header: None,
                released: true,
                arrived: false,
                release_allowed: Arc::new(AtomicBool::new(true)),
                seen: Vec::new(),
                ignored: Default::default(),
            }),
            wake: Condvar::new(),
        }
    }
    fn arm(&self, header: &str) {
        let mut s = self.state.lock().unwrap();
        let revision = s.revision + 1;
        // Old expired RPC callbacks may still drain from Net. Their exact
        // signatures must never satisfy a later operation's test barrier.
        let mut ignored = std::mem::take(&mut s.ignored);
        ignored.extend(s.seen.iter().map(|seen| seen.signature.clone()));
        *s = Barrier {
            revision,
            header: Some(header.into()),
            released: false,
            arrived: false,
            release_allowed: Arc::new(AtomicBool::new(false)),
            seen: Vec::new(),
            ignored,
        };
    }
    fn release(&self) {
        let mut state = self.state.lock().unwrap();
        state.release_allowed.store(true, Ordering::Release);
        state.released = true;
        drop(state);
        self.wake.notify_all();
    }
    fn wait(&self) {
        network_wait("authorized first chunk barrier", || {
            self.state.lock().unwrap().arrived
        });
    }
}
impl NodeBftPrivateReadGateV3 for BarrierGate {
    fn authorize(
        &self,
        transport: &oasis7_net::AuthenticatedPeerRequestContext,
        authorization: &NodeBftPrivateReadAuthorizationV3,
        scope: &NodeBftPrivateReadScopeV3,
        make: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String> {
        // No permission shortcut: the production gate constructs the bytes and
        // installs the actual transport deadline before this test can pause.
        let bytes = self
            .inner
            .authorize(transport, authorization, scope, make)?;
        let mut s = self.state.lock().map_err(|_| "test barrier poisoned")?;
        if scope.kind == NodeBftPrivateFetchKindV3::CommittedState
            && scope.subject_public_key
                == hex::encode(fixtures::key(130).verifying_key().to_bytes())
            && s.header.as_ref() == Some(&scope.value.header.block_id()?)
        {
            let probe: Probe = serde_cbor::from_slice(&bytes).map_err(|_| "test response probe")?;
            let first = probe.chunk_index == Some(0) && !s.arrived;
            let signature = match authorization {
                NodeBftPrivateReadAuthorizationV3::Committed(a) => a.signature_hex.clone(),
                _ => return Err("wrong committed proof".into()),
            };
            if s.ignored.contains(&signature) {
                return Ok(bytes);
            }
            if probe.chunk_index == Some(0) && !s.released {
                // Restriction is installed even though its initial false state
                // returns Err. Complete must retry, never release these bytes.
                let _ = transport.restrict_reply_while(s.release_allowed.clone());
            }
            s.seen.push(Seen { probe, signature });
            if first {
                let revision = s.revision;
                s.arrived = true;
                self.wake.notify_all();
                while !s.released && s.revision == revision {
                    s = self.wake.wait(s).map_err(|_| "test barrier poisoned")?;
                }
            }
        }
        Ok(bytes)
    }
}
// Committed fetch is independently reader-key signed and must never consult
// validator signing authority. Every method is an explicit tripwire.
struct RejectSigner;
impl BftSignerLeaseV3 for RejectSigner {
    fn checkpoint(&mut self) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        panic!("committed read must not use validator signer")
    }
    fn protected_time_ms(&mut self) -> Result<i64, BftSignerErrorV3> {
        panic!("unexpected validator clock")
    }
    fn start_height_timer(&mut self, _: u64) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        panic!("unexpected validator timer")
    }
    fn advance(
        &mut self,
        _: u64,
        _: &str,
        _: BftSignerCandidateV3,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        panic!("unexpected validator advance")
    }
    fn sign_proposal(
        &mut self,
        _: u64,
        _: &BftProposalIntentV3,
    ) -> Result<(BftPublicProposalV3, BftSignerCheckpointV3), BftSignerErrorV3> {
        panic!("unexpected proposal signing")
    }
    fn sign_vote(
        &mut self,
        _: u64,
        _: &BftVoteIntentV3,
    ) -> Result<(BftVoteV3, BftSignerCheckpointV3), BftSignerErrorV3> {
        panic!("unexpected vote signing")
    }
    fn confirm_applied(
        &mut self,
        _: u64,
        _: &str,
        _: u64,
        _: &str,
        _: &str,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        panic!("unexpected install confirmation")
    }
    fn append_committed_certificate(
        &mut self,
        _: u64,
        _: &str,
        _: FormalBftCertifiedHeaderV3,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        panic!("unexpected certificate append")
    }
}
pub(super) fn signer() -> NodeBftSharedSignerV3 {
    Arc::new(Mutex::new(Box::new(RejectSigner)))
}
pub(super) fn finish(
    fetch: &mut dyn NodeBftPrivateCandidateFetchV3,
) -> Result<oasis7_node::NodeBftPrivateCandidatePackageV3, String> {
    let end = Instant::now() + Duration::from_secs(70);
    loop {
        match fetch.poll() {
            Ok(Some(p)) => return Ok(p),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        assert!(Instant::now() < end, "fetch did not terminate");
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn wire3_formal_read_committed_private_fetch_survives_headadvance_cache_eviction_and_revocation() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| network_case_with_fetch(true))
        .unwrap()
        .join()
        .unwrap();
}
#[expect(
    clippy::too_many_arguments,
    reason = "actual three-Node fixture keeps independent owners explicit"
)]
pub(super) fn exercise(
    nodes: &[NodeRuntime],
    bindings: &[BftSignerBindingV3],
    snapshots: &[Arc<Mutex<BftSignerBackendSnapshotV3>>],
    actual: &[Arc<Mutex<FormalBftHeaderNodeDriverV3>>],
    networks: &[Arc<oasis7_net::Libp2pNetwork>],
    exchanges: &[Arc<NodeBftLibp2pPrivateCandidateExchangeV3>],
    views: &[Arc<NodeBftPeerExecutionStateV3>],
    barriers: &[Arc<BarrierGate>],
    delivery: &Arc<AtomicBool>,
    gates: &crate::node_lifecycle::NetworkGates,
    receipt: &oasis7_node::NodeCommittedActionBatch,
) {
    delivery.store(false, Ordering::Release);
    let certificate = snapshots[1]
        .lock()
        .unwrap()
        .state
        .history
        .iter()
        .find(|c| c.header.block_id().unwrap() == receipt.block_hash)
        .unwrap()
        .clone();
    assert_eq!(
        certificate.execution_commitment.height,
        bindings[0].initial_schedule().height()
    );
    let value = BftHeaderValueV3 {
        header: certificate.header.clone(),
        execution_commitment: certificate.execution_commitment.clone(),
    };
    let expected = actual[1]
        .lock()
        .unwrap()
        .export_bft_v3_private_candidate(&value)
        .unwrap();
    let request = NodeBftPrivateFetchRequestV3 {
        peer_id: networks[1].peer_id().to_string(),
        context: NodeBftExecutionContextV3::from_schedule(bindings[0].initial_schedule()),
        value,
        kind: NodeBftPrivateFetchKindV3::CommittedState,
    };
    barriers[1].arm(&receipt.block_hash);
    let mut operation = exchanges[0].start_fetch(request.clone(), signer()).unwrap();
    barriers[1].wait();
    {
        let s = barriers[1].state.lock().unwrap();
        assert!(
            s.seen.last().unwrap().probe.manifest.total_chunks >= 2,
            "fixture must really cross chunk boundary"
        );
    }
    let generation = views[1].view().unwrap().unwrap().generation;
    delivery.store(true, Ordering::Release);
    network_wait("ordinary committed head advance", || {
        views[1].is_serving()
            && views[1]
                .view()
                .unwrap()
                .is_some_and(|v| v.generation > generation)
    });
    delivery.store(false, Ordering::Release);
    barriers[1].release();
    assert_eq!(finish(operation.as_mut()).unwrap(), expected);
    {
        let s = barriers[1].state.lock().unwrap();
        let chunks = s
            .seen
            .iter()
            .filter(|s| s.probe.chunk_index == Some(0))
            .collect::<Vec<_>>();
        assert!(
            chunks.len() >= 2,
            "original chunk must retry after generation change"
        );
        for c in &chunks[1..] {
            assert_eq!(c.probe.request_digest, chunks[0].probe.request_digest);
            assert_eq!(c.signature, chunks[0].signature);
            assert_eq!(c.probe.manifest, chunks[0].probe.manifest);
        }
    }
    for _ in 0..10 {
        let mut fetch = exchanges[0].start_fetch(request.clone(), signer()).unwrap();
        assert_eq!(finish(fetch.as_mut()).unwrap(), expected);
    }
    // Narrow every chunk-zero release for this original operation until its
    // dedicated five-second deadline expires. Only one callback waits, so
    // retries do not occupy every worker needed for normal peer progress.
    barriers[1].arm(&receipt.block_hash);
    let began = Instant::now();
    let mut expired = exchanges[0]
        .start_fetch_with_timeout_for_testing(request.clone(), signer(), Duration::from_secs(5))
        .unwrap();
    barriers[1].wait();
    assert!(finish(expired.as_mut()).is_err());
    assert!(began.elapsed() < Duration::from_secs(8));
    barriers[1].release();
    {
        let s = barriers[1].state.lock().unwrap();
        assert_eq!(
            s.seen
                .iter()
                .filter(|entry| entry.probe.chunk_index.is_none())
                .count(),
            1,
            "deadline retry must not restart manifest"
        );
        let chunks = s
            .seen
            .iter()
            .filter(|entry| entry.probe.chunk_index == Some(0))
            .collect::<Vec<_>>();
        for chunk in &chunks[1..] {
            assert_eq!(chunk.probe.request_digest, chunks[0].probe.request_digest);
            assert_eq!(chunk.signature, chunks[0].signature);
            assert_eq!(chunk.probe.manifest, chunks[0].probe.manifest);
        }
    }
    barriers[1].arm(&receipt.block_hash);
    let mut revoked = exchanges[0].start_fetch(request, signer()).unwrap();
    barriers[1].wait();
    (gates.revoke_committed)(0);
    barriers[1].release();
    assert_eq!(
        finish(revoked.as_mut()).unwrap_err(),
        "private peer denied original read request"
    );
    assert!(
        nodes
            .iter()
            .all(|node| node.bft_v3_status().unwrap().running)
    );
}
