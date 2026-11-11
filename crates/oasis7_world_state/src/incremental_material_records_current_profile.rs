//! Exact Current optional MaterialProfile priority reads.
//!
//! The family qualification is tied to the transaction's parent RootSet. The
//! outer record pointer is then resolved through the actual transaction view,
//! and the pointed-to child is closed and decoded with the original material
//! record codec. A missing current outer pointer is `None`; a present pointer
//! with a missing or malformed required cell is an error.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_wasm_abi::MaterialDefaultPriority;

/// Continuous work for one optional Current profile point read. Callers share
/// these counters with other Current record reads in the same execution phase.
pub struct MaterialProfileCurrentReadWorkV1 {
    pub scan: StateScanBudget,
    pub cells_remaining: u64,
    pub max_payload_bytes: u64,
}

/// Read a profile's default priority from the real Current overlay.
///
/// `state` must read the same bounded state-page store used by the transaction,
/// including candidate pages written earlier in this transaction. A caller
/// that cannot provide that reader cannot use this helper as authority.
#[allow(clippy::too_many_arguments)]
pub fn read_current_material_default_priority_overlay_v1<M, S, R, E, C>(
    kind: &str,
    family: &ReservedValue<MaterialFamilyQualificationV1>,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut MaterialProfileCurrentReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<MaterialDefaultPriority>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        // RootSet encoding and the fixed record inspection/closure state must
        // be admitted before digesting roots or opening either record tree.
        let _phase = context
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !family.uses_budget(context.values.memory)
            || family.allocation_class() != MemoryClass::Ordinary
        {
            return Err(ValueTreeError::Invalid("Current material profile pool/class").into());
        }
        let parent_digest = transaction.parent_fence().parent_roots;
        family.validate_binding(&parent_digest)?;
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }

        let key = MaterialRecordKeyV1::Profile(kind);
        // `read::outer` validates the encoded root but returns an owned StateRoot
        // whose digest String remains live through Current closure and the final
        // cell read. Keep that fixed-format storage admitted for its full lifetime.
        let _child_root_fee = context
            .values
            .memory
            .reserve(
                std::mem::size_of::<oasis7_formal_protocol::incremental_state::StateRootV1>() + 256,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let (child, owner) = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut context.cancelled,
            };
            // This checks the Current field metadata and the format marker
            // before looking up the optional profile pointer.
            point::metadata(
                key,
                transaction,
                &mut records,
                &mut context.take_state_error,
            )?;
            let outer = read::outer_key(key, &mut records)?;
            let Some(root) = lookup(
                transaction,
                &outer.0,
                &mut records,
                &mut context.take_state_error,
            )?
            else {
                if (records.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                if let Some(error) = (context.take_state_error)() {
                    return Err(error.into());
                }
                // Point reads advance the transaction revision; bind the
                // successful absence to the exact final Current snapshot.
                let tail_guard = transaction.guard();
                transaction.validate_guard(&tail_guard, live)?;
                return Ok(None);
            };
            records
                .values
                .verify_closure(&root, records.record_bytes, &mut *records.cancelled)?;
            let child = read::outer(key.kind(), &outer, &root, &mut records)?;
            let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .semantic_key
                .ok_or(ValueTreeError::Invalid("Current material profile owner"))?;
            (child, owner)
        };

        // The parent family does not qualify a newly written child. Close the
        // exact Current child root before reading any cell, so same-transaction
        // edits cannot inherit a stale parent record proof.
        let proof = qualify_material_body_v1(
            MaterialRecordKindV1::Profile,
            owner,
            &mut WorldFieldReadContextV1 {
                root: &child,
                state,
                values: ValueReadContext {
                    source: context.values.source,
                    memory: context.values.memory,
                    class: context.values.class,
                    io: &mut *context.values.io,
                },
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                take_state_error: &mut context.take_state_error,
                cancelled: &mut context.cancelled,
            },
            &mut work.scan,
            &mut work.cells_remaining,
            work.max_payload_bytes,
        )?;
        if !proof.uses_budget(context.values.memory)
            || proof.allocation_class() != MemoryClass::Ordinary
        {
            return Err(ValueTreeError::Invalid("Current material profile closure").into());
        }

        work.cells_remaining =
            work.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Current material profile cells",
                ))?;
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let priority = {
            let mut child_context = WorldFieldReadContextV1 {
                root: &child,
                state,
                values: ValueReadContext {
                    source: context.values.source,
                    memory: context.values.memory,
                    class: context.values.class,
                    io: &mut *context.values.io,
                },
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                take_state_error: &mut context.take_state_error,
                cancelled: &mut context.cancelled,
            };
            let address = {
                let mut records = InventoryValueReader {
                    values: &mut child_context.values,
                    key_work: &mut *child_context.key_work,
                    semantic_bytes: child_context.semantic_bytes,
                    record_bytes: child_context.record_bytes,
                    cancelled: &mut child_context.cancelled,
                };
                read::cell_key(
                    MaterialRecordKindV1::Profile,
                    MaterialCellV1::DefaultPriority,
                    &mut records,
                )?
            };
            let root = child_context
                .lookup(&address.0)?
                .ok_or(ValueTreeError::Invalid(
                    "Current material profile priority missing",
                ))?;
            let mut records = InventoryValueReader {
                values: &mut child_context.values,
                key_work: &mut *child_context.key_work,
                semantic_bytes: child_context.semantic_bytes,
                record_bytes: child_context.record_bytes,
                cancelled: &mut child_context.cancelled,
            };
            match read::value(
                MaterialCellV1::DefaultPriority,
                &root,
                &address,
                &mut records,
                work.max_payload_bytes,
                &mut std::io::sink(),
            )? {
                MaterialValueV1::Unsigned(0) => MaterialDefaultPriority::Urgent,
                MaterialValueV1::Unsigned(1) => MaterialDefaultPriority::Standard,
                _ => {
                    return Err(
                        ValueTreeError::Invalid("Current material profile priority value").into(),
                    );
                }
            }
        };
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (context.take_state_error)() {
            return Err(error.into());
        }
        // The entry guard authorized this read; validate success against its
        // final read-set revision after all Current point reads have finished.
        let tail_guard = transaction.guard();
        transaction.validate_guard(&tail_guard, live)?;
        Ok(Some(priority))
    })
}

#[cfg(test)]
#[path = "incremental_material_records_current_profile_tests.rs"]
mod tests;
