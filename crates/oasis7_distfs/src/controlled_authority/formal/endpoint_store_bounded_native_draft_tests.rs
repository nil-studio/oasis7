use super::super::super::tests::{
    InsertPlanFixtureV1, with_index_insert_plan_fixture, with_signed_request_states_fixture,
    with_store_for_bounded_index_fixture,
};
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
fn memory() -> MemoryBudget {
    MemoryBudget::new(64 << 20, 0).unwrap()
}
fn work() -> FormalBoundedWorkV1 {
    FormalBoundedWorkV1::new(20_000_000, 64 << 20).unwrap()
}
fn compare(store: &Store) {
    let memory = memory();
    let meter = work();
    let mut stop = || false;
    let before = store.checkpoint.clone();
    let index = store.body.index.clone();
    let hot = store.checkpoint_bytes().unwrap();
    let root = store.path.join("formal-objects-v1");
    let retained = crate::controlled_authority::formal::physical::roots::count(&root).unwrap();
    let draft = {
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        store
            .prepare_native_checkpoint_draft_bounded(&mut ctx)
            .unwrap()
    };
    let mut expected = store.body.clone();
    expected.revision += 1;
    assert_eq!(draft.body(), &expected);
    assert_eq!(
        draft.preimage(),
        Store::checkpoint_signing_bytes(&expected).unwrap()
    );
    assert!(draft.preimage().len() <= PREIMAGE_LIMIT);
    assert_eq!(draft.before_checkpoint(), &before);
    assert_eq!(draft.before_index(), index.as_deref());
    assert!(draft.uses_budget(&memory));
    assert_eq!(draft.reserved_bytes(), memory.used());
    assert_eq!(draft.identity(), meter.identity());
    {
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        draft.validate_context(&mut ctx).unwrap();
    }
    assert_eq!(store.body.index, index);
    assert_eq!(store.checkpoint, before);
    assert_eq!(store.checkpoint_bytes().unwrap(), hot);
    assert_eq!(
        crate::controlled_authority::formal::physical::roots::count(&root).unwrap(),
        retained
    );
    drop(draft);
    assert_eq!(memory.used(), 0);
}
#[test]
fn genuine_current_empty_and_signed_index_metadata_match_original_preimage() {
    with_store_for_bounded_index_fixture(|store, _| compare(store));
    let request = "00".repeat(32);
    let entry = "09".repeat(32);
    with_index_insert_plan_fixture(
        InsertPlanFixtureV1::Canonical(&[(&request, &entry)]),
        |store, _| compare(store),
    );
}
#[test]
fn genuine_five_request_stages_preserve_every_current_metadata_field() {
    let mut visited = Vec::new();
    with_signed_request_states_fixture(|stage, endpoint, _, _| {
        compare(&endpoint.store);
        visited.push(stage.to_owned());
    });
    assert_eq!(
        visited,
        ["Frozen", "Prepared", "Decided", "Ready", "Qualified"]
    );
}
#[test]
fn body_h_minus_one_and_caller_limits_reject_without_retained_allocation() {
    with_store_for_bounded_index_fixture(|store, _| {
        let measure_memory = memory();
        let measure_work = work();
        let mut stop = || false;
        let (h, _) = body_layout(
            &store.body,
            &mut FormalBoundedContextV1::new(&measure_memory, &measure_work, &mut stop),
        )
        .unwrap();
        for mode in 0..5 {
            let memory = MemoryBudget::new(
                if mode == 0 {
                    h - 1
                } else if mode == 1 {
                    0
                } else {
                    64 << 20
                },
                0,
            )
            .unwrap();
            let meter = FormalBoundedWorkV1::new(
                if mode == 2 { 0 } else { 20_000_000 },
                if mode == 3 { 0 } else { 64 << 20 },
            )
            .unwrap();
            let mut cancel = || mode == 4;
            let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut cancel);
            let error = store
                .prepare_native_checkpoint_draft_bounded(&mut ctx)
                .unwrap_err();
            if mode == 0 {
                assert!(matches!(error,FormalError::Invalid(s)if s.contains("body admission")));
            }
            assert_eq!(memory.used(), 0);
        }
    });
}
#[test]
fn real_draft_context_cannot_switch_pool_or_meter() {
    with_store_for_bounded_index_fixture(|store, _| {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let draft = store
            .prepare_native_checkpoint_draft_bounded(&mut FormalBoundedContextV1::new(
                &memory, &meter, &mut stop,
            ))
            .unwrap();
        let baseline = memory.used();
        let foreign_memory = MemoryBudget::new(64 << 20, 0).unwrap();
        let foreign_work = work();
        assert!(
            draft
                .validate_context(&mut FormalBoundedContextV1::new(
                    &foreign_memory,
                    &meter,
                    &mut stop
                ))
                .is_err()
        );
        assert!(
            draft
                .validate_context(&mut FormalBoundedContextV1::new(
                    &memory,
                    &foreign_work,
                    &mut stop
                ))
                .is_err()
        );
        assert_eq!(memory.used(), baseline);
        assert_eq!(foreign_memory.used(), 0);
        drop(draft);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn real_record_structural_role_map_clone_shapes_have_closed_payload_and_node_fees() {
    use serde_cbor::Value;
    fn text_values(value: &Value) -> usize {
        match value {
            Value::Text(s) => s.len(),
            Value::Array(a) => a.iter().map(text_values).sum(),
            Value::Map(m) => m.values().map(text_values).sum(),
            Value::Tag(_, v) => text_values(v),
            _ => 0,
        }
    }
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
        ArtifactRole::NativeExecutionEnvironment,
        ArtifactRole::IncrementalGenesisConfig,
        ArtifactRole::IncrementalRootSet,
        ArtifactRole::IncrementalCommitBody,
        ArtifactRole::IncrementalHistoryPrefix,
    ];
    let mut tested = false;
    with_signed_request_states_fixture(|_, endpoint, _, original| {
        if tested {
            return;
        }
        let Some(candidate) = original.candidate() else {
            return;
        };
        tested = true;
        // This uses a genuine original-protocol record only as a structural
        // layout oracle. It does not mint a NativeDraft or serving permission.
        let source = candidate.candidate.record.clone();
        for entries in [0, 10, 11, 14] {
            let mut record = source.clone();
            record.artifact_roots.clear();
            for (i, role) in roles.iter().enumerate() {
                record.artifact_roots.insert(*role, format!("{i:064x}"));
            }
            // Removing the three greatest keys from the sorted 14-key tree
            // leaves the historical 11-key root+two-leaf shape (5+1+5).
            for role in roles[entries..].iter().rev() {
                record.artifact_roots.remove(role);
            }
            let mut body = endpoint.store.body.clone();
            body.base_parent = Some(CheckpointParent {
                record,
                finalizing_authority_id: "09".repeat(32),
            });
            let memory = memory();
            let meter = work();
            let mut stop = || false;
            let (h, payload) = body_layout(
                &body,
                &mut FormalBoundedContextV1::new(&memory, &meter, &mut stop),
            )
            .unwrap();
            let value: Value = serde_cbor::from_slice(&serde_cbor::to_vec(&body).unwrap()).unwrap();
            let enum_text = match &source.kind {
                FormalRecordKindV1::InitialActivation => "InitialActivation",
                FormalRecordKindV1::Execution => "Execution",
            };
            assert_eq!(payload, text_values(&value) - enum_text.len());
            assert_eq!(
                h,
                size_of::<HotBody>() + payload + role_map_nodes(entries).unwrap()
            );
            let clone = body.clone();
            assert_eq!(
                clone
                    .base_parent
                    .as_ref()
                    .unwrap()
                    .record
                    .artifact_roots
                    .len(),
                entries
            );
            if entries == 0 {
                assert_eq!(role_map_nodes(entries).unwrap(), 0);
            }
            if entries == 11 {
                assert!(role_map_nodes(entries).unwrap() > role_map_nodes(10).unwrap());
            }
            assert_eq!(memory.used(), 0);
        }
    });
    assert!(tested);
}

use super::super::super::tests::{
    NativeDraftFixtureV1, NativeDraftLayoutFixtureV1, with_native_draft_fixture,
    with_native_draft_layout_fixture,
};
#[test]
fn named_original_signed_revision_widths_and_exact_64k_preimage_are_preserved() {
    for mode in [
        NativeDraftFixtureV1::Plain,
        NativeDraftFixtureV1::Revision23,
        NativeDraftFixtureV1::Revision255,
        NativeDraftFixtureV1::Revision65535,
        NativeDraftFixtureV1::ExactPreimage,
    ] {
        with_native_draft_fixture(mode, |store, _| {
            compare(store);
            let memory = memory();
            let meter = work();
            let mut stop = || false;
            let draft = store
                .prepare_native_checkpoint_draft_bounded(&mut FormalBoundedContextV1::new(
                    &memory, &meter, &mut stop,
                ))
                .unwrap();
            assert_eq!(draft.body().revision, store.body.revision + 1);
            if Store::checkpoint_signing_bytes(&store.body).unwrap().len() == PREIMAGE_LIMIT {
                assert_eq!(draft.preimage().len(), PREIMAGE_LIMIT);
            }
            drop(draft);
            assert_eq!(memory.used(), 0);
        });
    }
}
#[test]
fn actual_seal_and_named_overflow_or_oversized_preimage_reject_without_source_change() {
    for mode in [
        NativeDraftFixtureV1::Sealed,
        NativeDraftFixtureV1::Overflow,
        NativeDraftFixtureV1::OverPreimage,
    ] {
        with_native_draft_fixture(mode, |store, _| {
            let memory = memory();
            let meter = work();
            let mut stop = || false;
            let before = store.checkpoint_bytes().unwrap();
            let body = serde_cbor::to_vec(&store.body).unwrap();
            let error = store
                .prepare_native_checkpoint_draft_bounded(&mut FormalBoundedContextV1::new(
                    &memory, &meter, &mut stop,
                ))
                .unwrap_err();
            if store.body.seal.is_some() {
                assert!(matches!(error,FormalError::Conflict(s)if s=="formal epoch sealed"));
            } else if store.body.revision == u64::MAX {
                assert!(matches!(error,FormalError::Invalid(s)if s=="formal revision overflow"));
            } else {
                assert!(
                    matches!(error,FormalError::Invalid(s)if s=="protected signature preimage bound")
                );
            }
            assert_eq!(memory.used(), 0);
            assert_eq!(store.checkpoint_bytes().unwrap(), before);
            assert_eq!(serde_cbor::to_vec(&store.body).unwrap(), body);
        });
    }
}
#[test]
fn complete_full_some_layout_visits_58_plus_role_values_without_claiming_draft_authority() {
    use serde_cbor::Value;
    fn strings(value: &Value) -> usize {
        match value {
            Value::Text(s) => s.len(),
            Value::Array(v) => v.iter().map(strings).sum(),
            Value::Map(v) => v.values().map(strings).sum(),
            Value::Tag(_, v) => strings(v),
            _ => 0,
        }
    }
    fn enum_text<T: serde::Serialize>(value: &T) -> usize {
        let value: Value = serde_cbor::from_slice(&serde_cbor::to_vec(value).unwrap()).unwrap();
        let Value::Text(s) = value else {
            panic!("expected original unit enum wire")
        };
        s.len()
    }
    for shape in [
        NativeDraftLayoutFixtureV1::Empty,
        NativeDraftLayoutFixtureV1::Ten,
        NativeDraftLayoutFixtureV1::HistoricalEleven,
        NativeDraftLayoutFixtureV1::Fourteen,
    ] {
        with_native_draft_layout_fixture(shape, |body| {
            let record = &body.base_parent.as_ref().unwrap().record;
            let seal = body.seal.as_ref().unwrap();
            let m = record.artifact_roots.len();
            let mut count = 0;
            let mut visited = 0;
            visit_body(body, &mut |s| {
                count += 1;
                visited += s.len();
                Ok(())
            })
            .unwrap();
            assert_eq!(count, 58 + m);
            let value: Value = serde_cbor::from_slice(&serde_cbor::to_vec(body).unwrap()).unwrap();
            let metadata_enums = enum_text(&record.kind)
                + enum_text(&seal.instance.endpoint_role)
                + enum_text(&seal.tail.kind);
            let expected_payload = strings(&value) - metadata_enums;
            assert_eq!(visited, expected_payload);
            let memory = memory();
            let meter = work();
            let mut stop = || false;
            let (h, payload) = body_layout(
                body,
                &mut FormalBoundedContextV1::new(&memory, &meter, &mut stop),
            )
            .unwrap();
            assert_eq!(payload, expected_payload);
            assert_eq!(
                h,
                size_of::<HotBody>() + expected_payload + role_map_nodes(m).unwrap()
            );
            let clone = body.clone();
            assert_eq!(&clone, body);
            assert_eq!(memory.used(), 0);
            // This helper supplies only borrowed structural data; in particular
            // seal=Some is never passed through a caller-created Store/draft.
        });
    }
}
