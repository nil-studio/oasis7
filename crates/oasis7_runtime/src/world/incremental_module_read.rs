//! Point reads for the module owner. Manifest/artifact/authorization selection
//! and the full tick route remain separate obligations.
use super::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_wasm_abi::ModuleKind;
use oasis7_world_state::{
    incremental_module_fields::ModuleStateKeyV1,
    incremental_module_instance_fields::{
        ModuleInstanceFieldV1 as Field, ModuleInstanceKeyV1, ModuleInstanceScalarRefV1 as Scalar,
    },
    incremental_module_instance_index::*,
};

pub struct ModuleReadContext<'a, V, C> {
    pub values: &'a V,
    pub memory: &'a MemoryBudget,
    pub io: &'a mut ValueIoBudget,
    pub max_value_bytes: u64,
    pub cancelled: C,
}
/// Content only, not an invocation/authority capability. The transaction keeps
/// the authenticated reads; borrowed getters never detach charged buffers.
pub struct PreparedModuleInstanceFields {
    fields: [ReservedValue<Vec<u8>>; 8],
}
impl PreparedModuleInstanceFields {
    pub fn field(&self, field: Field) -> Scalar<'_> {
        Scalar::from_canonical_bytes(&self.fields[field as usize - 1], field)
            .expect("immutable validated module field")
    }
}
impl<V: ValueNodeReader, C: FnMut() -> bool> ModuleReadContext<'_, V, C> {
    pub(super) fn check<R: MerkleNodeReader>(
        &mut self,
        txn: &ExecutionTransaction<'_, R>,
    ) -> Result<(), BodyMutationError> {
        if !txn.uses_budget(self.memory) {
            return Err(BodyMutationError::Invalid("module read transaction pool"));
        }
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    }
    pub(super) fn load(
        &mut self,
        id: &[u8; 32],
    ) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        let mut reader = ValueReadContext {
            source: self.values,
            memory: self.memory,
            class: MemoryClass::Ordinary,
            io: self.io,
        };
        let root = reader.load_root(id)?;
        if root.0.logical_len > self.max_value_bytes {
            return Err(ValueTreeError::ResourceLimited("module value bytes").into());
        }
        let length = usize::try_from(root.0.logical_len)
            .map_err(|_| ValueTreeError::ResourceLimited("module value allocation"))?;
        let charge = self
            .memory
            .reserve(
                length.checked_add(128).ok_or(BodyMutationError::Invalid(
                    "module value allocation overflow",
                ))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut bytes = vec![0; length];
        if length == 0 {
            reader.verify_closure(id, 0, &mut self.cancelled)?;
        } else {
            reader.read_range(
                &root,
                0,
                root.0.logical_len,
                &mut bytes.as_mut_slice(),
                &mut self.cancelled,
            )?;
        }
        Ok(charge.retain(bytes))
    }
    pub(super) fn instance_index_total<R: MerkleNodeReader>(
        &mut self,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<u64, BodyMutationError> {
        self.check(txn)?;
        let id = txn
            .read(Ns::Business, MODULE_INSTANCE_INDEX_KEY_V1)?
            .ok_or(BodyMutationError::Invalid(
                "module instance count index absent",
            ))?;
        let value = self.load(&id)?;
        module_instance_index_from_bytes(&value).map_err(BodyMutationError::Invalid)
    }
    /// All stored instances count, including inactive ones. A missing index is
    /// not zero. The parent owner must establish count/field consistency.
    pub fn module_instance_count<R: MerkleNodeReader>(
        &mut self,
        module_id: &str,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<u64, BodyMutationError> {
        txn.contribute(|txn| {
            let total = self.instance_index_total(txn)?;
            let key = module_instance_count_key(module_id);
            let count = match txn.read(Ns::Business, &key)? {
                None => 0,
                Some(id) => {
                    let value = self.load(&id)?;
                    module_instance_count_from_bytes(&value).map_err(BodyMutationError::Invalid)?
                }
            };
            if count > total {
                return Err(BodyMutationError::Invalid(
                    "module instance count exceeds total",
                ));
            }
            self.check(txn)?;
            Ok(count)
        })
    }
    /// Read only the requested scalar, e.g. Active before loading invocation
    /// metadata. Parent qualification owns complete-row/absence invariants.
    pub fn read_instance_field<R: MerkleNodeReader>(
        &mut self,
        map_key: &str,
        field: Field,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, BodyMutationError> {
        txn.contribute(|txn| {
            self.check(txn)?;
            let key =
                ModuleInstanceKeyV1::new(map_key, field).map_err(BodyMutationError::Invalid)?;
            let Some(id) = txn.read(Ns::Business, key.as_bytes())? else {
                self.check(txn)?;
                return Ok(None);
            };
            let value = self.load(&id)?;
            Scalar::from_canonical_bytes(&value, field).map_err(BodyMutationError::Invalid)?;
            self.check(txn)?;
            Ok(Some(value))
        })
    }
    /// A Pure call supplies no state and does not touch this state key. A
    /// Reducer receives a present charged empty buffer only for proven absence.
    /// `kind` must come from the caller's qualified manifest, not a cache guess.
    pub fn read_state<R: MerkleNodeReader>(
        &mut self,
        state_key: &str,
        kind: &ModuleKind,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, BodyMutationError> {
        txn.contribute(|txn| {
            self.check(txn)?;
            if *kind == ModuleKind::Pure {
                return Ok(None);
            }
            let key = ModuleStateKeyV1::new(state_key).map_err(BodyMutationError::Invalid)?;
            let value = match txn.read(Ns::Business, key.as_bytes())? {
                Some(id) => self.load(&id)?,
                None => self
                    .memory
                    .reserve(128, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?
                    .retain(Vec::new()),
            };
            self.check(txn)?;
            Ok(Some(value))
        })
    }
    /// Exact stored instance fields, without resolving registry fallback. The
    /// qualified parent must establish complete eight-field instance rows.
    pub fn read_instance<R: MerkleNodeReader>(
        &mut self,
        map_key: &str,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<Option<ReservedValue<PreparedModuleInstanceFields>>, BodyMutationError> {
        txn.contribute(|txn| {
            self.check(txn)?;
            let mut workspace = self
                .memory
                .reserve(16 * 1024, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let identity = ModuleInstanceKeyV1::new(map_key, Field::InstanceId)
                .map_err(BodyMutationError::Invalid)?;
            let Some(first) = txn.read(Ns::Business, identity.as_bytes())? else {
                self.check(txn)?;
                return Ok(None);
            };
            let mut fields = Vec::with_capacity(8);
            for field in Field::ALL {
                self.check(txn)?;
                let id = if field == Field::InstanceId {
                    first
                } else {
                    let key = ModuleInstanceKeyV1::new(map_key, field)
                        .map_err(BodyMutationError::Invalid)?;
                    txn.read(Ns::Business, key.as_bytes())?
                        .ok_or(BodyMutationError::Invalid("incomplete module instance row"))?
                };
                let value = self.load(&id)?;
                Scalar::from_canonical_bytes(&value, field).map_err(BodyMutationError::Invalid)?;
                fields.push(value);
            }
            let fields = fields
                .try_into()
                .map_err(|_| BodyMutationError::Invalid("module instance field count"))?;
            let charge = workspace
                .split(std::mem::size_of::<PreparedModuleInstanceFields>() + 128)
                .map_err(BodyMutationError::Invalid)?;
            self.check(txn)?;
            Ok(Some(charge.retain(PreparedModuleInstanceFields { fields })))
        })
    }
}
