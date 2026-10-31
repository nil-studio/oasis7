//! Closed fixed records can be deleted after a complete exact expected projection.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
pub struct DiplomacyClosedDeleteV1<'a> {
    pub id: &'a str,
    pub qualification: &'a ReservedValue<DiplomacyRecordQualificationV1>,
    pub expected: DiplomacySourceV1<'a>,
    pub max_current_payload_bytes: u64,
}
pub fn delete_qualified_diplomacy_closed_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: DiplomacyClosedDeleteV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let field = request.expected.field();
        if !matches!(
            field,
            DiplomacyFieldV1::Crises | DiplomacyFieldV1::EconomicContracts
        ) {
            return Err(closure::invalid().into());
        }
        let _scratch = context
            .values
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        let (outer, child) = access::current(
            field,
            request.id,
            request.qualification,
            tx,
            &mut records,
            &mut context.take_state_error,
        )?;
        for (spec, payload) in profile::specs(field)
            .iter()
            .zip(profile::fixed(request.expected))
        {
            let key = spec.path.overlay(field, &mut records)?;
            let root = inner_tree::lookup(child, &key.0, &mut records)?;
            match (payload, root) {
                (Some(payload), Some(value)) => scalars::expected(
                    payload,
                    &value,
                    &key,
                    &mut records,
                    request.max_current_payload_bytes,
                )?,
                (None, None) => {}
                _ => return Err(closure::invalid().into()),
            }
        }
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(StateNamespaceV1::Business, outer.as_bytes(), None)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
