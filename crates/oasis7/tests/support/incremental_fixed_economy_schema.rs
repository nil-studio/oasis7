//! Generic receipts authenticate bytes, not the required fixed-economy schema.
use super::*;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
fn edit(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    root: &StateRootV1,
    key: &WorldFieldKeyV1,
    after: Option<[u8; 32]>,
) -> StateRootV1 {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let before = root.get(key.as_bytes(), &state).unwrap();
    let delta = memory
        .reserve(1 << 20, MemoryClass::Ordinary)
        .unwrap()
        .retain(
            StateDeltaV1::build(
                root.clone(),
                vec![StateWriteV1 {
                    key: key.as_bytes().to_vec(),
                    before,
                    after,
                }],
                &state,
                2 << 20,
            )
            .unwrap(),
        );
    store
        .value_session(physical())
        .put_state_pages(&delta, memory)
        .unwrap();
    delta.after.clone()
}
fn cell(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    name: &str,
    payload: MemberPayloadRefV1<'_>,
) -> MemberRecordV1 {
    write_member_record_v1(
        descriptor(FixedEconomyFieldV1::MainTokenSupply),
        WorldFieldRecordKindV1::Entry,
        &((), name),
        payload,
        &mut MemberRecordWriteContext {
            sink: &mut store.value_session(physical()),
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8192,
            },
            limits: limits(),
            cancelled: || false,
        },
    )
    .unwrap()
}
#[test]
fn generic_valid_receipts_cannot_qualify_missing_unknown_wrong_codec_or_required_global_deletion() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("schema"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let parent = ExecutionParent {
        roots: seed(&store, &memory),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = read_ctx!(
        &parent.roots.state[0],
        &state,
        &values,
        &memory,
        MemoryClass::Ordinary,
        &mut io,
        &mut keys
    );
    let family = qualify_fixed_economy_family_v1(
        &parent.roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut 8192,
    )
    .unwrap();
    ctx.root = parent
        .roots
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .unwrap();
    let field = FixedEconomyFieldV1::MainTokenSupply;
    let old = inspect_fixed_economy_root_v1(field, &mut ctx).unwrap();
    for case in 0..7 {
        let mut bad = old.clone();
        match case {
            0 => {
                let row = cell(&store, &memory, "unknown", MemberPayloadRefV1::Unit);
                bad = edit(&store, &memory, &old, &row.field_key, Some(row.value_root));
                bad.leaf_count = old.leaf_count;
            }
            1 => {
                let row = cell(
                    &store,
                    &memory,
                    "total_supply",
                    MemberPayloadRefV1::U64(u64::MAX),
                );
                bad = edit(&store, &memory, &old, &row.field_key, None);
            }
            2 => bad.leaf_count += 1,
            3 => {
                let row = cell(&store, &memory, "total_supply", MemberPayloadRefV1::I64(-1));
                bad = edit(&store, &memory, &old, &row.field_key, Some(row.value_root));
            }
            4 => {
                let page = state
                    .read_node(&old.node_root, STATE_MAX_PAGE_BYTES)
                    .unwrap()
                    .unwrap();
                let mut bytes = page.as_ref().to_vec();
                let offset = b"oasis7.iws.state-page.v1\0".len() + 4 + 3;
                let left: [u8; 32] = bytes[offset..offset + 32].try_into().unwrap();
                let right: [u8; 32] = bytes[offset + 32..offset + 64].try_into().unwrap();
                assert_ne!(left, right);
                bytes[offset..offset + 32].copy_from_slice(&right);
                bytes[offset + 32..offset + 64].copy_from_slice(&left);
                bad.node_root = blake3::hash(&bytes).to_hex().to_string();
                StateNodeWriteV1 {
                    digest: bad.node_root.clone(),
                    bytes: bytes.clone(),
                }
                .verified_links()
                .unwrap();
                store
                    .value_session(physical())
                    .put_node(
                        *blake3::hash(&bytes).as_bytes(),
                        memory
                            .reserve(bytes.len(), MemoryClass::Ordinary)
                            .unwrap()
                            .retain(bytes),
                    )
                    .unwrap();
            }
            5 | 6 => {}
            _ => unreachable!(),
        }
        let reference = RecordRootRefV1::new(
            field.schema(),
            *blake3::hash(b"oasis7.iws.semantic-key.v1\0\0").as_bytes(),
            bad.clone(),
        )
        .unwrap();
        let mut writer = store.value_session(physical());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 8192,
        };
        let row = write_member_record_v1(
            descriptor(field),
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: limits(),
                cancelled: || false,
            },
        )
        .unwrap();
        let metadata =
            WorldFieldKeyV1::new(descriptor(field), WorldFieldRecordKindV1::Metadata, None)
                .unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 16,
                max_writes: 4,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let guard = tx.guard();
        tx.validate_guard(&guard, &live).unwrap();
        let key = if case == 6 {
            metadata.as_bytes()
        } else {
            row.field_key.as_bytes()
        };
        tx.write(
            field.namespace(),
            key,
            if case >= 5 {
                None
            } else {
                Some(row.value_root)
            },
        )
        .unwrap();
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let verified = verifier
            .verify(&sealed.record)
            .expect("genuinely valid generic receipt");
        for node in &verified.new_nodes {
            writer
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        let state2 = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values2 = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            &sealed.record.after.state[0],
            &state2,
            &values2,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        assert!(
            matches!(
                qualify_fixed_economy_family_v1(
                    &sealed.record.after,
                    &mut ctx,
                    &mut scan(),
                    &mut scan(),
                    &mut 8192
                ),
                Err(ValueTreeError::Invalid(_))
            ),
            "raw case{case}"
        );
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            &sealed.record.after.state[0],
            &state2,
            &values2,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        assert!(
            matches!(
                accept_fixed_economy_transition_v1(
                    &family,
                    &verifier,
                    &sealed.record,
                    &mut ctx,
                    &mut 8192
                ),
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ),
            "typed case{case}"
        );
    }
}
