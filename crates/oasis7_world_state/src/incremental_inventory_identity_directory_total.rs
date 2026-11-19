//! Original all-ledger material balance from the resident identity directory.
//! No legacy `materials` entries participate, and no inventory branch is cold
//! scanned or restored into full ledger maps during a normal call.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, PreparedGuard,
};
#[allow(clippy::too_many_arguments)]
pub fn sum_current_inventory_material_v1<M, R, E, C, P, V, ER, CR>(
    material: &str,
    directory: &ReservedValue<InventoryIdentityDirectoryV1>,
    family: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    family_work: &mut InventoryFamilyWorkV1,
    work: &mut InventoryIdentityDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<i64>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    let result = (|| {
        overlay.transaction.validate_guard(guard, live)?;
        work.step(material.len(), &mut || {
            (overlay.cancelled)() || (parent.cancelled)()
        })?;
        let tree = current::fold(directory, family, roots, overlay, parent, family_work, work)?;
        let fee = overlay
            .values
            .memory
            .reserve(size_of::<i64>() + 128, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let stop = std::cell::RefCell::new((&mut overlay.cancelled, &mut parent.cancelled));
        let mut cancelled = || {
            let mut s = stop.borrow_mut();
            (s.0)() || (s.1)()
        };
        let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
            values: &mut overlay.values,
            key_work: &mut *overlay.key_work,
            semantic_bytes: overlay.semantic_bytes,
            record_bytes: overlay.record_bytes,
            cancelled: &mut cancelled,
        };
        let mut total = 0i64;
        tree::material::<WorldFieldOverlayErrorV1>(
            &tree,
            material,
            work,
            &mut || {
                let mut s = stop.borrow_mut();
                (s.0)() || (s.1)()
            },
            &mut |ledger, kind| {
                let point = crate::incremental_inventory_overlay_read::resolve_point(
                    overlay.transaction,
                    &mut records,
                    &mut overlay.take_state_error,
                    "material_ledgers",
                    &(ledger, kind),
                    Some(ledger),
                )?;
                // A missing kind contributes zero by absence from the complete
                // resident directory. An indexed member missing from actual
                // Current is a broken source closure, not a fabricated zero.
                let crate::incremental_inventory_read::LedgerQuantityV1::Value(quantity) =
                    point.quantity()
                else {
                    return Err(
                        ValueTreeError::Invalid("inventory indexed sum member missing").into(),
                    );
                };
                // The original Iterator::sum uses ordinary i64 addition in
                // ledger BTree order: debug overflow panics; release wraps.
                // Do not add a new checked/saturating acceptance predicate.
                total += quantity;
                Ok(())
            },
        )?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let guard = overlay.transaction.guard();
        overlay.transaction.validate_guard(&guard, live)?;
        Ok(fee.retain(total))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = overlay
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
