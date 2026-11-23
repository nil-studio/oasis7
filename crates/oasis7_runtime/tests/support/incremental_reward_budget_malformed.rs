use super::*;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
fn field(kind: RewardRecordKindV1) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: kind.name(),
        shape: kind.shape(),
        destination: FieldDestination::State(kind.namespace()),
    }
}
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
macro_rules! cell {
    ($store:expr,$memory:expr,$kind:expr,$key:expr,$value:expr $(,)?) => {{
        write_member_record_v1(
            field($kind),
            WorldFieldRecordKindV1::Entry,
            $key,
            $value,
            &mut MemberRecordWriteContext {
                sink: &mut ($store).value_session(physical()),
                memory: $memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                limits: limits(),
                cancelled: || false,
            },
        )
        .unwrap()
    }};
}
fn outer(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    key: RewardRecordKeyV1,
    child: StateRootV1,
) -> MemberRecordV1 {
    let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
    semantic.push(2);
    semantic.extend_from_slice(&key.index.to_be_bytes());
    let reference = RecordRootRefV1::new(
        key.kind.schema(),
        *blake3::hash(&semantic).as_bytes(),
        child,
    )
    .unwrap();
    cell!(
        store,
        memory,
        key.kind,
        &key.index,
        MemberPayloadRefV1::RecordRoot(&reference),
    )
}
#[test]
fn generic_valid_reward_receipts_reject_orphans_wrong_columns_and_non_dense_ordinals() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("bad"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let parent = ExecutionParent {
        roots: seed(&store, &memory, &source()),
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
    let mut context = ctx!(
        &parent.roots.state[0],
        &state,
        &values,
        &memory,
        MemoryClass::Ordinary,
        &mut io,
        &mut keys
    );
    let family = qualify_reward_family_v1(
        &parent.roots,
        &mut context,
        &mut scan(),
        &mut scan(),
        &mut 8192,
        165,
    )
    .unwrap();
    let pool = RewardRecordKeyV1 {
        kind: RewardRecordKindV1::Pool,
        index: 7,
    };
    let mint = RewardRecordKeyV1 {
        kind: RewardRecordKindV1::Mint,
        index: 0,
    };
    context.root = parent
        .roots
        .state
        .iter()
        .find(|r| r.namespace == pool.kind.namespace())
        .unwrap();
    let pool_root = inspect_reward_record_root_v1(pool, &mut context)
        .unwrap()
        .unwrap();
    context.root = parent
        .roots
        .state
        .iter()
        .find(|r| r.namespace == mint.kind.namespace())
        .unwrap();
    let mint_root = inspect_reward_record_root_v1(mint, &mut context)
        .unwrap()
        .unwrap();
    for case in 0..12 {
        let mut mutations = Vec::<(StateNamespaceV1, WorldFieldKeyV1, Option<[u8; 32]>)>::new();
        if matches!(case,0..=3|9..=11) {
            let mut bad = (*pool_root).clone();
            match case {
                0 => {
                    let row = cell!(
                        &store,
                        &memory,
                        pool.kind,
                        &((), "unknown"),
                        MemberPayloadRefV1::Unit,
                    );
                    bad = edit(&store, &memory, &bad, &row.field_key, Some(row.value_root));
                    bad.leaf_count = pool_root.leaf_count;
                }
                1 => {
                    let row = cell!(
                        &store,
                        &memory,
                        pool.kind,
                        &((), "remaining_credit_budget"),
                        MemberPayloadRefV1::U64(u64::MAX),
                    );
                    bad = edit(&store, &memory, &bad, &row.field_key, None);
                }
                2 => {
                    let row = cell!(
                        &store,
                        &memory,
                        pool.kind,
                        &(((), "node_credit_caps"), ()),
                        MemberPayloadRefV1::Unit,
                    );
                    bad = edit(&store, &memory, &bad, &row.field_key, None);
                }
                3 => {
                    let row = cell!(
                        &store,
                        &memory,
                        pool.kind,
                        &(((), "node_credit_caps"), ""),
                        MemberPayloadRefV1::Text("wrong"),
                    );
                    bad = edit(&store, &memory, &bad, &row.field_key, Some(row.value_root));
                }
                9 => bad.leaf_count += 1,
                10 => {
                    let page = state
                        .read_node(&bad.node_root, STATE_MAX_PAGE_BYTES)
                        .unwrap()
                        .unwrap();
                    let mut bytes = page.as_ref().to_vec();
                    let at = b"oasis7.iws.state-page.v1\0".len() + 4 + 3;
                    let left: [u8; 32] = bytes[at..at + 32].try_into().unwrap();
                    let right: [u8; 32] = bytes[at + 32..at + 64].try_into().unwrap();
                    assert_ne!(left, right);
                    bytes[at..at + 32].copy_from_slice(&right);
                    bytes[at + 32..at + 64].copy_from_slice(&left);
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
                11 => {
                    let wrong = cell!(
                        &store,
                        &memory,
                        pool.kind,
                        &(((), "node_credit_caps"), "other"),
                        MemberPayloadRefV1::U64(7),
                    );
                    let expected = cell!(
                        &store,
                        &memory,
                        pool.kind,
                        &(((), "node_credit_caps"), ""),
                        MemberPayloadRefV1::U64(0),
                    );
                    bad = edit(
                        &store,
                        &memory,
                        &bad,
                        &expected.field_key,
                        Some(wrong.value_root),
                    );
                }
                _ => unreachable!(),
            };
            let row = outer(&store, &memory, pool, bad);
            mutations.push((pool.kind.namespace(), row.field_key, Some(row.value_root)));
        } else {
            if case == 4 || case == 6 || case == 7 || case == 8 {
                let length = if case == 4 || case == 8 { 4 } else { 2 };
                let row = cell!(
                    &store,
                    &memory,
                    RewardRecordKindV1::Mint,
                    &("reward_budget_record_closure_v1", "length"),
                    MemberPayloadRefV1::U64(length),
                );
                mutations.push((
                    StateNamespaceV1::Authorization,
                    row.field_key,
                    Some(row.value_root),
                ));
            }
            if case == 5 || case == 8 {
                let key = RewardRecordKeyV1 {
                    kind: RewardRecordKindV1::Mint,
                    index: if case == 5 { 3 } else { 4 },
                };
                let row = outer(&store, &memory, key, (*mint_root).clone());
                mutations.push((key.kind.namespace(), row.field_key, Some(row.value_root)));
            }
            if case == 6 || case == 7 {
                let row = outer(
                    &store,
                    &memory,
                    RewardRecordKeyV1 {
                        kind: RewardRecordKindV1::Mint,
                        index: if case == 6 { 1 } else { 2 },
                    },
                    (*mint_root).clone(),
                );
                mutations.push((StateNamespaceV1::Authorization, row.field_key, None));
            }
        }
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 32,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let guard = tx.guard();
        tx.validate_guard(&guard, &live).unwrap();
        for (ns, key, value) in mutations {
            tx.write(ns, key.as_bytes(), value).unwrap();
        }
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
            .expect("genuinely generic-valid outer receipt");
        for node in &verified.new_nodes {
            store
                .value_session(physical())
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
        let mut context = ctx!(
            &sealed.record.after.state[0],
            &state2,
            &values2,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let raw = qualify_reward_family_v1(
            &sealed.record.after,
            &mut context,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165,
        );
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut context = ctx!(
            &sealed.record.after.state[0],
            &state2,
            &values2,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let typed = accept_reward_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut context,
            &mut scan(),
            &mut 8192,
            165,
        );
        if case == 7 {
            let raw = raw.unwrap();
            assert_eq!(raw.record_counts(), [2, 2]);
            assert_eq!(raw.mint_length(), 2);
            let typed = typed.unwrap();
            assert_eq!(typed.record_counts(), [2, 2]);
            assert_eq!(typed.mint_length(), 2);
        } else {
            assert!(
                matches!(raw, Err(ValueTreeError::Invalid(_))),
                "raw case{case}: {raw:?}"
            );
            assert!(
                matches!(
                    typed,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
                ),
                "typed case{case}: {typed:?}"
            );
        }
    }
}
