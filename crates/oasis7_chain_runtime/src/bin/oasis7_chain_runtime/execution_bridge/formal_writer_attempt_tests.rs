//! Publication/unknown/partial-reclaim fault fixtures use modeled endpoints.
use super::*;

#[test]
fn published_attempt_reclamation_retries_independent_readback_and_partial_unlink() {
    let policy = ReleaseSecurityPolicy::default();
    let (world, _, anchor, _) = fixtures::actual_world_initial_fixture_with_execution_policy(
        formal_package::execution_policy_material(&policy)
            .unwrap()
            .2,
    );
    let original = continuation(&world);
    let primary = endpoint(&anchor, EndpointRole::Primary, false);
    let replica = endpoint(&anchor, EndpointRole::Replica, false);
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("oasis7-attempt-reclaim-{}", std::process::id()));
    assert!(!root.exists());
    let open = || {
        FormalWriter::open(
            root.clone(),
            fixtures::key(4),
            policy.clone(),
            world.clone(),
            anchor.clone(),
            primary.clone(),
            replica.clone(),
        )
    };
    let mut writer = open().unwrap();
    writer.prepare(&original).unwrap();
    let proof = writer.resume_original().unwrap();
    let attempt = std::fs::read_dir(root.join("attempts"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    #[derive(Deserialize)]
    struct Summary {
        phase: String,
        duplicate_outputs: Vec<String>,
    }
    let summary = || -> Summary {
        serde_cbor::from_slice(&std::fs::read(attempt.join("attempt-lifecycle-v1.cbor")).unwrap())
            .unwrap()
    };
    let outputs = summary().duplicate_outputs;
    assert!(!outputs.is_empty());
    assert_eq!(summary().phase, "RecoveryPinned");
    assert!(
        root.join("pending.cbor").is_file(),
        "pending acknowledgement remains independent"
    );
    for id in &outputs {
        assert!(
            attempt
                .join("storage/blobs")
                .join(format!("{id}.blob"))
                .is_file()
        );
    }
    // One healthy destination cannot unpin any output while the other is unknown.
    primary.0.lock().unwrap().closure_readback = true;
    assert_eq!(writer.resume_original().unwrap(), proof);
    assert_eq!(summary().phase, "RecoveryPinned");
    // Same signed candidate with altered local proof bytes is not the exact
    // independently stored publication and cannot authorize source deletion.
    replica.0.lock().unwrap().closure_readback = true;
    let publication_path = root
        .join("published")
        .join(format!("{:020}.cbor", proof.decision.record.position));
    let exact: Published = read(&publication_path).unwrap();
    let mut altered = exact.clone();
    altered.proof.decision.primary.signature_hex = "00".repeat(64);
    durable_transaction::write_private_file_durable(&publication_path, &to_cbor(&altered).unwrap())
        .unwrap();
    writer.reclaim_published_attempts(&altered);
    assert_eq!(summary().phase, "RecoveryPinned");
    for id in &outputs {
        assert!(
            attempt
                .join("storage/blobs")
                .join(format!("{id}.blob"))
                .is_file()
        );
    }
    durable_transaction::write_private_file_durable(&publication_path, &to_cbor(&exact).unwrap())
        .unwrap();
    // Retirement's original target is authenticated independently of the
    // writer's newer pending heartbeat. Tampered sidecars cannot move it.
    let target = root.join("retirement-target.cbor");
    let block = pending_test_block_hash(&root.join("pending.cbor")).unwrap();
    let digest =
        super::super::super::formal_attempt_lifecycle::candidate_digest(&exact.candidate).unwrap();
    let save_target = |position, block: &str, digest: &str| {
        durable_transaction::write_private_file_durable(
            &target,
            &to_cbor((1u32, position, block, digest)).unwrap(),
        )
        .unwrap();
    };
    save_target(proof.decision.record.position, &block, &digest);
    assert_eq!(
        retirement_test_target(&root, &target).unwrap(),
        (block.clone(), proof.decision.record.position)
    );
    save_target(proof.decision.record.position, &block, &"00".repeat(32));
    assert!(retirement_test_target(&root, &target).is_err());
    save_target(
        proof.decision.record.position,
        "other-original-block",
        &digest,
    );
    assert!(retirement_test_target(&root, &target).is_err());
    save_target(proof.decision.record.position + 1, &block, &digest);
    assert!(retirement_test_target(&root, &target).is_err());
    drop(writer);
    // Model interruption after an unlink: the durable exact delete list permits
    // retry, and independently stored material must be read again on reopening.
    let missing = attempt
        .join("storage/blobs")
        .join(format!("{}.blob", outputs[0]));
    std::fs::remove_file(missing).unwrap();
    let unique = attempt.join("unique-recovery-evidence");
    std::fs::write(&unique, b"unknown external delivery receipt").unwrap();
    replica.0.lock().unwrap().closure_readback = true;
    let mut reopened = open().unwrap();
    assert_eq!(summary().phase, "Reclaimed");
    for id in &outputs {
        assert!(
            !attempt
                .join("storage/blobs")
                .join(format!("{id}.blob"))
                .exists()
        );
    }
    assert_eq!(
        std::fs::read(&unique).unwrap(),
        b"unknown external delivery receipt"
    );
    assert!(attempt.join("world").is_dir());
    assert!(attempt.join("records").is_dir());
    assert!(root.join("pending.cbor").is_file());
    assert_eq!(
        reopened.resume_original().unwrap(),
        proof,
        "original publication works without attempt output"
    );
    reopened
        .acknowledge(&proof.decision.record.record_id().unwrap())
        .unwrap();
    drop(reopened);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pending_attempt_finishes_while_new_attempt_directories_keep_arriving() {
    let policy = ReleaseSecurityPolicy::default();
    let (world, _, anchor, _) = fixtures::actual_world_initial_fixture_with_execution_policy(
        formal_package::execution_policy_material(&policy)
            .unwrap()
            .2,
    );
    let primary = endpoint(&anchor, EndpointRole::Primary, false);
    let replica = endpoint(&anchor, EndpointRole::Replica, false);
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("oasis7-attempt-scheduler-{}", std::process::id()));
    assert!(!root.exists());
    let mut writer = FormalWriter::open(
        root.clone(),
        fixtures::key(4),
        policy,
        world.clone(),
        anchor,
        primary.clone(),
        replica.clone(),
    )
    .unwrap();
    writer.prepare(&continuation(&world)).unwrap();
    writer.resume_original().unwrap();
    let attempt = std::fs::read_dir(root.join("attempts"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let manifest = std::fs::read(attempt.join("attempt-lifecycle-v1.cbor")).unwrap();
    primary.0.lock().unwrap().closure_readback = true;
    primary.0.lock().unwrap().closure_pending_pages = 6;
    replica.0.lock().unwrap().closure_readback = true;
    // Start genuine discovery with only the original present. This call must
    // consume one modeled P/R page and preserve that job's scheduling priority.
    writer.attempt_scan = None;
    writer.advance_storage_maintenance();
    assert_eq!(primary.0.lock().unwrap().closure_pending_pages, 5);
    for tick in 0..8 {
        // These are independently discovered durable attempt manifests, never
        // deletion authority. Arrival continues faster than one page per tick.
        for arrival in 0..3 {
            let added = root
                .join("attempts")
                .join(format!("arriving-{tick}-{arrival}"));
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&added)
                .unwrap();
            let mut arriving: serde_cbor::Value = serde_cbor::from_slice(&manifest).unwrap();
            let serde_cbor::Value::Map(ref mut fields) = arriving else {
                panic!("attempt map")
            };
            fields.insert(
                serde_cbor::Value::Text("attempt_id".into()),
                serde_cbor::Value::Text(added.file_name().unwrap().to_str().unwrap().into()),
            );
            durable_transaction::write_private_file_durable(
                &added.join("attempt-lifecycle-v1.cbor"),
                &serde_cbor::to_vec(&arriving).unwrap(),
            )
            .unwrap();
        }
        writer.advance_storage_maintenance();
    }
    #[derive(Deserialize)]
    struct Summary {
        phase: String,
    }
    let original: Summary =
        serde_cbor::from_slice(&std::fs::read(attempt.join("attempt-lifecycle-v1.cbor")).unwrap())
            .unwrap();
    assert_eq!(
        original.phase, "Reclaimed",
        "new arrivals cannot dilute a live original job"
    );
    drop(writer);
    std::fs::remove_dir_all(root).unwrap();
}
