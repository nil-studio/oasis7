//! Real World/Node/native-provider closure replay. The protected backend/key are
//! explicit memory doubles; these tests do not prove adoption or height 65.
use super::super::super::super::formal_header::ActualBftSuccessorWindowDriverVerifierV1;
use super::super::super::super::formal_header::SuccessorPrivateExportRequestV1;
use super::*;

#[test]
fn wire3_successor_private_closure_replays_actual_node_and_rejects_tampering() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(closure_case)
        .unwrap()
        .join()
        .unwrap();
}
fn closure_case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
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
        successor_archive_transfer: None,
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let (address, server) = tcp(service(&binding, snapshot.clone()));
    let mut node = runtime(root.path(), actual.clone(), &binding, address);
    node.submit_consensus_action_payload(33001, payload("successor-private-original-action"))
        .unwrap();
    node.start().unwrap();
    let batch = wait_batch(&node, 33001);
    node.stop().unwrap();
    drop(node);
    server.join().unwrap().unwrap();
    assert!(
        actual
            .lock()
            .unwrap()
            .world()
            .state()
            .agents
            .contains_key("successor-private-original-action")
    );
    let state = snapshot.lock().unwrap().state.clone();
    assert!(state.pending_driver_apply.is_none());
    let certificate = state.history.last().unwrap();
    let height = certificate.execution_commitment.height;
    assert!(height >= batch.height);
    let id = certificate.header.block_id().unwrap();
    let source = BftSuccessorWindowSourceV1::from_binding(&binding).unwrap();
    let overlapping = LocalBftSuccessorArchiveStoreV1::new(store.clone()).unwrap();
    assert!(
        actual
            .lock()
            .unwrap()
            .export_successor_private_closure(SuccessorPrivateExportRequestV1 {
                archive: &overlapping,
                source: &source,
                public_state: &state,
                previous_usage: BftSuccessorWindowArchiveUsageV1::default(),
                height,
                header_id: &id,
                sequence: 1,
                predecessor: None,
                deadline: Instant::now() + Duration::from_secs(120)
            })
            .is_err(),
        "immutable archive must not share the live private CAS"
    );
    let archive_root = Scratch::new();
    let archive_cas = cas(archive_root.path());
    let archive_path = archive_cas.root().to_path_buf();
    let archive = Arc::new(LocalBftSuccessorArchiveStoreV1::new(archive_cas).unwrap());
    let exported = actual
        .lock()
        .unwrap()
        .export_successor_private_closure(SuccessorPrivateExportRequestV1 {
            archive: &archive,
            source: &source,
            public_state: &state,
            previous_usage: BftSuccessorWindowArchiveUsageV1::default(),
            height,
            header_id: &id,
            sequence: 1,
            predecessor: None,
            deadline: Instant::now() + Duration::from_secs(120),
        })
        .unwrap();
    assert_eq!(
        exported.cut().snapshot_ref.digest,
        certificate.execution_commitment.execution_state_root
    );
    assert_eq!(
        exported.cut().semantic_state_root,
        actual
            .lock()
            .unwrap()
            .world()
            .execution_state_identity()
            .unwrap()
            .state_root_hash
    );
    let mut schedule = binding.initial_schedule().clone();
    for certificate in &state.history {
        schedule = oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3(
            &schedule,
            certificate,
        )
        .unwrap()
        .next_height_schedule()
        .clone();
    }
    let journal_ref = state.journal.as_ref().map(|journal| {
        archive
            .persist_object(
                &oasis7_cas::to_canonical_cbor(journal).unwrap(),
                512 * 1024 * 1024,
            )
            .unwrap()
    });
    let body = BftSuccessorWindowCheckpointBodyV1 {
        schema_version: 1,
        source: source.clone(),
        request_id: "actual-private-closure-1".into(),
        sequence: 1,
        predecessor_checkpoint_digest: source.origin_digest().unwrap(),
        cut: exported.cut().clone(),
        successor_schedule_cbor: oasis7_cas::to_canonical_cbor(&schedule).unwrap(),
        successor_schedule_digest: schedule.digest().unwrap(),
        current_journal_ref: journal_ref,
        current_journal_digest: state.journal.as_ref().map(|j| j.digest().unwrap()),
        public_archive_root: "ab".repeat(32),
        execution_archive_root: exported.manifest().digest().unwrap(),
        execution_manifest_ref: exported.execution_manifest_ref().clone(),
        evidence_refs: vec![exported.metadata_ref().clone()],
        usage: exported.manifest().usage,
    };
    body.validate().unwrap();
    // Drop the actual driver and original CAS before verification. The adapter
    // receives only separately qualified H inputs and the immutable archive.
    drop(actual);
    drop(store);
    let reopened =
        Arc::new(LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new(&archive_path)).unwrap());
    let verifier =
        ActualBftSuccessorWindowDriverVerifierV1::new(h.anchor.clone(), h.target.clone(), reopened)
            .unwrap();
    verifier
        .verify_closure(
            &body,
            &state,
            exported.public_state_ref(),
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
    let mut changed_cut = body.clone();
    changed_cut.cut.driver_install_digest = "bb".repeat(32);
    assert!(
        verifier
            .verify_closure(
                &changed_cut,
                &state,
                exported.public_state_ref(),
                Instant::now() + Duration::from_secs(120)
            )
            .is_err()
    );
    let mut changed_state = state.clone();
    changed_state.history[0].execution_commitment.state_root = "cc".repeat(32);
    assert!(
        verifier
            .verify_closure(
                &body,
                &changed_state,
                exported.public_state_ref(),
                Instant::now() + Duration::from_secs(120)
            )
            .is_err()
    );
    assert!(
        verifier
            .verify_closure(&body, &state, exported.public_state_ref(), Instant::now())
            .is_err()
    );
    // A fresh reread must reject physical corruption even after this verifier
    // already read the same original Snapshot successfully.
    fs::write(
        archive_path
            .join("blobs")
            .join(format!("{}.blob", body.cut.snapshot_ref.digest)),
        b"truncated-original-snapshot",
    )
    .unwrap();
    assert!(
        verifier
            .verify_closure(
                &body,
                &state,
                exported.public_state_ref(),
                Instant::now() + Duration::from_secs(120)
            )
            .is_err()
    );
}
