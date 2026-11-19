use super::super::super::tests::{
    with_signed_request_states_fixture, with_store_for_bounded_index_fixture,
};
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(50_000_000, 1 << 30).unwrap()
}
fn check_index(store: &Store, root: &str, request: &str, entry: &str) {
    let mut cursor = root.to_owned();
    for _ in 0..=256 {
        match store.get::<Index>(&cursor).unwrap() {
            Index::Leaf {
                request: key,
                entry: value,
            } => {
                assert_eq!(key, request);
                assert_eq!(value, entry);
                return;
            }
            Index::Fork { bit, left, right } => {
                cursor = if Store::bit(request, bit).unwrap() {
                    right
                } else {
                    left
                }
            }
        }
    }
    panic!("rewritten Index exceeded bounded depth");
}
#[test]
fn all_five_real_signed_states_write_full_original_content_before_legacy_oracle() {
    let mut stages = Vec::new();
    with_signed_request_states_fixture(|stage, endpoint, request, original| {
        let memory = memory();
        let meter = work();
        let source = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let source_fee = source.reserved_bytes();
        assert_eq!(memory.used(), source_fee);
        with_store_for_bounded_index_fixture(|target, _| {
            let before = target.body.index.clone();
            let checkpoint = target.checkpoint_bytes().unwrap();
            let root = target.path.join("formal-objects-v1");
            let old_count = physical::roots::count(&root).unwrap();
            let output = target
                .prepare_request_update_bounded(&source, &memory, &meter, &mut || false)
                .unwrap();
            assert!(output.uses_budget(&memory));
            assert_eq!(memory.used(), source_fee + output.reserved_bytes());
            assert_eq!(output.request_digest(), request);
            assert_eq!(output.before_index(), before.as_deref());
            let canonical = serde_cbor::to_vec(original).unwrap();
            assert_eq!(
                output.entry_id(),
                blake3::hash(&canonical).to_hex().as_str()
            );
            let actual: FormalRequestStateV1 = target.get(output.entry_id()).unwrap();
            assert_eq!(&actual, original, "{stage}");
            assert_eq!(serde_cbor::to_vec(&actual).unwrap(), canonical);
            check_index(target, output.index_root(), request, output.entry_id());
            assert_eq!(physical::roots::count(&root).unwrap(), old_count + 2);
            let (ids, _, complete) =
                physical::roots::page(&root, &physical::roots::start(), 1024).unwrap();
            assert!(complete);
            assert!(ids.iter().any(|id| id == output.entry_id()));
            assert!(ids.iter().any(|id| id == output.index_root()));
            assert_eq!(target.body.index, before);
            assert_eq!(target.checkpoint_bytes().unwrap(), checkpoint);
            assert!(physical::pin(&root, true).is_err());
            let entry = output.entry_id().to_owned();
            let rewritten = output.index_root().to_owned();
            drop(output);
            assert_eq!(memory.used(), source_fee);
            drop(physical::pin(&root, true).unwrap());
            // Only after the NEW target's complete write and guard release may
            // the old producer populate its own oracle blobs.
            let old_entry = target.put(source.state()).unwrap();
            let old_index = target
                .insert(target.body.index.as_deref(), request, &old_entry)
                .unwrap();
            assert_eq!(entry, old_entry);
            assert_eq!(rewritten, old_index);
        });
        drop(source);
        assert_eq!(memory.used(), 0);
        stages.push(stage.to_owned());
    });
    assert_eq!(
        stages,
        ["Frozen", "Prepared", "Decided", "Ready", "Qualified"]
    );
}
#[test]
fn foreign_pool_and_continuous_target_limits_never_return_an_update() {
    let mut tested = false;
    with_signed_request_states_fixture(|_, endpoint, request, _| {
        if tested {
            return;
        }
        tested = true;
        let source_memory = memory();
        let source_work = work();
        let source = endpoint
            .load_bounded(request, &source_memory, &source_work, &mut || false)
            .unwrap()
            .unwrap();
        let baseline = source_memory.used();
        for mode in 0..5 {
            with_store_for_bounded_index_fixture(|target, _| {
                let meter = FormalArchiveReadWorkV1::new(
                    if mode == 1 { 0 } else { 50_000_000 },
                    if mode == 2 { 0 } else { 1 << 30 },
                )
                .unwrap();
                let foreign = memory();
                let budget = if mode == 0 { &foreign } else { &source_memory };
                let pressure = if mode == 4 {
                    Some(
                        source_memory
                            .reserve(
                                (256 << 20)
                                    - baseline
                                    - (source.state().request_digest().len()
                                        + std::mem::size_of::<String>()
                                        - 1),
                                MemoryClass::Ordinary,
                            )
                            .unwrap(),
                    )
                } else {
                    None
                };
                let error = target
                    .prepare_request_update_bounded(&source, budget, &meter, &mut || mode == 3)
                    .unwrap_err();
                if mode == 0 {
                    assert!(error.to_string().contains("foreign pool"));
                }
                assert_eq!(
                    source_memory.used(),
                    baseline + pressure.as_ref().map_or(0, |p| p.bytes())
                );
                assert_eq!(foreign.used(), 0);
                drop(pressure);
                assert_eq!(source_memory.used(), baseline);
                assert!(target.body.index.is_none());
            });
        }
        drop(source);
        assert_eq!(source_memory.used(), 0);
    });
}
#[test]
fn corrupt_actual_entry_output_is_not_overwritten_or_published() {
    let mut tested = false;
    with_signed_request_states_fixture(|_, endpoint, request, _| {
        if tested {
            return;
        }
        tested = true;
        let memory = memory();
        let meter = work();
        let source = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let baseline = memory.used();
        with_store_for_bounded_index_fixture(|target, _| {
            let wire = serde_cbor::to_vec(source.state()).unwrap();
            let id = blake3::hash(&wire).to_hex().to_string();
            let root = target.path.join("formal-objects-v1");
            let path = root.join(id);
            fs::write(&path, b"corrupt immutable state").unwrap();
            let checkpoint = target.checkpoint_bytes().unwrap();
            assert!(
                target
                    .prepare_request_update_bounded(&source, &memory, &meter, &mut || false)
                    .is_err()
            );
            assert_eq!(fs::read(path).unwrap(), b"corrupt immutable state");
            assert_eq!(memory.used(), baseline);
            assert_eq!(target.checkpoint_bytes().unwrap(), checkpoint);
            assert!(target.body.index.is_none());
            drop(physical::pin(&root, true).unwrap());
        });
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn cancellation_after_entry_or_final_index_retention_leaves_no_success_owner() {
    let mut tested = false;
    with_signed_request_states_fixture(|_, endpoint, request, _| {
        if tested {
            return;
        }
        tested = true;
        let memory = memory();
        let meter = work();
        let source = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let baseline = memory.used();
        for commits in [1, 2] {
            with_store_for_bounded_index_fixture(|target, _| {
                let root = target.path.join("formal-objects-v1");
                let before = physical::roots::count(&root).unwrap();
                let checkpoint = target.checkpoint_bytes().unwrap();
                let result =
                    target.prepare_request_update_bounded(&source, &memory, &meter, &mut || {
                        physical::roots::count(&root).unwrap() >= before + commits
                    });
                assert!(result.is_err());
                drop(result);
                assert_eq!(memory.used(), baseline);
                assert_eq!(physical::roots::count(&root).unwrap(), before + commits);
                assert!(target.body.index.is_none());
                assert_eq!(target.checkpoint_bytes().unwrap(), checkpoint);
                drop(physical::pin(&root, true).unwrap());
                let output = target
                    .prepare_request_update_bounded(&source, &memory, &meter, &mut || false)
                    .unwrap();
                check_index(target, output.index_root(), request, output.entry_id());
                drop(output);
                assert_eq!(memory.used(), baseline);
            });
        }
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn cancellation_after_actual_entry_publish_before_retention_returns_no_update() {
    let mut tested = false;
    with_signed_request_states_fixture(|_, endpoint, request, _| {
        if tested {
            return;
        }
        tested = true;
        let memory = memory();
        let meter = work();
        let source = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let baseline = memory.used();
        with_store_for_bounded_index_fixture(|target, _| {
            let wire = serde_cbor::to_vec(source.state()).unwrap();
            let id = blake3::hash(&wire).to_hex().to_string();
            let root = target.path.join("formal-objects-v1");
            let path = root.join(&id);
            let before = physical::roots::count(&root).unwrap();
            assert!(!path.exists());
            let checkpoint = target.checkpoint_bytes().unwrap();
            let result =
                target.prepare_request_update_bounded(&source, &memory, &meter, &mut || {
                    path.exists()
                });
            assert!(result.is_err());
            drop(result);
            assert_eq!(memory.used(), baseline);
            assert_eq!(fs::read(path).unwrap(), wire);
            assert_eq!(physical::roots::count(&root).unwrap(), before);
            assert!(target.body.index.is_none());
            assert_eq!(target.checkpoint_bytes().unwrap(), checkpoint);
            drop(physical::pin(&root, true).unwrap());
        });
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn nonempty_actual_signed_index_cancellation_between_rewritten_nodes_never_publishes() {
    use super::super::super::tests::{InsertPlanFixtureV1, with_index_insert_plan_fixture};
    let mut tested = false;
    with_signed_request_states_fixture(|_, endpoint, request, _| {
        if tested {
            return;
        }
        tested = true;
        let memory = memory();
        let meter = work();
        let source = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let baseline = memory.used();
        // A genuinely published signed Hot root from the original producer;
        // no mutable body/index setter or caller-root capability is used.
        let a = "00".repeat(32);
        let b = "40".repeat(32);
        let entry = "09".repeat(32);
        with_index_insert_plan_fixture(
            InsertPlanFixtureV1::Canonical(&[(&a, &entry), (&b, &entry)]),
            |target, _| {
                let root = target.path.join("formal-objects-v1");
                let before_count = physical::roots::count(&root).unwrap();
                let before = target.body.index.clone();
                let checkpoint = target.checkpoint_bytes().unwrap();
                // Entry and new Leaf are now durably retained, while the splice or
                // ancestor still remains. Cancellation must return no root owner.
                let result =
                    target.prepare_request_update_bounded(&source, &memory, &meter, &mut || {
                        physical::roots::count(&root).unwrap() >= before_count + 2
                    });
                assert!(result.is_err());
                drop(result);
                assert_eq!(memory.used(), baseline);
                assert_eq!(physical::roots::count(&root).unwrap(), before_count + 2);
                assert_eq!(target.body.index, before);
                assert_eq!(target.checkpoint_bytes().unwrap(), checkpoint);
                drop(physical::pin(&root, true).unwrap());
                let output = target
                    .prepare_request_update_bounded(&source, &memory, &meter, &mut || false)
                    .unwrap();
                check_index(target, output.index_root(), request, output.entry_id());
                drop(output);
                assert_eq!(memory.used(), baseline);
            },
        );
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn real_pack_physical_generation_entry_representations_are_reused() {
    let mut tested = false;
    with_signed_request_states_fixture(|_, endpoint, request, _| {
        if tested {
            return;
        }
        tested = true;
        let memory = memory();
        let meter = work();
        let source = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let baseline = memory.used();
        for mode in 0..3 {
            with_store_for_bounded_index_fixture(|target, parent| {
                let bytes = serde_cbor::to_vec(source.state()).unwrap();
                let id = blake3::hash(&bytes).to_hex().to_string();
                let root = target.path.join("formal-objects-v1");
                let representation = if mode == 0 {
                    // A compressible RequestState in an enabled namespace is
                    // correctly physicalized by persist. Produce a genuine old
                    // pack in a separate fresh legacy namespace instead.
                    let legacy = parent.join("legacy-request-pack-source");
                    fs::create_dir(&legacy).unwrap();
                    assert!(!physical::enabled(&legacy).unwrap());
                    crate::controlled_authority::formal::business_pack::persist(&legacy,&[
                        crate::controlled_authority::formal::business_index::BusinessIndexNodeWriteV1{digest:id.clone(),bytes:bytes.clone()}
                    ]).unwrap();
                    let source_path = legacy.join(&id);
                    let locator = fs::read(&source_path).unwrap();
                    assert!(locator.starts_with(b"oasis7.formal.business-cas.pack.v1\0"));
                    // Real new-namespace initialization preserves the original
                    // pack bytes; no marker is removed or forged.
                    physical::initialize(&legacy).unwrap();
                    assert_eq!(fs::read(&source_path).unwrap(), locator);
                    let target_path = root.join(&id);
                    fs::hard_link(&source_path, &target_path).unwrap();
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        let source_meta = fs::metadata(&source_path).unwrap();
                        let target_meta = fs::metadata(&target_path).unwrap();
                        assert_eq!(
                            (source_meta.dev(), source_meta.ino()),
                            (target_meta.dev(), target_meta.ino())
                        );
                    }
                    File::open(&root).unwrap().sync_all().unwrap();
                    assert_eq!(fs::read(&target_path).unwrap(), locator);
                    Some(locator)
                } else {
                    let padding = vec![92; 128 << 10];
                    let mut objects = vec![
                        (id.clone(), bytes.clone()),
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
                            let progress =
                                physical::advance_with_session(&root, &mut session).unwrap();
                            if !progress.building {
                                selected = progress.selected;
                                break;
                            }
                        }
                        assert!(selected.is_some());
                    }
                    None
                };
                let output = target
                    .prepare_request_update_bounded(&source, &memory, &meter, &mut || false)
                    .unwrap();
                assert_eq!(output.entry_id(), id);
                if let Some(locator) = representation {
                    assert_eq!(fs::read(root.join(&id)).unwrap(), locator);
                }
                assert_eq!(
                    serde_cbor::to_vec(&target.get::<FormalRequestStateV1>(&id).unwrap()).unwrap(),
                    bytes
                );
                check_index(target, output.index_root(), request, &id);
                drop(output);
                assert_eq!(memory.used(), baseline);
            });
        }
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}
