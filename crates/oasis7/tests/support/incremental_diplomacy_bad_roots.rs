use super::*;
use oasis7_formal_protocol::incremental_state::StateScanContext;
pub(super) fn rows(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    root: &StateRootV1,
) -> Rows {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut result = Vec::with_capacity(root.leaf_count as usize);
    StateScanContext {
        source: &state,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut scan(),
    }
    .visit_entries(
        root,
        |entry| {
            result.push((entry.key.clone(), entry.value));
            Ok::<_, ValueTreeError>(())
        },
        || false,
    )
    .unwrap();
    result
}
fn field_key(field: DiplomacyFieldV1, name: &str) -> WorldFieldKeyV1 {
    fixture::key(
        field.name(),
        FieldShape::Map,
        WorldFieldRecordKindV1::Entry,
        Some(&fixture::pair_token(&[0], &fixture::text_token(name))),
    )
    .0
}
#[test]
fn independent_orphan_required_column_option_enum_and_u32_roots_rejected() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("invalid"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let original = produce(&store, &memory, &source());
    let before = build(&store, &memory, &original);
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = fixed!(&before, &state, &values, &memory, &mut io, &mut keys);
    let mut children = Vec::new();
    for field in DiplomacyFieldV1::ALL {
        let root = inspect_diplomacy_root_v1(field, KEY, &mut ctx)
            .unwrap()
            .unwrap();
        let rows = rows(&store, &memory, &root);
        children.push((root, rows));
    }
    for case in 0..8 {
        let (field, index) = match case {
            0 => (DiplomacyFieldV1::Alliances, 0),
            1..=3 => (DiplomacyFieldV1::Wars, 1),
            4 | 5 => (DiplomacyFieldV1::Crises, 2),
            _ => (DiplomacyFieldV1::EconomicContracts, 3),
        };
        let (old, original_rows) = &children[index];
        let mut modified = original_rows.clone();
        match case {
            0 => {
                let mut ordinal = vec![2];
                ordinal.extend_from_slice(&1u64.to_be_bytes());
                let token = fixture::pair_token(
                    &fixture::pair_token(&[0], &fixture::text_token("members")),
                    &ordinal,
                );
                let key = fixture::key(
                    field.name(),
                    FieldShape::Map,
                    WorldFieldRecordKindV1::Entry,
                    Some(&token),
                )
                .0;
                modified.retain(|(k, _)| k != key.as_bytes());
            }
            1 | 2 => {
                let mut ordinal = vec![2];
                ordinal.extend_from_slice(&0u64.to_be_bytes());
                let member = fixture::pair_token(
                    &fixture::pair_token(&[0], &fixture::text_token("participant_outcomes")),
                    &ordinal,
                );
                let token = if case == 1 {
                    member
                } else {
                    fixture::pair_token(&member, &fixture::text_token("data_delta"))
                };
                let key = fixture::key(
                    field.name(),
                    FieldShape::Map,
                    WorldFieldRecordKindV1::Entry,
                    Some(&token),
                )
                .0;
                modified.retain(|(k, _)| k != key.as_bytes());
            }
            3 => {
                modified = replace(
                    &store,
                    &memory,
                    &modified,
                    &field_key(field, "intensity"),
                    |bytes| {
                        let len = bytes.len();
                        bytes[len - 8..].copy_from_slice(&(u64::from(u32::MAX) + 1).to_be_bytes());
                    },
                );
            }
            4 => {
                let token = fixture::pair_token(
                    &fixture::pair_token(&[0], &fixture::text_token("success")),
                    &[0],
                );
                let key = fixture::key(
                    field.name(),
                    FieldShape::Map,
                    WorldFieldRecordKindV1::Entry,
                    Some(&token),
                )
                .0;
                modified = replace(&store, &memory, &modified, &key, |bytes| {
                    *bytes.last_mut().unwrap() = 0
                });
            }
            5 => modified.retain(|(key, _)| key != field_key(field, "kind").as_bytes()),
            6 => {
                modified = replace(
                    &store,
                    &memory,
                    &modified,
                    &field_key(field, "status"),
                    |bytes| {
                        let len = bytes.len();
                        bytes[len - 7..].copy_from_slice(b"invalid");
                    },
                )
            }
            7 => {
                let mut writer = store.value_session(physical());
                let row = write_member_record_v1(
                    field.descriptor().unwrap(),
                    WorldFieldRecordKindV1::Entry,
                    &((), "unknown"),
                    MemberPayloadRefV1::Unit,
                    &mut MemberRecordWriteContext {
                        sink: &mut writer,
                        memory: &memory,
                        io: &mut logical(),
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 8192,
                        },
                        limits: limits(),
                        cancelled: || false,
                    },
                )
                .unwrap();
                modified.push((row.field_key.as_bytes().to_vec(), row.value_root));
            }
            _ => unreachable!(),
        }
        let bad = build_from(&store, &memory, old, original_rows, &modified);
        let mut writer = store.value_session(physical());
        let row = write_diplomacy_root_reference_v1(
            field,
            KEY,
            &bad,
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                limits: limits(),
                cancelled: || false,
            },
        )
        .unwrap();
        let mut outer = original.clone();
        *outer
            .iter_mut()
            .find(|(key, _)| key == row.field_key.as_bytes())
            .unwrap() = (row.field_key.as_bytes().to_vec(), row.value_root);
        let bad_outer = build_from(&store, &memory, &before, &original, &outer);
        let roots = roots(&bad_outer);
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = fixed!(&bad_outer, &state, &values, &memory, &mut io, &mut keys);
        assert!(
            matches!(
                qualify_diplomacy_family_v1(
                    &roots,
                    &mut ctx,
                    &mut scan(),
                    &mut scan(),
                    &mut 8192,
                    165
                ),
                Err(ValueTreeError::Invalid(_))
            ),
            "case {case}"
        );
    }
    // The valid saved root remains independently recoverable after every rejected candidate.
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = fixed!(&before, &state, &values, &memory, &mut io, &mut keys);
    assert_eq!(
        qualify_diplomacy_family_v1(
            &roots(&before),
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165
        )
        .unwrap()
        .record_counts(),
        [1; 4]
    );
}
