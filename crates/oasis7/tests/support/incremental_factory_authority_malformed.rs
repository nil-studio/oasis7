use super::*;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use std::collections::BTreeMap;
const NAME: &str = "factory_site_authorities";
fn field() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: NAME,
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Business),
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
        literal_key(NAME, StateNamespaceV1::Business, 3, Some(token))
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
    key(&pair(&[0], &text(name)))
}
fn schema() -> [u8; 32] {
    let mut bytes = b"oasis7.iws.factory-authority.v1\0".to_vec();
    bytes.extend_from_slice(b"factory_site_authorities");
    *blake3::hash(&bytes).as_bytes()
}
macro_rules! member {
    ($store:expr,$memory:expr,$key:expr,$payload:expr) => {
        member!($store, $memory, field(), $key, $payload)
    };
    ($store:expr,$memory:expr,$field:expr,$key:expr,$payload:expr) => {{
        let mut values = $store.value_session(physical_scope());
        write_member_record_v1(
            $field,
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
    let token = text(" outer-site\0界 ");
    let outer = key(&token);
    let root = roots
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Business)
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
            StateNamespaceV1::Business,
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
        *blake3::hash(&semantic(&text(" outer-site\0界 "))).as_bytes(),
        child,
    )
    .unwrap();
    member!(
        store,
        memory,
        " outer-site\0界 ",
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
        .position(|r| r.namespace == StateNamespaceV1::Business)
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
        .path(column("authority_revision").as_bytes(), &state)
        .unwrap();
    let b = child
        .path(column("registered_at").as_bytes(), &state)
        .unwrap();
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

pub(super) fn raw(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
) -> Result<ReservedValue<FactoryAuthorityFamilyQualificationV1>, ValueTreeError> {
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical_scope());
    qualify_factory_authority_family_v1(
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
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        },
        &mut scan(),
        &mut scan(),
        &mut 8192,
        4096,
    )
}
fn member_key(index: u64) -> WorldFieldKeyV1 {
    let mut number = vec![2];
    number.extend_from_slice(&index.to_be_bytes());
    key(&pair(&pair(&[0], &text("authorized_agent_ids")), &number))
}
#[test]
fn generic_valid_bad_factory_children_fail_raw_and_typed_but_tail_truncation_passes() {
    use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let directory = Directory::new();
    let store = IncrementalPageStore::create(
        &directory.0.join("malformed"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let top = seed(&source(), &store, &memory);
    let mut roots = RootSetV1::empty("factory-authority-malformed".into(), [0x81; 32]).unwrap();
    roots.state[0] = top;
    let qualification = raw(&store, &memory, &roots).unwrap();
    assert_eq!(qualification.record_counts(), [1, 3]);
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
    for case in 0..12 {
        let bad = match case {
            0 => {
                let row = member!(
                    &store,
                    &memory,
                    ((), "unknown_column"),
                    MemberPayloadRefV1::Unit
                );
                let mut bad = child_changes(
                    &store,
                    &memory,
                    child.clone(),
                    vec![(row.field_key, Some(row.value_root))],
                );
                bad.leaf_count = child.leaf_count;
                bad
            }
            1 => child_changes(&store, &memory, child.clone(), vec![(key(&[0]), None)]),
            2 => child_changes(
                &store,
                &memory,
                child.clone(),
                vec![(column("active"), None)],
            ),
            3 => {
                let row = member!(
                    &store,
                    &memory,
                    ((), "authorized_agent_count"),
                    MemberPayloadRefV1::U64(4)
                );
                child_changes(
                    &store,
                    &memory,
                    child.clone(),
                    vec![(row.field_key, Some(row.value_root))],
                )
            }
            4 => {
                let count = member!(
                    &store,
                    &memory,
                    ((), "authorized_agent_count"),
                    MemberPayloadRefV1::U64(4)
                );
                let gap = member!(
                    &store,
                    &memory,
                    (((), "authorized_agent_ids"), 4u64),
                    MemberPayloadRefV1::Text("gap")
                );
                child_changes(
                    &store,
                    &memory,
                    child.clone(),
                    vec![
                        (count.field_key, Some(count.value_root)),
                        (gap.field_key, Some(gap.value_root)),
                    ],
                )
            }
            5 | 6 => {
                let count = member!(
                    &store,
                    &memory,
                    ((), "authorized_agent_count"),
                    MemberPayloadRefV1::U64(2)
                );
                let removed = if case == 5 { 1 } else { 2 };
                child_changes(
                    &store,
                    &memory,
                    child.clone(),
                    vec![
                        (count.field_key, Some(count.value_root)),
                        (member_key(removed), None),
                    ],
                )
            }
            7 => {
                let row = member!(
                    &store,
                    &memory,
                    ((), "authority_revision"),
                    MemberPayloadRefV1::I64(-1)
                );
                child_changes(
                    &store,
                    &memory,
                    child.clone(),
                    vec![(row.field_key, Some(row.value_root))],
                )
            }
            8 => {
                let mut bad = child.clone();
                bad.leaf_count += 1;
                bad
            }
            9 => {
                let wrong = member!(
                    &store,
                    &memory,
                    ((), "registered_at"),
                    MemberPayloadRefV1::U64(17)
                );
                child_changes(
                    &store,
                    &memory,
                    child.clone(),
                    vec![(column("authority_revision"), Some(wrong.value_root))],
                )
            }
            10 => swapped_leaf_paths(&store, &memory, &child),
            _ => child_changes(
                &store,
                &memory,
                child.clone(),
                vec![(
                    WorldFieldKeyV1::new(field(), WorldFieldRecordKindV1::Metadata, None).unwrap(),
                    None,
                )],
            ),
        };
        let reference = outer_reference(&store, &memory, bad);
        let candidate = changed_roots(&store, &memory, &roots, &outer, reference.value_root);
        let raw_result = raw(&store, &memory, &candidate);
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [0x82; 32],
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
            StateNamespaceV1::Business,
            outer.as_bytes(),
            Some(reference.value_root),
        )
        .unwrap();
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [0x82; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        let generic = verifier.verify(&sealed.record).unwrap();
        assert_eq!(generic.record.after, sealed.record.after);
        drop(generic);
        let values = store.value_session(physical_scope());
        let typed = accept_factory_authority_transition_v1(
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
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            },
            &mut scan(),
            &mut 8192,
            4096,
        );
        if case == 6 {
            assert_eq!(raw_result.unwrap().record_counts(), [1, 3]);
            assert_eq!(typed.unwrap().record_counts(), [1, 3]);
            eprintln!(
                "factory malformed case={case}: genuine tail truncation accepted by raw/typed"
            );
        } else {
            assert!(
                matches!(raw_result, Err(ValueTreeError::Invalid(_))),
                "raw case={case}: {raw_result:?}"
            );
            assert!(
                matches!(
                    typed,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
                ),
                "typed case={case}: {typed:?}"
            );
            eprintln!("factory malformed case={case}: generic accepted, raw/typed Invalid");
        }
    }
}

fn power_child(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
) -> (WorldFieldKeyV1, [u8; 32], StateRootV1) {
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let token = text("empty");
    let outer = power_key(&token);
    let root = roots
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Business)
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
            &power_schema(),
            blake3::hash(&semantic(&token)).as_bytes(),
            StateNamespaceV1::Business,
        )
        .unwrap()
        .clone();
    (outer, value, child)
}
const POWER_NAME: &str = "factory_construction_power_profiles";
fn power_field() -> FieldDescriptor {
    FieldDescriptor {
        name: POWER_NAME,
        ..field()
    }
}
fn power_schema() -> [u8; 32] {
    let mut b = b"oasis7.iws.factory-authority.v1\0".to_vec();
    b.extend_from_slice(POWER_NAME.as_bytes());
    *blake3::hash(&b).as_bytes()
}
fn power_key(token: &[u8]) -> WorldFieldKeyV1 {
    let k = WorldFieldKeyV1::new(
        power_field(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic(token)).as_bytes()),
    )
    .unwrap();
    assert_eq!(
        k.as_bytes(),
        literal_key(POWER_NAME, StateNamespaceV1::Business, 3, Some(token))
    );
    k
}
fn power_column(name: &str) -> WorldFieldKeyV1 {
    power_key(&pair(&[0], &text(name)))
}

#[test]
fn generic_valid_power_option_relation_failures_and_valid_removal() {
    use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let directory = Directory::new();
    let store = IncrementalPageStore::create(
        &directory.0.join("malformed"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let top = seed(&source(), &store, &memory);
    let mut roots = RootSetV1::empty("factory-authority-malformed".into(), [0x81; 32]).unwrap();
    roots.state[0] = top;
    let qualification = raw(&store, &memory, &roots).unwrap();
    assert_eq!(qualification.record_counts(), [1, 3]);
    let (outer, _, child) = power_child(&store, &memory, &roots);
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    for case in 0..6 {
        let changes = match case {
            0 => {
                let r = member!(
                    &store,
                    &memory,
                    power_field(),
                    ((), "source_module_present"),
                    MemberPayloadRefV1::Bool(false)
                );
                vec![(r.field_key, Some(r.value_root))]
            }
            1 => vec![(power_column("source_module_id"), None)],
            2 => {
                let r = member!(
                    &store,
                    &memory,
                    power_field(),
                    ((), "source_module_present"),
                    MemberPayloadRefV1::Bool(false)
                );
                vec![
                    (r.field_key, Some(r.value_root)),
                    (power_column("source_module_id"), None),
                ]
            }
            3 => {
                let r = member!(
                    &store,
                    &memory,
                    power_field(),
                    ((), "mode"),
                    MemberPayloadRefV1::U64(1)
                );
                vec![(r.field_key, Some(r.value_root))]
            }
            4 => {
                let r = member!(
                    &store,
                    &memory,
                    power_field(),
                    ((), "source_module_id"),
                    MemberPayloadRefV1::Bool(true)
                );
                vec![(r.field_key, Some(r.value_root))]
            }
            _ => {
                let r = member!(
                    &store,
                    &memory,
                    power_field(),
                    ((), "unknown_column"),
                    MemberPayloadRefV1::Unit
                );
                vec![(r.field_key, Some(r.value_root))]
            }
        };
        let bad = child_changes(&store, &memory, child.clone(), changes);
        let root_ref = RecordRootRefV1::new(
            power_schema(),
            *blake3::hash(&semantic(&text("empty"))).as_bytes(),
            bad,
        )
        .unwrap();
        let reference = member!(
            &store,
            &memory,
            power_field(),
            "empty",
            MemberPayloadRefV1::RecordRoot(&root_ref)
        );
        let candidate = changed_roots(&store, &memory, &roots, &outer, reference.value_root);
        let raw_result = raw(&store, &memory, &candidate);
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [0x82; 32],
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
            StateNamespaceV1::Business,
            outer.as_bytes(),
            Some(reference.value_root),
        )
        .unwrap();
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [0x82; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        let generic = verifier.verify(&sealed.record).unwrap();
        assert_eq!(generic.record.after, sealed.record.after);
        drop(generic);
        let values = store.value_session(physical_scope());
        let typed = accept_factory_authority_transition_v1(
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
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            },
            &mut scan(),
            &mut 8192,
            4096,
        );
        if case == 2 {
            assert_eq!(raw_result.unwrap().record_counts(), [1, 3]);
            assert_eq!(typed.unwrap().record_counts(), [1, 3]);
            eprintln!("power malformed case={case}: genuine Option removal accepted by raw/typed");
        } else {
            assert!(
                matches!(raw_result, Err(ValueTreeError::Invalid(_))),
                "raw case={case}: {raw_result:?}"
            );
            assert!(
                matches!(
                    typed,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
                ),
                "typed case={case}: {typed:?}"
            );
            eprintln!("power malformed case={case}: generic accepted, raw/typed Invalid");
        }
    }
}
