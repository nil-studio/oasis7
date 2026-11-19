use super::*;
use oasis7_formal_protocol::incremental_execution::*;
struct Classed<'a> {
    pages: &'a Pages,
    class: MemoryClass,
}
struct ClassedNode(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for ClassedNode {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl Classed<'_> {
    fn copy(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
        if class != self.class || !self.pages.memory.same_pool(memory) {
            return Err(invalid());
        }
        let pages = self.pages.nodes.borrow();
        let source = pages.get(id).ok_or_else(invalid)?;
        if source.len() > max {
            return Err(invalid());
        }
        let charge = memory.reserve(source.len().checked_add(128).ok_or_else(invalid)?, class)?;
        Ok(charge.retain(source.as_slice().to_vec()))
    }
}
impl ValueNodeReader for Classed<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        Ok(Arc::new(self.copy(id, max, memory, class)?))
    }
}
impl MerkleNodeReader for Classed<'_> {
    type Node = ClassedNode;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.pages.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<ClassedNode>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        if !self.pages.nodes.borrow().contains_key(&id) {
            return Ok(None);
        }
        self.copy(&id, max, &self.pages.memory, self.class)
            .map(|n| Some(ClassedNode(n)))
            .map_err(|e| format!("{e:?}"))
    }
}
#[test]
fn recovery_controls_cannot_enter_ordinary_receiver_without_readmission() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let (pages, roots) = seed(&memory, "authority", None);
    let recovery = Classed {
        pages: &pages,
        class: MemoryClass::Recovery,
    };
    let mut values = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let qualified = qualify_runtime_controls_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &recovery,
            values: ValueReadContext {
                source: &recovery,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut values,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            leaves_remaining: 1000,
            read_bytes_remaining: 64 << 20,
        },
        1024,
    )
    .unwrap();
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let tx = ExecutionTransaction::begin(
        parent.clone(),
        [6; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let mut values = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let result = accept_runtime_controls_transition_v1(
        &qualified,
        &verifier,
        &sealed.record,
        &mut WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut values,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        1024,
    );
    assert!(
        result.is_err(),
        "Recovery family evidence bypassed explicit Ordinary admission"
    );
}

#[path = "incremental_runtime_controls_readmission_tests.rs"]
mod readmission_tests;
