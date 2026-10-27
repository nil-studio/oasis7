//! Real prequeued count-boundary input is split into protocol-valid batches.
use super::*;
#[test]
fn wire3_formal_read_actual_node_splits_1025_prequeued_actions_without_loss() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(batch_case)
        .unwrap()
        .join()
        .unwrap();
}
fn batch_case() {
    const AGENT: &str = "wire3-batch-fixed-agent";
    let h = qualified_handoff_v3_with_count_and_setup(1, |world| {
        world.submit_action(Action::RegisterAgent {
            agent_id: AGENT.into(),
            pos: oasis7::geometry::GeoPos::new(4, 5, 6),
        });
        world.step().unwrap();
        assert!(world.state().agents.contains_key(AGENT));
    });
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let (address, server) = tcp(service(&binding, snapshot.clone()));
    let mut node = runtime(root.path(), actual.clone(), &binding, address);
    let initial_world = actual.lock().unwrap().world().clone();
    let initial_agent_count = initial_world.state().agents.len();
    let initial_tick = initial_world.state().time;
    let query = oasis7::consensus_action_payload::encode_consensus_action_payload(
        &oasis7::consensus_action_payload::ConsensusActionPayloadEnvelope::from_runtime_action(
            Action::QueryObservation {
                agent_id: AGENT.into(),
            },
        ),
    )
    .unwrap();
    for id in 1..=1025 {
        node.submit_consensus_action_payload(10_000 + id, query.clone())
            .unwrap();
    }
    node.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut seen = std::collections::BTreeSet::new();
    let mut heights = std::collections::BTreeSet::new();
    while seen.len() < 1025 {
        for batch in node.drain_committed_action_batches() {
            if batch.actions.is_empty() {
                continue;
            }
            assert!(
                batch.actions.len()
                    <= oasis7_consensus::formal_bft_payload_v3::BFT_PRIVATE_ACTIONS_MAX_COUNT_V3
            );
            let bytes = serde_cbor::to_vec(&batch.actions).unwrap();
            assert!(bytes.len() <= oasis7_proto::distributed_bft::BFT_MAX_ACTIONS_CBOR_BYTES_V1);
            assert_eq!(
                oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(&bytes).unwrap(),
                batch.actions
            );
            heights.insert(batch.height);
            for action in batch.actions {
                assert!((10001..=11025).contains(&action.action_id));
                assert!(seen.insert(action.action_id), "duplicate action completion");
            }
        }
        let status = node.bft_v3_status().unwrap();
        assert!(status.running, "batch Node stopped: {status:?}");
        assert!(
            Instant::now() < deadline,
            "did not deliver all queued actions: {}",
            seen.len()
        );
        thread::sleep(Duration::from_millis(20));
    }
    node.stop().unwrap();
    drop(node);
    server.join().unwrap().unwrap();
    assert!(heights.len() >= 2);
    assert_eq!(seen.len(), 1025);
    let world = actual.lock().unwrap().world().clone();
    assert!(world.state().agents.contains_key(AGENT));
    assert_eq!(world.state().agents.len(), initial_agent_count);
    assert!(world.state().time > initial_tick);
    let protected = snapshot.lock().unwrap();
    assert!(protected.state.pending_driver_apply.is_none());
    assert_eq!(
        protected.state.applied_installs.len(),
        protected.state.history.len()
    );
}
