use super::*;
use oasis7_formal_protocol::incremental_material::FactLocationV1;

#[test]
fn detached_history_index_reopens_dense_original_locators_and_rejects_wrong_profile() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("index");
        let expected = [
            CognitionHistoryIndexRowV1 {
                location: FactLocationV1 {
                    height: 0,
                    ordinal: 7,
                    original: [0x35; 32],
                },
                encoded_bytes: 1,
            },
            CognitionHistoryIndexRowV1 {
                location: FactLocationV1 {
                    height: u64::MAX,
                    ordinal: 0,
                    original: [0xa5; 32],
                },
                encoded_bytes: u64::MAX,
            },
            // Repeated original and nonmonotonic fact hints remain intact.
            CognitionHistoryIndexRowV1 {
                location: FactLocationV1 {
                    height: 2,
                    ordinal: 1,
                    original: [0x35; 32],
                },
                encoded_bytes: 1,
            },
            CognitionHistoryIndexRowV1 {
                location: FactLocationV1 {
                    height: 3,
                    ordinal: 1,
                    original: [0x71; 32],
                },
                encoded_bytes: 73,
            },
        ];
        let root = {
            let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let mut writer = store.value_session(physical());
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let root = prepare_cognition_history_index_v1(
                CognitionHistoryFieldV1::Responses,
                3,
                expected[..3].iter().copied().map(Ok),
                &mut MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut io,
                    key_work: &mut keys,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 256,
                        payload_bytes: 256,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 32,
                    max_pending_node_bytes: 1 << 20,
                },
            )
            .unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut read_io = logical();
            let mut read_keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut read = WorldFieldReadContextV1 {
                root: &root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut read_io,
                },
                key_work: &mut read_keys,
                semantic_bytes: 256,
                record_bytes: 4096,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let index = CognitionHistoryIndexRefV1 {
                present: true,
                records: 3,
                root: root.clone(),
            };
            let parent = qualify_cognition_history_index_v1(
                CognitionHistoryFieldV1::Responses,
                &index,
                &mut read,
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 32,
                    max_pending_node_bytes: 1 << 20,
                },
                |ordinal, row| {
                    assert_eq!(row, expected[ordinal as usize]);
                    Ok(())
                },
            )
            .unwrap();
            // Bootstrap above is explicit full-scope work. This producer only
            // reads count/tail and COW-appends one actual locator.
            let after = prepare_cognition_history_index_append_v1(
                &parent,
                expected[3],
                &mut read,
                &mut MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut io,
                    key_work: &mut keys,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 256,
                        payload_bytes: 256,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 8,
                    max_pending_node_bytes: 1 << 20,
                },
            )
            .unwrap();
            assert_eq!(parent.record_count(), 3);
            assert_eq!(after.record_count(), 4);
            after.root().clone()
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: &root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 256,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let index = CognitionHistoryIndexRefV1 {
            present: true,
            records: 4,
            root: root.clone(),
        };
        let mut seen = 0;
        let proof = qualify_cognition_history_index_v1(
            CognitionHistoryFieldV1::Responses,
            &index,
            &mut context,
            &mut CognitionHistoryIndexWorkV1 {
                cells_remaining: 32,
                max_pending_node_bytes: 1 << 20,
            },
            |ordinal, row| {
                assert_eq!(ordinal as usize, seen);
                assert_eq!(row, expected[seen]);
                seen += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(seen, 4);
        assert_eq!(proof.record_count(), 4);
        proof
            .validate_binding(CognitionHistoryFieldV1::Responses, &index)
            .unwrap();
        assert!(matches!(
            qualify_cognition_history_index_v1(
                CognitionHistoryFieldV1::JournalEvents,
                &index,
                &mut context,
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 32,
                    max_pending_node_bytes: 1 << 20
                },
                |_, _| Ok(())
            ),
            Err(ValueTreeError::Invalid(_))
        ));
        let ordinary_state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut ordinary_io = logical();
        let mut ordinary_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut writer = store.value_session(physical());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        assert!(matches!(
            prepare_cognition_history_index_append_v1(
                &proof,
                expected[3],
                &mut WorldFieldReadContextV1 {
                    root: &root,
                    state: &ordinary_state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut ordinary_io
                    },
                    key_work: &mut ordinary_keys,
                    semantic_bytes: 256,
                    record_bytes: 4096,
                    take_state_error: || ordinary_state.take_read_error().map(state_error),
                    cancelled: || false,
                },
                &mut MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut write_io,
                    key_work: &mut write_keys,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 256,
                        payload_bytes: 256,
                        record_bytes: 4096
                    },
                    cancelled: || false,
                },
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 8,
                    max_pending_node_bytes: 1 << 20
                },
            ),
            Err(MemberRecordError::Invalid(_))
        ));
    }
}
