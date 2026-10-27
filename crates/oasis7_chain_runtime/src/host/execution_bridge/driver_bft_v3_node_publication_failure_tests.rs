//! Object failure injection at actual canonical pin publication. This is not
//! SIGKILL, physical persistence or a protected production backend claim.
use super::*;

#[test]
fn wire3_formal_read_actual_node_pending_apply_recovers_after_publication_failure() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(failure_case)
        .unwrap()
        .join()
        .unwrap();
}
fn wait_blocked(node: &NodeRuntime) {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let status = node.bft_v3_status().unwrap();
        if !status.running {
            assert!(status.blocked_reason.is_some());
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Node did not fail at publication: {status:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
fn failure_case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let height = binding.initial_schedule().height();
    let old_world = actual.lock().unwrap().world().clone();
    let old_tick = old_world.state().time;
    let old_root = old_world
        .execution_state_identity()
        .unwrap()
        .state_root_hash;
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    actual
        .lock()
        .unwrap()
        .inject_publication_failure(height, "pins")
        .unwrap();
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let (address, server) = tcp(service(&binding, snapshot.clone()));
    let mut node = runtime(root.path(), actual.clone(), &binding, address);
    // Prequeue under the configured actual H parent; no timing assumption can
    // let an empty proposal occupy the deliberately failed first height.
    node.submit_consensus_action_payload(9020, payload("node-wire3-failed-publication-agent"))
        .unwrap();
    node.start().unwrap();
    wait_blocked(&node);
    assert!(
        node.drain_committed_action_batches().is_empty(),
        "no batch before applied confirmation"
    );
    let failed = snapshot.lock().unwrap().clone();
    assert_eq!(failed.state.history.len(), 1);
    assert!(failed.state.applied_installs.is_empty());
    let pending = failed.state.pending_driver_apply.as_ref().unwrap();
    assert_eq!(pending.height, height);
    let original_certificate = failed.state.history[0].clone();
    assert_eq!(
        pending.header_id,
        original_certificate.header.block_id().unwrap()
    );
    let first_id = pending.header_id.clone();
    {
        let d = actual.lock().unwrap();
        assert_eq!(d.world().state().time, old_tick);
        assert_eq!(
            d.world()
                .execution_state_identity()
                .unwrap()
                .state_root_hash,
            old_root
        );
        assert!(
            !d.world()
                .state()
                .agents
                .contains_key("node-wire3-failed-publication-agent")
        );
    }
    node.stop().unwrap();
    drop(node);
    server.join().unwrap().unwrap();
    let directory = root.path().join("ledger/formal-bft-header-prefix-v1");
    let reserved = directory.join(format!("reservation-{height:020}.cbor"));
    let original_bytes = fs::read(&reserved).unwrap();
    let private_path = directory
        .join("node-private-v1")
        .join(format!("candidate-{height:020}.cbor"));
    let private_bytes = fs::read(&private_path).unwrap();
    drop(actual);

    let actual = driver_with_source(&h, root.path(), &store, Arc::new(EmptyIndexReader));
    assert_eq!(
        signer_binding(&actual).digest().unwrap(),
        binding.digest().unwrap()
    );
    let (address, server) = tcp(service(&binding, snapshot.clone()));
    let mut node = runtime(root.path(), actual.clone(), &binding, address);
    node.start().unwrap();
    // Startup itself must complete the original public QC and exact private
    // input, reconcile its receipt, and only then make the worker available.
    {
        let recovered = snapshot.lock().unwrap();
        assert!(recovered.state.pending_driver_apply.is_none());
        assert_eq!(recovered.state.history[0], original_certificate);
        let applied = recovered.state.applied_installs.first().unwrap();
        assert_eq!(applied.height, height);
        assert_eq!(applied.header_id, first_id);
        assert_eq!(applied.driver_install_digest.len(), 64);
    }
    {
        let d = actual.lock().unwrap();
        assert!(
            d.world()
                .state()
                .agents
                .contains_key("node-wire3-failed-publication-agent")
        );
        assert!(d.world().state().time >= height);
        assert_ne!(
            d.world()
                .execution_state_identity()
                .unwrap()
                .state_root_hash,
            old_root
        );
    }
    assert_eq!(
        fs::read(directory.join(format!("record-{height:020}.cbor"))).unwrap(),
        original_bytes
    );
    assert_eq!(fs::read(&private_path).unwrap(), private_bytes);
    node.submit_consensus_action_payload(9021, payload("node-wire3-after-recovery-agent"))
        .unwrap();
    let second = wait_batch(&node, 9021);
    assert!(second.height > height);
    node.stop().unwrap();
    drop(node);
    server.join().unwrap().unwrap();
    let recovered = snapshot.lock().unwrap();
    assert!(recovered.state.pending_driver_apply.is_none());
    assert_eq!(recovered.state.history[0], original_certificate);
    assert_eq!(
        recovered.state.applied_installs.len(),
        recovered.state.history.len()
    );
    assert!(
        actual
            .lock()
            .unwrap()
            .world()
            .state()
            .agents
            .contains_key("node-wire3-after-recovery-agent")
    );
    assert_eq!(fs::read(private_path).unwrap(), private_bytes);
}
