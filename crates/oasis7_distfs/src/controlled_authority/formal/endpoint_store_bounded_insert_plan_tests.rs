use super::*;
// This test-only fold computes original Index wire hashes solely as an oracle;
// it writes nothing and creates no retention/publication capability.
fn digest(node: &Index) -> String {
    blake3::hash(&serde_cbor::to_vec(node).unwrap())
        .to_hex()
        .to_string()
}
fn fold(plan: &IndexInsertPlanV1<'_>) -> String {
    let leaf = digest(&Index::Leaf {
        request: (*plan.request).clone(),
        entry: (*plan.entry).clone(),
    });
    let mut current = match &plan.terminal {
        Terminal::Empty | Terminal::Replace => leaf,
        Terminal::Splice { bit, old_subtree } => {
            let mut request = [0; 32];
            hex::decode_to_slice(plan.request.as_str(), &mut request).unwrap();
            let right = request[usize::from(bit / 8)] & (0x80 >> (bit % 8)) != 0;
            let (left, right) = if right {
                ((*old_subtree).to_string(), leaf)
            } else {
                (leaf, (*old_subtree).to_string())
            };
            digest(&Index::Fork {
                bit: *bit,
                left,
                right,
            })
        }
    };
    for frame in plan.ancestors.iter().rev() {
        let Index::Fork { bit, left, right } = &*frame.node else {
            panic!("non-fork ancestor")
        };
        current = digest(&Index::Fork {
            bit: *bit,
            left: if frame.right {
                left.clone()
            } else {
                current.clone()
            },
            right: if frame.right { current } else { right.clone() },
        });
    }
    current
}
fn memory() -> MemoryBudget {
    MemoryBudget::new(512 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(20_000_000, 1 << 30).unwrap()
}

use super::super::super::tests::{InsertPlanFixtureV1, with_index_insert_plan_fixture};
fn compare(store: &Store, request: &str, entry: &str) -> usize {
    let before = store.body.index.clone();
    let checkpoint = store.checkpoint_bytes().unwrap();
    let root = store.path.join("formal-objects-v1");
    let retained = crate::controlled_authority::formal::physical::roots::count(&root).unwrap();
    let memory = memory();
    let meter = work();
    let plan = store
        .prepare_index_insert_bounded(request, entry, &memory, &meter, &mut || false)
        .unwrap();
    assert!(plan.uses_budget(&memory));
    assert_eq!(plan.reserved_bytes(), memory.used());
    assert!(std::ptr::eq(plan._store, store));
    assert!(std::ptr::eq(plan._work, &meter));
    assert_eq!(
        plan.before.as_deref().map(String::as_str),
        before.as_deref()
    );
    assert_eq!(store.body.index, before);
    assert_eq!(store.checkpoint_bytes().unwrap(), checkpoint);
    assert_eq!(
        crate::controlled_authority::formal::physical::roots::count(&root).unwrap(),
        retained
    );
    let predicted = fold(&plan);
    let depth = plan.ancestors.len();
    drop(plan);
    assert_eq!(memory.used(), 0);
    let actual = store
        .insert(store.body.index.as_deref(), request, entry)
        .unwrap();
    assert_eq!(predicted, actual);
    assert_eq!(store.body.index, before);
    depth
}
#[test]
fn original_insert_oracle_for_empty_replace_and_splices() {
    let zero = "00".repeat(32);
    let forty = "40".repeat(32);
    let eighty = "80".repeat(32);
    let one = "01".repeat(32);
    let entry = "09".repeat(32);
    let replacement = "0a".repeat(32);
    for rows in [
        vec![],
        vec![(zero.as_str(), entry.as_str())],
        vec![
            (zero.as_str(), entry.as_str()),
            (forty.as_str(), entry.as_str()),
        ],
    ] {
        with_index_insert_plan_fixture(InsertPlanFixtureV1::Canonical(&rows), |store, _| {
            for request in [&zero, &forty, &eighty, &one] {
                compare(store, request, &replacement);
            }
        });
    }
}
#[test]
fn original_full_256_bit_chain_retains_only_paid_ancestors() {
    with_index_insert_plan_fixture(InsertPlanFixtureV1::FullDepth256, |store, _| {
        assert_eq!(compare(store, &"00".repeat(32), &"0a".repeat(32)), 256);
    });
}
#[test]
fn old_long_payload_and_nonhex_existing_key_semantics_are_preserved() {
    for shape in [
        InsertPlanFixtureV1::LongUnchosenSibling,
        InsertPlanFixtureV1::LongLeafEntry,
        InsertPlanFixtureV1::NonHexExistingKey,
    ] {
        with_index_insert_plan_fixture(shape, |store, _| {
            compare(store, &"00".repeat(32), &"0a".repeat(32));
        });
    }
}
#[test]
fn original_bad_route_and_source_errors_remain_fail_closed() {
    for shape in [
        InsertPlanFixtureV1::RepeatedBit,
        InsertPlanFixtureV1::OutOfRangeBit,
        InsertPlanFixtureV1::OverlongExistingSamePrefix,
        InsertPlanFixtureV1::ChosenMissing,
        InsertPlanFixtureV1::ChosenCorruptWire,
    ] {
        with_index_insert_plan_fixture(shape, |store, _| {
            let request = "00".repeat(32);
            let entry = "0a".repeat(32);
            let memory = memory();
            let meter = work();
            let expected = store
                .insert(store.body.index.as_deref(), &request, &entry)
                .unwrap_err();
            let actual = store
                .prepare_index_insert_bounded(&request, &entry, &memory, &meter, &mut || false)
                .unwrap_err();
            assert_eq!(
                std::mem::discriminant(&actual),
                std::mem::discriminant(&expected)
            );
            assert_eq!(
                actual.to_string().split(" at offset ").next(),
                expected.to_string().split(" at offset ").next()
            );
            assert_eq!(memory.used(), 0);
        });
    }
}
#[test]
fn short_valid_hex_existing_key_is_safe_invalid_instead_of_old_index_panic() {
    with_index_insert_plan_fixture(InsertPlanFixtureV1::ShortHexExistingKey, |store, _| {
        let memory = memory();
        let meter = work();
        let error = store
            .prepare_index_insert_bounded(
                &"00".repeat(32),
                &"0a".repeat(32),
                &memory,
                &meter,
                &mut || false,
            )
            .unwrap_err();
        assert!(
            matches!(&error,FormalError::Invalid(text) if text.contains("shorter than 256 bits"))
        );
        assert_eq!(memory.used(), 0);
        // The original unchecked Store::bit index can panic here. Reproducing
        // that panic is deliberately not treated as a compatibility obligation.
    });
}
#[test]
fn actual_path_slot_fee_minus_one_and_budgets_release_same_pool() {
    let request = "00".repeat(32);
    let entry = "09".repeat(32);
    with_index_insert_plan_fixture(
        InsertPlanFixtureV1::Canonical(&[(&request, &entry)]),
        |store, _| {
            let memory = memory();
            let meter = work();
            let fixed = 3 * (64 + std::mem::size_of::<String>())
                + 256 * std::mem::size_of::<Ancestor>()
                + std::mem::size_of::<Vec<Ancestor>>();
            let pressure = memory
                .reserve((512 << 20) - (fixed - 1), MemoryClass::Ordinary)
                .unwrap();
            let used = memory.used();
            let error = store
                .prepare_index_insert_bounded(&request, &entry, &memory, &meter, &mut || false)
                .unwrap_err();
            assert!(error.to_string().contains("path admission"));
            assert_eq!(memory.used(), used);
            drop(pressure);
            assert_eq!(memory.used(), 0);
            for mode in 0..4 {
                let meter = FormalArchiveReadWorkV1::new(
                    if mode == 0 { 0 } else { 20_000_000 },
                    if mode == 1 { 0 } else { 1 << 30 },
                )
                .unwrap();
                let mut calls = 0;
                assert!(
                    store
                        .prepare_index_insert_bounded(
                            &request,
                            &entry,
                            &memory,
                            &meter,
                            &mut || {
                                calls += 1;
                                mode == 2 || (mode == 3 && calls >= 20)
                            }
                        )
                        .is_err()
                );
                assert_eq!(memory.used(), 0);
            }
        },
    );
}
#[test]
fn safe_hex_pair_scan_validates_entire_tail_and_rejects_out_of_range_bits() {
    let memory = memory();
    let meter = work();
    let request = "00".repeat(32);
    assert_eq!(
        differing(
            &request,
            &(request.clone() + "zz"),
            &memory,
            &meter,
            &mut || false
        )
        .unwrap(),
        Some(0)
    );
    assert_eq!(
        differing(
            &request,
            &(request.clone() + "00"),
            &memory,
            &meter,
            &mut || false
        )
        .unwrap(),
        None
    );
    assert!(differing(&request, "00", &memory, &meter, &mut || false).is_err());
    assert!(key_bit(&request, 256, &meter, &mut || false).is_err());
    assert_eq!(memory.used(), 0);
}

#[test]
fn cancellation_at_second_pass_final_read_boundaries_releases_all_live_owners() {
    let request = "00".repeat(32);
    let entry = "09".repeat(32);
    with_index_insert_plan_fixture(
        InsertPlanFixtureV1::Canonical(&[(&request, &entry)]),
        |store, _| {
            let memory = memory();
            let baseline_meter = work();
            let mut total = 0;
            let plan = store
                .prepare_index_insert_bounded(
                    &request,
                    &entry,
                    &memory,
                    &baseline_meter,
                    &mut || {
                        total += 1;
                        false
                    },
                )
                .unwrap();
            drop(plan);
            assert_eq!(memory.used(), 0);
            assert!(total > 10);
            // For this Replace leaf the only callbacks after its second read are
            // key comparison and the final check. These last read/compare boundary
            // cutoffs therefore exercise live second-pass decoder/source owners.
            for cutoff in total - 6..total {
                let meter = work();
                let mut calls = 0;
                assert!(
                    store
                        .prepare_index_insert_bounded(
                            &request,
                            &entry,
                            &memory,
                            &meter,
                            &mut || {
                                calls += 1;
                                calls >= cutoff
                            }
                        )
                        .is_err()
                );
                assert_eq!(memory.used(), 0);
                assert!(meter.remaining_steps() < 20_000_000);
            }
        },
    );
}
