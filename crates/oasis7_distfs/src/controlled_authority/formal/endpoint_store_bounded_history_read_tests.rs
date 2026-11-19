use super::super::super::tests::{with_history_read_fixture, with_store_for_bounded_index_fixture};
use super::*;
use serde_cbor::Value;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(4_000_000, 256 << 20).unwrap()
}
fn node(links: usize) -> HistoryNode {
    HistoryNode {
        head: FormalHeadRefV1 {
            position: 99,
            record_id: "12".repeat(32),
        },
        request: "34".repeat(32),
        entry: "56".repeat(32),
        ancestors: (0..links)
            .map(|i| HistoryLink {
                head: FormalHeadRefV1 {
                    position: i as u64,
                    record_id: "78".repeat(32),
                },
                node: "9a".repeat(32),
            })
            .collect(),
    }
}
fn wire(store: &Store, bytes: &[u8]) -> String {
    let id = blake3::hash(bytes).to_hex().to_string();
    fs::write(store.path.join("formal-objects-v1").join(&id), bytes).unwrap();
    id
}
fn compare(store: &Store, id: &str) {
    let original: HistoryNode = store.get(id).unwrap();
    let expected = serde_cbor::to_vec(&original).unwrap();
    let before = store.checkpoint_bytes().unwrap();
    let memory = memory();
    let meter = work();
    let actual = store
        .read_history_object_bounded(id, &memory, &meter, &mut || false)
        .unwrap();
    assert_eq!(serde_cbor::to_vec(actual.node()).unwrap(), expected);
    assert!(actual.uses_budget(&memory));
    assert_eq!(actual.reserved_bytes(), memory.used());
    assert_eq!(store.checkpoint_bytes().unwrap(), before);
    drop(actual);
    assert_eq!(memory.used(), 0);
}
#[test]
fn actual_qualified_history_and_ancestor_objects_match_original_get() {
    with_history_read_fixture(|store, ids, _| {
        assert!(ids.len() >= 2);
        for id in ids {
            compare(store, id);
        }
    });
}
#[test]
fn greater_than_64_ancestors_remain_valid_point_data_without_global_business_cap() {
    with_store_for_bounded_index_fixture(|store, _| {
        let id = store.put(&node(65)).unwrap();
        compare(store, &id);
        let memory = memory();
        let actual = store
            .read_history_object_bounded(&id, &memory, &work(), &mut || false)
            .unwrap();
        assert_eq!(actual.node().ancestors.len(), 65);
        drop(actual);
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn original_named_packed_sequence_indefinite_and_ignored_fields_are_preserved() {
    with_store_for_bounded_index_fixture(|store, _| {
        let value = node(2);
        let named = serde_cbor::to_vec(&value).unwrap();
        assert_eq!(named[0], 0xa4);
        let mut packed = Vec::new();
        value
            .serialize(&mut serde_cbor::Serializer::new(&mut packed).packed_format())
            .unwrap();
        let mut indefinite = vec![0xbf];
        indefinite.extend_from_slice(&named[1..]);
        indefinite.push(0xff);
        let mut unknown: Value = serde_cbor::from_slice(&named).unwrap();
        let Value::Map(ref mut map) = unknown else {
            panic!("map")
        };
        map.insert(
            Value::Text("ignored-extra".into()),
            Value::Array(vec![Value::Integer(123)]),
        );
        let head = |h: &FormalHeadRefV1| {
            Value::Array(vec![
                Value::Integer(h.position.into()),
                Value::Text(h.record_id.clone()),
            ])
        };
        let sequence = Value::Array(vec![
            head(&value.head),
            Value::Text(value.request.clone()),
            Value::Text(value.entry.clone()),
            Value::Array(
                value
                    .ancestors
                    .iter()
                    .map(|a| Value::Array(vec![head(&a.head), Value::Text(a.node.clone())]))
                    .collect(),
            ),
        ]);
        for bytes in [
            named,
            packed,
            indefinite,
            serde_cbor::to_vec(&unknown).unwrap(),
            serde_cbor::to_vec(&sequence).unwrap(),
        ] {
            let id = wire(store, &bytes);
            compare(store, &id);
        }
    });
}
#[test]
fn original_codec_errors_and_huge_vector_hint_preserve_invalid_first_cause() {
    with_store_for_bounded_index_fixture(|store, _| {
        let base = serde_cbor::to_vec(&node(0)).unwrap();
        let key = serde_cbor::to_vec(&"ancestors").unwrap();
        let at = base.windows(key.len()).position(|s| s == key).unwrap() + key.len();
        assert_eq!(base[at], 0x80);
        let mut huge = base.clone();
        huge.splice(at..at + 1, [0x9a, 0xff, 0xff, 0xff, 0xff]);
        let mut trailing = base.clone();
        trailing.push(0);
        let mut duplicate = vec![0xa5];
        duplicate.extend_from_slice(&base[1..]);
        duplicate.extend(serde_cbor::to_vec(&"request").unwrap());
        duplicate.extend(serde_cbor::to_vec(&"duplicate").unwrap());
        for bytes in [
            huge,
            trailing,
            duplicate,
            vec![0xff],
            vec![0xa0],
            vec![0xf6],
        ] {
            let id = wire(store, &bytes);
            let expected = store.get::<HistoryNode>(&id).unwrap_err();
            let memory = memory();
            let actual = store
                .read_history_object_bounded(&id, &memory, &work(), &mut || false)
                .unwrap_err();
            assert_eq!(
                std::mem::discriminant(&actual),
                std::mem::discriminant(&expected)
            );
            assert!(matches!(actual, FormalError::Invalid(_)));
            assert_eq!(memory.used(), 0);
        }
    });
}
#[test]
fn missing_or_hash_corrupt_source_is_not_replaced_with_unqualified_data() {
    with_store_for_bounded_index_fixture(|store, _| {
        let missing = "ab".repeat(32);
        let memory = memory();
        let meter = work();
        assert!(
            store
                .read_history_object_bounded(&missing, &memory, &meter, &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
        let id = store.put(&node(1)).unwrap();
        fs::write(
            store.path.join("formal-objects-v1").join(&id),
            b"corrupt original",
        )
        .unwrap();
        assert!(
            store
                .read_history_object_bounded(&id, &memory, &meter, &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn exact_decoder_phase_minus_one_and_sticky_work_or_cancellation_release_fees() {
    with_store_for_bounded_index_fixture(|store, _| {
        let id = store.put(&node(3)).unwrap();
        let memory = memory();
        let meter = work();
        let raw = store
            .raw_object_bounded(&id, LIMIT, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let live = memory.used();
        let wrapper = std::mem::size_of::<PaidHistoryNodeV1>() - std::mem::size_of::<HistoryNode>();
        let phase = decode_workspace(raw.as_slice().len()).unwrap() + wrapper;
        let pressure = memory
            .reserve((256 << 20) - live - (phase - 1), MemoryClass::Ordinary)
            .unwrap();
        let retained = memory.used();
        assert!(
            decode_history(&raw, &memory, &meter, &mut || false)
                .unwrap_err()
                .to_string()
                .contains("heap admission")
        );
        assert_eq!(memory.used(), retained);
        drop(pressure);
        drop(raw);
        assert_eq!(memory.used(), 0);
        for mode in 0..4 {
            let meter = FormalArchiveReadWorkV1::new(
                if mode == 0 { 0 } else { 4_000_000 },
                if mode == 1 { 0 } else { 256 << 20 },
            )
            .unwrap();
            let mut calls = 0;
            assert!(
                store
                    .read_history_object_bounded(&id, &memory, &meter, &mut || {
                        calls += 1;
                        mode == 2 || (mode == 3 && calls >= 20)
                    })
                    .is_err()
            );
            assert_eq!(memory.used(), 0);
        }
    });
}

#[test]
fn direct_paid_decoder_preserves_parse_cancellation_and_exhausted_first_read_cause() {
    with_store_for_bounded_index_fixture(|store, _| {
        let id = store.put(&node(3)).unwrap();
        let memory = memory();
        let meter = work();
        let raw = store
            .raw_object_bounded(&id, LIMIT, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let raw_fee = memory.used();
        let wrapper = std::mem::size_of::<PaidHistoryNodeV1>() - std::mem::size_of::<HistoryNode>();
        let phase = decode_workspace(raw.as_slice().len()).unwrap() + wrapper;
        let mut calls = 0;
        let mut saw_paid_parse = false;
        let cancelled = decode_history(&raw, &memory, &meter, &mut || {
            calls += 1;
            if calls == 1 {
                return false;
            } // Decoder entry charge, before phase admission.
            assert_eq!(memory.used(), raw_fee + phase);
            saw_paid_parse = true;
            true // PaidRead's first attempted parser read, after admission.
        })
        .unwrap_err();
        assert!(saw_paid_parse);
        assert!(
            matches!(cancelled, FormalError::Invalid(text) if text == "bounded archive read cancelled")
        );
        assert_eq!(memory.used(), raw_fee);

        // Exercise the actual Store wrapper with a real zero-sized pool while
        // this SAME continuing meter still has ample work. No foreign raw owner
        // is passed into its decoder and no fresh meter bypasses consumption.
        let zero = MemoryBudget::new(0, 0).unwrap();
        let error = store
            .read_history_object_bounded(&id, &zero, &meter, &mut || false)
            .unwrap_err();
        assert!(matches!(error, FormalError::Invalid(text) if text.contains("admission")));
        assert_eq!(zero.used(), 0);
        assert_eq!(memory.used(), raw_fee);

        let remaining = meter.remaining_steps();
        assert!(remaining > 1);
        meter.consume_steps(remaining - 1, &mut || false).unwrap();
        assert_eq!(meter.remaining_steps(), 1);
        let error = decode_history(&raw, &memory, &meter, &mut || false).unwrap_err();
        // The decoder entry spends the final step; PaidRead's first attempt
        // fails. Its typed cause must win over serde's synthetic IO wrapper.
        assert!(
            matches!(error, FormalError::Invalid(text) if text == "bounded archive read steps")
        );
        assert_eq!(meter.remaining_steps(), 0);
        assert_eq!(memory.used(), raw_fee);
        drop(raw);
        assert_eq!(memory.used(), 0);
    });
}
