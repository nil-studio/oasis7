use super::super::super::tests::with_store_for_bounded_index_fixture;
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(2_000_000, 256 << 20).unwrap()
}
fn wire(store: &mut Store, bytes: &[u8]) -> String {
    let id = blake3::hash(bytes).to_hex().to_string();
    fs::write(store.path.join("formal-objects-v1").join(&id), bytes).unwrap();
    id
}
fn compare_rejection(store: &Store, request: &str, label: &str) {
    let old = store.lookup(request).unwrap_err();
    let memory = memory();
    let new = store
        .lookup_bounded(request, &memory, &work(), &mut || false)
        .unwrap_err();
    assert_eq!(
        std::mem::discriminant(&new),
        std::mem::discriminant(&old),
        "error class {label}: old={old:?}, new={new:?}"
    );
    assert!(matches!(new, FormalError::Invalid(_)), "{label}: {new:?}");
    let old_text = old.to_string();
    let new_text = new.to_string();
    assert!(!old_text.is_empty() && !new_text.is_empty(), "{label}");
    assert_eq!(
        new_text.split(" at offset ").next(),
        old_text.split(" at offset ").next(),
        "diagnostic {label}"
    );
    assert_eq!(
        memory.used(),
        0,
        "actual rejected lookup fee release {label}"
    );
    // IoRead and SliceRead may report offsets through different adapters; retain
    // the original Invalid category and diagnostic, without inventing equality.
}
fn compare(store: &Store, request: &str) {
    let expected = store.lookup(request).unwrap();
    let memory = memory();
    let meter = work();
    let actual = store
        .lookup_bounded(request, &memory, &meter, &mut || false)
        .unwrap();
    assert_eq!(actual.as_deref().map(String::as_str), expected.as_deref());
    if let Some(actual) = actual {
        assert!(actual.uses_budget(&memory));
        assert_eq!(memory.used(), actual.reserved_bytes());
        assert!(actual.reserved_bytes() >= actual.capacity() + std::mem::size_of::<String>());
        drop(actual);
    }
    assert_eq!(memory.used(), 0);
}
#[test]
fn actual_store_insert_multilevel_lookup_hit_miss_and_paid_long_leaf() {
    with_store_for_bounded_index_fixture(|store, _| {
        let requests = [
            "00".repeat(32),
            "40".repeat(32),
            "80".repeat(32),
            "f0".repeat(32),
        ];
        for (i, request) in requests.iter().enumerate() {
            let entry = store.put(&format!("real stored entry {i}")).unwrap();
            let root = store
                .insert(store.body.index.as_deref(), request, &entry)
                .unwrap();
            store.body.index = Some(root);
        }
        for request in &requests {
            compare(store, request);
        }
        compare(store, &"10".repeat(32));
        // insert's hash contract stays intact. The old reader separately
        // accepts an already-stored legacy leaf with a non-hash long entry.
        let legacy = Index::Leaf {
            request: requests[0].clone(),
            entry: "x".repeat(4096),
        };
        store.body.index = Some(wire(store, &serde_cbor::to_vec(&legacy).unwrap()));
        assert_eq!(store.lookup(&requests[0]).unwrap().unwrap().len(), 4096);
        compare(store, &requests[0]);
    });
}
#[test]
fn original_named_packed_legacy_and_indefinite_wires_are_preserved() {
    with_store_for_bounded_index_fixture(|store, _| {
        let request = "00".repeat(32);
        let entry = "long-old-entry:".to_owned() + &"z".repeat(2048);
        let node = Index::Leaf {
            request: request.clone(),
            entry: entry.clone(),
        };
        let mut variants = vec![("named", serde_cbor::to_vec(&node).unwrap(), true)];
        for packed in [false, true] {
            let mut bytes = Vec::new();
            let serializer = serde_cbor::Serializer::new(&mut bytes).legacy_enums();
            if packed {
                node.serialize(&mut serializer.packed_format()).unwrap();
            } else {
                node.serialize(&mut { serializer }).unwrap();
            }
            variants.push((if packed { "legacy-packed" } else { "legacy" }, bytes, true));
        }
        let mut packed = Vec::new();
        node.serialize(&mut serde_cbor::Serializer::new(&mut packed).packed_format())
            .unwrap();
        variants.push(("packed", packed, true));
        let mut self_described = vec![0xd9, 0xd9, 0xf7];
        self_described.extend_from_slice(&variants[0].1);
        variants.push(("outer-selfdescribe-rejected", self_described, false));
        // Original standard enum outer map stays definite; its struct is indefinite,
        // and request itself uses an indefinite text string of two chunks.
        let mut indefinite = vec![0xa1];
        indefinite.extend(serde_cbor::to_vec(&"Leaf").unwrap());
        indefinite.push(0xbf);
        indefinite.extend(serde_cbor::to_vec(&"request").unwrap());
        indefinite.push(0x7f);
        indefinite.extend(serde_cbor::to_vec(&&request[..32]).unwrap());
        indefinite.extend(serde_cbor::to_vec(&&request[32..]).unwrap());
        indefinite.push(0xff);
        indefinite.extend(serde_cbor::to_vec(&"entry").unwrap());
        indefinite.extend(serde_cbor::to_vec(&entry).unwrap());
        indefinite.push(0xff);
        variants.push(("indefinite-struct-and-text", indefinite, true));
        use serde_cbor::Value as V;
        let with_extra = V::Map(std::collections::BTreeMap::from([(
            V::Text("Leaf".into()),
            V::Map(std::collections::BTreeMap::from([
                (V::Text("request".into()), V::Text(request.clone())),
                (V::Text("entry".into()), V::Text(entry.clone())),
                (
                    V::Text("ignored-old-field".into()),
                    V::Array(vec![V::Integer(1); 1024]),
                ),
            ])),
        )]));
        variants.push((
            "ignored-extra",
            serde_cbor::to_vec(&with_extra).unwrap(),
            true,
        ));
        // Unlike enum dispatch at the root, String fields use parse_value;
        // serde_cbor_2 0.13.0 handle_tagged_value (default no tags feature)
        // recursively visits the underlying string for either tag below.
        for (label, tag) in [
            ("field-selfdescribe", vec![0xd9, 0xd9, 0xf7]),
            ("field-other-tag", vec![0xc1]),
        ] {
            let mut bytes = vec![0xa1];
            bytes.extend(serde_cbor::to_vec(&"Leaf").unwrap());
            bytes.push(0xa2);
            bytes.extend(serde_cbor::to_vec(&"request").unwrap());
            bytes.extend(tag);
            bytes.extend(serde_cbor::to_vec(&request).unwrap());
            bytes.extend(serde_cbor::to_vec(&"entry").unwrap());
            bytes.extend(serde_cbor::to_vec(&entry).unwrap());
            variants.push((label, bytes, true));
        }
        for (label, bytes, accepted) in variants {
            assert!(bytes.len() > 512, "{label}");
            let original = serde_cbor::from_slice::<Index>(&bytes);
            assert_eq!(
                original.is_ok(),
                accepted,
                "old codec {label}: {original:?}"
            );
            if !accepted {
                store.body.index = Some(wire(store, &bytes));
                compare_rejection(store, &request, label);
                continue;
            }
            assert!(matches!(original.unwrap(), Index::Leaf { .. }), "{label}");
            store.body.index = Some(wire(store, &bytes));
            compare(store, &request);
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(serde_cbor::from_slice::<Index>(&trailing).is_err());
            store.body.index = Some(wire(store, &trailing));
            compare_rejection(store, &request, &format!("{label}-trailing"));
        }
    });
}
#[test]
fn unused_bad_reference_is_not_checked_but_chosen_missing_and_bit_order_are_errors() {
    with_store_for_bounded_index_fixture(|store, _| {
        let request = "00".repeat(32);
        let leaf = store
            .put(&Index::Leaf {
                request: request.clone(),
                entry: "not-an-entry-hash".into(),
            })
            .unwrap();
        store.body.index = Some(
            store
                .put(&Index::Fork {
                    bit: 0,
                    left: leaf.clone(),
                    right: "unused-invalid-ref".repeat(100),
                })
                .unwrap(),
        );
        compare(store, &request);
        for chosen in ["ff".repeat(32), "chosen-invalid-hash".into()] {
            store.body.index = Some(
                store
                    .put(&Index::Fork {
                        bit: 0,
                        left: chosen,
                        right: leaf.clone(),
                    })
                    .unwrap(),
            );
            assert!(store.lookup(&request).is_err());
            assert!(
                store
                    .lookup_bounded(&request, &memory(), &work(), &mut || false)
                    .is_err()
            );
        }
        for bit in [4, 256] {
            let inner = store
                .put(&Index::Fork {
                    bit,
                    left: leaf.clone(),
                    right: leaf.clone(),
                })
                .unwrap();
            store.body.index = Some(
                store
                    .put(&Index::Fork {
                        bit: 4,
                        left: inner,
                        right: leaf.clone(),
                    })
                    .unwrap(),
            );
            let old = store.lookup(&request).unwrap_err().to_string();
            let new = store
                .lookup_bounded(&request, &memory(), &work(), &mut || false)
                .unwrap_err()
                .to_string();
            assert_eq!(new, old);
        }
    });
}
#[test]
fn hostile_actual_short_declared_huge_deep_and_long_error_wire_have_bounded_failures() {
    with_store_for_bounded_index_fixture(|store, _| {
        let request = "00".repeat(32);
        let mut huge = vec![0x7b];
        huge.extend_from_slice(&u64::MAX.to_be_bytes());
        let mut deep = vec![0x81; 140];
        deep.push(0);
        let unknown = serde_cbor::to_vec(&std::collections::BTreeMap::from([(
            "\u{1f}".repeat(4096),
            0,
        )]))
        .unwrap();
        for bytes in [huge, deep, unknown] {
            assert!(serde_cbor::from_slice::<Index>(&bytes).is_err());
            store.body.index = Some(wire(store, &bytes));
            compare_rejection(store, &request, "hostile-wire");
        }
    });
}
#[test]
fn actual_store_continuous_meter_cancellation_and_heap_admission_release_owners() {
    with_store_for_bounded_index_fixture(|store, _| {
        let request = "00".repeat(32);
        store.body.index = Some(
            store
                .put(&Index::Leaf {
                    request: request.clone(),
                    entry: "paid entry".into(),
                })
                .unwrap(),
        );
        for mode in 0..4 {
            let memory = if mode == 2 {
                MemoryBudget::new(0, 0).unwrap()
            } else {
                memory()
            };
            let meter = FormalArchiveReadWorkV1::new(
                if mode == 0 { 0 } else { 2_000_000 },
                if mode == 1 { 0 } else { 256 << 20 },
            )
            .unwrap();
            let mut observations = 0;
            assert!(
                store
                    .lookup_bounded(&request, &memory, &meter, &mut || {
                        observations += 1;
                        mode == 3 && observations >= 8
                    })
                    .is_err()
            );
            assert_eq!(memory.used(), 0);
        }
        let memory = memory();
        let meter = work();
        let before = meter.remaining_read_bytes();
        drop(
            store
                .lookup_bounded(&request, &memory, &meter, &mut || false)
                .unwrap(),
        );
        let spent = before - meter.remaining_read_bytes();
        assert!(spent > 0);
        meter
            .consume_read_bytes(meter.remaining_read_bytes() - (spent - 1), &mut || false)
            .unwrap();
        assert!(
            store
                .lookup_bounded(&request, &memory, &meter, &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn paid_reader_sticky_meter_first_cause_survives_serde_io_wrapper() {
    with_store_for_bounded_index_fixture(|store, _| {
        let request = "00".repeat(32);
        let id = store
            .put(&Index::Leaf {
                request,
                entry: "entry".into(),
            })
            .unwrap();
        let memory = memory();
        let meter = work();
        let raw = store
            .raw_object_bounded(&id, LIMIT, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        meter
            .consume_read_bytes(meter.remaining_read_bytes(), &mut || false)
            .unwrap();
        let error = decode_index_bounded_v1(&raw, &memory, &meter, &mut || false).unwrap_err();
        assert!(
            error.to_string().contains("bounded archive read bytes"),
            "{error}"
        );
        assert_eq!(memory.used(), raw.reserved_bytes());
        drop(raw);
        assert_eq!(memory.used(), 0);
    });
}
