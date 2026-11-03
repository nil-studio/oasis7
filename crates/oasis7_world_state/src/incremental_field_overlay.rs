//! Shared bounded transaction overlay access for typed field adapters.
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_semantic_key::SemanticKeyBudget, incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
#[derive(Debug)]
pub enum WorldFieldOverlayErrorV1 {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Member(crate::incremental_field_member::MemberRecordError),
}
impl From<ExecutionError> for WorldFieldOverlayErrorV1 {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}
impl From<ValueTreeError> for WorldFieldOverlayErrorV1 {
    fn from(error: ValueTreeError) -> Self {
        Self::Value(error)
    }
}
impl std::fmt::Display for WorldFieldOverlayErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execution(error) => error.fmt(f),
            Self::Value(error) => error.fmt(f),
            Self::Member(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for WorldFieldOverlayErrorV1 {}

pub struct WorldFieldOverlayContextV1<'t, 's, 'v, M, R, E, C> {
    pub transaction: &'t mut ExecutionTransaction<'s, M>,
    pub values: ValueReadContext<'v, R>,
    pub key_work: &'v mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub take_state_error: E,
    pub cancelled: C,
}

pub(crate) fn lookup<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    transaction: &mut ExecutionTransaction<'_, M>,
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    take_state_error: &mut E,
) -> Result<Option<[u8; 32]>, WorldFieldOverlayErrorV1> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _workspace = records
        .values
        .memory
        .reserve(512 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let namespace = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .namespace;
    lookup_raw_after_admission(
        transaction,
        namespace,
        key.as_bytes(),
        records,
        take_state_error,
    )
}
/// Dedicated codecs validate their own key and namespace before calling.
pub(crate) fn lookup_raw<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    transaction: &mut ExecutionTransaction<'_, M>,
    namespace: oasis7_formal_protocol::incremental_state::StateNamespaceV1,
    key: &[u8],
    records: &mut InventoryValueReader<'_, '_, R, C>,
    take_state_error: &mut E,
) -> Result<Option<[u8; 32]>, WorldFieldOverlayErrorV1> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _workspace = records
        .values
        .memory
        .reserve(512 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    lookup_raw_after_admission(transaction, namespace, key, records, take_state_error)
}
fn lookup_raw_after_admission<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    transaction: &mut ExecutionTransaction<'_, M>,
    namespace: oasis7_formal_protocol::incremental_state::StateNamespaceV1,
    key: &[u8],
    records: &mut InventoryValueReader<'_, '_, R, C>,
    take_state_error: &mut E,
) -> Result<Option<[u8; 32]>, WorldFieldOverlayErrorV1> {
    let result = transaction.read(namespace, key);
    let original = take_state_error();
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(error) = original {
        return Err(error.into());
    }
    result.map_err(WorldFieldOverlayErrorV1::Execution)
}

impl From<crate::incremental_field_member::MemberRecordError> for WorldFieldOverlayErrorV1 {
    fn from(error: crate::incremental_field_member::MemberRecordError) -> Self {
        Self::Member(error)
    }
}

/// Both required records stay in the same transaction read dependency set.
pub(crate) fn required_member<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: crate::incremental_projection::FieldDescriptor,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    take_state_error: &mut E,
) -> Result<(WorldFieldKeyV1, [u8; 32], u64), WorldFieldOverlayErrorV1> {
    use crate::incremental_world_field_key::WorldFieldRecordKindV1;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(transaction, &metadata, records, take_state_error)?
        .ok_or(ValueTreeError::Invalid("required field missing metadata"))?;
    records.presence(&root, &metadata, 0)?;
    let (key, bytes) = records.key(field, WorldFieldRecordKindV1::Entry, &())?;
    let root = lookup(transaction, &key, records, take_state_error)?
        .ok_or(ValueTreeError::Invalid("required field missing entry"))?;
    Ok((key, root, bytes))
}
