//! Actual Runtime GEN46 → typed conjunction → general transition → cold readmission.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::*,
    incremental_material::{OriginalObjectReader, OriginalObjectStreamReader},
    incremental_state::*,
    incremental_value::*,
};
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1, incremental_json_records::JsonReadWorkV1,
    incremental_runtime_revocation::RevocationReadWorkV1 as TypedRecordReadWorkV1,
    incremental_runtime_typed_complete::*,
};
use oasis7_world_state::{
    incremental_rollback_genesis::RollbackWorkV1,
    incremental_runtime_artifact_content::RuntimeArtifactOriginalWorkV1,
    incremental_runtime_artifact_resources::RuntimeArtifactResourcesWorkV1,
    incremental_runtime_governance::RuntimeGovernanceWorkV1,
};
use std::io::Write;
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 1 << 30,
        leaves_remaining: 1_000_000,
    }
}
fn json() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_depth: 128,
        max_payload_bytes: 16 << 20,
    }
}
fn typed() -> TypedRecordReadWorkV1 {
    TypedRecordReadWorkV1 {
        max_retained_trees: 100_000,
        max_tree_heap: 8 << 20,
        json: json(),
    }
}
fn read_work() -> RuntimeTypedCompleteWorkV1 {
    use oasis7_world_state::{
        incremental_runtime_artifact_config::ArtifactConfigReadWorkV1,
        incremental_runtime_module_authorization::ModuleAuthorizationReadWorkV1,
        incremental_runtime_proposal::RuntimeProposalReadWorkV1,
    };
    RuntimeTypedCompleteWorkV1 {outer_scan:scan(),body_scan:scan(),resource_scans:std::array::from_fn(|_|scan()),cells_remaining:1_000_000,max_text_bytes:8<<20,max_record_bytes:16<<20,max_selected_heap:8<<20,
 config:ArtifactConfigReadWorkV1 {cells_remaining:1_000_000,max_payload_bytes:8<<20,json:json()},resources:RuntimeArtifactResourcesWorkV1 {cells_remaining:1_000_000,max_pending_node_bytes:8<<20},content:RuntimeArtifactOriginalWorkV1 {objects_remaining:1000,bytes_remaining:64<<20,max_object_bytes:16<<20},effects:json(),module_authorization:ModuleAuthorizationReadWorkV1 {cells_remaining:1_000_000,comparison_bytes_remaining:64<<20,max_payload_bytes:8<<20},json:json(),revocation:typed(),proposals:RuntimeProposalReadWorkV1 {cells_remaining:1_000_000,json:json(),max_selected_heap:8<<20,max_text_bytes:8<<20},governance:RuntimeGovernanceWorkV1 {cells_remaining:1_000_000,max_pending_node_bytes:8<<20},rollback:RollbackWorkV1 {cells_remaining:1_000_000,max_pending_node_bytes:8<<20},pending:oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1 {index:oasis7_world_state::incremental_pending_action_family::PendingActionIndexWorkV1 {steps_remaining:1_000_000},typed:typed(),original:oasis7_world_state::incremental_pending_action_original_binding::PendingActionOriginalWorkV1 {objects_remaining:1000,original_bytes_remaining:64<<20,max_original_bytes:16<<20,canonical:oasis7_wasm_abi::CanonicalCborWorkV1 {visits_remaining:1_000_000,bytes_remaining:64<<20,max_depth:128,max_map_keys:1024,max_key_bytes:4096}},selected_scan:scan()}}
}
pub(super) fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 1 << 30,
    }
}
pub(super) struct Reader<'a> {
    pub(super) pages: &'a Pages,
    pub(super) memory: &'a MemoryBudget,
    pub(super) class: MemoryClass,
}
pub(super) struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl ValueNodeReader for Reader<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != self.class || !memory.same_pool(self.memory) {
            return Err(ValueTreeError::Invalid("complete fixture phase/pool"));
        }
        let bytes = ValueNodeReader::read_node(self.pages, id, max, memory, MemoryClass::Ordinary)?;
        if class == MemoryClass::Ordinary {
            return Ok(bytes);
        }
        let charge = memory.reserve(bytes.len() + 128, class)?;
        Ok(Arc::new(charge.retain(bytes.to_vec())))
    }
}
impl MerkleNodeReader for Reader<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        ValueNodeReader::read_node(self, &id, max, self.memory, self.class)
            .map(|v| Some(Node(v)))
            .map_err(|e| e.to_string())
    }
}
pub(super) struct Originals<'a> {
    pub(super) values: &'a BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>,
    pub(super) memory: &'a MemoryBudget,
    pub(super) class: MemoryClass,
    pub(super) calls: usize,
}
impl OriginalObjectReader for Originals<'_> {
    type Error = &'static str;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    fn read_original(
        &mut self,
        _: &[u8; 32],
        _: usize,
        _: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        panic!("complete original must stream")
    }
}
impl OriginalObjectStreamReader for Originals<'_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        memory: &MemoryBudget,
        out: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        self.calls += 1;
        if !memory.same_pool(self.memory) {
            return Err("original pool");
        }
        let value = self.values.get(id).ok_or("missing original")?;
        if value.len() as u64 > max {
            return Err("original bound");
        }
        out.write_all(value).map_err(|_| "original output")?;
        Ok(value.len() as u64)
    }
}
fn persist(memory: &MemoryBudget, pages: &mut Pages, nodes: Vec<StateNodeWriteV1>) {
    for node in nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap();
        pages.put_node(id, charge.retain(node.bytes)).unwrap();
    }
}
pub(super) fn context<'a>(
    roots: &'a RootSetV1,
    reader: &'a Reader<'a>,
    quota: &'a mut ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Reader<'a>,
    Reader<'a>,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: reader,
        values: ValueReadContext {
            source: reader,
            memory: reader.memory,
            class: reader.class,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
pub(super) fn exercise(
    memory: &MemoryBudget,
    pages: &mut Pages,
    rows: &[ExecutionWriteV1],
    originals: &BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>,
) {
    let mut roots = RootSetV1::empty("complete-runtime46".into(), [7; 32]).unwrap();
    for root in &mut roots.state {
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == root.namespace)
            .map(|r| r.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        if writes.is_empty() {
            continue;
        }
        let delta = StateDeltaV1::build(
            root.clone(),
            writes,
            &Reader {
                pages,
                memory,
                class: MemoryClass::Ordinary,
            },
            16 << 20,
        )
        .unwrap();
        persist(memory, pages, delta.new_nodes);
        *root = delta.after;
    }
    whole::exercise(memory, pages, &roots, originals);
    let proof = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Ordinary,
        };
        let mut objects = Originals {
            values: originals,
            memory,
            class: MemoryClass::Ordinary,
            calls: 0,
        };
        let proof = qualify_runtime_typed_complete_v1(
            &roots,
            &mut context(
                &roots,
                &reader,
                &mut io(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 30,
                },
            ),
            &mut objects,
            &mut read_work(),
        )
        .unwrap();
        assert_eq!(objects.calls, 2);
        proof
    };
    assert_eq!(proof.allocation_class(), MemoryClass::Ordinary);
    proof.validate_binding(&roots).unwrap();
    use oasis7_world_state::incremental_runtime_allocators_codec::{
        RuntimeAllocatorV1, RuntimeSequenceV1,
    };
    use oasis7_world_state::incremental_runtime_allocators_family::*;
    let allocators = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Ordinary,
        };
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = context(&roots, &reader, &mut quota, &mut keys);
        ctx.root = roots.state_root(StateNamespaceV1::Allocator);
        qualify_runtime_allocators_v1(
            &roots,
            &mut ctx,
            &mut RuntimeAllocatorsWorkV1 {
                records_remaining: 4,
            },
        )
        .unwrap()
    };
    let action_before = allocators.sequence(RuntimeAllocatorV1::Action);
    let action_after = RuntimeSequenceV1 {
        next: action_before.next.checked_add(1).unwrap(),
        era: action_before.era,
    };
    let (action_bytes, action_length) = RuntimeAllocatorV1::Action.canonical_bytes(action_after);
    let action_value = write_value(
        &mut &action_bytes[..action_length],
        action_length as u64,
        pages,
        memory,
        &mut io(),
        || false,
    )
    .unwrap()
    .digest()
    .unwrap();
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [2; 32],
        authority: AuthorityBinding {
            token: [3; 32],
            epoch: 4,
        },
    };
    let field=oasis7_world_state::incremental_runtime_controls::RuntimeControlFieldV1::NextGovernanceIdentityPenaltyId.descriptor();
    let row = oasis7_world_state::incremental_field_member::write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        &(),
        oasis7_world_state::incremental_field_member::MemberPayloadRefV1::U64(17),
        &mut MemberRecordWriteContext {
            sink: pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let sealed = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Ordinary,
        };
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &reader,
            memory,
            TransactionLimits {
                max_reads: 32,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 16 << 20,
            },
        )
        .unwrap();
        tx.write(
            StateNamespaceV1::Allocator,
            row.field_key.as_bytes(),
            Some(row.value_root),
        )
        .unwrap();
        let (key, length) = RuntimeAllocatorV1::Action.key();
        tx.write(
            StateNamespaceV1::Allocator,
            &key[..length],
            Some(action_value),
        )
        .unwrap();
        let guard = tx.guard();
        tx.seal(&guard, &parent.fence().unwrap(), 1).unwrap()
    };
    persist(memory, pages, sealed.new_nodes.clone());
    let after = &sealed.record.after;
    let next = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Ordinary,
        };
        let mut objects = Originals {
            values: originals,
            memory,
            class: MemoryClass::Ordinary,
            calls: 0,
        };
        let next = accept_runtime_typed_complete_transition_v1(
            &proof,
            &TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &reader,
                memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 16 << 20,
            },
            &sealed.record,
            &mut context(
                after,
                &reader,
                &mut io(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 30,
                },
            ),
            &mut objects,
            &mut read_work(),
        )
        .unwrap();
        assert_eq!(
            objects.calls, 0,
            "unchanged original actions must not be reread"
        );
        next
    };
    let allocator_next = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Ordinary,
        };
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = context(after, &reader, &mut quota, &mut keys);
        ctx.root = after.state_root(StateNamespaceV1::Allocator);
        let q = accept_runtime_allocators_transition_v1(
            &allocators,
            &TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &reader,
                memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 16 << 20,
            },
            &sealed.record,
            &mut ctx,
            &mut RuntimeAllocatorsWorkV1 {
                records_remaining: 16,
            },
        )
        .unwrap();
        assert_eq!(q.sequence(RuntimeAllocatorV1::Action), action_after);
        q
    };
    allocator_next.validate_binding(after).unwrap();
    next.validate_binding(after).unwrap();
    drop(next);
    drop(proof);
    let recovered = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Recovery,
        };
        let mut objects = Originals {
            values: originals,
            memory,
            class: MemoryClass::Recovery,
            calls: 0,
        };
        qualify_runtime_typed_complete_v1(
            after,
            &mut context(
                after,
                &reader,
                &mut io(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 30,
                },
            ),
            &mut objects,
            &mut read_work(),
        )
        .unwrap()
    };
    assert_eq!(recovered.allocation_class(), MemoryClass::Recovery);
    let allocator_recovery = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Recovery,
        };
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = context(after, &reader, &mut quota, &mut keys);
        ctx.root = after.state_root(StateNamespaceV1::Allocator);
        qualify_runtime_allocators_v1(
            after,
            &mut ctx,
            &mut RuntimeAllocatorsWorkV1 {
                records_remaining: 4,
            },
        )
        .unwrap()
    };
    let allocator_ordinary =
        readmit_runtime_allocators_ordinary_v1(&allocator_recovery, after, memory, || false)
            .unwrap();
    assert_eq!(
        allocator_ordinary.sequence(RuntimeAllocatorV1::Action),
        action_after
    );
    assert!(
        readmit_runtime_allocators_ordinary_v1(&allocator_ordinary, after, memory, || false)
            .is_err()
    );

    let mut index =
        oasis7_world_state::incremental_pending_action_family::PendingActionIndexWorkV1 {
            steps_remaining: 1_000_000,
        };
    let ordinary =
        readmit_runtime_typed_complete_ordinary_v1(&recovered, after, memory, &mut index, || false)
            .unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    ordinary.validate_binding(after).unwrap();
    assert!(
        readmit_runtime_typed_complete_ordinary_v1(&ordinary, after, memory, &mut index, || false)
            .is_err()
    );
    let mut wrong = after.clone();
    wrong.height += 1;
    assert!(
        readmit_runtime_typed_complete_ordinary_v1(&recovered, &wrong, memory, &mut index, || {
            false
        })
        .is_err()
    );
    assert!(matches!(
        readmit_runtime_typed_complete_ordinary_v1(&recovered, after, memory, &mut index, || true),
        Err(ValueTreeError::Cancelled)
    ));
}

#[path = "incremental_world_current_complete_tests.rs"]
mod whole;
