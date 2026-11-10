use super::super::super::tests::{
    with_signed_request_states_fixture, with_store_for_bounded_index_fixture,
};
use super::*;
use oasis7_proto::memory_budget::MemoryClass;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(50_000_000, 1 << 30).unwrap()
}
#[test]
fn four_real_nonqualified_states_derive_original_pending_index_and_decision_fields() {
    let mut seen = Vec::new();
    with_signed_request_states_fixture(|stage, endpoint, request, original| {
        if stage == "Qualified" {
            return;
        }
        let memory = memory();
        let meter = work();
        let source = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let source_fee = source.reserved_bytes();
        with_store_for_bounded_index_fixture(|target, _| {
            let before = target.checkpoint_bytes().unwrap();
            let oldbody = target.body.clone();
            let update = target
                .prepare_request_update_bounded(&source, &memory, &meter, &mut || false)
                .unwrap();
            let index = update.index_root().to_owned();
            let entry = update.entry_id().to_owned();
            let draft = target
                .prepare_pending_native_checkpoint_draft_bounded(
                    update,
                    &memory,
                    &meter,
                    &mut || false,
                )
                .unwrap();
            let mut expected = oldbody.clone();
            expected.revision += 1;
            expected.index = Some(index);
            expected.pending = Some(entry.clone());
            if let FormalRequestStateV1::Decided { candidate, .. }
            | FormalRequestStateV1::Ready { candidate, .. } = original
            {
                expected.decision_head = Some(FormalHeadRefV1 {
                    position: candidate.candidate.record.position,
                    record_id: candidate.candidate.record.record_id().unwrap(),
                });
            }
            assert_eq!(draft.body(), &expected, "{stage}");
            assert_eq!(
                draft.preimage(),
                Store::checkpoint_signing_bytes(&expected).unwrap()
            );
            let actual: FormalRequestStateV1 = target.get(&entry).unwrap();
            assert_eq!(&actual, original);
            // All references are actual newly retained objects. Original native
            // metadata validator accepts these four nonqualified states too.
            target.validate_refs(draft.body()).unwrap();
            assert!(draft.uses_budget(&memory));
            assert_eq!(memory.used(), source_fee + draft.reserved_bytes());
            assert_eq!(target.body, oldbody);
            assert_eq!(target.checkpoint_bytes().unwrap(), before);
            let root = target.path.join("formal-objects-v1");
            assert!(crate::controlled_authority::formal::physical::pin(&root, true).is_err());
            drop(draft);
            assert_eq!(memory.used(), source_fee);
            assert!(crate::controlled_authority::formal::physical::pin(&root, true).is_ok());
        });
        drop(source);
        assert_eq!(memory.used(), 0);
        seen.push(stage.to_owned());
    });
    assert_eq!(seen, ["Frozen", "Prepared", "Decided", "Ready"]);
}
#[test]
fn genuine_qualified_source_requires_real_history_producer_and_releases_update() {
    with_signed_request_states_fixture(|stage, endpoint, request, _| {
        if stage != "Qualified" {
            return;
        }
        let memory = memory();
        let meter = work();
        let source = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let source_fee = source.reserved_bytes();
        with_store_for_bounded_index_fixture(|target, _| {
            let before = target.checkpoint_bytes().unwrap();
            let update = target
                .prepare_request_update_bounded(&source, &memory, &meter, &mut || false)
                .unwrap();
            let error = target
                .prepare_pending_native_checkpoint_draft_bounded(
                    update,
                    &memory,
                    &meter,
                    &mut || false,
                )
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("Qualified history producer required")
            );
            assert_eq!(memory.used(), source_fee);
            assert_eq!(target.checkpoint_bytes().unwrap(), before);
        });
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn foreign_pool_core_cancel_and_admission_failure_consume_update_without_native_change() {
    with_signed_request_states_fixture(|stage, endpoint, request, _| {
        if stage != "Prepared" {
            return;
        }
        let memory = memory();
        let meter = work();
        let source = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let source_fee = source.reserved_bytes();
        for mode in 0..4 {
            with_store_for_bounded_index_fixture(|target, _| {
                let before = target.checkpoint_bytes().unwrap();
                let update = target
                    .prepare_request_update_bounded(&source, &memory, &meter, &mut || false)
                    .unwrap();
                let other = MemoryBudget::new(256 << 20, 0).unwrap();
                let other_meter = work();
                let pressure = if mode == 3 {
                    Some(
                        memory
                            .reserve((256 << 20) - memory.used(), MemoryClass::Ordinary)
                            .unwrap(),
                    )
                } else {
                    None
                };
                let error = target
                    .prepare_pending_native_checkpoint_draft_bounded(
                        update,
                        if mode == 0 { &other } else { &memory },
                        if mode == 1 { &other_meter } else { &meter },
                        &mut || mode == 2,
                    )
                    .unwrap_err();
                if mode == 2 {
                    assert!(error.to_string().contains("cancelled"));
                }
                assert_eq!(other.used(), 0);
                assert_eq!(
                    memory.used(),
                    source_fee + pressure.as_ref().map_or(0, |p| p.bytes())
                );
                drop(pressure);
                assert_eq!(memory.used(), source_fee);
                assert_eq!(target.checkpoint_bytes().unwrap(), before);
            });
        }
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn actual_retained_namespace_from_another_fresh_store_is_rejected() {
    with_signed_request_states_fixture(|stage, endpoint, request, _| {
        if stage != "Prepared" {
            return;
        }
        let memory = memory();
        let meter = work();
        let source = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let source_fee = source.reserved_bytes();
        with_store_for_bounded_index_fixture(|first, _| {
            let update = first
                .prepare_request_update_bounded(&source, &memory, &meter, &mut || false)
                .unwrap();
            with_store_for_bounded_index_fixture(|second, _| {
                let before = second.checkpoint_bytes().unwrap();
                assert!(
                    second
                        .prepare_pending_native_checkpoint_draft_bounded(
                            update,
                            &memory,
                            &meter,
                            &mut || false
                        )
                        .is_err()
                );
                assert_eq!(second.checkpoint_bytes().unwrap(), before);
                assert_eq!(memory.used(), source_fee);
            });
        });
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}
