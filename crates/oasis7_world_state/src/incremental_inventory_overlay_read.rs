//! Typed reads of the current shared transaction overlay, never a live head.
use crate::{
    MaterialLedgerId,
    incremental_inventory_genesis::descriptor,
    incremental_inventory_read::{
        InventoryPointV1, InventoryResolvedPointV1, LedgerPointV1, LedgerQuantityV1,
    },
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::FieldShape,
    incremental_semantic_key::SemanticKeyV1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;

use crate::incremental_field_overlay::lookup;
pub use crate::incremental_field_overlay::{
    WorldFieldOverlayContextV1 as InventoryOverlayReadContext,
    WorldFieldOverlayErrorV1 as InventoryOverlayReadError,
};

pub(crate) fn resolve_point<
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
) -> Result<InventoryResolvedPointV1, InventoryOverlayReadError> {
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
        let (key, length) = records.key(field, WorldFieldRecordKindV1::Group, ledger)?;
        let root = lookup(transaction, &key, records, take_state_error)?;
        if let Some(root) = root {
            records.presence(&root, &key, length)?;
        }
        Some((key, root))
    } else {
        None
    };
    let (entry, length) = records.key(field, WorldFieldRecordKindV1::Entry, key)?;
    let root = lookup(transaction, &entry, records, take_state_error)?;
    if group.as_ref().is_some_and(|(_, group)| group.is_none()) && root.is_some() {
        return Err(ValueTreeError::Invalid("inventory orphan member").into());
    }
    let quantity = root
        .map(|root| records.quantity(&root, &entry, length))
        .transpose()?;
    let member = InventoryPointV1 {
        metadata,
        metadata_root,
        key: entry,
        root,
        quantity,
    };
    Ok(match group {
        Some((group, group_root)) => InventoryResolvedPointV1::ledger(LedgerPointV1 {
            group,
            group_root,
            member,
        }),
        None => InventoryResolvedPointV1::simple(member),
    })
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
            resolve_point(
                transaction,
                &mut records,
                take_state_error,
                name,
                key,
                ledger,
            )
            .map(|point| point.quantity())
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
