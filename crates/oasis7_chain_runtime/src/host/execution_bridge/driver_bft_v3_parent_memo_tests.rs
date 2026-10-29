//! Black-box warm-parent checks: memory cannot replace durable authority.
use super::*;

fn on_stack(case: impl FnOnce() + Send + 'static) {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(case)
        .unwrap()
        .join()
        .unwrap();
}
fn damaged_warm_parent(kind: &str) {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let schedule = binding.initial_schedule();
    let mut d = actual.lock().unwrap();
    d.configure_bft_v3_execution(&binding).unwrap();
    d.current_bft_v3_execution_context(schedule).unwrap();
    let prefix = root.path().join("ledger/formal-bft-header-prefix-v1");
    let private = prefix.join("node-private-v1");
    match kind {
        "origin" => fs::write(prefix.join("origin.cbor"), b"corrupted origin").unwrap(),
        "private_origin" => fs::remove_file(private.join("origin.cbor")).unwrap(),
        "marker" => fs::remove_file(private.join("ready")).unwrap(),
        "pin" => {
            let scope = "formal-bft-header-prefix-origin-v1";
            let shards = store.list_pin_scope_shards(scope).unwrap();
            assert!(!shards.is_empty());
            store.remove_pin_scope_shard(scope, &shards[0]).unwrap();
        }
        "cas_missing" | "cas_corrupt" => {
            #[derive(serde::Deserialize)]
            struct References {
                references: Vec<String>,
            }
            let data: References =
                serde_cbor::from_slice(&fs::read(private.join("origin.cbor")).unwrap()).unwrap();
            let digest = data
                .references
                .first()
                .expect("qualified H retains index nodes");
            let path = store.blobs_dir().join(format!("{digest}.blob"));
            assert!(path.exists());
            if kind == "cas_missing" {
                fs::remove_file(path).unwrap();
            } else {
                fs::write(path, b"corrupted retained H node").unwrap();
            }
        }
        _ => unreachable!(),
    }
    assert!(
        d.current_bft_v3_execution_context(schedule).is_err(),
        "warm memo must reject {kind}"
    );
}
macro_rules! damage_test {
    ($name:ident, $kind:literal) => {
        #[test]
        fn $name() {
            on_stack(|| damaged_warm_parent($kind));
        }
    };
}
damage_test!(wire3_parent_memo_rechecks_origin, "origin");
damage_test!(wire3_parent_memo_rechecks_private_origin, "private_origin");
damage_test!(wire3_parent_memo_rechecks_ready_marker, "marker");
damage_test!(wire3_parent_memo_rechecks_h_pin, "pin");
damage_test!(wire3_parent_memo_rechecks_missing_h_cas, "cas_missing");
damage_test!(wire3_parent_memo_rechecks_corrupt_h_cas, "cas_corrupt");

#[test]
fn wire3_parent_memo_ignores_unrelated_gc_and_rejects_wrong_context() {
    on_stack(|| {
        let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
        let root = Scratch::new();
        let store = cas(root.path());
        let actual = driver(&h, root.path(), &store);
        let binding = signer_binding(&actual);
        let mut d = actual.lock().unwrap();
        d.configure_bft_v3_execution(&binding).unwrap();
        let expected = d
            .current_bft_v3_execution_context(binding.initial_schedule())
            .unwrap();
        let garbage = store.put_bytes(b"memo unrelated GC canary").unwrap();
        assert!(store.prune_orphan_blobs().unwrap() > 0);
        assert!(store.get_verified(&garbage).is_err());
        assert_eq!(
            d.current_bft_v3_execution_context(binding.initial_schedule())
                .unwrap(),
            expected
        );
        let mut wrong = expected.clone();
        wrong.parent_commit_ref = "a".repeat(64);
        assert!(d.pending_bft_v3_private_candidate(&wrong).is_err());
        assert_eq!(
            d.current_bft_v3_execution_context(binding.initial_schedule())
                .unwrap(),
            expected
        );
    });
}

fn build(
    d: &mut FormalBftHeaderNodeDriverV3,
    context: &NodeBftExecutionContextV3,
    id: u64,
) -> NodeBftPrivateCandidateV3 {
    d.build_bft_v3_candidate(NodeBftBuildContextV3 {
        execution: context.clone(),
        actions: vec![
            oasis7_node::NodeConsensusAction::from_payload(
                id,
                validator_id(0),
                payload(&format!("memo-agent-{id}")),
            )
            .unwrap(),
        ],
    })
    .unwrap()
}
#[test]
fn wire3_parent_memo_alternative_and_failed_preview_preserve_exact_dependency_package() {
    on_stack(|| {
        let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
        let root = Scratch::new();
        let store = cas(root.path());
        let actual = driver(&h, root.path(), &store);
        let binding = signer_binding(&actual);
        let mut d = actual.lock().unwrap();
        d.configure_bft_v3_execution(&binding).unwrap();
        let context = d
            .current_bft_v3_execution_context(binding.initial_schedule())
            .unwrap();
        let legal = build(&mut d, &context, 4101);
        let alternative = build(&mut d, &context, 4102);
        d.preview_bft_v3_candidate(&context, &alternative).unwrap();
        let mut invalid = alternative;
        invalid.actions_cbor.push(0);
        assert!(d.preview_bft_v3_candidate(&context, &invalid).is_err());
        d.retain_bft_v3_private_candidate(&context, &legal).unwrap();
        let observed = d
            .export_bft_v3_private_candidate(&legal.public_value)
            .unwrap();
        let fresh_root = Scratch::new();
        let fresh_store = cas(fresh_root.path());
        let fresh = driver(&h, fresh_root.path(), &fresh_store);
        let fresh_binding = signer_binding(&fresh);
        let mut fresh = fresh.lock().unwrap();
        fresh.configure_bft_v3_execution(&fresh_binding).unwrap();
        let fresh_context = fresh
            .current_bft_v3_execution_context(fresh_binding.initial_schedule())
            .unwrap();
        let canonical = build(&mut fresh, &fresh_context, 4101);
        assert_eq!(legal, canonical);
        fresh
            .retain_bft_v3_private_candidate(&fresh_context, &canonical)
            .unwrap();
        assert_eq!(
            observed,
            fresh
                .export_bft_v3_private_candidate(&canonical.public_value)
                .unwrap()
        );
    });
}

fn commit_one(
    d: &mut FormalBftHeaderNodeDriverV3,
    schedule: &BftHeaderHeightScheduleV3,
    id: u64,
) -> VerifiedBftHeaderCertificateV3 {
    use super::super::super::super::formal_header::FormalBftHeaderCandidateV3;
    use super::super::execution_tests::certify;
    use oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3;
    let context = d.current_bft_v3_execution_context(schedule).unwrap();
    let candidate = build(d, &context, id);
    d.retain_bft_v3_private_candidate(&context, &candidate)
        .unwrap();
    let certificate = certify(
        schedule,
        &FormalBftHeaderCandidateV3 {
            header: candidate.public_value.header,
            commitment: candidate.public_value.execution_commitment,
        },
    );
    let token = verify_commit_certificate_token_v3(schedule, &certificate).unwrap();
    d.commit_bft_v3_certificate(token.clone()).unwrap();
    token
}
fn damaged_successor_parent(kind: &str) {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let mut d = actual.lock().unwrap();
    d.configure_bft_v3_execution(&binding).unwrap();
    let token = commit_one(&mut d, binding.initial_schedule(), 4201);
    let schedule = token.next_height_schedule();
    let warm = d.current_bft_v3_execution_context(schedule).unwrap();
    assert!(warm.height > binding.initial_schedule().height());
    let path = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1")
        .join(format!("record-{:020}.cbor", warm.height - 1));
    #[derive(serde::Deserialize)]
    struct References {
        snapshot_ref: String,
        journal_ref: String,
        business_index_refs: Vec<String>,
    }
    let refs: References = serde_cbor::from_slice(&fs::read(&path).unwrap()).unwrap();
    if kind == "record_missing" {
        fs::remove_file(&path).unwrap();
    } else if kind == "record_corrupt" {
        fs::write(&path, b"corrupt cursor record").unwrap();
    } else {
        let digest = if kind.starts_with("snapshot") {
            &refs.snapshot_ref
        } else if kind.starts_with("journal") {
            &refs.journal_ref
        } else {
            refs.business_index_refs
                .first()
                .expect("actual parent retains index nodes")
        };
        let blob = store.blobs_dir().join(format!("{digest}.blob"));
        assert!(blob.exists());
        if kind.ends_with("missing") {
            fs::remove_file(blob).unwrap();
        } else {
            fs::write(blob, b"corrupt exact parent material").unwrap();
        }
    }
    assert!(
        d.current_bft_v3_execution_context(schedule).is_err(),
        "non-H warm memo must reject {kind}"
    );
}
macro_rules! successor_damage_test {
    ($name:ident, $kind:literal) => {
        #[test]
        fn $name() {
            on_stack(|| damaged_successor_parent($kind));
        }
    };
}
successor_damage_test!(
    wire3_parent_memo_rechecks_missing_cursor_record,
    "record_missing"
);
successor_damage_test!(
    wire3_parent_memo_rechecks_corrupt_cursor_record,
    "record_corrupt"
);
successor_damage_test!(
    wire3_parent_memo_rechecks_missing_parent_snapshot,
    "snapshot_missing"
);
successor_damage_test!(
    wire3_parent_memo_rechecks_corrupt_parent_snapshot,
    "snapshot_corrupt"
);
successor_damage_test!(
    wire3_parent_memo_rechecks_missing_parent_journal,
    "journal_missing"
);
successor_damage_test!(
    wire3_parent_memo_rechecks_corrupt_parent_journal,
    "journal_corrupt"
);
successor_damage_test!(
    wire3_parent_memo_rechecks_missing_parent_business_node,
    "business_missing"
);
successor_damage_test!(
    wire3_parent_memo_rechecks_corrupt_parent_business_node,
    "business_corrupt"
);

#[test]
fn wire3_parent_memo_advances_exact_parent_and_cold_replays_same_world() {
    on_stack(|| {
        let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
        let root = Scratch::new();
        let store = cas(root.path());
        let actual = driver(&h, root.path(), &store);
        let binding = signer_binding(&actual);
        let mut d = actual.lock().unwrap();
        d.configure_bft_v3_execution(&binding).unwrap();
        let initial = d
            .current_bft_v3_execution_context(binding.initial_schedule())
            .unwrap();
        let first = commit_one(&mut d, binding.initial_schedule(), 4301);
        let first_context = d
            .current_bft_v3_execution_context(first.next_height_schedule())
            .unwrap();
        assert_eq!(first_context.height, initial.height + 1);
        assert_ne!(first_context.parent_commit_ref, initial.parent_commit_ref);
        assert!(d.pending_bft_v3_private_candidate(&initial).is_err());
        let second = commit_one(&mut d, first.next_height_schedule(), 4302);
        let expected = d
            .current_bft_v3_execution_context(second.next_height_schedule())
            .unwrap();
        assert_eq!(expected.height, first_context.height + 1);
        assert_ne!(expected.parent_commit_ref, first_context.parent_commit_ref);
        assert_eq!(
            expected,
            NodeBftExecutionContextV3::from_schedule(second.next_height_schedule())
        );
        let identity = d
            .world()
            .execution_state_identity()
            .unwrap()
            .state_root_hash;
        drop(d);
        drop(actual);
        let cold = driver_with_source(&h, root.path(), &store, Arc::new(EmptyIndexReader));
        let mut cold = cold.lock().unwrap();
        cold.configure_bft_v3_execution(&binding).unwrap();
        cold.commit_bft_v3_certificate(first).unwrap();
        cold.commit_bft_v3_certificate(second.clone()).unwrap();
        assert_eq!(
            cold.current_bft_v3_execution_context(second.next_height_schedule())
                .unwrap(),
            expected
        );
        assert_eq!(
            cold.world()
                .execution_state_identity()
                .unwrap()
                .state_root_hash,
            identity
        );
    });
}
