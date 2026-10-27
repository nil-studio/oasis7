//! Native authenticated provider regressions using actual Node-produced journals.
use super::*;
use oasis7_consensus::tendermint_v3::{
    BftProposalIntentV3, BftSigningJournalEventV3, BftVoteIntentV3,
};

fn remote(binding: &BftSignerBindingV3, address: SocketAddr) -> RemoteBftSignerProviderV3 {
    RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(10),
        binding.clone(),
        fixtures::key(117),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: hex::encode(fixtures::key(116).verifying_key().to_bytes()),
            binding_digest: binding.digest().unwrap(),
        },
    )
    .unwrap()
}

/// Keeps only the pinned public identity. Any attempt to re-sign a cached
/// decision fails, even though Ed25519 would otherwise reproduce the same bytes.
struct DisabledConsensusKey {
    public_key_hex: String,
    calls: Arc<std::sync::atomic::AtomicUsize>,
}
impl BftSigningKeyHandleV3 for DisabledConsensusKey {
    fn public_key_hex(&self) -> String {
        self.public_key_hex.clone()
    }
    fn sign_preimage(&self, _: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(BftSignerErrorV3(
            "test consensus key signing is disabled during original-signature recovery".into(),
        ))
    }
}

pub(super) fn assert_completed_signature_retries(
    binding: &BftSignerBindingV3,
    snapshot: Arc<Mutex<BftSignerBackendSnapshotV3>>,
) {
    // A new service object replays independently retained state. Its consensus
    // key cannot sign; the separate provider identity still authenticates replies.
    let signing_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let cold_service = Arc::new(
        BftSignerServiceV3::open(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            vec![Box::new(DisabledConsensusKey {
                public_key_hex: binding.provisioned_signing_keys[0].public_key_hex.clone(),
                calls: signing_calls.clone(),
            })],
            Box::new(MemoryTestSignerBackend {
                binding_digest: binding.digest().unwrap(),
                snapshot: snapshot.clone(),
                fail_confirm_after_commit: None,
            }),
        )
        .unwrap(),
    );
    let (address, server) = tcp(cold_service);
    let client = remote(binding, address);
    let mut lease = client.acquire(binding).unwrap();
    let original = lease.checkpoint().unwrap().state;
    assert!(!original.completed_journals.is_empty());
    assert_eq!(original.completed_journals.len(), original.history.len());
    let mut proposal_count = 0;
    let mut vote_count = 0;
    for journal in original.completed_journals.iter().flatten() {
        for event in &journal.events {
            match event {
                BftSigningJournalEventV3::Proposal {
                    proposal,
                    local: true,
                    ..
                } => {
                    let mut header = proposal.header.clone();
                    header.signature_hex.clear();
                    let intent = BftProposalIntentV3 {
                        header,
                        block: proposal.block.clone(),
                        valid_round_evidence: proposal.valid_round_evidence.clone(),
                    };
                    let (retried, after) = lease.sign_proposal(0, &intent).unwrap();
                    assert_eq!(&retried, proposal.as_ref());
                    assert_eq!(after.state, original);
                    let mut conflict = intent;
                    conflict.header.round += 1;
                    assert!(lease.sign_proposal(original.revision, &conflict).is_err());
                    proposal_count += 1;
                }
                BftSigningJournalEventV3::Vote {
                    vote, local: true, ..
                } => {
                    let intent = BftVoteIntentV3 {
                        height: vote.height,
                        round: vote.round,
                        phase: vote.phase,
                        value: vote.value.clone(),
                        validator_id: vote.validator_id.clone(),
                        schedule_policy_hash: vote.schedule_policy_hash.clone(),
                    };
                    let (retried, after) = lease.sign_vote(0, &intent).unwrap();
                    assert_eq!(&retried, vote.as_ref());
                    assert_eq!(after.state, original);
                    let mut conflict = intent;
                    conflict.round += 1;
                    assert!(lease.sign_vote(original.revision, &conflict).is_err());
                    vote_count += 1;
                }
                _ => {}
            }
        }
    }
    assert!(proposal_count > 0);
    assert!(
        vote_count >= 2,
        "prevote as well as QC-contained precommit must survive"
    );
    assert_eq!(lease.checkpoint().unwrap().state, original);
    // The caller cannot rewrite an archived signed slot through Advance.
    let mut altered = original.candidate();
    altered.completed_journals[0]
        .as_mut()
        .unwrap()
        .events
        .clear();
    assert!(
        lease
            .advance(original.revision, &original.state_digest, altered)
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, original);
    drop(lease);
    server.join().unwrap().unwrap();
    assert_eq!(
        signing_calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "original proposal/prevote/precommit recovery must not invoke the consensus key"
    );
}

#[test]
fn wire3_provider_advance_cannot_install_a_caller_selected_timer_origin() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(timer_origin_case)
        .unwrap()
        .join()
        .unwrap();
}
fn timer_origin_case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let (address, server) = tcp(service(&binding, snapshot.clone()));
    let client = remote(&binding, address);
    let mut lease = client.acquire(&binding).unwrap();
    let original = lease.checkpoint().unwrap().state;
    for origin in [0, 100, i64::MAX] {
        let mut altered = original.candidate();
        altered.journal.as_mut().unwrap().height_started_at_ms = Some(origin);
        assert!(
            lease
                .advance(original.revision, &original.state_digest, altered)
                .is_err()
        );
        assert_eq!(lease.checkpoint().unwrap().state, original);
    }
    let started = lease.start_height_timer(original.revision).unwrap();
    let origin = started
        .state
        .journal
        .as_ref()
        .unwrap()
        .height_started_at_ms
        .unwrap();
    assert!(origin >= 100 && origin <= snapshot.lock().unwrap().protected_time_floor_ms);
    let mut altered = started.state.candidate();
    altered.journal.as_mut().unwrap().height_started_at_ms = None;
    assert!(
        lease
            .advance(started.state.revision, &started.state.state_digest, altered)
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, started.state);
    drop(lease);
    server.join().unwrap().unwrap();
}
