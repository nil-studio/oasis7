use super::super::super::tests::{LocalClosureReadFixtureV1, with_local_closure_read_fixture};
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(4_000_000, 256 << 20).unwrap()
}
fn raw(store: &Store, bytes: &[u8]) -> String {
    let id = blake3::hash(bytes).to_hex().to_string();
    fs::write(store.path.join("formal-objects-v1").join(&id), bytes).unwrap();
    id
}
fn compare(store: &Store, id: &str) {
    let old: LocalDataClosureRecordV1 = store.get(id).unwrap();
    let expected = serde_cbor::to_vec(&old).unwrap();
    let before = store.checkpoint_bytes().unwrap();
    let memory = memory();
    let meter = work();
    let paid = store
        .read_local_closure_object_bounded(id, &memory, &meter, &mut || false)
        .unwrap();
    assert_eq!(serde_cbor::to_vec(paid.record()).unwrap(), expected);
    assert!(paid.uses_budget(&memory));
    assert_eq!(paid.reserved_bytes(), memory.used());
    assert_eq!(store.checkpoint_bytes().unwrap(), before);
    drop(paid);
    assert_eq!(memory.used(), 0);
}
#[test]
fn actual_fixed_writer_raw_physical_pack_and_generation_match_original_metadata() {
    for mode in [
        LocalClosureReadFixtureV1::Raw,
        LocalClosureReadFixtureV1::Physical,
        LocalClosureReadFixtureV1::LegacyPack,
        LocalClosureReadFixtureV1::Generation,
    ] {
        with_local_closure_read_fixture(mode, |store, id, record, _| {
            compare(store, id);
            let memory = memory();
            let paid = store
                .read_local_closure_object_bounded(id, &memory, &work(), &mut || false)
                .unwrap();
            assert_eq!(
                serde_cbor::to_vec(paid.record()).unwrap(),
                serde_cbor::to_vec(record).unwrap()
            );
            drop(paid);
            assert_eq!(memory.used(), 0);
        });
    }
}
#[test]
fn actual_missing_selected_generation_does_not_fall_back_to_stale_overlay() {
    with_local_closure_read_fixture(
        LocalClosureReadFixtureV1::GenerationMissingSelected,
        |store, id, record, objects| {
            assert_eq!(
                fs::read(objects.join(id)).unwrap(),
                serde_cbor::to_vec(record).unwrap()
            );
            let memory = memory();
            assert!(
                store
                    .read_local_closure_object_bounded(id, &memory, &work(), &mut || false)
                    .is_err()
            );
            assert_eq!(memory.used(), 0);
        },
    );
}
#[test]
fn original_nonminimal_and_indefinite_byte_string_acceptance_is_preserved() {
    with_local_closure_read_fixture(LocalClosureReadFixtureV1::Raw, |store, _, record, _| {
        let canonical = serde_cbor::to_vec(record).unwrap();
        assert_eq!(canonical[0], 0x59);
        let payload = &canonical[3..];
        let mut wide32 = vec![0x5a];
        wide32.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        wide32.extend_from_slice(payload);
        let mut wide64 = vec![0x5b];
        wide64.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        wide64.extend_from_slice(payload);
        let mut indefinite = vec![0x5f];
        indefinite.extend(serde_cbor::to_vec(&serde_cbor::Value::Bytes(vec![])).unwrap());
        for chunk in payload.chunks(17) {
            indefinite
                .extend(serde_cbor::to_vec(&serde_cbor::Value::Bytes(chunk.to_vec())).unwrap());
        }
        indefinite.push(0xff);
        for bytes in [canonical, wide32, wide64, indefinite] {
            let id = raw(store, &bytes);
            // Acceptance is established by the old typed Store codec first;
            // WIRE_BYTES is never used as a new grammar limit.
            let old: LocalDataClosureRecordV1 = store.get(&id).unwrap();
            assert_eq!(
                serde_cbor::to_vec(&old).unwrap(),
                serde_cbor::to_vec(record).unwrap()
            );
            compare(store, &id);
        }
    });
}
#[test]
fn original_fixed_domain_version_generation_and_bad_wire_errors_are_preserved() {
    with_local_closure_read_fixture(LocalClosureReadFixtureV1::Raw, |store, _, record, _| {
        let canonical = serde_cbor::to_vec(record).unwrap();
        let domain = b"oasis7.iws.local-data-closure.v1\0";
        let mut wrong_domain = canonical.clone();
        wrong_domain[3] ^= 1;
        let mut wrong_version = canonical.clone();
        wrong_version[3 + domain.len() + 3] = 2;
        let option = 3 + domain.len() + 4 + 3 * 32 + 8 + 4 * 32 + 4 * 8;
        let mut wrong_tag = canonical.clone();
        wrong_tag[option] = 2;
        let mut none_nonzero = canonical.clone();
        none_nonzero[option] = 0;
        none_nonzero[option + 1] = 1;
        let mut trailing = canonical.clone();
        trailing.push(0);
        let shortened = serde_cbor::to_vec(&serde_cbor::Value::Bytes(
            canonical[3..canonical.len() - 1].to_vec(),
        ))
        .unwrap();
        let text = serde_cbor::to_vec(&"hostile\ntext").unwrap();
        for bytes in [
            wrong_domain,
            wrong_version,
            wrong_tag,
            none_nonzero,
            trailing,
            shortened,
            text,
            vec![0x5a, 0xff, 0xff, 0xff, 0xff],
            vec![0xff],
            vec![0xa0],
            vec![0x80],
        ] {
            let id = raw(store, &bytes);
            let expected = match store.get::<LocalDataClosureRecordV1>(&id) {
                Ok(_) => panic!("bad original fixture accepted"),
                Err(e) => e,
            };
            let memory = memory();
            let actual = store
                .read_local_closure_object_bounded(&id, &memory, &work(), &mut || false)
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
fn missing_and_hash_corrupt_originals_are_rejected_without_owner() {
    with_local_closure_read_fixture(LocalClosureReadFixtureV1::Raw, |store, id, _, objects| {
        let memory = memory();
        let meter = work();
        assert!(
            store
                .read_local_closure_object_bounded(&"ab".repeat(32), &memory, &meter, &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
        fs::write(objects.join(id), b"damaged original").unwrap();
        assert!(
            store
                .read_local_closure_object_bounded(id, &memory, &meter, &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn direct_decoder_phase_minus_one_cancellation_and_first_work_cause_release_fees() {
    with_local_closure_read_fixture(LocalClosureReadFixtureV1::Raw, |store, id, _, _| {
        let memory = memory();
        let meter = work();
        let raw = store
            .raw_object_bounded(id, LIMIT, &memory, &meter, &mut || false)
            .unwrap()
            .unwrap();
        let live = memory.used();
        let wrapper = std::mem::size_of::<PaidLocalClosureRecordV1>()
            - std::mem::size_of::<LocalDataClosureRecordV1>();
        let phase = decode_workspace(raw.as_slice().len()).unwrap() + wrapper;
        let pressure = memory
            .reserve((256 << 20) - live - (phase - 1), MemoryClass::Ordinary)
            .unwrap();
        let used = memory.used();
        assert!(
            decode_record(&raw, &memory, &meter, &mut || false)
                .unwrap_err()
                .to_string()
                .contains("heap admission")
        );
        assert_eq!(memory.used(), used);
        drop(pressure);
        let mut calls = 0;
        let mut saw_parse = false;
        let error = decode_record(&raw, &memory, &meter, &mut || {
            calls += 1;
            if calls == 1 {
                return false;
            }
            assert_eq!(memory.used(), live + phase);
            saw_parse = true;
            true
        })
        .unwrap_err();
        assert!(saw_parse);
        assert!(matches!(error,FormalError::Invalid(s)if s=="bounded archive read cancelled"));
        assert_eq!(memory.used(), live);
        let zero = MemoryBudget::new(0, 0).unwrap();
        let error = store
            .read_local_closure_object_bounded(id, &zero, &meter, &mut || false)
            .unwrap_err();
        assert!(matches!(error,FormalError::Invalid(s)if s.contains("admission")));
        assert_eq!(zero.used(), 0);
        let remaining = meter.remaining_steps();
        assert!(remaining > 1);
        meter.consume_steps(remaining - 1, &mut || false).unwrap();
        assert_eq!(meter.remaining_steps(), 1);
        let error = decode_record(&raw, &memory, &meter, &mut || false).unwrap_err();
        assert!(matches!(error,FormalError::Invalid(s)if s=="bounded archive read steps"));
        assert_eq!(meter.remaining_steps(), 0);
        assert_eq!(memory.used(), live);
        drop(raw);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn actual_fixed_writer_some_generation_and_all_nonzero_fields_match_full_wire() {
    with_local_closure_read_fixture(LocalClosureReadFixtureV1::Raw, |store, _, _, _| {
        // Data-only original Fixed payload: each hash and scalar has a distinct
        // nonzero value, including both complete locations and Some generation.
        let mut payload = b"oasis7.iws.local-data-closure.v1\0".to_vec();
        payload.extend_from_slice(&1u32.to_be_bytes());
        for value in [11u8, 12, 13] {
            payload.extend_from_slice(&[value; 32]);
        }
        payload.extend_from_slice(&101u64.to_be_bytes());
        for value in [21u8, 22, 23, 24] {
            payload.extend_from_slice(&[value; 32]);
        }
        for value in [201u64, 202, 301, 302] {
            payload.extend_from_slice(&value.to_be_bytes());
        }
        payload.push(1);
        payload.extend_from_slice(&[31; 32]);
        for (height, ordinal, original) in [(401u64, 402u64, 41u8), (501, 502, 51)] {
            payload.extend_from_slice(&height.to_be_bytes());
            payload.extend_from_slice(&ordinal.to_be_bytes());
            payload.extend_from_slice(&[original; 32]);
        }
        let wire = serde_cbor::to_vec(&serde_cbor::Value::Bytes(payload)).unwrap();
        let record: LocalDataClosureRecordV1 = serde_cbor::from_slice(&wire).unwrap();
        assert_eq!(serde_cbor::to_vec(&record).unwrap(), wire);
        let before = store.checkpoint_bytes().unwrap();
        let memory = memory();
        let id = store
            .put_local_closure_record(&record, &memory, &mut || false)
            .unwrap();
        assert_eq!(memory.used(), 0);
        let old: LocalDataClosureRecordV1 = store.get(&id).unwrap();
        assert_eq!(serde_cbor::to_vec(&old).unwrap(), wire);
        let meter = work();
        let paid = store
            .read_local_closure_object_bounded(&id, &memory, &meter, &mut || false)
            .unwrap();
        assert_eq!(serde_cbor::to_vec(paid.record()).unwrap(), wire);
        assert!(paid.uses_budget(&memory));
        assert_eq!(paid.reserved_bytes(), memory.used());
        assert_eq!(store.checkpoint_bytes().unwrap(), before);
        drop(paid);
        assert_eq!(memory.used(), 0);
    });
}
