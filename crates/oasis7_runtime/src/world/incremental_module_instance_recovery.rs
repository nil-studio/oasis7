//! Complete instance-family content verification at genesis/recovery only.
//! Rebuild counters in a private scratch Merkle tree; never expose that root.
use super::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError,
    incremental_scratch_counter::{CounterScratchStore, ScratchCounterRoot},
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_module_instance_fields::{
        ModuleInstanceFieldV1 as Field, ModuleInstanceKeyV1 as Key,
        ModuleInstanceScalarRefV1 as Scalar,
    },
    incremental_module_instance_index::*,
};
use std::cell::RefCell;

pub struct InstanceScratchBudget {
    pub reads: u64,
    pub read_bytes: u64,
    pub writes: u64,
    pub write_bytes: u64,
}
pub struct ModuleInstanceRecoveryContext<'a, R, V, S> {
    pub root: &'a StateRootV1,
    pub state: &'a R,
    pub values: &'a V,
    pub scratch: &'a S,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub state_io: &'a mut StateScanBudget,
    pub value_io: &'a mut ValueIoBudget,
    pub scratch_io: &'a mut InstanceScratchBudget,
    pub max_scalar_bytes: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedModuleInstanceContent {
    pub instances: u64,
    pub modules: u64,
}
struct Scratch<'a, 'b, S, C> {
    source: &'a S,
    io: &'a RefCell<&'b mut InstanceScratchBudget>,
    cancelled: &'a RefCell<C>,
}
impl<S: MerkleNodeReader, C: FnMut() -> bool> MerkleNodeReader for Scratch<'_, '_, S, C> {
    type Node = S::Node;
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if (self.cancelled.borrow_mut())() {
            return Err("instance scratch cancelled".into());
        }
        let admitted = {
            let mut io = self.io.borrow_mut();
            if io.reads == 0 || io.read_bytes == 0 {
                return Err("instance scratch read allowance".into());
            }
            io.reads -= 1;
            let n = (max as u64).min(io.read_bytes);
            io.read_bytes -= n;
            n as usize
        };
        let output = self.source.read_node(id, admitted)?;
        if let Some(node) = &output {
            if node.as_ref().len() > admitted {
                return Err("instance scratch source bound".into());
            }
            self.io.borrow_mut().read_bytes += (admitted - node.as_ref().len()) as u64;
        }
        Ok(output)
    }
}
impl<R: MerkleNodeReader, V: ValueNodeReader, S: CounterScratchStore>
    ModuleInstanceRecoveryContext<'_, R, V, S>
{
    pub fn verify(
        &mut self,
        cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<VerifiedModuleInstanceContent>, BodyMutationError> {
        let mut workspace = self
            .memory
            .reserve(64 << 10, self.class)
            .map_err(ValueTreeError::Memory)?;
        if self.root.namespace != StateNamespaceV1::Business
            || self.scratch.allocation_class() != Some(self.class)
            || !self
                .scratch
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(self.memory))
        {
            return Err(BodyMutationError::Invalid(
                "instance recovery namespace/pool/class",
            ));
        }
        let cancelled = RefCell::new(cancelled);
        let scratch_io = RefCell::new(&mut *self.scratch_io);
        let scratch = Scratch {
            source: self.scratch,
            io: &scratch_io,
            cancelled: &cancelled,
        };
        let mut rebuilt = ScratchCounterRoot::empty().map_err(ExecutionError::Tree)?;
        let mut rows = 0u64;
        let mut fields = 0u64;
        let mut count_leaves = 0u64;
        let mut declared = None;
        StateScanContext {
            source: self.state,
            memory: self.memory,
            class: self.class,
            io: self.state_io,
        }
        .visit_entries(
            self.root,
            |entry| {
                if entry.key == MODULE_INSTANCE_INDEX_KEY_V1 {
                    let bytes = load(
                        &entry.value,
                        self.values,
                        self.memory,
                        self.class,
                        self.value_io,
                        MODULE_INSTANCE_INDEX_BYTES as u64,
                        &cancelled,
                    )?;
                    declared = Some(
                        module_instance_index_from_bytes(&bytes)
                            .map_err(BodyMutationError::Invalid)?,
                    );
                } else if Key::is_instance_key(&entry.key) {
                    let (map_key, field) =
                        Key::from_bytes(&entry.key).map_err(BodyMutationError::Invalid)?;
                    let bytes = load(
                        &entry.value,
                        self.values,
                        self.memory,
                        self.class,
                        self.value_io,
                        self.max_scalar_bytes,
                        &cancelled,
                    )?;
                    let scalar = Scalar::from_canonical_bytes(&bytes, field)
                        .map_err(BodyMutationError::Invalid)?;
                    fields = fields
                        .checked_add(1)
                        .ok_or(BodyMutationError::Invalid("instance field count overflow"))?;
                    if field == Field::InstanceId {
                        rows = rows
                            .checked_add(1)
                            .ok_or(BodyMutationError::Invalid("instance row count overflow"))?;
                        // Every actual scalar is decoded by this complete pass. All
                        // eight keys plus total cardinality exclude orphan fields.
                        for required in Field::ALL {
                            let key =
                                Key::new(map_key, required).map_err(BodyMutationError::Invalid)?;
                            if point(
                                self.root,
                                key.as_bytes(),
                                self.state,
                                &scratch_io,
                                &cancelled,
                            )?
                            .is_none()
                            {
                                return Err(BodyMutationError::Invalid("incomplete instance row"));
                            }
                        }
                    } else if field == Field::ModuleId {
                        let Scalar::Text(module_id) = scalar else {
                            unreachable!("validated text")
                        };
                        let key = module_instance_count_key(module_id);
                        let mut pending = self
                            .memory
                            .reserve(1 << 20, self.class)
                            .map_err(ValueTreeError::Memory)?;
                        let delta = rebuilt
                            .increment(&key, &scratch, 256 << 10)
                            .map_err(ExecutionError::Tree)?;
                        let size = 1024
                            + delta.new_nodes.capacity()
                                * std::mem::size_of::<
                                    oasis7_formal_protocol::incremental_state::StateNodeWriteV1,
                                >()
                            + delta
                                .new_nodes
                                .iter()
                                .map(|node| node.bytes.capacity() + node.digest.capacity())
                                .sum::<usize>();
                        let delta = pending
                            .split(size)
                            .map_err(BodyMutationError::Invalid)?
                            .retain(delta);
                        drop(pending);
                        let bytes = delta
                            .new_nodes
                            .iter()
                            .try_fold(0u64, |sum, node| sum.checked_add(node.bytes.len() as u64))
                            .ok_or(BodyMutationError::Invalid("scratch state size overflow"))?;
                        {
                            let mut io = scratch.io.borrow_mut();
                            if io.writes < delta.new_nodes.len() as u64 || io.write_bytes < bytes {
                                return Err(ValueTreeError::ResourceLimited(
                                    "instance scratch writes",
                                )
                                .into());
                            }
                            io.writes -= delta.new_nodes.len() as u64;
                            io.write_bytes -= bytes;
                        }
                        if (cancelled.borrow_mut())() {
                            return Err(ValueTreeError::Cancelled.into());
                        }
                        self.scratch
                            .put_scratch_pages(&delta)
                            .map_err(ExecutionError::Tree)?;
                        rebuilt = delta.after.clone();
                    }
                } else if is_module_instance_count_key(&entry.key) {
                    validate_module_instance_count_key(&entry.key)
                        .map_err(BodyMutationError::Invalid)?;
                    count_leaves = count_leaves
                        .checked_add(1)
                        .ok_or(BodyMutationError::Invalid("module count leaves overflow"))?;
                }
                Ok::<_, BodyMutationError>(())
            },
            || (cancelled.borrow_mut())(),
        )
        .map_err(scan_error)?;
        if declared != Some(rows)
            || fields
                != rows
                    .checked_mul(8)
                    .ok_or(BodyMutationError::Invalid("instance field total overflow"))?
            || count_leaves != rebuilt.entries
        {
            return Err(BodyMutationError::Invalid("instance family cardinality"));
        }
        StateScanContext {
            source: self.state,
            memory: self.memory,
            class: self.class,
            io: self.state_io,
        }
        .visit_entries(
            self.root,
            |entry| {
                if is_module_instance_count_key(&entry.key) {
                    let bytes = load(
                        &entry.value,
                        self.values,
                        self.memory,
                        self.class,
                        self.value_io,
                        MODULE_INSTANCE_COUNT_BYTES as u64,
                        &cancelled,
                    )?;
                    let actual = module_instance_count_from_bytes(&bytes)
                        .map_err(BodyMutationError::Invalid)?;
                    let expected = rebuilt
                        .get(&entry.key, &scratch)
                        .map_err(ExecutionError::Tree)?
                        .ok_or(BodyMutationError::Invalid("extra module instance count"))?;
                    if actual != expected {
                        return Err(BodyMutationError::Invalid("module instance count mismatch"));
                    }
                }
                Ok::<_, BodyMutationError>(())
            },
            || (cancelled.borrow_mut())(),
        )
        .map_err(scan_error)?;
        if (cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(workspace
            .split(128)
            .map_err(BodyMutationError::Invalid)?
            .retain(VerifiedModuleInstanceContent {
                instances: rows,
                modules: count_leaves,
            }))
    }
}
fn load<V: ValueNodeReader, C: FnMut() -> bool>(
    id: &[u8; 32],
    values: &V,
    memory: &MemoryBudget,
    class: MemoryClass,
    io: &mut ValueIoBudget,
    max: u64,
    cancelled: &RefCell<C>,
) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
    let mut read = ValueReadContext {
        source: values,
        memory,
        class,
        io,
    };
    let root = read.load_root(id)?;
    if root.0.logical_len > max {
        return Err(ValueTreeError::ResourceLimited("instance recovery scalar").into());
    }
    let length = usize::try_from(root.0.logical_len)
        .map_err(|_| BodyMutationError::Invalid("instance scalar length"))?;
    let charge = memory
        .reserve(length, class)
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::with_capacity(length);
    read.read_range(&root, 0, root.0.logical_len, &mut bytes, || {
        (cancelled.borrow_mut())()
    })?;
    Ok(charge.retain(bytes))
}
fn scan_error(error: StateScanError<BodyMutationError>) -> BodyMutationError {
    match error {
        StateScanError::Visitor(error) => error,
        StateScanError::Memory(error) => ValueTreeError::Memory(error).into(),
        StateScanError::ResourceLimited => {
            ValueTreeError::ResourceLimited("instance recovery scan").into()
        }
        StateScanError::Cancelled => ValueTreeError::Cancelled.into(),
        StateScanError::Tree(error) => ExecutionError::Tree(error).into(),
        StateScanError::Invalid(reason) => BodyMutationError::Invalid(reason),
    }
}
// Extra row-completeness proofs share the finite scratch/read allowance.
fn point<R: MerkleNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    key: &[u8],
    source: &R,
    io: &RefCell<&mut InstanceScratchBudget>,
    cancelled: &RefCell<C>,
) -> Result<Option<[u8; 32]>, BodyMutationError> {
    let reader = Scratch {
        source,
        io,
        cancelled,
    };
    Ok(root.get(key, &reader).map_err(ExecutionError::Tree)?)
}
