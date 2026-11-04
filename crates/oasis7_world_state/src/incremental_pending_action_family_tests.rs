use super::*;
use crate::{
    incremental_pending_action_original_binding::tests::{Objects, work as original_work},
    incremental_runtime_pending_action_record::tests::scheduler_fixture,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, io},
};
use oasis7_proto::memory_budget::MemoryBudget;
fn work() -> PendingActionFamilyWorkV1 {
    PendingActionFamilyWorkV1 {
        index: PendingActionIndexWorkV1 {
            steps_remaining: 100000,
        },
        typed: TypedRecordReadWorkV1 {
            json: crate::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 100000,
                json_nodes_remaining: 100000,
                max_depth: 32,
                max_payload_bytes: 2 << 20,
            },
            max_retained_trees: 4096,
            max_tree_heap: 2 << 20,
        },
        original: original_work(),
        selected_scan: scan(),
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10000000,
        read_bytes_remaining: 512 << 20,
        leaves_remaining: 100000,
    }
}
fn qualify(
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
    objects: &mut Objects,
    w: &mut PendingActionFamilyWorkV1,
) -> Result<ReservedValue<PendingActionQualificationV1>, PendingActionFamilyErrorV1<std::io::Error>>
{
    qualify_pending_actions_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Scheduler),
            state: pages,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            semantic_bytes: 16384,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        w,
        objects,
    )
}
#[test]
fn full_pending_family_requires_actual_original_and_exact_roots() {
    let (memory, pages, root, bytes) = scheduler_fixture();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let mut roots = RootSetV1::empty("world".into(), [0; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Scheduler)
        .unwrap() = root;
    let mut objects = Objects {
        memory: memory.clone(),
        class: MemoryClass::Ordinary,
        bytes: bytes.clone(),
        swallow: false,
    };
    let q = qualify(&roots, &pages, &memory, &mut objects, &mut work()).unwrap();
    assert_eq!(q.index().len(), 1);
    q.validate_binding(&roots).unwrap();
    let mut different = roots.clone();
    different.height = 1;
    assert!(q.validate_binding(&different).is_err());
    assert!(
        readmit_pending_actions_v1(&q, &roots, &memory, &mut work().index, &mut || false).is_err()
    );
    objects.bytes[0] ^= 1;
    assert!(qualify(&roots, &pages, &memory, &mut objects, &mut work()).is_err());
    objects.bytes = bytes;
    objects.class = MemoryClass::Recovery;
    assert!(qualify(&roots, &pages, &memory, &mut objects, &mut work()).is_err());
    objects.class = MemoryClass::Ordinary;
    let mut limited = work();
    limited.index.steps_remaining = 0;
    assert!(qualify(&roots, &pages, &memory, &mut objects, &mut limited).is_err());
    let empty = RootSetV1::empty("world".into(), [0; 32]).unwrap();
    assert!(qualify(&empty, &pages, &memory, &mut objects, &mut work()).is_err());
}

fn row(p: Position, id: u64, pages: &Pages, memory: &MemoryBudget) -> MemberRecordV1 {
    use crate::incremental_typed_record_test_support::Writer;
    let envelope = crate::ActionEnvelope {
        governance_authorization: None,
        committed_recipe_origin: None,
        id,
        action: crate::Action::DeployModuleArtifact {
            publisher_agent_id: "publisher".into(),
            wasm_hash: "hash".into(),
            wasm_bytes: vec![0, 255, 0, 255],
        },
    };
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&envelope).unwrap();
    let original = oasis7_formal_protocol::incremental_artifacts::OriginalArtifactRefV1 {
        id: *blake3::hash(&bytes).as_bytes(),
        byte_len: bytes.len() as u64,
    };
    crate::incremental_runtime_pending_action_record::prepare_pending_action_record_v1(
        p,
        &envelope,
        original,
        &mut MemberRecordWriteContext {
            sink: &mut Writer(pages),
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 16384,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut TypedRecordWriteWorkV1 {
            cells_remaining: 100000,
            max_pending_node_bytes: 8 << 20,
            json: crate::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 100000,
                json_nodes_remaining: 100000,
                max_json_depth: 32,
                max_pending_node_bytes: 8 << 20,
            },
        },
    )
    .unwrap()
}
fn derive(
    q: &ReservedValue<PendingActionQualificationV1>,
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
    writes: Vec<(Vec<u8>, Option<[u8; 32]>)>,
    objects: &mut Objects,
) -> Result<
    (ReservedValue<PendingActionQualificationV1>, RootSetV1),
    PendingActionFamilyErrorV1<std::io::Error>,
> {
    use crate::incremental_typed_record_test_support::Writer;
    use oasis7_formal_protocol::incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
    };
    let parent = ExecutionParent {
        commit: [4; 32],
        roots: roots.clone(),
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [6; 32],
        pages,
        memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    for (key, value) in writes {
        tx.write(StateNamespaceV1::Scheduler, &key, value).unwrap();
    }
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, roots.height + 1).unwrap();
    for node in &prepared.new_nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: pages,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let record = &prepared.record;
    let result = accept_pending_actions_transition_v1(
        q,
        &verifier,
        record,
        &mut WorldFieldReadContextV1 {
            root: record.after.state_root(StateNamespaceV1::Scheduler),
            state: pages,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            semantic_bytes: 16384,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut work(),
        objects,
    )?;
    Ok((result, record.after.clone()))
}
#[test]
fn replayed_creation_era_append_pop_preserves_positions_and_rejects_rewrite_wrong_era() {
    let (memory, pages, root, bytes) = scheduler_fixture();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let mut roots = RootSetV1::empty("pending-d".into(), [0; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Scheduler)
        .unwrap() = root;
    let mut objects = Objects {
        memory: memory.clone(),
        class: MemoryClass::Ordinary,
        bytes: bytes.clone(),
        swallow: false,
    };
    let q = qualify(&roots, &pages, &memory, &mut objects, &mut work()).unwrap();
    objects.bytes.clear();
    let (noop, noop_roots) = derive(&q, &roots, &pages, &memory, vec![], &mut objects).unwrap();
    assert_eq!(noop.index().len(), 1);
    objects.bytes = bytes;
    let append = row(Position { era: 2, ordinal: 0 }, 17, &pages, &memory);
    let old = row(Position { era: 0, ordinal: 0 }, 17, &pages, &memory);
    let (next, next_roots) = derive(
        &noop,
        &noop_roots,
        &pages,
        &memory,
        vec![
            (
                append.field_key.as_bytes().to_vec(),
                Some(append.value_root),
            ),
            (old.field_key.as_bytes().to_vec(), None),
        ],
        &mut objects,
    )
    .unwrap();
    assert_eq!(next.index().len(), 1);
    assert_eq!(
        next.index()
            .first(&mut work().index, &mut || false)
            .unwrap()
            .unwrap()
            .position
            .ordinal,
        0
    );
    let valid_gap = row(Position { era: 3, ordinal: 3 }, 17, &pages, &memory);
    assert!(
        derive(
            &next,
            &next_roots,
            &pages,
            &memory,
            vec![(
                valid_gap.field_key.as_bytes().to_vec(),
                Some(valid_gap.value_root)
            )],
            &mut objects
        )
        .is_ok()
    );
    let gap = row(Position { era: 0, ordinal: 3 }, 17, &pages, &memory);
    assert!(
        derive(
            &next,
            &next_roots,
            &pages,
            &memory,
            vec![(gap.field_key.as_bytes().to_vec(), Some(gap.value_root))],
            &mut objects
        )
        .is_err()
    );
    let rewritten = row(Position { era: 2, ordinal: 0 }, 18, &pages, &memory);
    assert!(
        derive(
            &next,
            &next_roots,
            &pages,
            &memory,
            vec![(
                rewritten.field_key.as_bytes().to_vec(),
                Some(rewritten.value_root)
            )],
            &mut objects
        )
        .is_err()
    );
    assert!(
        derive(
            &next,
            &next_roots,
            &pages,
            &memory,
            vec![
                (append.field_key.as_bytes().to_vec(), None),
                (old.field_key.as_bytes().to_vec(), Some(old.value_root))
            ],
            &mut objects
        )
        .is_err()
    );
}

struct RecoveryPages<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
}
struct RecoveryNode(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for RecoveryNode {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl MerkleNodeReader for RecoveryPages<'_> {
    type Node = RecoveryNode;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let node = MerkleNodeReader::read_node(self.pages, id, max)?;
        node.map(|n| {
            let bytes = n.as_ref();
            let charge = self
                .memory
                .reserve(bytes.len(), MemoryClass::Recovery)
                .map_err(|e| e.to_string())?;
            Ok(RecoveryNode(charge.retain(bytes.to_vec())))
        })
        .transpose()
    }
}
impl ValueNodeReader for RecoveryPages<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if !memory.same_pool(self.memory) || class != MemoryClass::Recovery {
            return Err(invalid());
        }
        let source =
            ValueNodeReader::read_node(self.pages, id, max, self.memory, MemoryClass::Ordinary)?;
        let charge = self.memory.reserve(source.len(), class)?;
        Ok(std::sync::Arc::new(charge.retain(source.to_vec())))
    }
}
#[test]
fn recovery_family_requires_explicit_per_node_ordinary_readmission() {
    let (memory, pages, root, bytes) = scheduler_fixture();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let source = RecoveryPages {
        pages: &pages,
        memory: &memory,
    };
    let mut roots = RootSetV1::empty("pending-recovery".into(), [0; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Scheduler)
        .unwrap() = root;
    let mut objects = Objects {
        memory: memory.clone(),
        class: MemoryClass::Recovery,
        bytes,
        swallow: false,
    };
    let q = qualify_pending_actions_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Scheduler),
            state: &source,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            semantic_bytes: 16384,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut work(),
        &mut objects,
    )
    .unwrap();
    assert_eq!(q.allocation_class(), MemoryClass::Recovery);
    let ordinary =
        readmit_pending_actions_v1(&q, &roots, &memory, &mut work().index, &mut || false).unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(ordinary.index().len(), 1);
    assert!(
        readmit_pending_actions_v1(
            &q,
            &roots,
            &MemoryBudget::new(128 << 20, 8 << 20).unwrap(),
            &mut work().index,
            &mut || false
        )
        .is_err()
    );
}

#[test]
fn empty_recovery_readmission_checks_entry_final_cancellation_and_source_class() {
    let memory = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    let roots = RootSetV1::empty("empty-readmit".into(), [0; 32]).unwrap();
    let recovery = mint::<std::io::Error>(
        &roots,
        PendingActionIndexV1::new(&memory, MemoryClass::Recovery),
        &memory,
    )
    .unwrap();
    assert!(matches!(
        readmit_pending_actions_v1(&recovery, &roots, &memory, &mut work().index, &mut || true),
        Err(ValueTreeError::Cancelled)
    ));
    let mut calls = 0;
    assert!(matches!(
        readmit_pending_actions_v1(&recovery, &roots, &memory, &mut work().index, &mut || {
            calls += 1;
            calls == 4
        }),
        Err(ValueTreeError::Cancelled)
    ));
    let ordinary =
        readmit_pending_actions_v1(&recovery, &roots, &memory, &mut work().index, &mut || false)
            .unwrap();
    assert_eq!(ordinary.len(), 0);
    assert!(
        readmit_pending_actions_v1(&ordinary, &roots, &memory, &mut work().index, &mut || false)
            .is_err()
    );
    let mut wrong = roots.clone();
    wrong.height = 1;
    assert!(
        readmit_pending_actions_v1(&recovery, &wrong, &memory, &mut work().index, &mut || false)
            .is_err()
    );
    assert!(
        readmit_pending_actions_v1(
            &recovery,
            &roots,
            &MemoryBudget::new(8 << 20, 1 << 20).unwrap(),
            &mut work().index,
            &mut || false
        )
        .is_err()
    );
    assert!(matches!(
        readmit_pending_actions_v1(&ordinary, &wrong, &memory, &mut work().index, &mut {
            let mut n = 0;
            move || {
                n += 1;
                n == 2
            }
        }),
        Err(ValueTreeError::Cancelled)
    ));
}
