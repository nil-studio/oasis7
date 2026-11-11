//! Recovery reads genuinely copy each physical page under Recovery admission;
//! only the explicit same-pool readmission creates an Ordinary metadata proof.
use super::*;
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use std::{cell::RefCell, sync::Arc};
struct Recovery<'a> {
    base: &'a Pages,
    memory: MemoryBudget,
    error: RefCell<Option<ValueTreeError>>,
}
struct Copied(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Copied {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl MerkleNodeReader for Recovery<'_> {
    type Node = Copied;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let Some(source) = MerkleNodeReader::read_node(self.base, digest, max)? else {
            return Ok(None);
        };
        let charge = match self
            .memory
            .reserve(source.as_ref().len() + 256, MemoryClass::Recovery)
        {
            Ok(charge) => charge,
            Err(error) => {
                *self.error.borrow_mut() = Some(ValueTreeError::Memory(error));
                return Err("recovery state copy admission".into());
            }
        };
        Ok(Some(Copied(charge.retain(source.as_ref().to_vec()))))
    }
}
impl ValueNodeReader for Recovery<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Recovery || !memory.same_pool(&self.memory) {
            return Err(ValueTreeError::Invalid("recovery fixture pool/class"));
        }
        let source = ValueNodeReader::read_node(self.base, id, max, memory, MemoryClass::Ordinary)?;
        let charge = memory.reserve(source.len() + 256, MemoryClass::Recovery)?;
        Ok(Arc::new(charge.retain(source.as_slice().to_vec())))
    }
}
#[test]
fn native_field_recovery_qualification_requires_explicit_same_pool_ordinary_readmission() {
    let memory = MemoryBudget::new(192 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let rows = rows::<MetaProgressFieldV1>(&memory, &pages, &[("outer".into(), meta("inner"))]);
    let roots = install(&memory, &pages, StateNamespaceV1::Business, &rows);
    let recovery = Recovery {
        base: &pages,
        memory: memory.clone(),
        error: RefCell::new(None),
    };
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut context = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Business),
        state: &recovery,
        values: ValueReadContext {
            source: &recovery,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut quota,
        },
        key_work: &mut keys,
        semantic_bytes: 1 << 20,
        record_bytes: 3 << 20,
        take_state_error: || recovery.error.borrow_mut().take(),
        cancelled: || false,
    };
    let proof = qualify_world_native_field_v1::<MetaProgressFieldV1>(
        &roots,
        &mut context,
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.class, MemoryClass::Recovery);
    let ordinary =
        readmit_world_native_field_ordinary_v1(&proof, &roots, &memory, || false).unwrap();
    assert_eq!(ordinary.class, MemoryClass::Ordinary);
    assert_eq!(ordinary.records(), 1);
    ordinary.validate_binding(&roots).unwrap();
    let foreign = MemoryBudget::new(192 << 20, 16 << 20).unwrap();
    assert!(readmit_world_native_field_ordinary_v1(&proof, &roots, &foreign, || false).is_err());
    let mut wrong = roots.clone();
    wrong.world_id.push_str("-wrong");
    assert!(readmit_world_native_field_ordinary_v1(&proof, &wrong, &memory, || false).is_err());
    assert!(matches!(
        readmit_world_native_field_ordinary_v1(&proof, &roots, &memory, || true),
        Err(ValueTreeError::Cancelled)
    ));
    let used = memory.used();
    let held = ordinary.reserved_bytes();
    drop(ordinary);
    assert_eq!(memory.used(), used - held);
}
