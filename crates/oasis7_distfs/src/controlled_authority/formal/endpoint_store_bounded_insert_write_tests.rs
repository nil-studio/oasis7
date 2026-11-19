use super::super::super::tests::{InsertPlanFixtureV1, with_index_insert_plan_fixture};
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(512 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(50_000_000, 2 << 30).unwrap()
}
fn find(store: &Store, root: &str, request: &str) -> String {
    let mut id = root.to_owned();
    let mut depth = 0;
    loop {
        assert!(depth <= 256);
        depth += 1;
        match store.get::<Index>(&id).unwrap() {
            Index::Leaf {
                request: key,
                entry,
            } => {
                assert_eq!(key, request);
                return entry;
            }
            Index::Fork { bit, left, right } => {
                id = if Store::bit(request, bit).unwrap() {
                    right
                } else {
                    left
                }
            }
        }
    }
}
fn run(store: &Store, request: &str, entry: &str, expected: &str) -> u64 {
    let memory = memory();
    let meter = work();
    let before = store.body.index.clone();
    let checkpoint = store.checkpoint_bytes().unwrap();
    let directory = store.path.join("formal-objects-v1");
    let old_count = physical::roots::count(&directory).unwrap();
    let plan = store
        .prepare_index_insert_bounded(request, entry, &memory, &meter, &mut || false)
        .unwrap();
    let owner = store
        .write_index_insert_plan_bounded(plan, &memory, &meter, &mut || false)
        .unwrap();
    if !expected.is_empty() {
        assert_eq!(owner.id(), expected);
    }
    assert_eq!(owner.before_index(), before.as_deref());
    assert!(owner.uses_budget(&memory));
    assert_eq!(owner.reserved_bytes(), memory.used());
    assert_eq!(find(store, owner.id(), request), entry);
    assert!(physical::roots::count(&directory).unwrap() > old_count);
    assert_eq!(store.body.index, before);
    assert_eq!(store.checkpoint_bytes().unwrap(), checkpoint);
    assert!(physical::pin(&directory, true).is_err());
    let rewritten = owner.id().to_owned();
    let written = physical::roots::count(&directory).unwrap() - old_count;
    drop(owner);
    assert_eq!(memory.used(), 0);
    drop(physical::pin(&directory, true).unwrap());
    // The NEW writer has completed and its output was read/retained before
    // the independent legacy writer is permitted to prepopulate any blobs.
    let old = store
        .insert(store.body.index.as_deref(), request, entry)
        .unwrap();
    assert_eq!(rewritten, old);
    written
}
#[test]
fn fresh_leaf_replace_and_multilevel_rewrite_match_independent_original_insert() {
    let zero = "00".repeat(32);
    let forty = "40".repeat(32);
    let eighty = "80".repeat(32);
    let entry = "09".repeat(32);
    let next = "0a".repeat(32);
    for rows in [
        vec![],
        vec![(zero.as_str(), entry.as_str())],
        vec![
            (zero.as_str(), entry.as_str()),
            (forty.as_str(), entry.as_str()),
        ],
    ] {
        for request in [&zero, &forty, &eighty] {
            let expected = with_index_insert_plan_fixture(
                InsertPlanFixtureV1::Canonical(&rows),
                |store, _| {
                    store
                        .insert(store.body.index.as_deref(), request, &next)
                        .unwrap()
                },
            );
            with_index_insert_plan_fixture(InsertPlanFixtureV1::Canonical(&rows), |store, _| {
                run(store, request, &next, &expected)
            });
        }
    }
}
#[test]
fn actual_long_sibling_is_copied_paid_and_preserved() {
    let request = "00".repeat(32);
    let entry = "0a".repeat(32);
    let expected =
        with_index_insert_plan_fixture(InsertPlanFixtureV1::LongUnchosenSibling, |store, _| {
            store
                .insert(store.body.index.as_deref(), &request, &entry)
                .unwrap()
        });
    with_index_insert_plan_fixture(InsertPlanFixtureV1::LongUnchosenSibling, |store, _| {
        run(store, &request, &entry, &expected);
        let Index::Fork { right, .. } = store.get::<Index>(&expected).unwrap() else {
            panic!("expected fork")
        };
        assert_eq!(right, "unselected/".repeat(512));
    });
}
#[test]
fn maximum_256_ancestor_writer_reuses_single_fixture_and_retains_every_rewrite() {
    // Deliberately one expensive 257-old-insert fixture: this exercises the NEW
    // writer's maximum path, not another pure Plan regression.
    with_index_insert_plan_fixture(InsertPlanFixtureV1::FullDepth256, |store, _| {
        let request = "00".repeat(32);
        let entry = "0a".repeat(32);
        assert_eq!(run(store, &request, &entry, ""), 257);
    });
}
#[test]
fn wrong_work_pool_and_store_cannot_consume_a_true_plan() {
    let request = "00".repeat(32);
    let entry = "09".repeat(32);
    with_index_insert_plan_fixture(InsertPlanFixtureV1::Canonical(&[]), |store, _| {
        for wrong_pool in [false, true] {
            let foreign_memory = memory();
            let memory = memory();
            let meter = work();
            let foreign_work = work();
            let plan = store
                .prepare_index_insert_bounded(&request, &entry, &memory, &meter, &mut || false)
                .unwrap();
            let error = store
                .write_index_insert_plan_bounded(
                    plan,
                    if wrong_pool { &foreign_memory } else { &memory },
                    if wrong_pool { &meter } else { &foreign_work },
                    &mut || false,
                )
                .unwrap_err();
            assert!(error.to_string().contains(if wrong_pool {
                "foreign pool"
            } else {
                "foreign Work"
            }));
            assert_eq!(memory.used(), 0);
            assert_eq!(foreign_memory.used(), 0);
        }
        with_index_insert_plan_fixture(InsertPlanFixtureV1::Canonical(&[]), |foreign, _| {
            let memory = memory();
            let meter = work();
            let plan = store
                .prepare_index_insert_bounded(&request, &entry, &memory, &meter, &mut || false)
                .unwrap();
            assert!(
                foreign
                    .write_index_insert_plan_bounded(plan, &memory, &meter, &mut || false)
                    .unwrap_err()
                    .to_string()
                    .contains("foreign Store")
            );
            assert_eq!(memory.used(), 0);
        });
    });
}

#[test]
fn malformed_existing_output_is_not_overwritten_and_failure_releases_session() {
    let request = "00".repeat(32);
    let entry = "0a".repeat(32);
    with_index_insert_plan_fixture(InsertPlanFixtureV1::Canonical(&[]), |store, _| {
        let memory = memory();
        let meter = work();
        let checkpoint = store.checkpoint_bytes().unwrap();
        let plan = store
            .prepare_index_insert_bounded(&request, &entry, &memory, &meter, &mut || false)
            .unwrap();
        let wire = serde_cbor::to_vec(&Index::Leaf {
            request: request.clone(),
            entry: entry.clone(),
        })
        .unwrap();
        let id = blake3::hash(&wire).to_hex().to_string();
        let root = store.path.join("formal-objects-v1");
        let target = root.join(id);
        fs::write(&target, b"damaged existing output").unwrap();
        assert!(
            store
                .write_index_insert_plan_bounded(plan, &memory, &meter, &mut || false)
                .is_err()
        );
        assert_eq!(fs::read(target).unwrap(), b"damaged existing output");
        assert_eq!(memory.used(), 0);
        assert_eq!(store.checkpoint_bytes().unwrap(), checkpoint);
        assert!(store.body.index.is_none());
        drop(physical::pin(&root, true).unwrap());
    });
}
#[test]
fn exhausted_continuous_work_and_one_byte_memory_remainder_fail_without_owner() {
    let request = "00".repeat(32);
    let entry = "0a".repeat(32);
    for mode in 0..4 {
        with_index_insert_plan_fixture(InsertPlanFixtureV1::Canonical(&[]), |store, _| {
            let memory = memory();
            let meter = work();
            let plan = store
                .prepare_index_insert_bounded(&request, &entry, &memory, &meter, &mut || false)
                .unwrap();
            let pressure = if mode == 0 {
                Some(
                    memory
                        .reserve((512 << 20) - memory.used() - 1, MemoryClass::Ordinary)
                        .unwrap(),
                )
            } else {
                None
            };
            if mode == 1 {
                meter
                    .consume_steps(meter.remaining_steps(), &mut || false)
                    .unwrap();
            }
            if mode == 2 {
                meter
                    .consume_read_bytes(meter.remaining_read_bytes(), &mut || false)
                    .unwrap();
            }
            let result =
                store.write_index_insert_plan_bounded(plan, &memory, &meter, &mut || mode == 3);
            assert!(result.is_err());
            drop(result);
            assert_eq!(memory.used(), pressure.as_ref().map_or(0, |p| p.bytes()));
            drop(pressure);
            assert_eq!(memory.used(), 0);
            assert!(store.body.index.is_none());
        });
    }
}
#[test]
fn cancellation_between_retained_nodes_leaves_data_but_never_publishes_root() {
    let request = "00".repeat(32);
    let other = "40".repeat(32);
    let old_entry = "09".repeat(32);
    let entry = "0a".repeat(32);
    with_index_insert_plan_fixture(
        InsertPlanFixtureV1::Canonical(&[(&request, &old_entry), (&other, &old_entry)]),
        |store, _| {
            let memory = memory();
            let meter = work();
            let root = store.path.join("formal-objects-v1");
            let before = store.body.index.clone();
            let checkpoint = store.checkpoint_bytes().unwrap();
            let count = physical::roots::count(&root).unwrap();
            let plan = store
                .prepare_index_insert_bounded(&request, &entry, &memory, &meter, &mut || false)
                .unwrap();
            let result = store.write_index_insert_plan_bounded(plan, &memory, &meter, &mut || {
                physical::roots::count(&root).unwrap() > count
            });
            assert!(result.is_err());
            drop(result);
            assert_eq!(memory.used(), 0);
            assert_eq!(physical::roots::count(&root).unwrap(), count + 1);
            assert_eq!(store.body.index, before);
            assert_eq!(store.checkpoint_bytes().unwrap(), checkpoint);
            drop(physical::pin(&root, true).unwrap());
            let plan = store
                .prepare_index_insert_bounded(&request, &entry, &memory, &meter, &mut || false)
                .unwrap();
            let owner = store
                .write_index_insert_plan_bounded(plan, &memory, &meter, &mut || false)
                .unwrap();
            assert_eq!(find(store, owner.id(), &request), entry);
            drop(owner);
            assert_eq!(memory.used(), 0);
        },
    );
}

#[test]
fn real_pack_physical_and_generation_existing_nodes_are_reused_under_session() {
    let request = "00".repeat(32);
    let entry = "0a".repeat(32);
    let expected =
        with_index_insert_plan_fixture(InsertPlanFixtureV1::Canonical(&[]), |store, _| {
            store.insert(None, &request, &entry).unwrap()
        });
    for mode in 0..3 {
        with_index_insert_plan_fixture(InsertPlanFixtureV1::Canonical(&[]), |store, _| {
            let root = store.path.join("formal-objects-v1");
            let bytes = serde_cbor::to_vec(&Index::Leaf {
                request: request.clone(),
                entry: entry.clone(),
            })
            .unwrap();
            let id = blake3::hash(&bytes).to_hex().to_string();
            if mode == 0 {
                crate::controlled_authority::formal::business_pack::persist(&root,&[
                    crate::controlled_authority::formal::business_index::BusinessIndexNodeWriteV1{digest:id.clone(),bytes:bytes.clone()}
                ]).unwrap();
                assert!(
                    fs::read(root.join(&id))
                        .unwrap()
                        .starts_with(b"oasis7.formal.business-cas.pack.v1\0")
                );
            } else {
                let padding = vec![92; 128 << 10];
                let mut objects = vec![
                    (id.clone(), bytes),
                    (blake3::hash(&padding).to_hex().to_string(), padding),
                ];
                objects.sort_by(|a, b| a.0.cmp(&b.0));
                assert!(physical::persist(&root, &objects).unwrap());
                assert!(root.join(".representation-v1").join(&id).is_file());
                if mode == 2 {
                    physical::roots::retain(&root, std::slice::from_ref(&id)).unwrap();
                    let mut session = physical::GenerationSession::default();
                    let mut selected = None;
                    for _ in 0..100 {
                        let progress = physical::advance_with_session(&root, &mut session).unwrap();
                        if !progress.building {
                            selected = progress.selected;
                            break;
                        }
                    }
                    assert!(selected.is_some());
                }
            }
            run(store, &request, &entry, &expected);
        });
    }
}

#[test]
fn final_retention_commit_cancellation_returns_no_root_owner() {
    let request = "00".repeat(32);
    let entry = "0a".repeat(32);
    with_index_insert_plan_fixture(InsertPlanFixtureV1::Canonical(&[]), |store, _| {
        let memory = memory();
        let meter = work();
        let root = store.path.join("formal-objects-v1");
        let before = physical::roots::count(&root).unwrap();
        let plan = store
            .prepare_index_insert_bounded(&request, &entry, &memory, &meter, &mut || false)
            .unwrap();
        let error = store
            .write_index_insert_plan_bounded(plan, &memory, &meter, &mut || {
                physical::roots::count(&root).unwrap() > before
            })
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert_eq!(memory.used(), 0);
        assert_eq!(physical::roots::count(&root).unwrap(), before + 1);
        assert!(store.body.index.is_none());
        drop(physical::pin(&root, true).unwrap());
    });
}
#[test]
fn exact_typed_fork_copy_fee_minus_one_is_admitted_before_allocating_strings() {
    let left = "legitimate-long-sibling/".repeat(200);
    let right = "0a".repeat(32);
    let bytes = left.len() + right.len() + std::mem::size_of::<Index>();
    let memory = MemoryBudget::new(bytes - 1, 0).unwrap();
    let meter = work();
    let error = match paid_node(false, 1, &left, &right, &memory, &meter, &mut || false) {
        Ok(_) => panic!("one-byte-short fork admitted"),
        Err(e) => e,
    };
    assert!(error.to_string().contains("node admission"));
    assert_eq!(memory.used(), 0);
}
