//! Independent schema/failure/constant-delta acceptance boundaries.
use super::*;
use oasis7_distfs::incremental_page_store::PageStoreError;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
fn state_error(e: PageStoreError) -> ValueTreeError {
    match e {
        PageStoreError::ResourceLimited(v) => ValueTreeError::ResourceLimited(v),
        PageStoreError::Memory(v) => ValueTreeError::Memory(v),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(v) => ValueTreeError::Io(v),
        _ => ValueTreeError::Invalid("material boundary state"),
    }
}
macro_rules! read_ctx {
    ($root:expr,$state:expr,$values:expr,$memory:expr,$class:expr,$io:expr,$keys:expr) => {
        WorldFieldReadContextV1 {
            root: $root,
            state: $state,
            values: ValueReadContext {
                source: $values,
                memory: $memory,
                class: $class,
                io: $io,
            },
            key_work: $keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || $state.take_read_error().map(state_error),
            cancelled: || false,
        }
    };
}
macro_rules! write_ctx {
    ($writer:expr,$memory:expr,$io:expr,$keys:expr) => {
        MemberRecordWriteContext {
            sink: $writer,
            memory: $memory,
            io: $io,
            key_work: $keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 165,
                record_bytes: 4096,
            },
            cancelled: || false,
        }
    };
}
fn field(kind: MaterialRecordKindV1) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: kind.field_name(),
        shape: FieldShape::Map,
        destination: FieldDestination::State(kind.namespace()),
    }
}
fn parent(roots: RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots,
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    }
}
fn semantic(key: MaterialRecordKeyV1<'_>) -> [u8; 32] {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    match key {
        MaterialRecordKeyV1::Profile(v) => {
            bytes.push(1);
            bytes.extend_from_slice(&(v.len() as u64).to_be_bytes());
            bytes.extend_from_slice(v.as_bytes());
        }
        MaterialRecordKeyV1::Transfer(v) => {
            bytes.push(2);
            bytes.extend_from_slice(&v.to_be_bytes());
        }
    }
    *blake3::hash(&bytes).as_bytes()
}
fn edit(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    root: &StateRootV1,
    key: &WorldFieldKeyV1,
    after: Option<[u8; 32]>,
) -> StateRootV1 {
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let before = root.get(key.as_bytes(), &state).unwrap();
    let charge = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let delta = charge.retain(
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
        .value_session(physical_scope())
        .put_state_pages(&delta, memory)
        .unwrap();
    delta.after.clone()
}
fn raw_cell(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    kind: MaterialRecordKindV1,
    key: &((), &str),
    value: MemberPayloadRefV1<'_>,
) -> MemberRecordV1 {
    write_member_record_v1(
        field(kind),
        WorldFieldRecordKindV1::Entry,
        key,
        value,
        &mut write_ctx!(
            &mut store.value_session(physical_scope()),
            memory,
            &mut logical(),
            &mut SemanticKeyBudget {
                bytes_remaining: 8192
            }
        ),
    )
    .unwrap()
}
fn outer(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    key: MaterialRecordKeyV1<'_>,
    child: StateRootV1,
) -> MemberRecordV1 {
    let reference = RecordRootRefV1::new(key.kind().schema(), semantic(key), child).unwrap();
    let mut writer = store.value_session(physical_scope());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8192,
    };
    let mut ctx = write_ctx!(&mut writer, memory, &mut io, &mut keys);
    match key {
        MaterialRecordKeyV1::Profile(id) => write_member_record_v1(
            field(key.kind()),
            WorldFieldRecordKindV1::Entry,
            id,
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut ctx,
        ),
        MaterialRecordKeyV1::Transfer(id) => write_member_record_v1(
            field(key.kind()),
            WorldFieldRecordKindV1::Entry,
            &id,
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut ctx,
        ),
    }
    .unwrap()
}
#[test]
fn material_generic_receipts_do_not_qualify_malformed_owner_bodies() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("bad"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let parent = parent(seed(&store, &memory, &source()));
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical_scope());
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
    let family = qualify_material_family_v1(
        &parent.roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut 8192,
        165,
    )
    .unwrap();
    let mut children = Vec::new();
    for key in [
        MaterialRecordKeyV1::Profile(PROFILE),
        MaterialRecordKeyV1::Transfer(0),
    ] {
        ctx.root = parent
            .roots
            .state
            .iter()
            .find(|r| r.namespace == key.kind().namespace())
            .unwrap();
        children.push(
            inspect_material_record_root_v1(key, &mut ctx)
                .unwrap()
                .unwrap(),
        );
    }
    for case in 0..6 {
        let key = if case == 2 {
            MaterialRecordKeyV1::Transfer(0)
        } else {
            MaterialRecordKeyV1::Profile(PROFILE)
        };
        let old: &StateRootV1 = &children[usize::from(case == 2)];
        let mut bad = old.clone();
        match case {
            0 => {
                let row = raw_cell(
                    &store,
                    &memory,
                    key.kind(),
                    &((), "unknown"),
                    MemberPayloadRefV1::Unit,
                );
                bad = edit(&store, &memory, old, &row.field_key, Some(row.value_root));
                bad.leaf_count = old.leaf_count;
            }
            1 => {
                let row = raw_cell(
                    &store,
                    &memory,
                    key.kind(),
                    &((), "tier"),
                    MemberPayloadRefV1::U64(255),
                );
                bad = edit(&store, &memory, old, &row.field_key, None);
            }
            2 => {
                let row = raw_cell(
                    &store,
                    &memory,
                    key.kind(),
                    &((), "route_id"),
                    MemberPayloadRefV1::Text(""),
                );
                bad = edit(&store, &memory, old, &row.field_key, Some(row.value_root));
            }
            3 => bad.leaf_count += 1,
            4 => {
                let state = store
                    .state_session(physical_scope(), MemoryClass::Ordinary)
                    .unwrap();
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
                let id = *blake3::hash(&bytes).as_bytes();
                bad.node_root = blake3::hash(&bytes).to_hex().to_string();
                StateNodeWriteV1 {
                    digest: bad.node_root.clone(),
                    bytes: bytes.clone(),
                }
                .verified_links()
                .unwrap();
                store
                    .value_session(physical_scope())
                    .put_node(
                        id,
                        memory
                            .reserve(bytes.len(), MemoryClass::Ordinary)
                            .unwrap()
                            .retain(bytes),
                    )
                    .unwrap();
            }
            5 => {
                let row = raw_cell(
                    &store,
                    &memory,
                    key.kind(),
                    &((), "tier"),
                    MemberPayloadRefV1::U64(256),
                );
                bad = edit(&store, &memory, old, &row.field_key, Some(row.value_root));
            }
            _ => unreachable!(),
        }
        let row = outer(&store, &memory, key, bad.clone());
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
        tx.write(
            key.kind().namespace(),
            row.field_key.as_bytes(),
            Some(row.value_root),
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
            .expect("outer Merkle receipt is genuinely generic-valid");
        for node in &verified.new_nodes {
            store
                .value_session(physical_scope())
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
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let values2 = store.value_session(physical_scope());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            &bad,
            &state2,
            &values2,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        assert!(
            matches!(
                qualify_material_body_v1(
                    key.kind(),
                    semantic(key),
                    &mut ctx,
                    &mut scan(),
                    &mut 1000,
                    165
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
                accept_material_transition_v1(
                    &family,
                    &verifier,
                    &sealed.record,
                    &mut ctx,
                    &mut scan(),
                    &mut 8192,
                    165
                ),
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ),
            "typed case{case}"
        );
    }
}
