use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
fn semantic_invalid(error: WorldFieldOverlayErrorV1) -> bool {
    matches!(
        error,
        WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_))
            | WorldFieldOverlayErrorV1::Member(MemberRecordError::Invalid(_))
            | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(ValueTreeError::Invalid(
                _
            )))
    )
}
fn descriptor(name: &'static str) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name,
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Effects),
    }
}
fn field<W: ValueNodeWriter, C: FnMut() -> bool>(
    descriptor: FieldDescriptor,
    name: &str,
    payload: MemberPayloadRefV1<'_>,
    write: &mut MemberRecordWriteContext<'_, W, C>,
) -> MemberRecordV1 {
    write_member_record_v1(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        &((), name),
        payload,
        write,
    )
    .unwrap()
}
fn reference_field<W: ValueNodeWriter, C: FnMut() -> bool>(
    descriptor: FieldDescriptor,
    name: &str,
    child: StateRootV1,
    schema: [u8; 32],
    write: &mut MemberRecordWriteContext<'_, W, C>,
) -> MemberRecordV1 {
    let key = scoped_key(descriptor, &((), name));
    let root = RecordRootRefV1::new(schema, *blake3::hash(&key.1).as_bytes(), child).unwrap();
    field(
        descriptor,
        name,
        MemberPayloadRefV1::RecordRoot(&root),
        write,
    )
}
fn edit<W: StatePageWriter, M: MerkleNodeReader>(
    root: &StateRootV1,
    key: &WorldFieldKeyV1,
    after: Option<[u8; 32]>,
    state: &M,
    writer: &mut W,
    memory: &MemoryBudget,
) -> StateRootV1 {
    let before = root.get(key.as_bytes(), state).unwrap();
    let charge = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let delta = charge.retain(
        StateDeltaV1::build(
            root.clone(),
            vec![StateWriteV1 {
                key: key.as_bytes().to_vec(),
                before,
                after,
            }],
            state,
            2 << 20,
        )
        .unwrap(),
    );
    writer.put_state_pages(&delta, memory).unwrap();
    delta.after.clone()
}
#[test]
fn generic_valid_effect_changes_do_not_certify_invalid_owner_closure() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("malformed"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let intent = EffectIntent {
        birth_era: None,
        intent_id: "id".into(),
        kind: "kind".into(),
        params: serde_json::Value::Bool(true),
        cap_ref: "cap".into(),
        origin: EffectOrigin::System,
    };
    let before = super::scaling::seed(&store, &memory, intent, 165);
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let reader = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: before.state_root(StateNamespaceV1::Effects),
        state: &state,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || {
            state
                .take_read_error()
                .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
        },
        cancelled: || false,
    };
    let family = qualify_runtime_effect_records_v1(
        &before,
        &mut ctx,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            leaves_remaining: 1000,
            read_bytes_remaining: 64 << 20,
        },
        &mut RuntimeEffectJsonWorkV1 {
            cells_remaining: 29,
            json_nodes_remaining: 1,
            max_depth: 0,
            max_payload_bytes: 165,
        },
    )
    .unwrap();
    let body = open_runtime_effect_record_v1(
        RuntimeEffectRecordKeyV1::Pending(0),
        &before,
        &family,
        &mut ctx,
    )
    .unwrap()
    .unwrap();
    let pending_key = literal_key(true, &0u64);
    let wrapper = reference(
        &lookup(
            before.state_root(StateNamespaceV1::Effects),
            &pending_key,
            &state,
        ),
        &pending_key,
        true,
        false,
        &mut ctx.values,
    );
    let json = {
        let mut ctx = WorldFieldReadContextV1 {
            root: body.root(),
            state: &state,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
            },
            cancelled: || false,
        };
        read_qualified_runtime_effect_params_root_v1(&body, &mut ctx).unwrap()
    };
    for mode in 0..8 {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut writer = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut io,
            key_work: &mut keys,
            limits: limits(),
            cancelled: || false,
        };
        let mut mutated = wrapper.clone();
        let mut changed_body = body.root().clone();
        match mode {
            0 => {
                // Unknown body column plus a forged old count; all fixed keys remain.
                let row = field(
                    descriptor("effect_intent_body_v1"),
                    "unknown",
                    MemberPayloadRefV1::Unit,
                    &mut write,
                );
                changed_body = edit(
                    &changed_body,
                    &row.field_key,
                    Some(row.value_root),
                    &state,
                    write.sink,
                    &memory,
                );
                assert_eq!(changed_body.leaf_count, 11);
                changed_body.leaf_count = 10;
            }
            1 => {
                let key = key_for_column(true, "cap_ref");
                changed_body = edit(&changed_body, &key.0, None, &state, write.sink, &memory);
            }
            2 => {
                let row = field(
                    descriptor("effect_intent_body_v1"),
                    "origin.kind",
                    MemberPayloadRefV1::Text("Reducer"),
                    &mut write,
                );
                changed_body = edit(
                    &changed_body,
                    &row.field_key,
                    Some(row.value_root),
                    &state,
                    write.sink,
                    &memory,
                );
            }
            3 => {
                let row = field(
                    descriptor("effect_pending_item_v1"),
                    "prev",
                    MemberPayloadRefV1::U64(0),
                    &mut write,
                );
                mutated = edit(
                    &mutated,
                    &row.field_key,
                    Some(row.value_root),
                    &state,
                    write.sink,
                    &memory,
                );
            }
            4 => {
                let mut bad = (*json).clone();
                for (name, payload) in [
                    ("json.kind", MemberPayloadRefV1::Text("f64")),
                    ("json.value", MemberPayloadRefV1::U64(f64::NAN.to_bits())),
                ] {
                    let row = field(descriptor("effect_json_node_v1"), name, payload, &mut write);
                    bad = edit(
                        &bad,
                        &row.field_key,
                        Some(row.value_root),
                        &state,
                        write.sink,
                        &memory,
                    );
                }
                let row = reference_field(
                    descriptor("effect_intent_body_v1"),
                    "params",
                    bad,
                    schema(true, true),
                    &mut write,
                );
                changed_body = edit(
                    &changed_body,
                    &row.field_key,
                    Some(row.value_root),
                    &state,
                    write.sink,
                    &memory,
                );
            }
            5..=7 => (),
            _ => unreachable!(),
        }
        let outer = if mode <= 4 {
            if mode != 3 {
                let row = reference_field(
                    descriptor("effect_pending_item_v1"),
                    "body",
                    changed_body,
                    schema(true, false),
                    &mut write,
                );
                mutated = edit(
                    &mutated,
                    &row.field_key,
                    Some(row.value_root),
                    &state,
                    write.sink,
                    &memory,
                );
            }
            let root = RecordRootRefV1::new(
                *blake3::hash(b"oasis7.iws.runtime-effect-pending-item.v1\0").as_bytes(),
                *blake3::hash(&pending_key.1).as_bytes(),
                mutated,
            )
            .unwrap();
            write_member_record_v1(
                FieldDescriptor {
                    owner: FieldOwner::Runtime,
                    name: "pending_effects",
                    shape: FieldShape::Ordered,
                    destination: FieldDestination::State(StateNamespaceV1::Effects),
                },
                WorldFieldRecordKindV1::Entry,
                &0u64,
                MemberPayloadRefV1::RecordRoot(&root),
                &mut write,
            )
            .unwrap()
        } else {
            let (name, value) = match mode {
                5 => ("count", 2),
                6 => ("next_ordinal", 0),
                _ => ("head", 1),
            };
            write_member_record_v1(
                FieldDescriptor {
                    owner: FieldOwner::Runtime,
                    name: "pending_effects",
                    shape: FieldShape::Ordered,
                    destination: FieldDestination::State(StateNamespaceV1::Effects),
                },
                WorldFieldRecordKindV1::Entry,
                &(("runtime_effect_intent_closure_v1", ()), name),
                MemberPayloadRefV1::U64(value),
                &mut write,
            )
            .unwrap()
        };
        let parent = ExecutionParent {
            roots: before.clone(),
            commit: [12; 32],
            authority: AuthorityBinding {
                token: [13; 32],
                epoch: 14,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [15; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 16,
                max_writes: 16,
                max_facts: 0,
                max_pending_node_bytes: 2 << 20,
            },
        )
        .unwrap();
        tx.write(
            StateNamespaceV1::Effects,
            outer.field_key.as_bytes(),
            Some(outer.value_root),
        )
        .unwrap();
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [15; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 2 << 20,
        };
        let generic = verifier.verify(&sealed.record).unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: sealed.record.after.state_root(StateNamespaceV1::Effects),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
            },
            cancelled: || false,
        };
        let result = accept_runtime_effect_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut ctx,
            &mut RuntimeEffectJsonWorkV1 {
                cells_remaining: 512,
                json_nodes_remaining: 16,
                max_depth: 8,
                max_payload_bytes: 165,
            },
        );
        let error = result.expect_err("typed owner accepted malformed graph");
        assert!(
            semantic_invalid(error),
            "typed mode {mode} failed outside semantic Invalid"
        );
        // Publish only provisional generic pages so the independent raw gate
        // reaches the same actual after graph, never a fabricated RootSet.
        let bytes = generic
            .new_nodes
            .iter()
            .map(|node| {
                node.bytes.capacity()
                    + node.digest.capacity()
                    + std::mem::size_of::<StateNodeWriteV1>()
            })
            .sum::<usize>();
        let delta = memory
            .reserve(bytes + 4096, MemoryClass::Ordinary)
            .unwrap()
            .retain(StateDeltaV1 {
                before: before.state_root(StateNamespaceV1::Effects).clone(),
                after: sealed
                    .record
                    .after
                    .state_root(StateNamespaceV1::Effects)
                    .clone(),
                writes: Vec::new(),
                new_nodes: generic.new_nodes.clone(),
            });
        writer.put_state_pages(&delta, &memory).unwrap();
        let state2 = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values2 = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: sealed.record.after.state_root(StateNamespaceV1::Effects),
            state: &state2,
            values: ValueReadContext {
                source: &values2,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state2
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
            },
            cancelled: || false,
        };
        let raw = qualify_runtime_effect_records_v1(
            &sealed.record.after,
            &mut ctx,
            &mut StateScanBudget {
                nodes_remaining: 100000,
                leaves_remaining: 1000,
                read_bytes_remaining: 64 << 20,
            },
            &mut RuntimeEffectJsonWorkV1 {
                cells_remaining: 512,
                json_nodes_remaining: 16,
                max_depth: 8,
                max_payload_bytes: 165,
            },
        );
        assert!(
            matches!(raw, Err(ValueTreeError::Invalid(_))),
            "raw mode {mode}: {raw:?}"
        );
    }
}
