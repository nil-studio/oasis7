//! Actual Noise fairness: distinct independently authorized reader identities.
use super::*;
use oasis7_node::{
    NodeBftPrivateCandidateExchangeV3, NodeBftPrivateFetchKindV3, NodeBftPrivateFetchRequestV3,
};
use std::sync::atomic::{AtomicBool, Ordering};
#[test]
fn wire3_formal_read_private_nonce_exhaustion_isolated_between_real_noise_readers() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| network_case_options(false, false, true))
        .unwrap()
        .join()
        .unwrap();
}
#[expect(
    clippy::too_many_arguments,
    reason = "independent actual multi-Node owners remain explicit"
)]
pub(super) fn exercise(
    bindings: &[BftSignerBindingV3],
    snapshots: &[Arc<Mutex<BftSignerBackendSnapshotV3>>],
    actual: &[Arc<Mutex<FormalBftHeaderNodeDriverV3>>],
    networks: &[Arc<oasis7_net::Libp2pNetwork>],
    exchanges: &[Arc<NodeBftLibp2pPrivateCandidateExchangeV3>],
    delivery: &Arc<AtomicBool>,
    reader: usize,
    server: usize,
    receipt: &oasis7_node::NodeCommittedActionBatch,
) {
    delivery.store(false, Ordering::Release);
    let other = (0..3).find(|i| *i != reader && *i != server).unwrap();
    assert_ne!(
        bindings[reader].local_validator_id,
        bindings[other].local_validator_id
    );
    let certificate = snapshots[server]
        .lock()
        .unwrap()
        .state
        .history
        .iter()
        .find(|c| c.header.block_id().unwrap() == receipt.block_hash)
        .unwrap()
        .clone();
    let value = BftHeaderValueV3 {
        header: certificate.header,
        execution_commitment: certificate.execution_commitment,
    };
    let expected = actual[server]
        .lock()
        .unwrap()
        .export_bft_v3_private_candidate(&value)
        .unwrap();
    let request = NodeBftPrivateFetchRequestV3 {
        peer_id: networks[server].peer_id().to_string(),
        context: NodeBftExecutionContextV3::from_schedule(bindings[reader].initial_schedule()),
        value,
        kind: NodeBftPrivateFetchKindV3::CommittedState,
    };
    let peer_a = networks[reader].peer_id().to_string();
    let peer_b = networks[other].peer_id().to_string();
    let baseline_a = exchanges[server].nonce_count_for_testing(&peer_a).unwrap();
    assert!(baseline_a <= 2);
    // A pending read during actual startup may already have consumed one nonce.
    // Count the retained server fact rather than guessing/resetting its budget.
    for expected_count in baseline_a + 1..=2 {
        let mut operation = exchanges[reader]
            .start_fetch(request.clone(), fetch_cases::signer())
            .unwrap();
        assert_eq!(fetch_cases::finish(operation.as_mut()).unwrap(), expected);
        assert_eq!(
            exchanges[server].nonce_count_for_testing(&peer_a).unwrap(),
            expected_count
        );
    }
    assert_eq!(
        exchanges[server].nonce_count_for_testing(&peer_a).unwrap(),
        2
    );
    let mut denied = exchanges[reader]
        .start_fetch(request.clone(), fetch_cases::signer())
        .unwrap();
    assert_eq!(
        fetch_cases::finish(denied.as_mut()).unwrap_err(),
        "private peer denied original read request"
    );
    assert_eq!(
        exchanges[server].nonce_count_for_testing(&peer_a).unwrap(),
        2
    );
    let baseline_b = exchanges[server].nonce_count_for_testing(&peer_b).unwrap();
    assert!(
        baseline_b < 2,
        "B must retain its independent configured-peer share"
    );
    let mut independent = exchanges[other]
        .start_fetch(request, fetch_cases::signer())
        .unwrap();
    assert_eq!(fetch_cases::finish(independent.as_mut()).unwrap(), expected);
    assert_eq!(
        exchanges[server].nonce_count_for_testing(&peer_b).unwrap(),
        baseline_b + 1
    );
    assert_eq!(
        exchanges[server].nonce_count_for_testing(&peer_a).unwrap(),
        2
    );
}
