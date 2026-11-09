//! Only this oracle collects small legacy CBOR. Production caches no old body.
use super::*;
use serde_json::Value;

fn work() -> CognitionHeadHashWorkV1 {
    CognitionHeadHashWorkV1 {
        input_bytes_remaining: 128 << 20,
        hash_nodes_remaining: 10_000_000,
    }
}
fn pool() -> MemoryBudget {
    MemoryBudget::new(8 << 20, 4 << 20).unwrap()
}
fn body(records: &[Value]) -> Vec<u8> {
    let mut body = Vec::new();
    for record in records {
        body.extend(oasis7_wasm_abi::encode_canonical_cbor(record).unwrap());
    }
    body
}
fn legacy(kind: CognitionHeadHashV1, seq: u64, records: &[Value]) -> [u8; 32] {
    #[derive(serde::Serialize)]
    struct Journal<'a> {
        head_seq: u64,
        events: &'a [Value],
    }
    let bytes = if kind == CognitionHeadHashV1::Journal {
        oasis7_wasm_abi::encode_canonical_cbor(&(
            kind.domain(),
            Journal {
                head_seq: seq,
                events: records,
            },
        ))
    } else {
        oasis7_wasm_abi::encode_canonical_cbor(&(kind.domain(), (seq, records)))
    }
    .unwrap();
    *blake3::hash(&bytes).as_bytes()
}
fn compare(records: &[Value], pieces: &[usize], sequences: &[u64]) {
    let memory = pool();
    let mut work = work();
    let bytes = body(records);
    let mut builder =
        CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false).unwrap();
    let mut position = 0;
    for &piece in pieces {
        let end = (position + piece).min(bytes.len());
        builder
            .append(&bytes[position..end], &mut work, || false)
            .unwrap();
        position = end;
    }
    builder
        .append(&bytes[position..], &mut work, || false)
        .unwrap();
    let state = builder.finish(|| false).unwrap();
    assert_eq!(state.body_bytes(), bytes.len() as u64);
    for kind in CognitionHeadHashV1::ALL {
        for &seq in sequences {
            assert_eq!(
                state
                    .digest(kind, records.len() as u64, seq, &mut work, || false)
                    .unwrap(),
                legacy(kind, seq, records),
                "{kind:?} seq={seq} records={}",
                records.len()
            );
        }
    }
    assert_eq!(memory.used(), CognitionHeadHashBuilderV1::admission_bytes());
}

#[test]
fn original_three_formats_and_all_twenty_nine_offsets() {
    let memory = pool();
    let builder =
        CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false).unwrap();
    let mut counts = [0; 3];
    for row in &builder.data.rows {
        counts[row.kind as usize] += 1;
    }
    assert_eq!(counts, [5, 12, 12]);
    compare(&[], &[], &[0, 24, 256, 65_536, u64::MAX]);
    compare(
        &[serde_json::json!({"a":"原文", "longer":[null,-1,1.5,-0.0]})],
        &[1, 7, 31],
        &[1, 24, 256, 65_536, u64::MAX],
    );
    // Independent literal CBOR header boundary, no giant source allocation.
    for kind in CognitionHeadHashV1::ALL {
        let h = alignment::header(kind, 1_u64 << 32, u64::MAX).unwrap();
        assert!(h.prefix.ends_with(&[0x9b, 0, 0, 0, 1, 0, 0, 0, 0]));
    }
}

#[test]
fn absolute_chunk_counters_and_header_width_crossings_match_flat_hash() {
    let _input_pool = MemoryBudget::new(16 << 20, 0).unwrap();
    let _input = _input_pool
        .reserve(12 << 20, MemoryClass::Ordinary)
        .unwrap();
    for count in [
        0, 1, 2, 23, 24, 25, 255, 256, 257, 951, 952, 953, 975, 976, 977, 1023, 1024, 1025, 1980,
        1981, 1982, 2047, 2048, 2049, 4095, 4096, 4097, 16_383, 16_384, 16_385, 65_535, 65_536,
        65_537,
    ] {
        let records = vec![Value::Null; count];
        compare(
            &records,
            &[1, 1023, 1, 1024, 1, 4095, 1],
            &[0, 24, 256, 65_536, u64::MAX],
        );
    }
}

#[test]
fn immutable_append_reuses_prefix_and_can_seal_an_exact_tail_chunk() {
    let memory = pool();
    let mut work = work();
    let builder =
        CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false).unwrap();
    let mut state = builder.finish(|| false).unwrap();
    let mut records = Vec::new();
    for addition in [1, 22, 1, 231, 1, 768, 1, 1023, 1, 4096] {
        let mut builder =
            CognitionHeadHashBuilderV1::append_to(&state, &memory, MemoryClass::Ordinary, || false)
                .unwrap();
        records.extend(std::iter::repeat_n(Value::Null, addition));
        builder
            .append(&vec![0xf6; addition], &mut work, || false)
            .unwrap();
        let next = builder.finish(|| false).unwrap();
        // Both admitted caches coexist, then the old immutable state is freed.
        assert_eq!(
            memory.used(),
            2 * CognitionHeadHashBuilderV1::admission_bytes()
        );
        state = next;
        for kind in CognitionHeadHashV1::ALL {
            assert_eq!(
                state
                    .digest(
                        kind,
                        records.len() as u64,
                        records.len() as u64,
                        &mut work,
                        || false
                    )
                    .unwrap(),
                legacy(kind, records.len() as u64, &records)
            );
        }
    }
}

#[test]
fn one_byte_append_really_completes_the_selected_physical_tail() {
    let memory = pool();
    let mut work = work();
    for kind in CognitionHeadHashV1::ALL {
        let count = (1..4096)
            .find(|count| {
                let header = alignment::header(kind, *count, *count).unwrap();
                (header.prefix.len() as u64 + count).is_multiple_of(CHUNK as u64)
            })
            .unwrap();
        let mut builder =
            CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false).unwrap();
        builder
            .append(&vec![0xf6; count as usize - 1], &mut work, || false)
            .unwrap();
        let before = builder.finish(|| false).unwrap();
        let mut builder =
            CognitionHeadHashBuilderV1::append_to(&before, &memory, MemoryClass::Ordinary, || {
                false
            })
            .unwrap();
        builder.append(&[0xf6], &mut work, || false).unwrap();
        let after = builder.finish(|| false).unwrap();
        let header = alignment::header(kind, count, count).unwrap();
        let row = after
            .data
            .rows
            .iter()
            .find(|row| row.kind == kind && usize::from(row.prefix_len) == header.prefix.len())
            .unwrap();
        assert_eq!(row.tail_len, 0);
        assert!(row.complete > 0);
        assert_eq!(
            after
                .digest(kind, count, count, &mut work, || false)
                .unwrap(),
            legacy(kind, count, &vec![Value::Null; count as usize])
        );
    }
}

#[test]
fn all_offsets_include_large_header_geometry_without_claiming_record_schema() {
    // The 9+9 header width corresponds to >2^32 records. This deliberately
    // tests only raw hash geometry, NOT canonical-array completeness: it does
    // not allocate billions of business records or mint any History token.
    let memory = pool();
    let mut work = work();
    let raw: Vec<u8> = (0..8193).map(|index| (index % 251) as u8).collect();
    let mut builder =
        CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false).unwrap();
    builder.append(&raw, &mut work, || false).unwrap();
    let state = builder.finish(|| false).unwrap();
    let representatives = [0, 24, 256, 65_536, 1_u64 << 32];
    for kind in CognitionHeadHashV1::ALL {
        for count in representatives {
            for seq in representatives {
                let header = alignment::header(kind, count, seq).unwrap();
                let row = state
                    .data
                    .rows
                    .iter()
                    .find(|row| {
                        row.kind == kind && usize::from(row.prefix_len) == header.prefix.len()
                    })
                    .unwrap();
                let mut hasher = blake3::Hasher::new();
                hasher
                    .update(&header.prefix)
                    .update(&raw)
                    .update(&header.suffix);
                assert_eq!(
                    row.digest(
                        &state.data.first,
                        raw.len() as u64,
                        &header,
                        &mut work,
                        &mut || false
                    )
                    .unwrap(),
                    *hasher.finalize().as_bytes()
                );
            }
        }
    }
}

#[test]
fn finite_work_pool_class_and_last_cancel_are_not_relabelled() {
    let memory = pool();
    let hold = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    assert!(matches!(
        CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false),
        Err(ValueTreeError::Memory(_))
    ));
    let recovery = CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Recovery, || false)
        .unwrap()
        .finish(|| false)
        .unwrap();
    drop(hold);
    assert!(matches!(
        CognitionHeadHashBuilderV1::append_to(&recovery, &memory, MemoryClass::Ordinary, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    let foreign = pool();
    assert!(matches!(
        CognitionHeadHashBuilderV1::append_to(&recovery, &foreign, MemoryClass::Recovery, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    let mut builder =
        CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false).unwrap();
    let mut zero = CognitionHeadHashWorkV1 {
        input_bytes_remaining: 0,
        hash_nodes_remaining: 100,
    };
    assert!(matches!(
        builder.append(&[0xf6], &mut zero, || false),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(matches!(
        builder.finish(|| false),
        Err(ValueTreeError::Invalid(_))
    ));
    let builder =
        CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false).unwrap();
    let mut polls = 0;
    let base = builder
        .finish(|| {
            polls += 1;
            false
        })
        .unwrap();
    let builder =
        CognitionHeadHashBuilderV1::append_to(&base, &memory, MemoryClass::Ordinary, || false)
            .unwrap();
    let before = memory.used() - CognitionHeadHashBuilderV1::admission_bytes();
    let mut observed = 0;
    assert!(matches!(
        builder.finish(|| {
            observed += 1;
            observed == polls
        }),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(memory.used(), before);
}

#[test]
fn large_literal_record_uses_fixed_cache_and_old_body_is_not_an_append_input() {
    // Oracle-only source/output storage is paid separately before allocation.
    let inputs = MemoryBudget::new(128 << 20, 0).unwrap();
    let _input = inputs.reserve(100 << 20, MemoryClass::Ordinary).unwrap();
    let memory = pool();
    let records = vec![Value::String("原".repeat((16 << 20) / 3))];
    let encoded = body(&records);
    let mut work = work();
    let mut builder =
        CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false).unwrap();
    for bytes in encoded.chunks(8191) {
        builder.append(bytes, &mut work, || false).unwrap();
    }
    let state = builder.finish(|| false).unwrap();
    for kind in CognitionHeadHashV1::ALL {
        assert_eq!(
            state.digest(kind, 1, 1, &mut work, || false).unwrap(),
            legacy(kind, 1, &records)
        );
    }
    drop(encoded);
    drop(records);
    // No old Read, DTO or original is supplied to this successor operation.
    let mut next =
        CognitionHeadHashBuilderV1::append_to(&state, &memory, MemoryClass::Ordinary, || false)
            .unwrap();
    let before = work.input_bytes_remaining;
    next.append(&[0xf6], &mut work, || false).unwrap();
    assert_eq!(before - work.input_bytes_remaining, 1);
    let next = next.finish(|| false).unwrap();
    assert_eq!(next.body_bytes(), state.body_bytes() + 1);
    assert!(memory.used() < 2 << 20);
}

#[test]
fn persisted_frontier_roundtrips_original_heads_and_rejects_malformed_wire() {
    let records = vec![serde_json::json!({"opaque":"汉🙂".repeat(3000)})];
    let memory = pool();
    let bytes = body(&records);
    let mut work = work();
    let mut build =
        CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false).unwrap();
    build.append(&bytes, &mut work, || false).unwrap();
    let state = build.finish(|| false).unwrap();
    let mut wire = Vec::new();
    state
        .write_canonical_cache_v1(&mut wire, &mut work, || false)
        .unwrap();
    assert_eq!(wire.len(), COGNITION_HEAD_CACHE_BYTES_V1);
    drop(state);
    let recovery = read_cognition_head_cache_v1(
        &mut wire.as_slice(),
        &memory,
        MemoryClass::Recovery,
        &mut work,
        || false,
    )
    .unwrap();
    for kind in CognitionHeadHashV1::ALL {
        for seq in [0, 1, 256, u64::MAX] {
            assert_eq!(
                recovery.digest(kind, 1, seq, &mut work, || false).unwrap(),
                legacy(kind, seq, &records)
            );
        }
    }
    drop(recovery);
    for n in [0, 1, 1024, wire.len() - 1] {
        assert!(
            read_cognition_head_cache_v1(
                &mut &wire[..n],
                &memory,
                MemoryClass::Recovery,
                &mut work,
                || false
            )
            .is_err()
        );
    }
    let mut bad = wire.clone();
    bad.push(0);
    assert!(
        read_cognition_head_cache_v1(
            &mut bad.as_slice(),
            &memory,
            MemoryClass::Recovery,
            &mut work,
            || false
        )
        .is_err()
    );
    bad = wire.clone();
    bad[0] ^= 1;
    assert!(
        read_cognition_head_cache_v1(
            &mut bad.as_slice(),
            &memory,
            MemoryClass::Recovery,
            &mut work,
            || false
        )
        .is_err()
    );
    let used_before = memory.used();
    assert!(matches!(
        read_cognition_head_cache_v1(
            &mut wire.as_slice(),
            &memory,
            MemoryClass::Recovery,
            &mut work,
            || true
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(memory.used(), used_before);
}
