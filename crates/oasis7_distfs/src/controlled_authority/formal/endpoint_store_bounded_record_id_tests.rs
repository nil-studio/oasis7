use super::super::super::tests::with_history_read_fixture;
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(8_000_000, 512 << 20).unwrap()
}
fn with_record(run: impl FnOnce(&Store, &str, &CanonicalRecordV1)) {
    with_history_read_fixture(|store, ids, _| {
        let id = ids.first().expect("genuine Qualified history");
        let node: HistoryNode = store.get(id).unwrap();
        let state: FormalRequestStateV1 = store.get(&node.entry).unwrap();
        let FormalRequestStateV1::Qualified { proof, .. } = &state else {
            panic!("real Qualified record");
        };
        run(store, id, &proof.decision.record);
    });
}
#[test]
fn actual_history_consumer_matches_original_record_id_without_hot_change() {
    with_history_read_fixture(|store, ids, _| {
        assert!(!ids.is_empty());
        let before = store.checkpoint_bytes().unwrap();
        for id in ids {
            let node: HistoryNode = store.get(id).unwrap();
            let state: FormalRequestStateV1 = store.get(&node.entry).unwrap();
            let FormalRequestStateV1::Qualified { proof, .. } = state else {
                panic!("genuine history entry");
            };
            let expected = proof.decision.record.record_id().unwrap();
            let memory = memory();
            let meter = work();
            let paid = store
                .read_qualified_history_record_id_bounded(id, &memory, &meter, &mut || false)
                .unwrap();
            assert_eq!(paid.id(), expected);
            assert_eq!(paid.id().len(), 64);
            assert!(paid.uses_budget(&memory));
            assert_eq!(paid.reserved_bytes(), memory.used());
            drop(paid);
            assert_eq!(memory.used(), 0);
        }
        assert_eq!(store.checkpoint_bytes().unwrap(), before);
    });
}
#[test]
fn primitive_matches_original_domain_frame_and_retains_only_output_fee() {
    with_record(|_, _, record| {
        let encoded = serde_cbor::to_vec(record).unwrap();
        let domain = CANONICAL_RECORD_ID_DOMAIN_V1.as_bytes();
        let mut frame = (domain.len() as u64).to_be_bytes().to_vec();
        frame.extend_from_slice(domain);
        frame.extend_from_slice(&encoded);
        let expected = blake3::hash(&frame).to_hex().to_string();
        assert_eq!(expected, record.record_id().unwrap());
        let memory = memory();
        let meter = work();
        let paid =
            hash_canonical_record_bounded_v1(record, &memory, &meter, &mut || false).unwrap();
        assert_eq!(paid.id(), expected);
        assert_eq!(
            paid.reserved_bytes(),
            size_of::<PaidCanonicalRecordIdV1>() + 64
        );
        assert_eq!(memory.used(), paid.reserved_bytes());
        drop(paid);
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn original_world_schema_roots_digest_reservation_and_handoff_first_errors_match() {
    with_record(|_, _, original| {
        let mut variants = Vec::new();
        let mut r = original.clone();
        r.world.world_id = "  ".into();
        r.schema_version = 0;
        variants.push(r);
        let mut r = original.clone();
        r.schema_version = 0;
        r.parent_record_id = "bad".into();
        variants.push(r);
        let mut r = original.clone();
        r.artifact_roots.clear();
        variants.push(r);
        let mut r = original.clone();
        r.parent_record_id = "UPPER".into();
        variants.push(r);
        let mut r = original.clone();
        r.kind = FormalRecordKindV1::Execution;
        r.frozen_reservation_id = None;
        variants.push(r);
        let mut r = original.clone();
        r.handoff = Some(ProfileHandoffV1 {
            next_authority_digest: "x".into(),
            next_profile: " ".into(),
            next_profile_version: 0,
            valid_from_position: 0,
        });
        variants.push(r);
        for record in variants {
            let old = record.record_id().unwrap_err();
            let memory = memory();
            let new = hash_canonical_record_bounded_v1(&record, &memory, &work(), &mut || false)
                .unwrap_err();
            assert_eq!(new.to_string(), old.to_string());
            assert_eq!(std::mem::discriminant(&new), std::mem::discriminant(&old));
            assert_eq!(memory.used(), 0);
        }
    });
}
#[test]
fn validation_admission_minus_one_zero_and_same_meter_exhaustion_release_fees() {
    with_record(|store, id, record| {
        for cap in [0, validation_layout().unwrap() - 1] {
            let memory = MemoryBudget::new(cap, 0).unwrap();
            assert!(
                hash_canonical_record_bounded_v1(record, &memory, &work(), &mut || false).is_err()
            );
            assert_eq!(memory.used(), 0);
        }
        let memory = memory();
        let meter = work();
        let first =
            hash_canonical_record_bounded_v1(record, &memory, &meter, &mut || false).unwrap();
        drop(first);
        assert_eq!(memory.used(), 0);
        let left = meter.remaining_steps();
        meter.consume_steps(left - 1, &mut || false).unwrap();
        let error =
            hash_canonical_record_bounded_v1(record, &memory, &meter, &mut || false).unwrap_err();
        assert!(error.to_string().contains("bounded archive read steps"));
        assert_eq!(meter.remaining_steps(), 0);
        assert_eq!(memory.used(), 0);
        let zero = MemoryBudget::new(0, 0).unwrap();
        assert!(
            store
                .read_qualified_history_record_id_bounded(id, &zero, &work(), &mut || false)
                .is_err()
        );
        assert_eq!(zero.used(), 0);
    });
}
#[test]
fn continuous_validation_and_encoding_cancellation_release_phases() {
    with_record(|_, _, record| {
        for late in [false, true] {
            let memory = memory();
            let meter = work();
            let mut reached = false;
            let mut calls = 0;
            let error = hash_canonical_record_bounded_v1(record, &memory, &meter, &mut || {
                calls += 1;
                let in_encoding_phase = memory.used() > validation_layout().unwrap()
                    && memory.used() >= size_of::<PaidCanonicalRecordIdV1>() + 64;
                // Observe an admitted phase beyond fixed validation diagnostics.
                // This tests later encoder work, not a full-frame/hash claim.
                let cancel = if late {
                    calls > 30 && in_encoding_phase
                } else {
                    calls == 2
                };
                reached |= cancel;
                cancel
            })
            .unwrap_err();
            assert!(reached);
            assert!(error.to_string().contains("bounded archive read cancelled"));
            assert_eq!(memory.used(), 0);
        }
    });
}
#[test]
fn actual_missing_and_malformed_referenced_history_sources_are_data_errors() {
    with_record(|store, id, _| {
        let memory = memory();
        assert!(
            store
                .read_qualified_history_record_id_bounded(
                    &"ab".repeat(32),
                    &memory,
                    &work(),
                    &mut || false
                )
                .is_err()
        );
        assert_eq!(memory.used(), 0);
        let node: HistoryNode = store.get(id).unwrap();
        let malformed = serde_cbor::to_vec(&serde_cbor::Value::Null).unwrap();
        let bad_entry = store.put(&serde_cbor::Value::Null).unwrap();
        assert_eq!(store.raw_object(&bad_entry).unwrap().unwrap(), malformed);
        let mut changed = node.clone();
        changed.entry = bad_entry;
        let bad_node = store.put(&changed).unwrap();
        assert!(
            store
                .read_qualified_history_record_id_bounded(&bad_node, &memory, &work(), &mut || {
                    false
                })
                .is_err()
        );
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn real_protocol_nonqualified_states_are_rejected_before_record_hashing() {
    super::super::super::tests::with_signed_request_states_fixture(|stage, _, _, original| {
        if stage == "Qualified" {
            return;
        }
        with_record(|store, id, _| {
            // Data-only history wire points at an unchanged genuine protocol
            // state; no Qualified proof or accepted capability is fabricated.
            let entry = store.put(original).unwrap();
            let mut node: HistoryNode = store.get(id).unwrap();
            node.entry = entry;
            let node_id = store.put(&node).unwrap();
            let memory = memory();
            let error = store
                .read_qualified_history_record_id_bounded(&node_id, &memory, &work(), &mut || false)
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("formal record ID requires Qualified history entry")
            );
            assert_eq!(memory.used(), 0);
        });
    });
}

#[test]
fn all_original_schema_domains_and_valid_handoff_use_the_paid_path() {
    use crate::controlled_authority::replicated_protocol::ArtifactRole;
    with_record(|_, _, original| {
        // Data-only variants of a real signed source record. No new proof is
        // signed, installed, or presented as Qualified/IWS execution evidence.
        for schema in 1u32..=8 {
            let mut record = original.clone();
            record.schema_version = schema;
            record.kind = FormalRecordKindV1::Execution;
            record.position = 2;
            record.original_epoch = 1;
            record.frozen_reservation_id = Some("61".repeat(32));
            record.durable_set_transition_digest = (schema % 2 == 0).then(|| "62".repeat(32));
            record.handoff = None;
            record.artifact_roots = if schema >= 7 {
                [
                    (ArtifactRole::Input, record.payload_digest.clone()),
                    (
                        ArtifactRole::IncrementalGenesisConfig,
                        record.world.genesis_digest.clone(),
                    ),
                    (
                        ArtifactRole::IncrementalRootSet,
                        record.after_state_root.clone(),
                    ),
                    (ArtifactRole::IncrementalCommitBody, "63".repeat(32)),
                    (ArtifactRole::IncrementalHistoryPrefix, "64".repeat(32)),
                    (
                        ArtifactRole::ExecutionManifest,
                        record.runtime_manifest_digest.clone(),
                    ),
                    (ArtifactRole::Wasm, "65".repeat(32)),
                    (ArtifactRole::Rules, "66".repeat(32)),
                    (ArtifactRole::NativeExecutionEnvironment, "67".repeat(32)),
                ]
                .into_iter()
                .collect()
            } else {
                let roles = [
                    ArtifactRole::Input,
                    ArtifactRole::Result,
                    ArtifactRole::Snapshot,
                    ArtifactRole::Journal,
                    ArtifactRole::NonceIndex,
                    ArtifactRole::EffectOutbox,
                    ArtifactRole::ExecutionManifest,
                    ArtifactRole::Wasm,
                    ArtifactRole::Rules,
                ];
                let mut roots: std::collections::BTreeMap<_, _> = roles
                    .into_iter()
                    .map(|role| (role, "68".repeat(32)))
                    .collect();
                if schema >= 5 {
                    roots.insert(ArtifactRole::NativeExecutionEnvironment, "69".repeat(32));
                }
                roots
            };
            let mut variants = vec![record.clone()];
            if schema % 2 == 1 {
                record.handoff = Some(ProfileHandoffV1 {
                    next_authority_digest: "70".repeat(32),
                    next_profile: "successor-profile".into(),
                    next_profile_version: 1,
                    valid_from_position: 3,
                });
                variants.push(record);
            }
            for record in variants {
                let expected = record
                    .record_id()
                    .expect("old schema/handoff acceptance oracle");
                let wire = serde_cbor::to_vec(&record).unwrap();
                let domain = CANONICAL_RECORD_ID_DOMAIN_V1.as_bytes();
                let mut frame = (domain.len() as u64).to_be_bytes().to_vec();
                frame.extend_from_slice(domain);
                frame.extend_from_slice(&wire);
                assert_eq!(blake3::hash(&frame).to_hex().as_str(), expected);
                let memory = memory();
                let meter = work();
                let paid =
                    hash_canonical_record_bounded_v1(&record, &memory, &meter, &mut || false)
                        .unwrap();
                assert_eq!(paid.id(), expected, "schema {schema}");
                assert_eq!(memory.used(), paid.reserved_bytes());
                drop(paid);
                assert_eq!(memory.used(), 0);
                if record.handoff.is_some() {
                    for mode in 0..3 {
                        let mut bad = record.clone();
                        let h = bad.handoff.as_mut().unwrap();
                        match mode {
                            0 => h.next_profile = " ".into(),
                            1 => h.next_profile_version = 0,
                            _ => h.valid_from_position = 4,
                        }
                        let old = bad.record_id().unwrap_err();
                        let new =
                            hash_canonical_record_bounded_v1(&bad, &memory, &meter, &mut || false)
                                .unwrap_err();
                        assert_eq!(new.to_string(), old.to_string());
                        assert_eq!(memory.used(), 0);
                    }
                }
            }
        }
    });
}
