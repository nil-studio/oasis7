//! Typed reads of the current shared transaction overlay, never a live head.
use crate::{
    MaterialLedgerId,
    incremental_inventory_genesis::descriptor,
    incremental_inventory_read::LedgerQuantityV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::FieldShape,
    incremental_semantic_key::{SemanticKeyBudget, SemanticKeyV1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;

#[derive(Debug)]
pub enum InventoryOverlayReadError {
    Execution(ExecutionError),
    Value(ValueTreeError),
}
impl From<ExecutionError> for InventoryOverlayReadError {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}
impl From<ValueTreeError> for InventoryOverlayReadError {
    fn from(error: ValueTreeError) -> Self {
        Self::Value(error)
    }
}
impl std::fmt::Display for InventoryOverlayReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execution(error) => error.fmt(f),
            Self::Value(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for InventoryOverlayReadError {}

pub struct InventoryOverlayReadContext<'t, 's, 'v, M, R, E, C> {
    pub transaction: &'t mut ExecutionTransaction<'s, M>,
    pub values: ValueReadContext<'v, R>,
    pub key_work: &'v mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub take_state_error: E,
    pub cancelled: C,
}

fn lookup<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    transaction: &mut ExecutionTransaction<'_, M>,
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    take_state_error: &mut E,
) -> Result<Option<[u8; 32]>, InventoryOverlayReadError> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _workspace = records
        .values
        .memory
        .reserve(512 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let result = transaction.read(StateNamespaceV1::Business, key.as_bytes());
    let original = take_state_error();
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(error) = original {
        return Err(error.into());
    }
    result.map_err(InventoryOverlayReadError::Execution)
}

fn query<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
>(
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    take_state_error: &mut E,
    name: &'static str,
    key: &K,
    ledger: Option<&MaterialLedgerId>,
) -> Result<LedgerQuantityV1, InventoryOverlayReadError> {
    let field = descriptor(
        name,
        if ledger.is_some() {
            FieldShape::NestedMap
        } else {
            FieldShape::Map
        },
    )
    .map_err(|_| ValueTreeError::Invalid("inventory registration"))?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let metadata_root = lookup(transaction, &metadata, records, take_state_error)?
        .ok_or(ValueTreeError::Invalid("inventory missing field metadata"))?;
    records.presence(&metadata_root, &metadata, 0)?;
    let group = if let Some(ledger) = ledger {
        let (group_key, length) = records.key(field, WorldFieldRecordKindV1::Group, ledger)?;
        let root = lookup(transaction, &group_key, records, take_state_error)?;
        if let Some(root) = root {
            records.presence(&root, &group_key, length)?;
        }
        root
    } else {
        None
    };
    let (entry, length) = records.key(field, WorldFieldRecordKindV1::Entry, key)?;
    let value = lookup(transaction, &entry, records, take_state_error)?;
    if ledger.is_some() && group.is_none() {
        if value.is_some() {
            return Err(ValueTreeError::Invalid("inventory orphan member").into());
        }
        return Ok(LedgerQuantityV1::LedgerAbsent);
    }
    match value {
        None => Ok(LedgerQuantityV1::MaterialAbsent),
        Some(root) => records
            .quantity(&root, &entry, length)
            .map(LedgerQuantityV1::Value)
            .map_err(Into::into),
    }
}

impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> InventoryOverlayReadContext<'_, '_, '_, M, R, E, C>
{
    fn query<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        name: &'static str,
        key: &K,
        ledger: Option<&MaterialLedgerId>,
    ) -> Result<LedgerQuantityV1, InventoryOverlayReadError> {
        let mut records = InventoryValueReader {
            values: &mut self.values,
            key_work: self.key_work,
            semantic_bytes: self.semantic_bytes,
            record_bytes: self.record_bytes,
            cancelled: &mut self.cancelled,
        };
        let take_state_error = &mut self.take_state_error;
        self.transaction.contribute(|transaction| {
            if records.values.class != MemoryClass::Ordinary
                || !transaction.uses_budget(records.values.memory)
            {
                return Err(ValueTreeError::Invalid("inventory overlay pool/class").into());
            }
            query(
                transaction,
                &mut records,
                take_state_error,
                name,
                key,
                ledger,
            )
        })
    }
    pub fn resource(
        &mut self,
        key: ResourceKind,
    ) -> Result<Option<i64>, InventoryOverlayReadError> {
        self.query("resources", &key, None)
            .map(|quantity| match quantity {
                LedgerQuantityV1::Value(value) => Some(value),
                _ => None,
            })
    }
    pub fn material(&mut self, key: &str) -> Result<Option<i64>, InventoryOverlayReadError> {
        self.query("materials", key, None)
            .map(|quantity| match quantity {
                LedgerQuantityV1::Value(value) => Some(value),
                _ => None,
            })
    }
    pub fn ledger(
        &mut self,
        ledger: &MaterialLedgerId,
        material: &str,
    ) -> Result<LedgerQuantityV1, InventoryOverlayReadError> {
        self.query("material_ledgers", &(ledger, material), Some(ledger))
    }
}
