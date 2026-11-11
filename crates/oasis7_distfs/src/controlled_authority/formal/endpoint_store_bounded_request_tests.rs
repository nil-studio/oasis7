use super::super::super::tests::{
    with_fresh_endpoint_for_bounded_request_fixture, with_signed_request_states_fixture,
    with_store_for_bounded_index_fixture,
};
use super::*;
use serde_cbor::Value as V;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(4_000_000, 256 << 20).unwrap()
}
fn data_wire(store: &mut Store, request: &str, bytes: &[u8]) -> String {
    let id = blake3::hash(bytes).to_hex().to_string();
    fs::write(store.path.join("formal-objects-v1").join(&id), bytes).unwrap();
    store.body.index = Some(
        store
            .insert(store.body.index.as_deref(), request, &id)
            .unwrap(),
    );
    id
}
#[test]
fn actual_signed_all_five_states_load_as_borrowed_paid_data_without_verification_grant() {
    let mut stages = Vec::new();
    with_signed_request_states_fixture(|stage, endpoint, request, expected| {
        let memory = memory();
        let meter = work();
        let actual = endpoint
            .load_bounded(request, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        assert_eq!(actual.state(), expected, "{stage}");
        assert_eq!(
            serde_cbor::to_vec(actual.state()).unwrap(),
            serde_cbor::to_vec(expected).unwrap(),
            "canonical {stage}"
        );
        assert!(actual.uses_budget(&memory));
        assert_eq!(memory.used(), actual.reserved_bytes());
        assert!(meter.remaining_read_bytes() < 256 << 20);
        // These remain borrowed content comparisons, not a verifier or permit.
        match (actual.state(), expected) {
            (
                FormalRequestStateV1::Frozen {
                    input,
                    material,
                    receipt,
                    certificate,
                },
                FormalRequestStateV1::Frozen {
                    input: i,
                    material: m,
                    receipt: r,
                    certificate: c,
                },
            ) => {
                assert_eq!(input, i);
                assert_eq!(material, m);
                assert_eq!(receipt, r);
                assert_eq!(certificate, c);
            }
            (actual, expected) => {
                let a = actual.candidate().unwrap();
                let b = expected.candidate().unwrap();
                assert_eq!(a.frozen, b.frozen);
                assert_eq!(a.frozen_material, b.frozen_material);
            }
        }
        drop(actual);
        assert_eq!(memory.used(), 0, "{stage}");
        stages.push(stage.to_owned());
    });
    assert_eq!(
        stages,
        ["Frozen", "Prepared", "Decided", "Ready", "Qualified"]
    );
}
fn frozen_value(state: &FormalRequestStateV1) -> V {
    assert!(matches!(state, FormalRequestStateV1::Frozen { .. }));
    serde_cbor::value::to_value(state).unwrap()
}
fn material(value: &mut V) -> &mut std::collections::BTreeMap<V, V> {
    let V::Map(outer) = value else { panic!() };
    let V::Map(fields) = outer.get_mut(&V::Text("Frozen".into())).unwrap() else {
        panic!()
    };
    let V::Map(material) = fields.get_mut(&V::Text("material".into())).unwrap() else {
        panic!()
    };
    material
}
fn object_value() -> V {
    V::Map(std::collections::BTreeMap::from([
        (V::Text("content_hash".into()), V::Text(String::new())),
        (V::Text("bytes".into()), V::Array(Vec::new())),
        (V::Text("references".into()), V::Array(Vec::new())),
    ]))
}
fn rejection(request: &str, bytes: &[u8]) {
    with_store_for_bounded_index_fixture(|store, _| {
        let id = data_wire(store, request, bytes);
        let old = store.get::<FormalRequestStateV1>(&id).unwrap_err();
        let memory = memory();
        let new = store
            .read_request_bounded(request, &memory, &work(), &mut || false)
            .unwrap_err();
        assert_eq!(
            std::mem::discriminant(&new),
            std::mem::discriminant(&old),
            "{old:?} / {new:?}"
        );
        assert!(matches!(new, FormalError::Invalid(_)), "{new:?}");
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn original_request_codec_trailing_and_bounded_objects_references_reject_as_invalid() {
    with_signed_request_states_fixture(|stage, _, request, state| {
        if stage != "Frozen" {
            return;
        }
        let mut trailing = serde_cbor::to_vec(state).unwrap();
        trailing.push(0);
        rejection(request, &trailing);
        let mut objects = frozen_value(state);
        material(&mut objects).insert(
            V::Text("objects".into()),
            V::Array(vec![object_value(); 257]),
        );
        rejection(request, &serde_cbor::to_vec(&objects).unwrap());
        let mut refs = object_value();
        let V::Map(fields) = &mut refs else { panic!() };
        fields.insert(
            V::Text("references".into()),
            V::Array(vec![V::Text(String::new()); 257]),
        );
        let mut value = frozen_value(state);
        material(&mut value).insert(V::Text("objects".into()), V::Array(vec![refs]));
        rejection(request, &serde_cbor::to_vec(&value).unwrap());
    });
}
#[test]
fn indefinite_257th_artifact_and_reference_follow_original_post_next_element_limit() {
    with_signed_request_states_fixture(|stage, _, request, state| {
        if stage != "Frozen" {
            return;
        }
        for refs in [false, true] {
            let mut value = frozen_value(state);
            let mut object = object_value();
            if refs {
                let V::Map(fields) = &mut object else {
                    panic!()
                };
                fields.insert(
                    V::Text("references".into()),
                    V::Array(vec![V::Text(String::new()); 257]),
                );
                material(&mut value).insert(V::Text("objects".into()), V::Array(vec![object]));
            } else {
                material(&mut value).insert(V::Text("objects".into()), V::Array(vec![object; 257]));
            }
            let mut bytes = serde_cbor::to_vec(&value).unwrap();
            let key = serde_cbor::to_vec(&if refs { "references" } else { "objects" }).unwrap();
            let mut marker = key.clone();
            marker.extend_from_slice(&[0x99, 0x01, 0x01]);
            let at = bytes
                .windows(marker.len())
                .position(|v| v == marker)
                .expect("real encoded 257 array");
            // Count each complete encoded element via the original Value Deserializer,
            // solely in the data fixture, then replace definite length with indefinite.
            let start = at + marker.len();
            let mut stream = serde_cbor::Deserializer::from_slice(&bytes[start..]).into_iter::<V>();
            for _ in 0..257 {
                stream.next().unwrap().unwrap();
            }
            let end = start + stream.byte_offset();
            drop(stream);
            bytes.splice(end..end, [0xff]);
            bytes.splice(at + key.len()..at + marker.len(), [0x9f]);
            rejection(request, &bytes);
        }
    });
}
#[test]
fn request_binding_and_referenced_missing_are_errors_not_absence() {
    with_signed_request_states_fixture(|stage, _, _, state| {
        if stage != "Frozen" {
            return;
        }
        with_store_for_bounded_index_fixture(|store, _| {
            let wrong = "ab".repeat(32);
            data_wire(store, &wrong, &serde_cbor::to_vec(state).unwrap());
            let memory = memory();
            let error = store
                .read_request_bounded(&wrong, &memory, &work(), &mut || false)
                .unwrap_err();
            assert!(error.to_string().contains("formal index request binding"));
            assert_eq!(memory.used(), 0);
            store.body.index = Some(store.insert(None, &wrong, &"ff".repeat(32)).unwrap());
            assert!(
                store
                    .read_request_bounded(&wrong, &memory, &work(), &mut || false)
                    .is_err()
            );
            assert_eq!(memory.used(), 0);
            store.body.index = None;
            assert!(
                store
                    .read_request_bounded(&wrong, &memory, &work(), &mut || false)
                    .unwrap()
                    .is_none()
            );
        });
    });
}
#[test]
fn actual_reader_meter_first_cause_and_exact_decode_heap_admission() {
    with_signed_request_states_fixture(|stage, endpoint, request, _| {
        if stage != "Frozen" {
            return;
        }
        let memory = memory();
        let meter = work();
        let id = endpoint.store.lookup(request).unwrap().unwrap();
        let raw = endpoint
            .store
            .raw_object_bounded(&id, LIMIT, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let phase = decode_workspace(raw.as_slice().len()).unwrap();
        let gate = memory
            .reserve(
                (256 << 20) - memory.used() - (phase - 1),
                MemoryClass::Ordinary,
            )
            .unwrap();
        let error = decode_request_bounded_v1(&raw, &memory, &meter, &mut || false).unwrap_err();
        assert!(error.to_string().contains("bounded request heap admission"));
        drop(gate);
        assert_eq!(memory.used(), raw.reserved_bytes());
        meter
            .consume_read_bytes(meter.remaining_read_bytes(), &mut || false)
            .unwrap();
        let error = decode_request_bounded_v1(&raw, &memory, &meter, &mut || false).unwrap_err();
        assert!(
            error.to_string().contains("bounded archive read bytes"),
            "{error}"
        );
        assert_eq!(memory.used(), raw.reserved_bytes());
        drop(raw);
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn same_meter_continues_and_cancellation_errors_release_all_request_owners() {
    with_signed_request_states_fixture(|stage, endpoint, request, _| {
        if stage != "Frozen" {
            return;
        }
        for mode in 0..4 {
            let memory = if mode == 2 {
                MemoryBudget::new(0, 0).unwrap()
            } else {
                memory()
            };
            let meter = FormalArchiveReadWorkV1::new(
                if mode == 0 { 0 } else { 4_000_000 },
                if mode == 1 { 0 } else { 256 << 20 },
            )
            .unwrap();
            let mut calls = 0;
            assert!(
                endpoint
                    .store
                    .read_request_bounded(request, &memory, &meter, &mut || {
                        calls += 1;
                        mode == 3 && calls >= 8
                    })
                    .is_err()
            );
            assert_eq!(memory.used(), 0);
        }
        let memory = memory();
        let meter = work();
        let initial = meter.remaining_read_bytes();
        drop(
            endpoint
                .store
                .read_request_bounded(request, &memory, &meter, &mut || false)
                .unwrap()
                .unwrap(),
        );
        let used = initial - meter.remaining_read_bytes();
        meter
            .consume_read_bytes(meter.remaining_read_bytes() - (used - 1), &mut || false)
            .unwrap();
        assert!(
            endpoint
                .store
                .read_request_bounded(request, &memory, &meter, &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn endpoint_bounded_data_load_preserves_real_error_poisoning() {
    with_fresh_endpoint_for_bounded_request_fixture(|endpoint, _| {
        let request = "aa".repeat(32);
        let id = data_wire(&mut endpoint.store, &request, &[0xff]);
        let old = endpoint.store.get::<FormalRequestStateV1>(&id).unwrap_err();
        let memory = memory();
        let error = endpoint
            .load_bounded(&request, &memory, &work(), &mut || false)
            .unwrap_err();
        assert_eq!(std::mem::discriminant(&error), std::mem::discriminant(&old));
        assert!(matches!(error, FormalError::Invalid(_)));
        assert_eq!(memory.used(), 0);
        assert!(
            endpoint.head().is_err(),
            "failed bounded load must poison actual endpoint"
        );
    });
}

#[test]
fn actual_short_wire_oversized_hints_and_duplicate_role_replacement_keep_original_codec() {
    with_signed_request_states_fixture(|stage, _, request, state| {
        if stage != "Frozen" {
            return;
        }
        for field in ["objects", "bytes"] {
            let mut value = frozen_value(state);
            material(&mut value).insert(
                V::Text("objects".into()),
                V::Array(if field == "bytes" {
                    vec![object_value()]
                } else {
                    Vec::new()
                }),
            );
            let mut bytes = serde_cbor::to_vec(&value).unwrap();
            let mut marker = serde_cbor::to_vec(&field).unwrap();
            marker.push(0x80);
            let at = bytes
                .windows(marker.len())
                .position(|v| v == marker)
                .unwrap()
                + marker.len()
                - 1;
            let mut declared = vec![0x9b];
            declared.extend_from_slice(&u64::MAX.to_be_bytes());
            bytes.splice(at..at + 1, declared);
            rejection(request, &bytes);
        }
        // Duplicate map keys are legacy replacement semantics, not additional
        // authority. Compare the original typed result from this mutated data wire.
        let mut value = frozen_value(state);
        material(&mut value).insert(V::Text("roots".into()), V::Map(Default::default()));
        let mut bytes = serde_cbor::to_vec(&value).unwrap();
        let mut marker = serde_cbor::to_vec(&"roots").unwrap();
        marker.push(0xa0);
        let at = bytes
            .windows(marker.len())
            .position(|v| v == marker)
            .unwrap()
            + marker.len()
            - 1;
        let mut duplicate = vec![0xa2];
        for text in ["a".repeat(2048), "b".repeat(2048)] {
            duplicate.extend(serde_cbor::to_vec(&"Input").unwrap());
            duplicate.extend(serde_cbor::to_vec(&text).unwrap());
        }
        bytes.splice(at..at + 1, duplicate);
        with_store_for_bounded_index_fixture(|store, _| {
            let id = data_wire(store, request, &bytes);
            let old = store.get::<FormalRequestStateV1>(&id).unwrap();
            let memory = memory();
            let actual = store
                .read_request_bounded(request, &memory, &work(), &mut || false)
                .unwrap()
                .unwrap();
            assert_eq!(actual.state(), &old);
            drop(actual);
            assert_eq!(memory.used(), 0);
        });
    });
}

#[test]
fn original_request_named_packed_and_legacy_formats_use_the_same_dto_decoder() {
    with_signed_request_states_fixture(|stage, _, request, state| {
        if stage != "Frozen" {
            return;
        }
        let mut formats = vec![("named", serde_cbor::to_vec(state).unwrap())];
        let mut bytes = Vec::new();
        state
            .serialize(&mut serde_cbor::Serializer::new(&mut bytes).packed_format())
            .unwrap();
        formats.push(("packed", bytes));
        let mut bytes = Vec::new();
        state
            .serialize(&mut serde_cbor::Serializer::new(&mut bytes).legacy_enums())
            .unwrap();
        formats.push(("legacy", bytes));
        let mut bytes = Vec::new();
        state
            .serialize(
                &mut serde_cbor::Serializer::new(&mut bytes)
                    .legacy_enums()
                    .packed_format(),
            )
            .unwrap();
        formats.push(("legacy-packed", bytes));
        with_store_for_bounded_index_fixture(|store, _| {
            for (label, bytes) in formats {
                let id = data_wire(store, request, &bytes);
                let old = store
                    .get::<FormalRequestStateV1>(&id)
                    .unwrap_or_else(|e| panic!("old {label}: {e}"));
                let memory = memory();
                let actual = store
                    .read_request_bounded(request, &memory, &work(), &mut || false)
                    .unwrap_or_else(|e| panic!("bounded {label}: {e}"))
                    .unwrap();
                assert_eq!(actual.state(), &old, "{label}");
                drop(actual);
                assert_eq!(memory.used(), 0);
            }
        });
    });
}
