//! Fixed selected authority/epoch/revocation inputs for module admission.
use super::current_identity as identity;
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_typed_record_diff::{ChangeDecoder, point_typed_column_v1},
};
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
use oasis7_proto::memory_budget::ReservedValue;
pub enum ModuleAdmissionRevocationRequestV1<'a> {
    Body,
    Authority(&'a str),
    Proof(&'a str),
    Epoch,
    Revoked(&'a str),
    Superseded(&'a str),
}
pub enum ModuleAdmissionRevocationValueV1 {
    Body(StateRootV1),
    Authority(Option<RevocationRecoveredV1<crate::CapabilityAuthorityRecord>>),
    Proof(Option<RevocationRecoveredV1<crate::CapabilityAuthorityFinalityProof>>),
    Epoch(u64),
    Revoked(bool),
    Superseded(bool),
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_module_admission_revocation_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
>(
    request: ModuleAdmissionRevocationRequestV1<'_>,
    q: &ReservedValue<RuntimeRevocationQualificationV1>,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<ModuleAdmissionRevocationValueV1>, WorldFieldOverlayErrorV1> {
    overlay.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        q.validate_digest(&tx.parent_fence().parent_roots)?;
        if q.allocation_class() != MemoryClass::Ordinary
            || !q.uses_budget(overlay.values.memory)
            || !tx.uses_budget(overlay.values.memory)
            || overlay.values.class != MemoryClass::Ordinary
            || read.values.class != MemoryClass::Ordinary
            || !read.values.memory.same_pool(overlay.values.memory)
        {
            return Err(invalid().into());
        }
        let fee = overlay
            .values
            .memory
            .reserve(
                std::mem::size_of::<ModuleAdmissionRevocationValueV1>() + 8192,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let body = {
            let mut records = InventoryValueReader {
                values: &mut overlay.values,
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                cancelled: &mut overlay.cancelled,
            };
            identity::current_body(tx, &mut records, &mut overlay.take_state_error)?
        };
        let value = match request {
            ModuleAdmissionRevocationRequestV1::Body => {
                ModuleAdmissionRevocationValueV1::Body(body)
            }
            ModuleAdmissionRevocationRequestV1::Authority(id) => {
                let cell = identity::map_entry(&body, "authority_records", id, read, scan, work)?;
                let value = match cell.as_ref().map(|v| &v.cell) {
                    None => None,
                    Some(ColumnCellV1::Root(root)) => Some(
                        crate::incremental_typed_record_codec::recover_typed_record_body_v1(
                            root,
                            &profile(),
                            read,
                            scan,
                            work,
                        )?,
                    ),
                    _ => return Err(invalid().into()),
                };
                ModuleAdmissionRevocationValueV1::Authority(value)
            }
            ModuleAdmissionRevocationRequestV1::Proof(id) => {
                let cell =
                    identity::map_entry(&body, "authority_finality_proofs", id, read, scan, work)?;
                let value = match cell.as_ref().map(|v| &v.cell) {
                    None => None,
                    Some(ColumnCellV1::Root(root)) => Some(
                        crate::incremental_typed_record_codec::recover_typed_record_body_v1(
                            root,
                            &profile(),
                            read,
                            scan,
                            work,
                        )?,
                    ),
                    _ => return Err(invalid().into()),
                };
                ModuleAdmissionRevocationValueV1::Proof(value)
            }
            ModuleAdmissionRevocationRequestV1::Epoch => {
                let profile = profile();
                let trees_remaining = work.max_retained_trees;
                let mut decoder = ChangeDecoder {
                    ctx: read,
                    scan,
                    work,
                    profile,
                    trees_remaining,
                    tree_depth: 0,
                };
                let value = point_typed_column_v1(&profile, &body, "epoch", &mut decoder, 8192)?;
                let ColumnCellV1::U64(epoch) = &value.cell else {
                    return Err(invalid().into());
                };
                ModuleAdmissionRevocationValueV1::Epoch(*epoch)
            }
            ModuleAdmissionRevocationRequestV1::Revoked(id) => {
                let cell = identity::map_entry(&body, "revoked_grant_ids", id, read, scan, work)?;
                let revoked = match cell.as_ref().map(|v| &v.cell) {
                    None => false,
                    Some(ColumnCellV1::Bool(true)) => true,
                    _ => return Err(invalid().into()),
                };
                ModuleAdmissionRevocationValueV1::Revoked(revoked)
            }
            ModuleAdmissionRevocationRequestV1::Superseded(id) => {
                let cell = identity::map_entry(&body, "superseded_by", id, read, scan, work)?;
                let superseded = match cell.as_ref().map(|v| &v.cell) {
                    None => false,
                    Some(ColumnCellV1::Text(_)) => true,
                    _ => return Err(invalid().into()),
                };
                ModuleAdmissionRevocationValueV1::Superseded(superseded)
            }
        };
        read.check()?;
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (overlay.take_state_error)() {
            return Err(error.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(fee.retain(value))
    })
}
