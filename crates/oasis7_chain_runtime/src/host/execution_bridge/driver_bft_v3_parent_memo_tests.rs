//! Black-box warm-parent checks: memory cannot replace durable authority.
use super::super::super::super::formal_header::OriginalHeaderIdentityV1;
use super::*;

fn on_stack(case: impl FnOnce() + Send + 'static) {
    case();
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
    let context = d.current_bft_v3_execution_context(schedule).unwrap();
    let candidate = build(d, &context, id);
    d.retain_bft_v3_private_candidate(&context, &candidate)
        .unwrap();
    let token = certify_original_candidate(schedule, candidate);
    d.commit_bft_v3_certificate(token.clone()).unwrap();
    token
}
fn certify_original_candidate(
    schedule: &BftHeaderHeightScheduleV3,
    candidate: NodeBftPrivateCandidateV3,
) -> VerifiedBftHeaderCertificateV3 {
    use super::super::{key_for_id, signed_vote};
    use oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3;
    use oasis7_proto::distributed_bft::v3::{
        BftHeaderCommitCertificateV3, BftProposalHeaderV3, BftVotePhaseV3,
    };
    let value = candidate.public_value;
    let block_id = value.block_id().unwrap();
    let proposer = schedule.expected_proposer_for_round(0).unwrap();
    let proposal = BftProposalHeaderV3::sign(
        schedule.scope().clone(),
        schedule.height(),
        0,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        proposer.clone(),
        block_id.clone(),
        None,
        &key_for_id(&proposer),
    )
    .unwrap();
    let precommits = schedule
        .active_set()
        .validators()
        .values()
        .map(|validator| {
            signed_vote(
                schedule,
                &value.execution_commitment,
                &block_id,
                &validator.validator_id,
                0,
                BftVotePhaseV3::Precommit,
                &key_for_id(&validator.validator_id),
            )
        })
        .collect();
    let certificate = FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal,
            valid_round_evidence: None,
            precommits,
        },
        value.header,
        value.execution_commitment,
    )
    .unwrap();
    verify_commit_certificate_token_v3(schedule, &certificate).unwrap()
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
            #[derive(serde::Deserialize)]
            struct OriginReferences {
                references: Vec<String>,
            }
            let origin: OriginReferences = serde_cbor::from_slice(
                &fs::read(
                    root.path()
                        .join("ledger/formal-bft-header-prefix-v1/node-private-v1/origin.cbor"),
                )
                .unwrap(),
            )
            .unwrap();
            refs.business_index_refs
                .iter()
                .find(|digest| !origin.references.contains(digest))
                .expect("nonempty actual transition derives a post-H business node")
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

#[test]
fn wire3_typed_capacity_preflight_detects_actual_derived_write_boundary_before_retention() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let mut d = actual.lock().unwrap();
    d.configure_bft_v3_execution(&binding).unwrap();
    let mut schedule = binding.initial_schedule().clone();
    let mut saw_roll = false;
    for id in 10_001..10_065 {
        let context = d.current_bft_v3_execution_context(&schedule).unwrap();
        let actions = vec![
            oasis7_node::NodeConsensusAction::from_payload(
                id,
                validator_id(0),
                payload(&format!("memo-agent-{id}")),
            )
            .unwrap(),
        ];
        let before = d.world().execution_state_identity().unwrap();
        let admission = d
            .preflight_bft_v3_capacity(&NodeBftBuildContextV3 {
                execution: context.clone(),
                actions,
            })
            .unwrap();
        assert_eq!(d.world().execution_state_identity().unwrap(), before);
        match admission {
            oasis7_node::NodeBftCapacityAdmissionV3::Fits => {
                let token = commit_one(&mut d, &schedule, id);
                assert!(
                    d.world()
                        .state()
                        .agents
                        .contains_key(&format!("memo-agent-{id}"))
                );
                schedule = token.next_height_schedule().clone();
            }
            oasis7_node::NodeBftCapacityAdmissionV3::NeedsRoll => {
                assert!(
                    d.pending_bft_v3_private_candidate(&context)
                        .unwrap()
                        .is_none()
                );
                saw_roll = true;
                break;
            }
            oasis7_node::NodeBftCapacityAdmissionV3::Blocked => {
                panic!("single ordinary action must fit closed new layer")
            }
        }
    }
    assert!(
        saw_roll,
        "actual derived writes must naturally exhaust a bounded layer"
    );
}

#[test]
fn wire3_capacity_new_layer_refuses_live_recorder_as_parent_authority() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let mut driver = actual.lock().unwrap();
    driver.configure_bft_v3_execution(&binding).unwrap();
    driver
        .current_bft_v3_execution_context(binding.initial_schedule())
        .unwrap();
    assert!(
        driver.reject_live_capacity_parent_for_test().is_err(),
        "valid live recorder nodes cannot mint a closed-parent capability"
    );
}

#[test]
fn wire3_publication_reservation_binding_and_damaged_stage_remain_fail_closed() {
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
    let candidate = build(&mut d, &context, 12_001);
    let blobs = store.root().join("blobs");
    let inventory = || {
        std::fs::read_dir(&blobs)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let before_rejection = inventory();
    assert!(
        d.retain_publication_integrity_fixture_for_test(&context, &candidate, true)
            .is_err()
    );
    assert_eq!(
        inventory(),
        before_rejection,
        "reserve rejection must precede any metadata CAS write"
    );
    assert!(
        d.pending_bft_v3_private_candidate(&context)
            .unwrap()
            .is_none()
    );
    let reference = d
        .retain_publication_integrity_fixture_for_test(&context, &candidate, false)
        .unwrap();
    let metadata = store.root().join("blobs").join(format!("{reference}.blob"));
    let metadata_bytes = fs::read(&metadata).unwrap();
    let metadata_permissions = fs::metadata(&metadata).unwrap().permissions();
    let data = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1/node-private-v1")
        .join(format!("candidate-{:020}.cbor", context.height));
    let data_bytes = fs::read(&data).unwrap();
    let value: serde_cbor::Value = serde_cbor::from_slice(&data_bytes).unwrap();
    let serde_cbor::Value::Map(fields) = value else {
        panic!("candidate Data must be a map")
    };
    assert_eq!(
        fields.get(&serde_cbor::Value::Text(
            "publication_reservation_ref".into()
        )),
        Some(&serde_cbor::Value::Text(reference.clone()))
    );
    drop(d);
    drop(actual);
    let open_identity = || {
        OriginalHeaderIdentityV1::open(
            &root.path().join("ledger"),
            store.clone(),
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            Arc::new(EmptyIndexReader),
        )
    };
    let open_material = || {
        FormalBftHeaderNodeDriverV3::open_from_handoff(
            &root.path().join("ledger"),
            store.clone(),
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            Arc::new(EmptyIndexReader),
        )
    };
    fs::remove_file(&metadata).unwrap();
    let identity =
        open_identity().expect("read-only H identity does not authorize candidate material");
    assert_eq!(identity.material_identity(), binding.material_identity);
    drop(identity);
    assert!(
        open_material().is_err(),
        "missing reserved metadata cannot become legacy None"
    );
    assert!(
        !metadata.exists(),
        "cold open must not recreate missing reservation"
    );
    fs::write(&metadata, b"substituted publication metadata").unwrap();
    assert!(
        open_material().is_err(),
        "metadata substitution must reject even after prior read"
    );
    fs::write(&metadata, &metadata_bytes).unwrap();
    fs::set_permissions(&metadata, metadata_permissions).unwrap();
    drop(
        open_material().expect("restoring exact metadata must restore original material consumer"),
    );
    let reservation = data.with_extension("reservation.cbor");
    fs::rename(&data, &reservation).unwrap();
    let stage = reservation.parent().unwrap().join(format!(
        ".{}.checkpoint-install-1-2-3.tmp",
        reservation.file_name().unwrap().to_str().unwrap()
    ));
    fs::write(&stage, b"damaged original reservation stage").unwrap();
    assert!(
        open_material().is_err(),
        "damaged reservation stage cannot be ignored"
    );
    assert_eq!(
        fs::read(&stage).unwrap(),
        b"damaged original reservation stage"
    );
    fs::remove_file(&stage).unwrap();
    let cold = driver_with_source(&h, root.path(), &store, Arc::new(EmptyIndexReader));
    let mut cold = cold.lock().unwrap();
    cold.configure_bft_v3_execution(&binding).unwrap();
    cold.retain_bft_v3_private_candidate(&context, &candidate)
        .unwrap();
    assert_eq!(
        fs::read(&data).unwrap(),
        data_bytes,
        "same original intent must preserve committed metadata digest"
    );
    assert_eq!(fs::read(&metadata).unwrap(), metadata_bytes);
}

#[test]
fn wire3_publication_actual_qc_and_record_fit_encoded_upper_bound_without_signing_authority() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let mut d = actual.lock().unwrap();
    d.configure_bft_v3_execution(&binding).unwrap();
    let schedule = binding.initial_schedule().clone();
    let context = d.current_bft_v3_execution_context(&schedule).unwrap();
    let candidate = build(&mut d, &context, 13_001);
    let before = d.world().execution_state_identity().unwrap();
    let (record_bound, prefix_bound, total_bound, encoded_qc) =
        d.publication_bounds_for_test(&context, &candidate).unwrap();
    assert_eq!(d.world().execution_state_identity().unwrap(), before);
    assert!(
        oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3(&schedule, &encoded_qc)
            .is_err(),
        "size placeholders cannot become signing/publication authority"
    );
    // A valid-round proposal may itself carry an earlier valid_round. Exercise
    // actual signatures across every relevant CBOR integer-width boundary.
    for round in [2, 23, 24, 255, 256, 4_095, 4_096] {
        let multi_round = signed_multi_round_certificate(&schedule, &candidate.public_value, round);
        oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3(
            &schedule,
            &multi_round,
        )
        .unwrap();
        assert_eq!(
            multi_round
                .certificate
                .valid_round_evidence
                .as_ref()
                .unwrap()
                .proposal
                .valid_round,
            Some(round - 2)
        );
        let prefix = oasis7_formal_protocol::bft::FormalBftCertifiedHeaderPrefixV3 {
            schema_version: 1,
            entries: vec![multi_round],
        };
        assert!(
            prefix.canonical_bytes().unwrap().len() <= prefix_bound,
            "nested valid-round certificate at round {round} exceeds actual prefix bound"
        );
    }
    // Independent codec stress only: every entry has a legal wire schema,
    // but this vector is deliberately NOT a signed/quorate admitted history.
    let mut large = signed_multi_round_certificate(&schedule, &candidate.public_value, 3);
    let mut prevotes = Vec::new();
    let mut precommits = Vec::new();
    for index in 0..256 {
        let id = format!("{index:03}{}", "x".repeat(253));
        let mut prevote = large
            .certificate
            .valid_round_evidence
            .as_ref()
            .unwrap()
            .prevotes[0]
            .clone();
        prevote.validator_id = id.clone();
        prevotes.push(prevote);
        let mut precommit = large.certificate.precommits[0].clone();
        precommit.validator_id = id;
        precommits.push(precommit);
    }
    large
        .certificate
        .valid_round_evidence
        .as_mut()
        .unwrap()
        .prevotes = prevotes;
    large.certificate.precommits = precommits;
    large.validate_binding().unwrap();
    assert!(
        oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3(&schedule, &large)
            .is_err()
    );
    let mut prior = vec![large.clone()];
    loop {
        oasis7_formal_protocol::bft::FormalBftCertifiedHeaderPrefixV3 {
            schema_version: 1,
            entries: prior.clone(),
        }
        .canonical_bytes()
        .expect("prior codec vector remains below the protocol byte bound");
        let planned =
            FormalBftHeaderNodeDriverV3::publication_prefix_codec_bound_for_test(&prior, &large);
        if planned.is_err() {
            assert!(
                prior.len() < 64,
                "failure must be independent byte capacity, not height capacity"
            );
            break;
        }
        prior.push(large.clone());
        assert!(prior.len() < 64);
    }
    let token = commit_one(&mut d, &schedule, 13_001);
    let record = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1")
        .join(format!("record-{:020}.cbor", context.height));
    assert!(fs::read(record).unwrap().len() <= record_bound);
    let prefix = oasis7_formal_protocol::bft::FormalBftCertifiedHeaderPrefixV3 {
        schema_version: 1,
        entries: vec![token.certified_header().clone()],
    };
    assert!(prefix.canonical_bytes().unwrap().len() <= prefix_bound);
    assert!(total_bound >= record_bound * 4 + prefix_bound);
    assert!(d.world().state().agents.contains_key("memo-agent-13001"));
}

fn signed_multi_round_certificate(
    schedule: &BftHeaderHeightScheduleV3,
    value: &oasis7_consensus::tendermint_v3::BftHeaderValueV3,
    round: u64,
) -> FormalBftCertifiedHeaderV3 {
    use super::super::{key_for_id, signed_vote};
    use oasis7_proto::distributed_bft::v3::{
        BftHeaderCommitCertificateV3, BftProposalHeaderV3, BftValidRoundEvidenceV3, BftVotePhaseV3,
    };
    let id = value.block_id().unwrap();
    let proposal = |at, valid| {
        let proposer = schedule.expected_proposer_for_round(at).unwrap();
        BftProposalHeaderV3::sign(
            schedule.scope().clone(),
            schedule.height(),
            at,
            schedule.active_set().set_hash().into(),
            schedule.policy_hash().into(),
            proposer.clone(),
            id.clone(),
            valid,
            &key_for_id(&proposer),
        )
        .unwrap()
    };
    let votes = |at, phase| {
        schedule
            .active_set()
            .validators()
            .values()
            .map(|validator| {
                signed_vote(
                    schedule,
                    &value.execution_commitment,
                    &id,
                    &validator.validator_id,
                    at,
                    phase,
                    &key_for_id(&validator.validator_id),
                )
            })
            .collect()
    };
    FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal: proposal(round, Some(round - 1)),
            valid_round_evidence: Some(BftValidRoundEvidenceV3 {
                proposal: proposal(round - 1, Some(round - 2)),
                prevotes: votes(round - 1, BftVotePhaseV3::Prevote),
            }),
            precommits: votes(round, BftVotePhaseV3::Precommit),
        },
        value.header.clone(),
        value.execution_commitment.clone(),
    )
    .unwrap()
}

#[test]
fn wire3_reserved_record_production_consumer_transfers_once_and_cold_recomputes() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let mut d = actual.lock().unwrap();
    d.configure_bft_v3_execution(&binding).unwrap();
    let schedule = binding.initial_schedule().clone();
    let context = d.current_bft_v3_execution_context(&schedule).unwrap();
    let candidate = build(&mut d, &context, 14_001);
    let receipt = d
        .retain_bft_v3_reserved_candidate(&context, &candidate)
        .unwrap();
    let before = d.publication_usage_for_test().unwrap();
    assert_eq!(before.3, 0);
    assert!(
        before.1 > before.2,
        "future promise must be held before any canonical Record exists"
    );
    let token = certify_original_candidate(&schedule, candidate);
    d.commit_bft_v3_certificate(token.clone()).unwrap();
    let after = d.publication_usage_for_test().unwrap();
    assert_eq!(
        before.0, after.0,
        "actual publication consumes a promise without refund or duplicate charge"
    );
    assert!(after.3 > 0);
    assert!(after.1 < before.1);
    let record_path = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1")
        .join(format!("record-{:020}.cbor", context.height));
    let record_bytes = fs::read(&record_path).unwrap();
    let data_path = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1/node-private-v1")
        .join(format!("candidate-{:020}.cbor", context.height));
    let data_bytes = fs::read(&data_path).unwrap();
    assert_eq!(oasis7_runtime::blake3_hex(&data_bytes), receipt);
    let record: serde_cbor::Value = serde_cbor::from_slice(&record_bytes).unwrap();
    let data: serde_cbor::Value = serde_cbor::from_slice(&data_bytes).unwrap();
    let (serde_cbor::Value::Map(record), serde_cbor::Value::Map(data)) = (record, data) else {
        panic!("Record and Data must be maps")
    };
    let field = serde_cbor::Value::Text("publication_reservation_ref".into());
    assert!(record.contains_key(&field));
    assert_eq!(
        record.get(&field),
        data.get(&field),
        "actual Record must bind the same original promise"
    );
    drop(d);
    drop(actual);
    let cold = driver_with_source(&h, root.path(), &store, Arc::new(EmptyIndexReader));
    let mut cold = cold.lock().unwrap();
    cold.configure_bft_v3_execution(&binding).unwrap();
    assert_eq!(
        cold.publication_usage_for_test().unwrap(),
        after,
        "cold accounting must derive the same actual/residual split"
    );
    cold.commit_bft_v3_certificate(token).unwrap();
    assert_eq!(
        cold.publication_usage_for_test().unwrap(),
        after,
        "same original QC cannot consume the promise twice"
    );
    assert_eq!(fs::read(record_path).unwrap(), record_bytes);
    assert_eq!(fs::read(data_path).unwrap(), data_bytes);
    assert!(cold.world().state().agents.contains_key("memo-agent-14001"));
}

#[test]
fn wire3_reserved_record_foreign_owner_cannot_spend_original_private_promise() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let mut d = actual.lock().unwrap();
    d.configure_bft_v3_execution(&binding).unwrap();
    let schedule = binding.initial_schedule().clone();
    let context = d.current_bft_v3_execution_context(&schedule).unwrap();
    let candidate = build(&mut d, &context, 14_101);
    d.retain_bft_v3_reserved_candidate(&context, &candidate)
        .unwrap();
    let token = certify_original_candidate(&schedule, candidate);
    d.commit_bft_v3_certificate(token).unwrap();
    d.replace_record_with_foreign_publication_for_test(context.height)
        .unwrap();
    drop(d);
    drop(actual);
    let cold = FormalBftHeaderNodeDriverV3::open_from_handoff(
        &root.path().join("ledger"),
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        Arc::new(EmptyIndexReader),
    );
    assert!(
        cold.is_err(),
        "same execution with coherent foreign metadata and pins cannot consume another Data promise"
    );
}

fn reserved_record_crash_recovery(phase: &str) {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let mut d = actual.lock().unwrap();
    d.configure_bft_v3_execution(&binding).unwrap();
    let schedule = binding.initial_schedule().clone();
    let context = d.current_bft_v3_execution_context(&schedule).unwrap();
    let candidate = build(&mut d, &context, 14_201);
    d.retain_bft_v3_reserved_candidate(&context, &candidate)
        .unwrap();
    let before = d.publication_usage_for_test().unwrap();
    let token = certify_original_candidate(&schedule, candidate);
    d.inject_publication_failure(context.height, phase).unwrap();
    assert!(d.commit_bft_v3_certificate(token.clone()).is_err());
    let directory = root.path().join("ledger/formal-bft-header-prefix-v1");
    let fixed = directory.join(format!(
        "{}-{:020}.cbor",
        if phase == "link" {
            "record"
        } else {
            "reservation"
        },
        context.height
    ));
    let fixed: serde_cbor::Value = serde_cbor::from_slice(
        &fs::read(fixed).expect("injection must follow original Some QC WAL publication"),
    )
    .unwrap();
    let serde_cbor::Value::Map(fields) = fixed else {
        panic!("original Record map missing");
    };
    assert!(fields.contains_key(&serde_cbor::Value::Text(
        "publication_reservation_ref".into()
    )));
    drop(d);
    drop(actual);
    let cold = FormalBftHeaderNodeDriverV3::open_from_handoff(
        &root.path().join("ledger"),
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        Arc::new(EmptyIndexReader),
    )
    .unwrap_or_else(|error| panic!("same Some(ref) {phase} cold recovery rejected: {error}"));
    let mut cold = cold;
    cold.configure_bft_v3_execution(&binding).unwrap();
    cold.commit_bft_v3_certificate(token.clone()).unwrap();
    let after = cold.publication_usage_for_test().unwrap();
    assert_eq!(
        after.0, before.0,
        "{phase} cannot duplicate actual plus held charge"
    );
    assert!(after.3 > 0);
    // A live driver already advanced its parent. Historical replay is the
    // configured cold recovery path; relaxing this guard would admit old work
    // against the wrong current execution schedule.
    assert!(cold.commit_bft_v3_certificate(token.clone()).is_err());
    assert_eq!(cold.publication_usage_for_test().unwrap(), after);
    drop(cold);
    let mut retry = FormalBftHeaderNodeDriverV3::open_from_handoff(
        &root.path().join("ledger"),
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        Arc::new(EmptyIndexReader),
    )
    .expect("exact original cold retry must reopen");
    retry.configure_bft_v3_execution(&binding).unwrap();
    retry.commit_bft_v3_certificate(token).unwrap();
    assert_eq!(
        retry.publication_usage_for_test().unwrap(),
        after,
        "same original QC on real cold retry cannot consume another promise"
    );
    assert!(
        retry
            .world()
            .state()
            .agents
            .contains_key("memo-agent-14201")
    );
}
#[test]
fn wire3_reserved_record_reservation_fault_recovers_same_promise() {
    reserved_record_crash_recovery("reservation");
}
#[test]
fn wire3_reserved_record_pins_fault_recovers_same_promise() {
    reserved_record_crash_recovery("pins");
}
#[test]
fn wire3_reserved_record_link_fault_recovers_same_promise() {
    reserved_record_crash_recovery("link");
}
