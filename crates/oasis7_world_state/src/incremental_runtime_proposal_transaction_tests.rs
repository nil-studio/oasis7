//! A real status transaction and independent replay must not read old JSON.
use super::*;
use std::{cell::Cell, collections::BTreeSet, rc::Rc};
struct DenyJson<'a> {
    inner: Combined<'a>,
    blocked: Rc<RefCell<BTreeSet<[u8; 32]>>>,
    hits: Rc<Cell<usize>>,
}
impl ValueNodeReader for DenyJson<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if self.blocked.borrow().contains(id) {
            self.hits.set(self.hits.get() + 1);
            return Err(ValueTreeError::Invalid("unexpected unchanged JSON read"));
        }
        ValueNodeReader::read_node(&self.inner, id, max, memory, class)
    }
}
impl MerkleNodeReader for DenyJson<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.inner.base.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let hash = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        if self.blocked.borrow().contains(&hash) {
            self.hits.set(self.hits.get() + 1);
            return Err("unexpected unchanged JSON state read".into());
        }
        MerkleNodeReader::read_node(&self.inner, id, max)
    }
}
#[test]
fn proposal_status_replay_skips_unchanged_manifest_and_patch_json() {
    let memory = MemoryBudget::new(192 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let mut original = proposal(ProposalStatus::Proposed, true);
    original.manifest.content = serde_json::Value::String("x".repeat(256 << 10));
    let mut entries = BTreeMap::new();
    entries.insert(13, original);
    let (pages, roots) = seed(&memory, &entries);
    drop(entries);
    let mut read_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut plain = context(&pages, &roots, &memory, &mut read_io, &mut keys);
    plain.record_bytes = 3 << 20;
    plain.semantic_bytes = 4096;
    let family = qualify_runtime_proposals_v1(&roots, &mut plain, &mut scan(), &mut rw()).unwrap();
    let mut scope = read_runtime_proposal_body_qualification_v1(&family, &roots, 13, &mut plain)
        .unwrap()
        .unwrap();
    let mut columns = crate::incremental_record_columns_read::read_columns_v1(
        crate::incremental_record_columns_read::ColumnReadProfileV1 {
            field: super::super::columns(),
            schema: schema(),
            groups: &[],
            reference_schema: Some(recovery::reference_schema),
        },
        scope.root(),
        &mut plain,
        &mut scan(),
        4 << 20,
    )
    .unwrap();
    let blocked = Rc::new(RefCell::new(BTreeSet::new()));
    for cell in columns.cells_mut().values() {
        if let crate::incremental_record_columns_read::ColumnCellV1::Root(root) = cell {
            blocked
                .borrow_mut()
                .insert(*blake3::Hash::from_hex(&root.node_root).unwrap().as_bytes());
        }
    }
    assert!(blocked.borrow().len() >= 2);
    drop(columns);
    let extra = RefCell::new(Pages::new(&memory));
    let hits = Rc::new(Cell::new(0));
    let values = DenyJson {
        inner: Combined {
            base: &pages,
            extra: &extra,
        },
        blocked,
        hits: hits.clone(),
    };
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
        &values,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let mut reads = io();
    let mut writes = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut sink = Sink(&extra);
    let mut read = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: &values,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut reads,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut writes,
        key_work: &mut wk,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    };
    let mut expected = ProposalStatus::Proposed;
    let mut work = rw();
    for after in [
        ProposalStatus::Approved {
            manifest_hash: "h".into(),
            approver: "a".into(),
        },
        ProposalStatus::Applied {
            manifest_hash: "h".into(),
        },
    ] {
        let guard = tx.guard();
        scope = change_runtime_proposal_status_v1(
            RuntimeProposalStatusChangeV1 {
                expected: &expected,
                after: &after,
                max_pending_node_bytes: 1 << 20,
            },
            &scope,
            &mut tx,
            &mut read,
            &mut write,
            &guard,
            &live,
            &mut work,
        )
        .unwrap()
        .unwrap();
        expected = after;
    }
    assert_eq!(hits.get(), 0);
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    for node in &prepared.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        extra
            .borrow_mut()
            .put_node(
                id,
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let mut vi = io();
    let mut after_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: prepared
            .record
            .after
            .state_root(StateNamespaceV1::Authorization),
        state: &values,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut after_keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let inherited = accept_runtime_proposals_transition_v1(
        &family,
        &verifier,
        &prepared.record,
        &mut ctx,
        &mut scan(),
        &mut rw(),
    )
    .unwrap();
    assert_eq!(inherited.proposals(), 1);
    assert_eq!(
        hits.get(),
        0,
        "independent receiver must skip old JSON graphs too"
    );
    values.blocked.borrow_mut().clear();
    let recovered = recover_runtime_proposal_body_v1(
        scope.root(),
        &mut ctx,
        &mut scan(),
        &mut rw().json,
        4 << 20,
    )
    .unwrap();
    assert_eq!(
        recovered.value().status,
        ProposalStatus::Applied {
            manifest_hash: "h".into()
        }
    );
}
