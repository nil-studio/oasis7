use super::*;
fn snapshot() -> BftSignerBackendSnapshotV3 {
    super::super::super::private_retention_tests::snapshot(10, "0a".repeat(32))
}
#[test]
fn original_view_transfer_and_clock_observations_do_not_reset_nonce_generation() {
    let mut epoch = OriginalInputViewEpochV1::default();
    let mut original = snapshot();
    let (generation, guard) = epoch.observe(&original).unwrap();
    original.successor_archive_transfer = Some(
        super::super::super::archive_transfer::BftSuccessorArchiveTransferLedgerV1 {
            schema_version: 1,
            reserved_response_bytes: 123,
        },
    );
    original.protected_time_floor_ms += 1;
    let (next, _) = epoch.observe(&original).unwrap();
    assert_eq!(generation, next);
    assert!(guard.load(Ordering::Acquire));
    original.state.revision += 1;
    original.state.state_digest = "0b".repeat(32);
    let (advanced, _) = epoch.observe(&original).unwrap();
    assert!(advanced > generation);
    assert!(!guard.load(Ordering::Acquire));
}
#[test]
fn original_view_lease_mutation_invalidates_partial_release_guards() {
    let mut epoch = OriginalInputViewEpochV1::default();
    let mut original = snapshot();
    let (generation, guard) = epoch.observe(&original).unwrap();
    epoch.invalidate().unwrap();
    assert!(!guard.load(Ordering::Acquire));
    original.lease_generation += 1;
    original.lease_token_digest = "0c".repeat(32);
    let (advanced, _) = epoch.observe(&original).unwrap();
    assert!(advanced > generation);
    original.state.revision = u64::MAX;
    assert!(epoch.observe(&original).is_err());
}
#[test]
fn original_response_frame_bound_handles_integer_and_length_edges() {
    for length in [1usize, 23, 24, 255, 256, 65_535, 65_536] {
        let payload = (0..length).map(|n| (n % 256) as u8).collect::<Vec<_>>();
        let response = oasis7_proto::distributed_net::NetworkResponse {
            payload: payload.clone(),
        };
        assert_eq!(
            bft_original_input_network_response_bytes_v1(&payload).unwrap(),
            serde_cbor::to_vec(&response).unwrap().len() as u64
        );
    }
    assert!(bft_original_input_network_response_bytes_v1(&[]).is_err());
    assert!(bft_original_input_network_response_bytes_v1(&vec![0; 65_537]).is_err());
}
