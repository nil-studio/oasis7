//! Replayed local messages traverse real Noise before the worker ignores them.
use super::*;
use oasis7_consensus::tendermint_v3::{BftPublicPeerMessageV3, BftSigningJournalEventV3};
use oasis7_node::NodeBftPeerTransportV3;
use oasis7_proto::distributed_bft::v3::BftVotePhaseV3;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[test]
fn wire3_formal_read_three_noise_nodes_ignore_echoed_local_proposal_and_prevote() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| network_case_mode(false, true))
        .unwrap()
        .join()
        .unwrap();
}
#[expect(
    clippy::too_many_arguments,
    reason = "independent actual network owners are explicit test inputs"
)]
pub(super) fn exercise(
    nodes: &[NodeRuntime],
    snapshots: &[Arc<Mutex<BftSignerBackendSnapshotV3>>],
    public: &[Arc<NodeBftLibp2pPeerTransportV3>],
    networks: &[Arc<oasis7_net::Libp2pNetwork>],
    local: usize,
    delivery: &Arc<AtomicBool>,
    drains: &[Arc<AtomicU64>],
    received: &[Arc<AtomicU64>],
) {
    let mut originals = None;
    network_wait("current local proposal and prevote before QC", || {
        let state = &snapshots[local].lock().unwrap().state;
        let Some(journal) = &state.journal else {
            return false;
        };
        let proposal = journal.events.iter().find_map(|e| match e {
            BftSigningJournalEventV3::Proposal {
                proposal,
                local: true,
                ..
            } => Some(proposal.clone()),
            _ => None,
        });
        let vote = journal.events.iter().find_map(|e| match e {
            BftSigningJournalEventV3::Vote {
                vote, local: true, ..
            } if vote.phase == BftVotePhaseV3::Prevote => Some(vote.clone()),
            _ => None,
        });
        if let (Some(proposal), Some(vote)) = (proposal, vote)
            && journal.pending_proposal_intent.is_none()
            && journal.pending_vote_intent.is_none()
        {
            originals = Some((proposal, vote));
            return true;
        }
        false
    });
    let before_loop = drains[local].load(Ordering::Acquire);
    network_wait("local signing effect loop completed", || {
        drains[local].load(Ordering::Acquire) > before_loop + 1
    });
    let before = snapshots[local].lock().unwrap().state.clone();
    assert!(
        before.history.is_empty(),
        "echo must exercise the current height, not stale committed messages"
    );
    let (proposal, vote) = originals.unwrap();
    let remote = (local + 1) % 3;
    let expected = received[local].load(Ordering::Acquire) + 2;
    public[remote]
        .send_public(
            &networks[local].peer_id().to_string(),
            &BftPublicPeerMessageV3::Proposal(proposal),
        )
        .unwrap();
    public[remote]
        .send_public(
            &networks[local].peer_id().to_string(),
            &BftPublicPeerMessageV3::Vote(vote),
        )
        .unwrap();
    network_wait("two actual Noise echoes drained", || {
        received[local].load(Ordering::Acquire) >= expected
    });
    let drained = drains[local].load(Ordering::Acquire);
    network_wait("worker completed echo processing loop", || {
        assert!(
            nodes[local].bft_v3_status().unwrap().running,
            "echo halted actual worker"
        );
        drains[local].load(Ordering::Acquire) > drained + 1
    });
    let after = snapshots[local].lock().unwrap().state.clone();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.state_digest, before.state_digest);
    assert_eq!(after.journal, before.journal);
    assert!(
        nodes
            .iter()
            .all(|node| node.bft_v3_status().unwrap().running)
    );
    // Release the original bounded queued frames, not fabricated votes. The
    // outer fixture then requires all three actual nonempty committed results.
    delivery.store(true, Ordering::Release);
}
