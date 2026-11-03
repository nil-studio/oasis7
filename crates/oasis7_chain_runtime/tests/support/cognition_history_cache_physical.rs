use super::*;
use oasis7_world_state::incremental_cognition_history_hash_outboard::*;
fn work() -> CognitionHeadHashWorkV1 {
    CognitionHeadHashWorkV1 {
        input_bytes_remaining: 128 << 20,
        hash_nodes_remaining: 10000000,
    }
}
#[test]
fn raw_and_zstd_frontiers_recover_readmit_append_and_cold_recover_again() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("cache");
        let (id, expected) = {
            let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let _fixture = memory.reserve(512 << 10, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let records = vec![serde_json::json!({"payload":"原🙂".repeat(5000)})];
            let body = oasis7_wasm_abi::encode_canonical_cbor(&records[0]).unwrap();
            let mut builder =
                CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false).unwrap();
            builder.append(&body, &mut work(), || false).unwrap();
            let cache = builder.finish(|| false).unwrap();
            let id = persist_cognition_head_cache_v1(
                &cache,
                &mut store.value_session(physical()),
                &memory,
                &mut logical(),
                &mut work(),
                || false,
            )
            .unwrap();
            let expected = *blake3::hash(
                &oasis7_wasm_abi::encode_canonical_cbor(&(
                    CognitionHeadHashV1::Economy.domain(),
                    (1u64, &records),
                ))
                .unwrap(),
            )
            .as_bytes();
            (id, expected)
        };
        let next_id = {
            let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Recovery,
            )
            .unwrap();
            let values = store.value_session(physical());
            let recovered = recover_cognition_head_cache_v1(
                id,
                &mut ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Recovery,
                    io: &mut logical(),
                },
                &mut work(),
                || false,
            )
            .unwrap();
            assert_eq!(
                recovered
                    .state()
                    .digest(CognitionHeadHashV1::Economy, 1, 1, &mut work(), || false)
                    .unwrap(),
                expected
            );
            let foreign = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            assert!(
                readmit_persisted_cognition_head_cache_ordinary_v1(
                    &recovered,
                    id,
                    &foreign,
                    || false
                )
                .is_err()
            );
            let mut wrong = id;
            wrong[0] ^= 1;
            assert!(
                readmit_persisted_cognition_head_cache_ordinary_v1(
                    &recovered,
                    wrong,
                    &memory,
                    || false
                )
                .is_err()
            );
            let before = memory.used();
            let mut polls = 0;
            assert!(matches!(
                readmit_persisted_cognition_head_cache_ordinary_v1(&recovered, id, &memory, || {
                    polls += 1;
                    polls == 4
                }),
                Err(ValueTreeError::Cancelled)
            ));
            assert_eq!(memory.used(), before);
            let blocked = memory
                .reserve((28 << 20) - memory.used(), MemoryClass::Ordinary)
                .unwrap();
            assert!(matches!(
                readmit_persisted_cognition_head_cache_ordinary_v1(&recovered, id, &memory, || {
                    false
                }),
                Err(ValueTreeError::Memory(_))
            ));
            drop(blocked);
            let admitted =
                readmit_persisted_cognition_head_cache_ordinary_v1(&recovered, id, &memory, || {
                    false
                })
                .unwrap();
            assert_eq!(admitted.id(), id);
            assert!(admitted.uses_budget(&memory));
            assert!(
                readmit_persisted_cognition_head_cache_ordinary_v1(&admitted, id, &memory, || {
                    false
                })
                .is_err()
            );
            drop(recovered);
            assert_eq!(memory.used(), before);
            let mut builder = CognitionHeadHashBuilderV1::append_to(
                admitted.state(),
                &memory,
                MemoryClass::Ordinary,
                || false,
            )
            .unwrap();
            drop(admitted);
            let next = serde_json::json!({"payload":"第二条"});
            let bytes = oasis7_wasm_abi::encode_canonical_cbor(&next).unwrap();
            builder.append(&bytes, &mut work(), || false).unwrap();
            let cache = builder.finish(|| false).unwrap();
            let next_id = persist_cognition_head_cache_v1(
                &cache,
                &mut store.value_session(physical()),
                &memory,
                &mut logical(),
                &mut work(),
                || false,
            )
            .unwrap();
            let records = vec![serde_json::json!({"payload":"原🙂".repeat(5000)}), next];
            let expected = *blake3::hash(
                &oasis7_wasm_abi::encode_canonical_cbor(&(
                    CognitionHeadHashV1::Economy.domain(),
                    (2u64, &records),
                ))
                .unwrap(),
            )
            .as_bytes();
            (next_id, expected)
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let values = store.value_session(physical());
        let recovered = recover_cognition_head_cache_v1(
            next_id.0,
            &mut ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut logical(),
            },
            &mut work(),
            || false,
        )
        .unwrap();
        assert_eq!(
            recovered
                .state()
                .digest(CognitionHeadHashV1::Economy, 2, 2, &mut work(), || false)
                .unwrap(),
            next_id.1
        );
    }
}
