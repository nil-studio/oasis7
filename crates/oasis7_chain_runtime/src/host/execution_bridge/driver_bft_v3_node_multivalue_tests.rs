//! Actual-World multi-value original retention, independently reopened on disk.
use super::*;
use oasis7_node::NodeBftPrivateCandidatePackageV3;

fn candidate(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    id: u64,
) -> NodeBftPrivateCandidateV3 {
    let mut driver = actual.lock().unwrap();
    let execution = driver
        .current_bft_v3_execution_context(binding.initial_schedule())
        .unwrap();
    driver
        .build_bft_v3_candidate(NodeBftBuildContextV3 {
            execution,
            actions: vec![
                oasis7_node::NodeConsensusAction::from_payload(
                    id,
                    validator_id(0),
                    payload(&format!("multivalue-agent-{id}")),
                )
                .unwrap(),
            ],
        })
        .unwrap()
}
fn package(h: &QualifiedH<BftProfileTargetV2>, id: u64) -> NodeBftPrivateCandidatePackageV3 {
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(h, root.path(), &store);
    let binding = signer_binding(&actual);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let value = candidate(&actual, &binding, id);
    let context = NodeBftExecutionContextV3::from_schedule(binding.initial_schedule());
    let mut d = actual.lock().unwrap();
    d.retain_bft_v3_private_candidate(&context, &value).unwrap();
    d.export_bft_v3_private_candidate(&value.public_value)
        .unwrap()
}
#[test]
fn wire3_node_multivalue_retention_preserves_local_selection_and_cold_originals() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(multivalue_case)
        .unwrap()
        .join()
        .unwrap();
}
fn multivalue_case() {
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
    let context = NodeBftExecutionContextV3::from_schedule(binding.initial_schedule());
    let local = candidate(&actual, &binding, 1);
    let digest = actual
        .lock()
        .unwrap()
        .retain_bft_v3_private_candidate(&context, &local)
        .unwrap();
    let legacy_path = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1/node-private-v1")
        .join(format!("candidate-{:020}.cbor", context.height));
    let legacy_bytes = fs::read(&legacy_path).unwrap();
    let mut originals = Vec::new();
    for id in 2..=16 {
        let package = package(&h, id);
        let receipt = actual
            .lock()
            .unwrap()
            .retain_received_bft_v3_private_candidate(&context, &package)
            .unwrap();
        assert_eq!(
            actual
                .lock()
                .unwrap()
                .retain_received_bft_v3_private_candidate(&context, &package)
                .unwrap(),
            receipt
        );
        assert_eq!(
            actual
                .lock()
                .unwrap()
                .pending_bft_v3_private_candidate(&context)
                .unwrap(),
            Some(local.clone())
        );
        originals.push((package, receipt));
    }
    let excess = package(&h, 17);
    assert!(
        actual
            .lock()
            .unwrap()
            .retain_received_bft_v3_private_candidate(&context, &excess)
            .is_err()
    );
    let first = originals[0].0.clone();
    let mut changed = first.clone();
    changed.candidate.actions_cbor.push(0);
    assert!(
        actual
            .lock()
            .unwrap()
            .retain_received_bft_v3_private_candidate(&context, &changed)
            .is_err()
    );
    let mut changed = first.clone();
    changed.dependency_nodes.pop();
    assert!(
        actual
            .lock()
            .unwrap()
            .retain_received_bft_v3_private_candidate(&context, &changed)
            .is_err()
    );
    let mut wrong_parent = context.clone();
    wrong_parent.parent_commit_ref = "a".repeat(64);
    assert!(
        actual
            .lock()
            .unwrap()
            .retain_received_bft_v3_private_candidate(&wrong_parent, &first)
            .is_err()
    );
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .retain_bft_v3_private_candidate(&context, &local)
            .unwrap(),
        digest
    );
    let garbage = store.put_bytes(b"multivalue-unretained-GC-canary").unwrap();
    assert!(store.prune_orphan_blobs().unwrap() > 0);
    assert!(store.get_verified(&garbage).is_err());
    drop(actual);
    let actual = driver(&h, root.path(), &store);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .pending_bft_v3_private_candidate(&context)
            .unwrap(),
        Some(local.clone())
    );
    assert_eq!(fs::read(&legacy_path).unwrap(), legacy_bytes);
    for (package, receipt) in &originals {
        assert_eq!(
            actual
                .lock()
                .unwrap()
                .load_bft_v3_private_candidate(&package.candidate.public_value)
                .unwrap(),
            package.candidate
        );
        assert_eq!(
            actual
                .lock()
                .unwrap()
                .export_bft_v3_private_candidate(&package.candidate.public_value)
                .unwrap(),
            *package
        );
        assert_eq!(
            actual
                .lock()
                .unwrap()
                .retain_received_bft_v3_private_candidate(&context, package)
                .unwrap(),
            *receipt
        );
    }
    // Loss of one completed header record cannot be healed from CAS or another
    // candidate at that height, even though the original pin receipt survives.
    let missing = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1/node-private-v1")
        .join(format!(
            "candidate-{:020}-{}.cbor",
            context.height,
            first.candidate.public_value.block_id().unwrap()
        ));
    fs::remove_file(&missing).unwrap();
    assert!(
        actual
            .lock()
            .unwrap()
            .load_bft_v3_private_candidate(&first.candidate.public_value)
            .is_err()
    );
    assert!(
        actual
            .lock()
            .unwrap()
            .retain_received_bft_v3_private_candidate(&context, &first)
            .is_err()
    );
    assert!(!missing.exists());
}
#[test]
fn wire3_node_remote_first_local_selection_is_durable_and_keeps_original_digest() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(selection_case)
        .unwrap()
        .join()
        .unwrap();
}
fn selection_case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let first = package(&h, 101);
    let second = package(&h, 102);
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let context = NodeBftExecutionContextV3::from_schedule(binding.initial_schedule());
    let first_digest = actual
        .lock()
        .unwrap()
        .retain_received_bft_v3_private_candidate(&context, &first)
        .unwrap();
    actual
        .lock()
        .unwrap()
        .retain_received_bft_v3_private_candidate(&context, &second)
        .unwrap();
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .pending_bft_v3_private_candidate(&context)
            .unwrap(),
        None
    );
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .retain_bft_v3_private_candidate(&context, &first.candidate)
            .unwrap(),
        first_digest
    );
    assert!(
        actual
            .lock()
            .unwrap()
            .retain_bft_v3_private_candidate(&context, &second.candidate)
            .is_err()
    );
    drop(actual);
    let actual = driver(&h, root.path(), &store);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .pending_bft_v3_private_candidate(&context)
            .unwrap(),
        Some(first.candidate.clone())
    );
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .retain_bft_v3_private_candidate(&context, &first.candidate)
            .unwrap(),
        first_digest
    );
    let selected_path = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1/node-private-v1")
        .join(format!("selection-{:020}.cbor", context.height));
    let selected_bytes = fs::read(&selected_path).unwrap();
    // Simulate interruption after the original selection intent was linked,
    // before the complete name became durable. It must resume that same value.
    let intent = selected_path.with_extension("intent.cbor");
    fs::hard_link(&selected_path, &intent).unwrap();
    fs::remove_file(&selected_path).unwrap();
    drop(actual);
    let actual = driver(&h, root.path(), &store);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .pending_bft_v3_private_candidate(&context)
            .unwrap(),
        Some(first.candidate.clone())
    );
    assert!(
        actual
            .lock()
            .unwrap()
            .retain_bft_v3_private_candidate(&context, &second.candidate)
            .is_err()
    );
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .retain_bft_v3_private_candidate(&context, &first.candidate)
            .unwrap(),
        first_digest
    );
    assert_eq!(fs::read(&selected_path).unwrap(), selected_bytes);
    assert!(!intent.exists());
    fs::remove_file(&selected_path).unwrap();
    assert!(
        actual
            .lock()
            .unwrap()
            .pending_bft_v3_private_candidate(&context)
            .is_err()
    );
    assert!(
        actual
            .lock()
            .unwrap()
            .retain_bft_v3_private_candidate(&context, &first.candidate)
            .is_err()
    );
    assert!(!selected_path.exists());
    assert!(!selected_bytes.is_empty());
}

#[test]
fn wire3_node_selection_pin_loss_is_not_healed_on_cold_reopen() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(selection_pin_loss_case)
        .unwrap()
        .join()
        .unwrap();
}
fn selection_pin_loss_case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let original = package(&h, 301);
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let context = NodeBftExecutionContextV3::from_schedule(binding.initial_schedule());
    let receipt = actual
        .lock()
        .unwrap()
        .retain_received_bft_v3_private_candidate(&context, &original)
        .unwrap();
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .retain_bft_v3_private_candidate(&context, &original.candidate)
            .unwrap(),
        receipt
    );
    let path = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1/node-private-v1")
        .join(format!("selection-{:020}.cbor", context.height));
    let bytes = fs::read(&path).unwrap();
    let scope = "formal-bft-header-node-private-v3";
    let shard = store
        .list_pin_scope_shards(scope)
        .unwrap()
        .into_iter()
        .find(|shard| shard.ends_with(&format!("-selection-{:020}", context.height)))
        .unwrap();
    store.remove_pin_scope_shard(scope, &shard).unwrap();
    assert!(
        actual
            .lock()
            .unwrap()
            .pending_bft_v3_private_candidate(&context)
            .is_err()
    );
    assert!(
        actual
            .lock()
            .unwrap()
            .retain_bft_v3_private_candidate(&context, &original.candidate)
            .is_err()
    );
    drop(actual);
    assert!(
        FormalBftHeaderNodeDriverV3::open_from_handoff(
            &root.path().join("ledger"),
            store.clone(),
            h.anchor,
            h.target,
            h.material,
            h.reader
        )
        .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert!(!store.list_pin_scope_shards(scope).unwrap().contains(&shard));
}

#[test]
fn wire3_candidate_read_keyproof_native_opt_in_is_typed_and_does_not_install_voting() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(read_keyproof_case)
        .unwrap()
        .join()
        .unwrap();
}
fn read_keyproof_case() {
    use oasis7_proto::distributed_bft::v3::{
        BftCandidateReadBodyV3, BftCandidateReadOperationV3, BftVotePhaseV3, BftVoteValueV3,
    };
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let mut binding = signer_binding(&actual);
    let future_key = hex::encode(fixtures::key(121).verifying_key().to_bytes());
    binding
        .provisioned_signing_keys
        .push(BftSignerKeyIdentityV3 {
            validator_id: binding.local_validator_id.clone(),
            public_key_hex: future_key.clone(),
        });
    binding
        .provisioned_signing_keys
        .sort_by(|a, b| a.public_key_hex.cmp(&b.public_key_hex));
    binding.validate().unwrap();
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let candidate = candidate(&actual, &binding, 501);
    let schedule = binding.initial_schedule();
    let mut body = BftCandidateReadBodyV3 {
        schema_version: 3,
        activation_digest: binding.activation_digest.clone(),
        target_digest: binding.handoff().target_digest().into(),
        scope: binding.scope.clone(),
        validator_id: binding.local_validator_id.clone(),
        validator_public_key_hex: future_key.clone(),
        candidate_policy_digest: "21".repeat(32),
        candidate_grant_generation: 1,
        authority_epoch: schedule.authority_epoch(),
        active_validator_set_hash: schedule.active_set().set_hash().into(),
        schedule_policy_hash: schedule.policy_hash().into(),
        candidate_height: schedule.height(),
        parent_height: schedule.parent_tick(),
        parent_commit_ref: schedule.parent_commit_ref().into(),
        parent_state_root: schedule.parent_state_root().into(),
        parent_manifest_hash: schedule.parent_manifest_hash().into(),
        header_id: candidate.public_value.block_id().unwrap(),
        execution_commitment_hash: candidate
            .public_value
            .header
            .execution_commitment_hash
            .clone(),
        requester_peer_id: "QmYwAPJzv5CZsnAzt8auVZRnGiRAkCfE2xJznsUKWK8GWX".into(),
        serving_peer_id: "QmYwAPJzv5CZsnAzt8auVZRnGiRAkCfE2xJznsUKWK8GWX".into(),
        nonce: "22".repeat(32),
        operation: BftCandidateReadOperationV3::Manifest,
    };
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let open = |enabled: bool, policy: BftCandidateReadPolicyV3| {
        let provider_key: Box<dyn BftSigningKeyHandleV3> =
            Box::new(MemoryTestSignerKey(fixtures::key(116)));
        let keys: Vec<Box<dyn BftSigningKeyHandleV3>> = vec![
            Box::new(MemoryTestSignerKey(fixtures::key(120))),
            Box::new(MemoryTestSignerKey(fixtures::key(121))),
        ];
        let backend: Box<dyn BftSignerBackendV3> = Box::new(MemoryTestSignerBackend {
            binding_digest: binding.digest().unwrap(),
            fail_confirm_after_commit: None,
            snapshot: snapshot.clone(),
        });
        Arc::new(
            if enabled {
                BftSignerServiceV3::open_with_candidate_read_proofs(
                    binding.clone(),
                    provider_key,
                    keys,
                    backend,
                    policy,
                )
            } else {
                BftSignerServiceV3::open(binding.clone(), provider_key, keys, backend)
            }
            .unwrap(),
        )
    };
    let connect = |address| {
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
    };
    let (address, server) = tcp(open(false, BftCandidateReadPolicyV3::STANDARD));
    let mut lease = connect(address).acquire(&binding).unwrap();
    let original = lease.checkpoint().unwrap().state;
    assert!(lease.sign_candidate_read_request(&body).is_err());
    assert_eq!(lease.checkpoint().unwrap().state, original);
    drop(lease);
    server.join().unwrap().unwrap();
    let (address, server) = tcp(open(
        true,
        BftCandidateReadPolicyV3::bounded(2, 32).unwrap(),
    ));
    let mut lease = connect(address).acquire(&binding).unwrap();
    let original = lease.checkpoint().unwrap().state;
    let original_bytes = serde_cbor::to_vec(&original).unwrap();
    let mut wrong = body.clone();
    wrong.activation_digest = "ff".repeat(32);
    assert!(lease.sign_candidate_read_request(&wrong).is_err());
    let mut wrong = body.clone();
    wrong.target_digest = "ff".repeat(32);
    assert!(lease.sign_candidate_read_request(&wrong).is_err());
    let mut wrong = body.clone();
    wrong.scope.world_id = "other-world".into();
    assert!(lease.sign_candidate_read_request(&wrong).is_err());
    let mut wrong = body.clone();
    wrong.validator_id = "other-validator".into();
    assert!(lease.sign_candidate_read_request(&wrong).is_err());
    let mut wrong = body.clone();
    wrong.validator_public_key_hex = hex::encode(fixtures::key(119).verifying_key().to_bytes());
    assert!(lease.sign_candidate_read_request(&wrong).is_err());
    let mut wrong = body.clone();
    wrong.requester_peer_id = "unverified-self-report".into();
    assert!(lease.sign_candidate_read_request(&wrong).is_err());
    let mut wrong = body.clone();
    wrong.operation = BftCandidateReadOperationV3::Package {
        package_digest: "33".repeat(32),
        total_bytes: u64::MAX,
        chunk_size: 16_384,
        total_chunks: 1,
    };
    assert!(lease.sign_candidate_read_request(&wrong).is_err());
    let proof = lease.sign_candidate_read_request(&body).unwrap();
    proof.verify_for(&body, &future_key).unwrap();
    assert_ne!(
        future_key,
        schedule
            .active_set()
            .validator(&binding.local_validator_id)
            .unwrap()
            .public_key_hex
    );
    assert!(body.operation.chunk_range(0).is_err());
    // Even though this preprovisioned future key can prove possession, no
    // consensus timer, intent, signed vote or membership install was created.
    let intent = oasis7_consensus::tendermint_v3::BftVoteIntentV3 {
        height: schedule.height(),
        round: 0,
        phase: BftVotePhaseV3::Prevote,
        value: BftVoteValueV3::Nil,
        validator_id: binding.local_validator_id.clone(),
        schedule_policy_hash: schedule.policy_hash().into(),
    };
    assert!(lease.sign_vote(original.revision, &intent).is_err());
    body.operation = BftCandidateReadOperationV3::Package {
        package_digest: "33".repeat(32),
        total_bytes: 16_385,
        chunk_size: 16_384,
        total_chunks: 2,
    };
    let package_proof = lease.sign_candidate_read_request(&body).unwrap();
    package_proof.verify_for(&body, &future_key).unwrap();
    assert_eq!(
        package_proof.body.operation.chunk_range(1).unwrap(),
        (16_384, 1)
    );
    assert!(package_proof.body.operation.chunk_range(2).is_err());
    assert!(
        lease.sign_candidate_read_request(&body).is_err(),
        "same-body retry consumes the bounded session signature budget"
    );
    assert_eq!(
        serde_cbor::to_vec(&lease.checkpoint().unwrap().state).unwrap(),
        original_bytes
    );
    assert_eq!(snapshot.lock().unwrap().state, original);
    drop(lease);
    server.join().unwrap().unwrap();
    // The protected-time rate window is separate from the session allowance.
    let (address, server) = tcp(open(
        true,
        BftCandidateReadPolicyV3::bounded(1024, 1).unwrap(),
    ));
    let mut lease = connect(address).acquire(&binding).unwrap();
    lease.sign_candidate_read_request(&body).unwrap();
    assert!(lease.sign_candidate_read_request(&body).is_err());
    assert_eq!(lease.checkpoint().unwrap().state, original);
    drop(lease);
    server.join().unwrap().unwrap();
}
