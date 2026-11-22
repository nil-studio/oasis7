use super::*;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
fn field(kind: RuntimeCapabilityRecordKindV1) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: kind.name(),
        shape: FieldShape::Map,
        destination: FieldDestination::State(kind.namespace()),
    }
}
fn raw_cell(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    kind: RuntimeCapabilityRecordKindV1,
    name: &str,
    payload: MemberPayloadRefV1<'_>,
) -> MemberRecordV1 {
    write_member_record_v1(
        field(kind),
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
        .value_session(physical())
        .put_state_pages(&delta, memory)
        .unwrap();
    delta.after.clone()
}
fn outer(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    key: RuntimeCapabilityRecordKeyV1<'_>,
    child: StateRootV1,
) -> MemberRecordV1 {
    // Independent original String semantic preimage, not a producer projection.
    let mut h = blake3::Hasher::new();
    h.update(b"oasis7.iws.semantic-key.v1\0");
    h.update(&[1]);
    h.update(&(key.id.len() as u64).to_be_bytes());
    h.update(key.id.as_bytes());
    let reference =
        RecordRootRefV1::new(key.kind.schema(), *h.finalize().as_bytes(), child).unwrap();
    write_member_record_v1(
        field(key.kind),
        WorldFieldRecordKindV1::Entry,
        &key.id,
        MemberPayloadRefV1::RecordRoot(&reference),
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
fn generic_valid_capability_mutations_require_owner_closure() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("schema"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = seed(&store, &memory);
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = read_ctx!(
        roots
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Authorization)
            .unwrap(),
        &state,
        &values,
        &memory,
        MemoryClass::Ordinary,
        &mut io,
        &mut keys
    );
    let family = qualify_runtime_capability_family_v1(
        &roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut 1000,
        165,
    )
    .unwrap();
    use RuntimeCapabilityRecordKindV1 as K;
    let nonce_key = RuntimeCapabilityRecordKeyV1 {
        kind: K::Nonce,
        id: "nonce-key",
    };
    let budget_key = RuntimeCapabilityRecordKeyV1 {
        kind: K::Budget,
        id: "agent-key",
    };
    let nonce = open_qualified_runtime_capability_record_v1(nonce_key, &roots, &family, &mut ctx)
        .unwrap()
        .unwrap();
    let budget = open_qualified_runtime_capability_record_v1(budget_key, &roots, &family, &mut ctx)
        .unwrap()
        .unwrap();
    for mode in 0..6 {
        let (key, base, name, payload, delete) = match mode {
            0 => (
                nonce_key,
                nonce.root(),
                "state",
                MemberPayloadRefV1::Text(""),
                true,
            ),
            1 => (
                nonce_key,
                nonce.root(),
                "committed_receipt_id",
                MemberPayloadRefV1::Bool(false),
                false,
            ),
            2 => (
                budget_key,
                budget.root(),
                "subject.system_id",
                MemberPayloadRefV1::Text("orphan"),
                false,
            ),
            3 => (
                nonce_key,
                nonce.root(),
                "unknown_original_field",
                MemberPayloadRefV1::Text(""),
                false,
            ),
            4 => (
                budget_key,
                budget.root(),
                "subject.kind",
                MemberPayloadRefV1::Text("alien!"),
                false,
            ),
            5 => (
                budget_key,
                budget.root(),
                "subject.generation",
                MemberPayloadRefV1::Unit,
                false,
            ),
            _ => unreachable!(),
        };
        let cell = raw_cell(&store, &memory, key.kind, name, payload);
        let child = edit(
            &store,
            &memory,
            base,
            &cell.field_key,
            if delete { None } else { Some(cell.value_root) },
        );
        let row = outer(&store, &memory, key, child);
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 16,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        tx.write(
            key.kind.namespace(),
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
        let generic = verifier.verify(&sealed.record).unwrap();
        let state2 = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values2 = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut receiver = read_ctx!(
            sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap(),
            &state2,
            &values2,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let result = accept_runtime_capability_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut receiver,
            &mut scan(),
            &mut 8192,
            165,
        );
        assert!(
            matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ),
            "typed mode{mode}: {result:?}"
        );
        let mut writer = store.value_session(physical());
        for node in &generic.new_nodes {
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
        let state3 = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values3 = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut raw = read_ctx!(
            sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap(),
            &state3,
            &values3,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let result = qualify_runtime_capability_family_v1(
            &sealed.record.after,
            &mut raw,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            165,
        );
        assert!(
            matches!(result, Err(ValueTreeError::Invalid(_))),
            "raw mode{mode}: {result:?}"
        );
    }
}
