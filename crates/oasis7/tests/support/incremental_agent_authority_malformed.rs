use super::*;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
use std::collections::BTreeMap;
const NAME: &str = "agent_intent_ledger";
fn field() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: NAME,
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
fn semantic(token: &[u8]) -> Vec<u8> {
    let mut b = b"oasis7.iws.semantic-key.v1\0".to_vec();
    b.extend_from_slice(token);
    b
}
fn literal_key(name: &str, ns: StateNamespaceV1, kind: u8, token: Option<&[u8]>) -> Vec<u8> {
    let mut b = b"oasis7.iws.world-field-key.v1\0".to_vec();
    b.push(1);
    b.extend_from_slice(&(ns as u16).to_be_bytes());
    b.push(2);
    b.push(kind);
    b.extend_from_slice(&(name.len() as u16).to_be_bytes());
    b.extend_from_slice(name.as_bytes());
    b.extend_from_slice(
        &token
            .map(|t| *blake3::hash(&semantic(t)).as_bytes())
            .unwrap_or([0; 32]),
    );
    b
}
fn key(token: &[u8]) -> WorldFieldKeyV1 {
    let key = WorldFieldKeyV1::new(
        field(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic(token)).as_bytes()),
    )
    .unwrap();
    assert_eq!(
        key.as_bytes(),
        literal_key(NAME, StateNamespaceV1::Authorization, 3, Some(token))
    );
    key
}
fn text(s: &str) -> Vec<u8> {
    let mut b = vec![1];
    b.extend_from_slice(&(s.len() as u64).to_be_bytes());
    b.extend_from_slice(s.as_bytes());
    b
}
fn pair(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut b0 = vec![5];
    b0.extend_from_slice(a);
    b0.extend_from_slice(b);
    b0
}
fn column(name: &str) -> WorldFieldKeyV1 {
    key(&pair(&pair(&[0], &text("record")), &text(name)))
}
fn schema() -> [u8; 32] {
    let mut b = [0; 32];
    let s = b"oasis7.agent.intent-ledger.v1";
    b[..s.len()].copy_from_slice(s);
    b
}
macro_rules! member {
    ($store:expr,$memory:expr,$key:expr,$payload:expr) => {{
        let mut values = $store.value_session(physical_scope());
        write_member_record_v1(
            field(),
            WorldFieldRecordKindV1::Entry,
            &$key,
            $payload,
            &mut MemberRecordWriteContext {
                sink: &mut values,
                memory: $memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 4096,
                    record_bytes: 8192,
                },
                cancelled: || false,
            },
        )
        .unwrap()
    }};
}
pub(super) fn original_child(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
) -> (WorldFieldKeyV1, [u8; 32], StateRootV1) {
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let token = text(" outer intent ");
    let outer = key(&token);
    let root = roots
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap();
    let value = root.get(outer.as_bytes(), &state).unwrap().unwrap();
    let values = store.value_session(physical_scope());
    let mut out = [0; 165];
    read_member_record_v1(
        &value,
        &outer,
        &mut ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut logical(),
        },
        MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 165,
            record_bytes: 8192,
        },
        &mut std::io::sink(),
        &mut &mut out[..],
        &mut || false,
    )
    .unwrap();
    let reference = RecordRootRefV1::from_canonical_bytes(&out).unwrap();
    let child = reference
        .validate_binding(
            &schema(),
            blake3::hash(&semantic(&token)).as_bytes(),
            StateNamespaceV1::Authorization,
        )
        .unwrap()
        .clone();
    (outer, value, child)
}
fn child_changes(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    child: StateRootV1,
    changes: Vec<(WorldFieldKeyV1, Option<[u8; 32]>)>,
) -> StateRootV1 {
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let mut rows: Vec<_> = changes
        .into_iter()
        .map(|(key, after)| StateWriteV1 {
            before: child.get(key.as_bytes(), &state).unwrap(),
            key: key.as_bytes().to_vec(),
            after,
        })
        .collect();
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    let charge = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let delta = charge.retain(StateDeltaV1::build(child, rows, &state, 8 << 20).unwrap());
    let mut writer = store.value_session(physical_scope());
    writer.put_state_pages(&delta, memory).unwrap();
    delta.after.clone()
}
fn outer_reference(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    child: StateRootV1,
) -> MemberRecordV1 {
    let reference = RecordRootRefV1::new(
        schema(),
        *blake3::hash(&semantic(&text(" outer intent "))).as_bytes(),
        child,
    )
    .unwrap();
    member!(
        store,
        memory,
        " outer intent ",
        MemberPayloadRefV1::RecordRoot(&reference)
    )
}
fn changed_roots(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    outer: &WorldFieldKeyV1,
    value: [u8; 32],
) -> RootSetV1 {
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let index = roots
        .state
        .iter()
        .position(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap();
    let before = roots.state[index].get(outer.as_bytes(), &state).unwrap();
    let charge = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let delta = charge.retain(
        StateDeltaV1::build(
            roots.state[index].clone(),
            vec![StateWriteV1 {
                key: outer.as_bytes().to_vec(),
                before,
                after: Some(value),
            }],
            &state,
            8 << 20,
        )
        .unwrap(),
    );
    store
        .value_session(physical_scope())
        .put_state_pages(&delta, memory)
        .unwrap();
    let mut roots = roots.clone();
    roots.state[index] = delta.after.clone();
    roots
}
fn raw(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
) -> Result<ReservedValue<AgentAuthorityFamilyQualificationV1>, ValueTreeError> {
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical_scope());
    qualify_agent_authority_family_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(super::pages::state_error),
            cancelled: || false,
        },
        &mut work().scan,
        &mut work().scan,
        &mut 100_000,
        4096,
    )
}
fn swapped_leaf_paths(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    child: &StateRootV1,
) -> StateRootV1 {
    // Both complete authenticated leaf key/value pairs remain exactly original;
    // only their sparse-tree positions are swapped. Schema/cardinality alone
    // would accept this, so the receiver must authenticate the real key paths.
    let _path_admission = memory.reserve(512 << 10, MemoryClass::Ordinary).unwrap();
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let a = child
        .path(column("logical_time").as_bytes(), &state)
        .unwrap();
    let b = child.path(column("event_seq").as_bytes(), &state).unwrap();
    let a_leaf = a.nodes.last().unwrap();
    let b_leaf = b.nodes.last().unwrap();
    assert!(matches!(
        a_leaf.verified_links().unwrap(),
        StatePageLinksV1::Value(_)
    ));
    assert!(matches!(
        b_leaf.verified_links().unwrap(),
        StatePageLinksV1::Value(_)
    ));
    let mut replaced: BTreeMap<String, String> = [
        (a_leaf.digest.clone(), b_leaf.digest.clone()),
        (b_leaf.digest.clone(), a_leaf.digest.clone()),
    ]
    .into();
    let mut branches = BTreeMap::new();
    for node in a.nodes.iter().chain(&b.nodes) {
        if let StatePageLinksV1::Branch { depth, children } = node.verified_links().unwrap() {
            branches
                .entry(node.digest.clone())
                .or_insert((depth, children, node.bytes.clone()));
        }
    }
    let mut ordered: Vec<_> = branches.into_iter().collect();
    ordered.sort_by_key(|(_, (depth, _, _))| std::cmp::Reverse(*depth));
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut io = physical_scope();
    let mut never = || false;
    let mut writer = store
        .buffered_page_writer(&mut io, &cancel, &mut never)
        .unwrap();
    for (old, (depth, children, mut bytes)) in ordered {
        let mut changed = false;
        for (slot, kid) in children.into_iter().enumerate() {
            if let Some(new) = kid.and_then(|id| replaced.get(&id).cloned()) {
                let offset = b"oasis7.iws.state-page.v1\0".len() + 4 + 3 + slot * 32;
                bytes[offset..offset + 32]
                    .copy_from_slice(blake3::Hash::from_hex(&new).unwrap().as_bytes());
                changed = true;
            }
        }
        if changed {
            let digest = blake3::hash(&bytes);
            StateNodeWriteV1 {
                digest: digest.to_hex().to_string(),
                bytes: bytes.clone(),
            }
            .verified_links()
            .unwrap();
            let charge = memory.reserve(bytes.len(), MemoryClass::Ordinary).unwrap();
            writer
                .put_node(*digest.as_bytes(), charge.retain(bytes))
                .unwrap();
            replaced.insert(old, digest.to_hex().to_string());
        }
        assert!(depth < 256);
    }
    writer.finish().unwrap();
    let mut bad = child.clone();
    bad.node_root = replaced.remove(&child.node_root).unwrap();
    bad
}
#[test]
fn malformed_raw_and_changed_closures_are_rejected() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let dir = Directory::new();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let source = source();
    let roots = seed(&store, &memory, &source);
    let qualification = raw(&store, &memory, &roots).unwrap();
    assert_eq!(qualification.record_counts(), [1, 1, 1]);
    let (outer, _, child) = original_child(&store, &memory, &roots);
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    for case in 0..7 {
        let bad = match case {
            0 => {
                let row = member!(
                    &store,
                    &memory,
                    (((), "record"), "unknown_column"),
                    MemberPayloadRefV1::Unit
                );
                child_changes(
                    &store,
                    &memory,
                    child.clone(),
                    vec![(row.field_key, Some(row.value_root))],
                )
            }
            1 => {
                let row = member!(
                    &store,
                    &memory,
                    (((), "record"), "schema_version"),
                    MemberPayloadRefV1::U64(u64::MAX)
                );
                child_changes(
                    &store,
                    &memory,
                    child.clone(),
                    vec![(row.field_key, Some(row.value_root))],
                )
            }
            2 => child_changes(&store, &memory, child.clone(), vec![(key(&[0]), None)]),
            3 => child_changes(
                &store,
                &memory,
                child.clone(),
                vec![(column("target_id"), None)],
            ),
            4 => {
                let row = member!(
                    &store,
                    &memory,
                    (((), "record"), "summary"),
                    MemberPayloadRefV1::Bool(false)
                );
                child_changes(
                    &store,
                    &memory,
                    child.clone(),
                    vec![(row.field_key, Some(row.value_root))],
                )
            }
            5 => {
                let mut bad = child.clone();
                bad.leaf_count += 1;
                bad
            }
            _ => swapped_leaf_paths(&store, &memory, &child),
        };
        let reference = outer_reference(&store, &memory, bad);
        let bad_roots = changed_roots(&store, &memory, &roots, &outer, reference.value_root);
        let raw_result = raw(&store, &memory, &bad_roots);
        assert!(
            matches!(raw_result, Err(ValueTreeError::Invalid(_))),
            "raw scenario={case}: {raw_result:?}"
        );
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [0x93; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 32,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        tx.write(
            StateNamespaceV1::Authorization,
            outer.as_bytes(),
            Some(reference.value_root),
        )
        .unwrap();
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let generic = TransitionVerifier {
            parent: &parent,
            operation: [0x93; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        }
        .verify(&sealed.record)
        .unwrap();
        assert_eq!(generic.record.after, sealed.record.after);
        drop(generic);
        let values = store.value_session(physical_scope());
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [0x93; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        let typed = accept_agent_authority_transition_v1(
            &qualification,
            &verifier,
            &sealed.record,
            &mut WorldFieldReadContextV1 {
                root: &sealed.record.after.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 16 << 20,
                },
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(super::pages::state_error),
                cancelled: || false,
            },
            &mut work().scan,
            &mut 100_000,
            4096,
        );
        assert!(
            matches!(
                typed,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ),
            "typed scenario={case}"
        );
        eprintln!("agent authority malformed scenario={case}: generic accepted, raw/typed refused");
    }
}
