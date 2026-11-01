#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::*;
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_record_root::RecordRootRefV1, incremental_state::*,
    incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::incremental_field_read::WorldFieldReadContextV1;
use oasis7_runtime::{
    EffectIntent, EffectOrigin, incremental_field_member::*, incremental_runtime_effect_records::*,
};
use std::collections::{BTreeMap, VecDeque};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
#[path = "support/incremental_runtime_effect_faults.rs"]
mod faults;
#[path = "support/incremental_runtime_effect_malformed.rs"]
mod malformed;
#[path = "support/incremental_runtime_effect_pages.rs"]
mod pages;
#[path = "support/incremental_runtime_effect_scaling.rs"]
mod scaling;
// Complete GEN forest: same bounded setup profile as the adjacent 72-leaf
// capability fixture, keeping its physical byte/write and all logical quotas.
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_250_000,
        ..fixture::physical()
    }
}
fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 4096,
        payload_bytes: 165,
        record_bytes: 8192,
    }
}
fn descriptor(pending: bool) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: if pending {
            "pending_effects"
        } else {
            "inflight_effects"
        },
        shape: if pending {
            FieldShape::Ordered
        } else {
            FieldShape::Map
        },
        destination: FieldDestination::State(StateNamespaceV1::Effects),
    }
}
fn literal_key<K: LiteralKey + ?Sized>(pending: bool, source: &K) -> (WorldFieldKeyV1, Vec<u8>) {
    scoped_key(descriptor(pending), source)
}
fn inner_descriptor(json: bool) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: if json {
            "effect_json_node_v1"
        } else {
            "effect_intent_body_v1"
        },
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Effects),
    }
}
fn json_member_key<K: LiteralKey + ?Sized>(source: &K) -> (WorldFieldKeyV1, Vec<u8>) {
    scoped_key(inner_descriptor(true), source)
}
fn scoped_key<K: LiteralKey + ?Sized>(
    descriptor: FieldDescriptor,
    source: &K,
) -> (WorldFieldKeyV1, Vec<u8>) {
    let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
    source.encode(&mut semantic);
    let key = WorldFieldKeyV1::new(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic).as_bytes()),
    )
    .unwrap();
    (key, semantic)
}
fn cell<R: ValueNodeReader>(
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, Vec<u8>),
    codec: MemberPayloadCodecV1,
    values: &mut ValueReadContext<'_, R>,
) -> Vec<u8> {
    let mut semantic = Vec::new();
    let mut payload = Vec::new();
    read_member_record_profile_v1(
        root,
        &key.0,
        values,
        MemberReadProfileV1 {
            codec,
            semantic_bytes: key.1.len() as u64,
            limits: limits(),
        },
        &mut semantic,
        &mut payload,
        &mut || false,
    )
    .unwrap();
    assert_eq!(semantic, key.1);
    payload
}
fn schema(_pending: bool, json: bool) -> [u8; 32] {
    *blake3::hash(if json {
        b"oasis7.iws.runtime-effect-json-node.v1\0"
    } else {
        b"oasis7.iws.runtime-effect-intent-body.v1\0"
    })
    .as_bytes()
}
fn reference<R: ValueNodeReader>(
    value: &[u8; 32],
    key: &(WorldFieldKeyV1, Vec<u8>),
    pending: bool,
    json: bool,
    values: &mut ValueReadContext<'_, R>,
) -> StateRootV1 {
    let bytes = cell(value, key, MemberPayloadCodecV1::RecordRoot, values);
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).unwrap();
    reference
        .validate_binding(
            &if pending
                && !json
                && WorldFieldKeyV1::from_bytes(key.0.as_bytes()).unwrap().name == "pending_effects"
            {
                *blake3::hash(b"oasis7.iws.runtime-effect-pending-item.v1\0").as_bytes()
            } else {
                schema(pending, json)
            },
            blake3::hash(&key.1).as_bytes(),
            StateNamespaceV1::Effects,
        )
        .unwrap()
        .clone()
}
fn lookup<M: MerkleNodeReader>(
    root: &StateRootV1,
    key: &(WorldFieldKeyV1, Vec<u8>),
    state: &M,
) -> [u8; 32] {
    root.get(key.0.as_bytes(), state).unwrap().unwrap()
}
#[test]
fn original_effect_collections_and_json_nodes_have_independent_literal_wire() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&directory.0.join("genesis"), memory.clone(), compression)
                .unwrap();
        let Source { pending, inflight } = source();
        let before = serde_json::to_vec(&(&pending, &inflight)).unwrap();
        let mut writer = store.value_session(physical());
        let mut rows = Vec::with_capacity(10);
        let mut work = RuntimeEffectRecordsWorkV1 {
            cells_remaining: 69,
            json_nodes_remaining: 6,
            max_json_depth: 16,
            max_pending_node_bytes: 2 << 20,
        };
        let generated = prepare_runtime_effect_records_genesis_v1(
            &RuntimeEffectRecordsRefV1 {
                pending: &pending,
                inflight: &inflight,
            },
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                limits: limits(),
                cancelled: || false,
            },
            &mut work,
            &mut |row| {
                assert!(rows.len() < 10);
                rows.push(row);
                Ok(())
            },
        );
        eprintln!(
            "effect GEN result={generated:?} remaining_cells={} remaining_json_nodes={} physical={:?}",
            work.cells_remaining,
            work.json_nodes_remaining,
            writer.remaining_io().unwrap()
        );
        assert_eq!(generated.unwrap(), 2);
        assert_eq!(work.cells_remaining, 0);
        assert_eq!(work.json_nodes_remaining, 0);
        assert_eq!(rows.len(), 10);
        assert_eq!(serde_json::to_vec(&(&pending, &inflight)).unwrap(), before);
        let mut roots = oasis7_formal_protocol::incremental_execution::RootSetV1::empty(
            "effect-records".into(),
            [0x65; 32],
        )
        .unwrap();
        {
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut writes = rows
                .iter()
                .map(|row| StateWriteV1 {
                    key: row.mutation.key.clone(),
                    before: None,
                    after: row.mutation.after,
                })
                .collect::<Vec<_>>();
            writes.sort_by(|a, b| a.key.cmp(&b.key));
            let charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
            let delta = charge.retain(
                StateDeltaV1::build(
                    StateRootV1::empty(StateNamespaceV1::Effects).unwrap(),
                    writes,
                    &state,
                    2 << 20,
                )
                .unwrap(),
            );
            writer.put_state_pages(&delta, &memory).unwrap();
            *roots
                .state
                .iter_mut()
                .find(|root| root.namespace == StateNamespaceV1::Effects)
                .unwrap() = delta.after.clone();
        }
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let source = store.value_session(physical());
        let mut io = logical();
        let mut values = ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        };
        let mut family_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: roots
                .state
                .iter()
                .find(|root| root.namespace == StateNamespaceV1::Effects)
                .unwrap(),
            state: &state,
            values: ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *values.io,
            },
            key_work: &mut family_keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state.take_read_error().map(|e| match e {
                    PageStoreError::Memory(e) => ValueTreeError::Memory(e),
                    PageStoreError::ResourceLimited(e) => ValueTreeError::ResourceLimited(e),
                    PageStoreError::Cancelled => ValueTreeError::Cancelled,
                    PageStoreError::Io(e) => ValueTreeError::Io(e),
                    _ => ValueTreeError::Invalid("effect family state"),
                })
            },
            cancelled: || false,
        };
        let mut family_work = RuntimeEffectJsonWorkV1 {
            cells_remaining: 69,
            json_nodes_remaining: 6,
            max_depth: 16,
            max_payload_bytes: 165,
        };
        let family = qualify_runtime_effect_records_v1(
            &roots,
            &mut context,
            &mut StateScanBudget {
                nodes_remaining: 100000,
                leaves_remaining: 1000,
                read_bytes_remaining: 64 << 20,
            },
            &mut family_work,
        )
        .unwrap();
        assert_eq!(family.record_counts(), [1, 1]);
        assert_eq!(
            family.pending_frontier(),
            RuntimeEffectQueueFrontierV1 {
                head: Some(0),
                tail: Some(0),
                count: 1,
                next_ordinal: 1
            }
        );
        family.validate_binding(&roots.digest().unwrap()).unwrap();
        assert!(family.validate_binding(&[0; 32]).is_err());
        assert_eq!(family_work.cells_remaining, 0);
        assert_eq!(family_work.json_nodes_remaining, 0);
        let pending_key = literal_key(true, &0u64);
        let inflight_key = literal_key(false, &"different-outer-key");
        for (pending, key) in [(true, &pending_key), (false, &inflight_key)] {
            let row = rows
                .iter()
                .find(|r| r.mutation.key == key.0.as_bytes())
                .unwrap();
            let root = reference(
                &row.mutation.after.unwrap(),
                key,
                pending,
                false,
                &mut values,
            );
            let root = if pending {
                assert_eq!(root.leaf_count, 5);
                let descriptor = FieldDescriptor {
                    owner: FieldOwner::Runtime,
                    name: "effect_pending_item_v1",
                    shape: FieldShape::Map,
                    destination: FieldDestination::State(StateNamespaceV1::Effects),
                };
                for name in ["prev", "next"] {
                    let field = scoped_key(descriptor, &((), name));
                    assert_eq!(
                        cell(
                            &lookup(&root, &field, &state),
                            &field,
                            MemberPayloadCodecV1::Unit,
                            &mut values
                        ),
                        b""
                    );
                }
                let field = scoped_key(descriptor, &((), "body"));
                reference(
                    &lookup(&root, &field, &state),
                    &field,
                    false,
                    false,
                    &mut values,
                )
            } else {
                root
            };
            assert_eq!(root.leaf_count, 10);
            let birth = key_for_column(pending, "birth_era");
            assert_eq!(
                cell(
                    &lookup(&root, &birth, &state),
                    &birth,
                    if pending {
                        MemberPayloadCodecV1::Unit
                    } else {
                        MemberPayloadCodecV1::U64
                    },
                    &mut values
                ),
                if pending {
                    vec![]
                } else {
                    0u64.to_be_bytes().to_vec()
                }
            );
            for (name, want) in [
                (
                    "intent_id",
                    if pending {
                        " embedded pending "
                    } else {
                        "embedded-inflight"
                    },
                ),
                ("origin.kind", if pending { "Reducer" } else { "Module" }),
            ] {
                let key = key_for_column(pending, name);
                assert_eq!(
                    cell(
                        &lookup(&root, &key, &state),
                        &key,
                        MemberPayloadCodecV1::Text,
                        &mut values
                    ),
                    want.as_bytes()
                );
            }
            let name = key_for_column(pending, "origin.name");
            assert_eq!(
                cell(
                    &lookup(&root, &name, &state),
                    &name,
                    if pending {
                        MemberPayloadCodecV1::Text
                    } else {
                        MemberPayloadCodecV1::Unit
                    },
                    &mut values
                ),
                b""
            );
            let param_key = key_for_column(pending, "params");
            let json = reference(
                &lookup(&root, &param_key, &state),
                &param_key,
                pending,
                true,
                &mut values,
            );
            let mut key_work = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut read = WorldFieldReadContextV1 {
                root: &json,
                state: &state,
                values: ValueReadContext {
                    source: &source,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *values.io,
                },
                key_work: &mut key_work,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || {
                    state.take_read_error().map(|e| match e {
                        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
                        PageStoreError::ResourceLimited(e) => ValueTreeError::ResourceLimited(e),
                        PageStoreError::Cancelled => ValueTreeError::Cancelled,
                        PageStoreError::Io(e) => ValueTreeError::Io(e),
                        _ => ValueTreeError::Invalid("effect raw state source"),
                    })
                },
                cancelled: || false,
            };
            let mut json_work = RuntimeEffectJsonWorkV1 {
                cells_remaining: if pending { 29 } else { 5 },
                json_nodes_remaining: if pending { 5 } else { 1 },
                max_depth: 16,
                max_payload_bytes: 165,
            };
            let proof =
                inspect_runtime_effect_json_body_v1(&json, &mut read, &mut json_work).unwrap();
            assert_eq!(proof.root(), &json);
            assert_eq!(proof.node_count(), if pending { 5 } else { 1 });
            assert_eq!(json_work.cells_remaining, 0);
            assert_eq!(json_work.json_nodes_remaining, 0);

            let kind = key_for_column(pending, "json.kind");
            assert_eq!(
                cell(
                    &lookup(&json, &kind, &state),
                    &kind,
                    MemberPayloadCodecV1::Text,
                    &mut values
                ),
                if pending {
                    b"object".as_slice()
                } else {
                    b"bool".as_slice()
                }
            );
            if pending {
                assert_eq!(json.leaf_count, 7);
                let array_key = json_member_key(&(((), "members"), "array"));
                let array = reference(
                    &lookup(&json, &array_key, &state),
                    &array_key,
                    true,
                    true,
                    &mut values,
                );
                assert_eq!(array.leaf_count, 7);
                let first_key = json_member_key(&(((), "members"), 0u64));
                let first = reference(
                    &lookup(&array, &first_key, &state),
                    &first_key,
                    true,
                    true,
                    &mut values,
                );
                let value = key_for_column(true, "json.value");
                assert_eq!(
                    cell(
                        &lookup(&first, &value, &state),
                        &value,
                        MemberPayloadCodecV1::U64,
                        &mut values
                    ),
                    (-0.0f64).to_bits().to_be_bytes()
                );
                let first_kind = key_for_column(true, "json.kind");
                assert_eq!(
                    cell(
                        &lookup(&first, &first_kind, &state),
                        &first_kind,
                        MemberPayloadCodecV1::Text,
                        &mut values
                    ),
                    b"f64"
                );
                let second_key = json_member_key(&(((), "members"), 1u64));
                let second = reference(
                    &lookup(&array, &second_key, &state),
                    &second_key,
                    true,
                    true,
                    &mut values,
                );
                assert_eq!(
                    cell(
                        &lookup(&second, &value, &state),
                        &value,
                        MemberPayloadCodecV1::Text,
                        &mut values
                    ),
                    "界".as_bytes()
                );
            }
        }
        typed_snapshot_reads(&store, &memory, &roots, &family);
        pages::roundtrip(&store, &memory, &roots);
        membership_sequence(&store, &memory, &roots, &family, pending.front().unwrap());
    }
}
fn key_for_column(_pending: bool, name: &str) -> (WorldFieldKeyV1, Vec<u8>) {
    scoped_key(inner_descriptor(name.starts_with("json.")), &((), name))
}

// Independent literal preimage codec for the bounded keys used by this oracle.
trait LiteralKey {
    fn encode(&self, out: &mut Vec<u8>);
}
impl LiteralKey for () {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(0);
    }
}
impl LiteralKey for str {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(1);
        out.extend_from_slice(&(self.len() as u64).to_be_bytes());
        out.extend_from_slice(self.as_bytes());
    }
}
impl LiteralKey for u64 {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(2);
        out.extend_from_slice(&self.to_be_bytes());
    }
}
impl<T: LiteralKey + ?Sized> LiteralKey for &T {
    fn encode(&self, out: &mut Vec<u8>) {
        (**self).encode(out);
    }
}
impl<A: LiteralKey, B: LiteralKey> LiteralKey for (A, B) {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(5);
        self.0.encode(out);
        self.1.encode(out);
    }
}

struct Source {
    pending: VecDeque<EffectIntent>,
    inflight: BTreeMap<String, EffectIntent>,
}
fn source() -> Source {
    let pending: VecDeque<_> = [EffectIntent {
        birth_era: None,
        intent_id: " embedded pending ".into(),
        kind: "".into(),
        params: serde_json::json!({"":null,"array":[-0.0,"界"]}),
        cap_ref: "cap".into(),
        origin: EffectOrigin::Reducer { name: "".into() },
    }]
    .into();
    let inflight: BTreeMap<_, _> = [(
        "different-outer-key".into(),
        EffectIntent {
            birth_era: Some(0),
            intent_id: "embedded-inflight".into(),
            kind: "kind".into(),
            params: serde_json::json!(true),
            cap_ref: "".into(),
            origin: EffectOrigin::Module {
                module_id: "module".into(),
            },
        },
    )]
    .into();
    Source { pending, inflight }
}
#[test]
fn complete_json_genesis_exhaustion_returns_no_completed_forest() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("limited"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let Source { pending, inflight } = source();
    let mut writer = store.value_session(fixture::physical());
    let mut work = RuntimeEffectRecordsWorkV1 {
        cells_remaining: 69,
        json_nodes_remaining: 6,
        max_json_depth: 16,
        max_pending_node_bytes: 2 << 20,
    };
    let mut emitted = 0;
    let result = prepare_runtime_effect_records_genesis_v1(
        &RuntimeEffectRecordsRefV1 {
            pending: &pending,
            inflight: &inflight,
        },
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: limits(),
            cancelled: || false,
        },
        &mut work,
        &mut |_| {
            emitted += 1;
            Ok(())
        },
    );
    assert!(matches!(
        result,
        Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(
            "physical read allowance"
        )))
    ));
    assert_eq!(
        emitted, 8,
        "only provisional metadata/frontier, no complete Intent root"
    );
    assert_eq!(writer.remaining_io().unwrap().reads_remaining, 0);
    assert!(work.cells_remaining > 0);
    assert!(work.json_nodes_remaining > 0);
}

fn membership_sequence(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    before: &RootSetV1,
    family: &oasis7_proto::memory_budget::ReservedValue<RuntimeEffectFamilyQualificationV1>,
    intent: &EffectIntent,
) {
    use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
    let parent = ExecutionParent {
        roots: before.clone(),
        commit: [7; 32],
        authority: AuthorityBinding {
            token: [8; 32],
            epoch: 9,
        },
    };
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let reader = store.value_session(physical());
    let mut io = logical();
    let mut values = ValueReadContext {
        source: &reader,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut io,
    };
    let outer = literal_key(true, &0u64);
    let root = before
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Effects)
        .unwrap();
    let wrapper = reference(
        &lookup(root, &outer, &state),
        &outer,
        true,
        false,
        &mut values,
    );
    let descriptor = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "effect_pending_item_v1",
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Effects),
    };
    let key = scoped_key(descriptor, &((), "body"));
    let body = reference(
        &lookup(&wrapper, &key, &state),
        &key,
        false,
        false,
        &mut values,
    );
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [10; 32],
        &state,
        memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 32,
            max_facts: 0,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &reader,
            memory,
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
    let mut write_io = logical();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    // Functional seven-operation scope, not the one-operation GEN or fixed-Δ
    // performance profile. Three upfront partitions bound the total at 4.5M
    // physical reads: 2M writer + 1.25M state source + 1.25M value source.
    // None is reset/recreated until the complete chain and publication finish.
    let writer_budget = PageIoBudget {
        reads_remaining: 2_000_000,
        ..physical()
    };
    let mut writer = store.value_session(writer_budget.clone());
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: limits(),
        cancelled: || false,
    };
    let mut work = RuntimeEffectRecordsWorkV1 {
        cells_remaining: 512,
        json_nodes_remaining: 64,
        max_json_depth: 16,
        max_pending_node_bytes: 2 << 20,
    };
    for change in [
        RuntimeEffectMembershipChangeV1::AppendPending { intent },
        RuntimeEffectMembershipChangeV1::AppendPending { intent },
        RuntimeEffectMembershipChangeV1::RemovePending {
            ordinal: 1,
            expected_body: &body,
        },
        RuntimeEffectMembershipChangeV1::Handoff {
            ordinal: 0,
            expected_body: &body,
            inflight_key: "handoff-first",
            expected_inflight: None,
        },
        RuntimeEffectMembershipChangeV1::RemovePending {
            ordinal: 2,
            expected_body: &body,
        },
        RuntimeEffectMembershipChangeV1::AppendPending { intent },
        RuntimeEffectMembershipChangeV1::Handoff {
            ordinal: 3,
            expected_body: &body,
            inflight_key: "handoff-after-empty",
            expected_inflight: None,
        },
    ] {
        let guard = overlay.transaction.guard();
        change_runtime_effect_membership_v1(
            change,
            family,
            &mut overlay,
            &mut write,
            &mut work,
            &guard,
            &live,
        )
        .unwrap();
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [10; 32],
        source: &state,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    };
    let mut receiver = WorldFieldReadContextV1 {
        root: sealed.record.after.state_root(StateNamespaceV1::Effects),
        state: &state,
        values: ValueReadContext {
            source: &reader,
            memory,
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
    // All final bodies come from the qualified original pending record. A zero
    // JSON-node allowance proves the changed receiver does not requalify params.
    let accepted = accept_runtime_effect_transition_v1(
        family,
        &verifier,
        &sealed.record,
        &mut receiver,
        &mut RuntimeEffectJsonWorkV1 {
            cells_remaining: 512,
            json_nodes_remaining: 0,
            max_depth: 16,
            max_payload_bytes: 165,
        },
    )
    .unwrap();
    assert_eq!(accepted.record_counts(), [0, 3]);
    accepted
        .validate_binding(&sealed.record.after.digest().unwrap())
        .unwrap();
    let generic = &sealed;
    eprintln!(
        "membership continuous writer before outer persistence={:?}; outer pages={}",
        writer.remaining_io().unwrap(),
        generic.new_nodes.len()
    );
    for node in &generic.new_nodes {
        assert_eq!(node.digest, blake3::hash(&node.bytes).to_hex().to_string());
    }
    // Use the existing bounded State producer, rather than expanding a complete
    // transition into thousands of independent pack/index publications.
    let bytes = generic
        .new_nodes
        .iter()
        .try_fold(0usize, |total, node| {
            total.checked_add(
                node.bytes.capacity()
                    + node.digest.capacity()
                    + std::mem::size_of::<StateNodeWriteV1>(),
            )
        })
        .unwrap();
    let charge = memory.reserve(bytes + 4096, MemoryClass::Ordinary).unwrap();
    let delta = charge.retain(StateDeltaV1 {
        before: before
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Effects)
            .unwrap()
            .clone(),
        after: sealed
            .record
            .after
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Effects)
            .unwrap()
            .clone(),
        writes: Vec::new(),
        new_nodes: generic.new_nodes.clone(),
    });
    writer.put_state_pages(&delta, memory).unwrap();
    eprintln!(
        "membership sender after outer persistence: {:?}",
        writer.remaining_io().unwrap()
    );
    let writer_remaining = writer.remaining_io().unwrap();
    let state_remaining = state.remaining_io().unwrap();
    let values_remaining = reader.remaining_io().unwrap();
    let total_reads = writer_budget.reads_remaining - writer_remaining.reads_remaining
        + physical().reads_remaining
        - state_remaining.reads_remaining
        + physical().reads_remaining
        - values_remaining.reads_remaining;
    let total_bytes = writer_budget.read_bytes_remaining - writer_remaining.read_bytes_remaining
        + physical().read_bytes_remaining
        - state_remaining.read_bytes_remaining
        + physical().read_bytes_remaining
        - values_remaining.read_bytes_remaining;
    eprintln!(
        "membership continuous complete sender+generic+publish all-channel physical reads={total_reads} bytes={total_bytes}; state={state_remaining:?}; values={values_remaining:?}"
    );
    assert!(total_reads <= 4_500_000);
    let after = &sealed.record.after;
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let reader = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: after
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Effects)
            .unwrap(),
        state: &state,
        values: ValueReadContext {
            source: &reader,
            memory,
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
    let mut work = RuntimeEffectJsonWorkV1 {
        cells_remaining: 104,
        json_nodes_remaining: 11,
        max_depth: 16,
        max_payload_bytes: 165,
    };
    let proof = qualify_runtime_effect_records_v1(
        after,
        &mut ctx,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            leaves_remaining: 1000,
            read_bytes_remaining: 64 << 20,
        },
        &mut work,
    )
    .unwrap();
    assert_eq!(proof.record_counts(), [0, 3]);
    assert_eq!(
        proof.pending_frontier(),
        RuntimeEffectQueueFrontierV1 {
            head: None,
            tail: None,
            count: 0,
            next_ordinal: 4
        }
    );
    assert_eq!(work.cells_remaining, 0);
    assert_eq!(work.json_nodes_remaining, 0);
    for name in ["handoff-first", "handoff-after-empty"] {
        let key = literal_key(false, &name);
        let id = lookup(ctx.root, &key, &state);
        assert_eq!(reference(&id, &key, false, false, &mut ctx.values), body);
    }
}

fn typed_snapshot_reads(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    family: &oasis7_proto::memory_budget::ReservedValue<RuntimeEffectFamilyQualificationV1>,
) {
    let state = store
        .state_session(physical(), MemoryClass::Recovery)
        .unwrap();
    let reader = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    for (key, pending) in [
        (RuntimeEffectRecordKeyV1::Pending(0), true),
        (
            RuntimeEffectRecordKeyV1::Inflight("different-outer-key"),
            false,
        ),
    ] {
        let proof = {
            let mut ctx = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Effects),
                state: &state,
                values: ValueReadContext {
                    source: &reader,
                    memory,
                    class: MemoryClass::Recovery,
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
            open_runtime_effect_record_v1(key, roots, family, &mut ctx)
                .unwrap()
                .unwrap()
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: proof.root(),
            state: &state,
            values: ValueReadContext {
                source: &reader,
                memory,
                class: MemoryClass::Recovery,
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
        let mut bytes = Vec::new();
        let value = read_qualified_runtime_effect_field_v1(
            &proof,
            RuntimeEffectFieldV1::BirthEra,
            &mut ctx,
            165,
            &mut bytes,
        )
        .unwrap();
        assert_eq!(
            value,
            if pending {
                RuntimeEffectFieldValueV1::Unit
            } else {
                RuntimeEffectFieldValueV1::Unsigned(0)
            }
        );
        assert!(bytes.is_empty());
        for (field, expected) in [
            (
                RuntimeEffectFieldV1::IntentId,
                if pending {
                    " embedded pending "
                } else {
                    "embedded-inflight"
                },
            ),
            (
                RuntimeEffectFieldV1::Kind,
                if pending { "" } else { "kind" },
            ),
            (
                RuntimeEffectFieldV1::CapRef,
                if pending { "cap" } else { "" },
            ),
            (
                RuntimeEffectFieldV1::OriginKind,
                if pending { "Reducer" } else { "Module" },
            ),
        ] {
            bytes.clear();
            let value =
                read_qualified_runtime_effect_field_v1(&proof, field, &mut ctx, 165, &mut bytes)
                    .unwrap();
            assert_eq!(bytes, expected.as_bytes());
            assert_eq!(
                value,
                RuntimeEffectFieldValueV1::Text {
                    utf8_bytes: expected.len() as u64
                }
            );
        }
        bytes.clear();
        let value = read_qualified_runtime_effect_field_v1(
            &proof,
            RuntimeEffectFieldV1::OriginName,
            &mut ctx,
            165,
            &mut bytes,
        )
        .unwrap();
        assert!(bytes.is_empty());
        assert_eq!(
            value,
            if pending {
                RuntimeEffectFieldValueV1::Text { utf8_bytes: 0 }
            } else {
                RuntimeEffectFieldValueV1::Unit
            }
        );
        bytes.clear();
        let value = read_qualified_runtime_effect_field_v1(
            &proof,
            RuntimeEffectFieldV1::OriginModuleId,
            &mut ctx,
            165,
            &mut bytes,
        )
        .unwrap();
        assert_eq!(
            value,
            if pending {
                RuntimeEffectFieldValueV1::Unit
            } else {
                RuntimeEffectFieldValueV1::Text { utf8_bytes: 6 }
            }
        );
        assert_eq!(bytes, if pending { b"" as &[u8] } else { b"module" });
        let json = read_qualified_runtime_effect_params_root_v1(&proof, &mut ctx).unwrap();
        assert_eq!(json.namespace, StateNamespaceV1::Effects);
        assert_eq!(json.leaf_count, if pending { 7 } else { 5 });
    }
}
