//! Bounded fixed-root field reads; typed adapters bind registered descriptors.
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::FieldDescriptor,
    incremental_semantic_key::{SemanticKeyBudget, SemanticKeyV1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use std::cell::{Cell, RefCell};
pub struct WorldFieldReadContextV1<'a, M, R, E, C> {
    pub root: &'a StateRootV1,
    pub state: &'a M,
    pub values: ValueReadContext<'a, R>,
    pub key_work: &'a mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    /// Recover the original typed failure across Merkle's String boundary.
    /// Any outstanding failure aborts the current lookup, even after Ok.
    pub take_state_error: E,
    pub cancelled: C,
}
struct CancelState<'a, M, C> {
    source: &'a M,
    cancelled: RefCell<&'a mut C>,
    seen: Cell<bool>,
}
impl<M: MerkleNodeReader, C: FnMut() -> bool> MerkleNodeReader for CancelState<'_, M, C> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<oasis7_proto::memory_budget::MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&oasis7_proto::memory_budget::MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Self::Node>, String> {
        if (self.cancelled.borrow_mut())() {
            self.seen.set(true);
            return Err("inventory cancelled".into());
        }
        let result = self.source.read_node(digest, max_bytes);
        if (self.cancelled.borrow_mut())() {
            self.seen.set(true);
            return Err("inventory cancelled".into());
        }
        result
    }
}
impl<
    'v,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> WorldFieldReadContextV1<'v, M, R, E, C>
{
    pub(crate) fn check(&mut self) -> Result<(), ValueTreeError> {
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        if self.state.allocation_class() != Some(self.values.class)
            || !self
                .state
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(self.values.memory))
        {
            return Err(ValueTreeError::Invalid("inventory state pool/class"));
        }
        Ok(())
    }
    pub(crate) fn lookup(
        &mut self,
        key: &WorldFieldKeyV1,
    ) -> Result<Option<[u8; 32]>, ValueTreeError> {
        self.check()?;
        let expected =
            WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
        if self.root.namespace != expected.namespace {
            return Err(ValueTreeError::Invalid("field namespace"));
        }
        let _workspace = self.values.memory.reserve(512 << 10, self.values.class)?;
        let (result, cancelled) = {
            let source = CancelState {
                source: self.state,
                cancelled: RefCell::new(&mut self.cancelled),
                seen: Cell::new(false),
            };
            let result = self.root.get(key.as_bytes(), &source);
            (result, source.seen.get())
        };
        let original = (self.take_state_error)();
        if cancelled || (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = original {
            return Err(error);
        }
        result.map_err(|_| ValueTreeError::Invalid("inventory state lookup"))
    }
    pub(crate) fn records(&mut self) -> InventoryValueReader<'_, 'v, R, C> {
        InventoryValueReader {
            values: &mut self.values,
            key_work: self.key_work,
            semantic_bytes: self.semantic_bytes,
            record_bytes: self.record_bytes,
            cancelled: &mut self.cancelled,
        }
    }
    pub(crate) fn key<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        kind: WorldFieldRecordKindV1,
        key: &K,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        self.records().key(field, kind, key)
    }
    pub(crate) fn quantity(
        &mut self,
        value: &[u8; 32],
        key: &WorldFieldKeyV1,
        semantic_bytes: u64,
    ) -> Result<i64, ValueTreeError> {
        self.records().quantity(value, key, semantic_bytes)
    }
}
