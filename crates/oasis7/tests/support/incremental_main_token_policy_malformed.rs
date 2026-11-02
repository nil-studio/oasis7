use super::*;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
fn field(kind: MainTokenPolicyKindV1) -> FieldDescriptor {
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
    key: MainTokenPolicyKeyV1<'_>,
    child: StateRootV1,
) -> MemberRecordV1 {
    let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
    match key {
        MainTokenPolicyKeyV1::Config => semantic.push(0),
        MainTokenPolicyKeyV1::Bucket(id) | MainTokenPolicyKeyV1::Grant(id) => {
            semantic.push(1);
            semantic.extend_from_slice(&(id.len() as u64).to_be_bytes());
            semantic.extend_from_slice(id.as_bytes());
        }
        MainTokenPolicyKeyV1::Issuance(n) | MainTokenPolicyKeyV1::Scheduled(n) => {
            semantic.push(2);
            semantic.extend_from_slice(&n.to_be_bytes());
        }
    }
    let reference = RecordRootRefV1::new(
        key.kind().schema(),
        *blake3::hash(&semantic).as_bytes(),
        child,
    )
    .unwrap();
    match key {
        MainTokenPolicyKeyV1::Config => cell!(
            store,
            memory,
            key.kind(),
            &(),
            MemberPayloadRefV1::RecordRoot(&reference)
        ),
        MainTokenPolicyKeyV1::Bucket(id) | MainTokenPolicyKeyV1::Grant(id) => cell!(
            store,
            memory,
            key.kind(),
            id,
            MemberPayloadRefV1::RecordRoot(&reference)
        ),
        MainTokenPolicyKeyV1::Issuance(n) | MainTokenPolicyKeyV1::Scheduled(n) => cell!(
            store,
            memory,
            key.kind(),
            &n,
            MemberPayloadRefV1::RecordRoot(&reference)
        ),
    }
}
#[test]
fn generic_valid_policy_receipts_require_closed_fields_original_keys_and_exact_domains() {
    use MainTokenPolicyKeyV1 as K;
    use MainTokenPolicyKindV1 as Kind;
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
    let bootstrap_state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let bootstrap_values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = ctx!(
        &parent.roots.state[0],
        &bootstrap_state,
        &bootstrap_values,
        &memory,
        MemoryClass::Ordinary,
        &mut io,
        &mut keys
    );
    let family = qualify_main_token_policy_family_v1(
        &parent.roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut 8192,
        165,
    )
    .unwrap();
    let mut children = Vec::with_capacity(5);
    for key in [
        K::Config,
        K::Bucket(""),
        K::Issuance(7),
        K::Grant(" raw grant "),
        K::Scheduled(9),
    ] {
        ctx.root = parent
            .roots
            .state
            .iter()
            .find(|r| r.namespace == key.kind().namespace())
            .unwrap();
        children.push(
            inspect_main_token_policy_record_root_v1(key, &mut ctx)
                .unwrap()
                .unwrap(),
        );
    }
    for case in 0..16 {
        let (key, slot) = match case {
            5 | 6 => (K::Grant(" raw grant "), 3),
            7 => (K::Issuance(7), 2),
            8 => (K::Bucket(""), 1),
            9 | 10 => (K::Scheduled(9), 4),
            _ => (K::Config, 0),
        };
        let kind = key.kind();
        let mut bad = (*children[slot]).clone();
        match case {
            0 => {
                let row = cell!(
                    &store,
                    &memory,
                    kind,
                    &((), "unknown"),
                    MemberPayloadRefV1::Unit
                );
                bad = edit(&store, &memory, &bad, &row.field_key, Some(row.value_root));
                bad.leaf_count = children[slot].leaf_count;
            }
            1 => {
                let row = cell!(
                    &store,
                    &memory,
                    kind,
                    &((), "max_supply"),
                    MemberPayloadRefV1::Unit
                );
                bad = edit(&store, &memory, &bad, &row.field_key, None);
            }
            2 => {
                let row = cell!(
                    &store,
                    &memory,
                    kind,
                    &((), "decimals"),
                    MemberPayloadRefV1::U64(256)
                );
                bad = edit(&store, &memory, &bad, &row.field_key, Some(row.value_root));
            }
            3 => {
                let row = cell!(
                    &store,
                    &memory,
                    kind,
                    &(((), "inflation_policy"), "base_rate_bps"),
                    MemberPayloadRefV1::U64(u64::from(u32::MAX) + 1)
                );
                bad = edit(&store, &memory, &bad, &row.field_key, Some(row.value_root));
            }
            4 => {
                let row = cell!(
                    &store,
                    &memory,
                    kind,
                    &((), "max_supply"),
                    MemberPayloadRefV1::Text("wrong")
                );
                bad = edit(&store, &memory, &bad, &row.field_key, Some(row.value_root));
            }
            5 => {
                let row = cell!(
                    &store,
                    &memory,
                    kind,
                    &((), "status"),
                    MemberPayloadRefV1::Text("unknown status")
                );
                bad = edit(&store, &memory, &bad, &row.field_key, Some(row.value_root));
            }
            6 => {
                let row = cell!(
                    &store,
                    &memory,
                    kind,
                    &((), "status_reason"),
                    MemberPayloadRefV1::Bool(false)
                );
                bad = edit(&store, &memory, &bad, &row.field_key, Some(row.value_root));
            }
            7 => bad.leaf_count += 1,
            8 => {
                let page = bootstrap_state
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
            9 => {
                let row = cell!(
                    &store,
                    &memory,
                    kind,
                    &(((), "next_config"), "symbol"),
                    MemberPayloadRefV1::Text("")
                );
                bad = edit(&store, &memory, &bad, &row.field_key, None);
            }
            10 => {
                let row = cell!(
                    &store,
                    &memory,
                    kind,
                    &(((), "next_config"), "max_supply"),
                    MemberPayloadRefV1::I64(-1)
                );
                bad = edit(&store, &memory, &bad, &row.field_key, Some(row.value_root));
            }
            11 | 12 => {}
            13 => bad = (*children[3]).clone(),
            14 => {
                let row = cell!(
                    &store,
                    &memory,
                    kind,
                    &((), "max_supply"),
                    MemberPayloadRefV1::Unit
                );
                bad = edit(&store, &memory, &bad, &row.field_key, Some(row.value_root));
            }
            15 => {
                let wrong = cell!(
                    &store,
                    &memory,
                    kind,
                    &((), "other"),
                    MemberPayloadRefV1::U64(255)
                );
                let expected = cell!(
                    &store,
                    &memory,
                    kind,
                    &((), "decimals"),
                    MemberPayloadRefV1::U64(255)
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
        }
        let row = if case == 11 {
            let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
            semantic.push(1);
            semantic.extend_from_slice(&8u64.to_be_bytes());
            semantic.extend_from_slice(b"not-unit");
            let reference = RecordRootRefV1::new(
                Kind::Config.schema(),
                *blake3::hash(&semantic).as_bytes(),
                bad,
            )
            .unwrap();
            cell!(
                &store,
                &memory,
                Kind::Config,
                "not-unit",
                MemberPayloadRefV1::RecordRoot(&reference)
            )
        } else {
            outer(&store, &memory, key, bad)
        };
        // Each variant is a separate operation with its own finite provider session.
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
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
        tx.write(
            kind.namespace(),
            row.field_key.as_bytes(),
            if case == 12 {
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
        let generic = verifier
            .verify(&sealed.record)
            .expect("actual generic-valid opaque child mutation");
        for node in &generic.new_nodes {
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
        let raw_state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let raw_values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = ctx!(
            &sealed.record.after.state[0],
            &raw_state,
            &raw_values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let raw = qualify_main_token_policy_family_v1(
            &sealed.record.after,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165,
        );
        let receiver_state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let receiver_values = store.value_session(physical());
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &receiver_state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = ctx!(
            &sealed.record.after.state[0],
            &receiver_state,
            &receiver_values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let typed = accept_main_token_policy_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut ctx,
            &mut scan(),
            &mut 8192,
            165,
        );
        if case == 14 {
            assert_eq!(raw.unwrap().record_counts(), [1, 1, 1, 2, 1]);
            assert_eq!(typed.unwrap().record_counts(), [1, 1, 1, 2, 1]);
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
        eprintln!("generic-valid policy case{case} validated");
    }
}
