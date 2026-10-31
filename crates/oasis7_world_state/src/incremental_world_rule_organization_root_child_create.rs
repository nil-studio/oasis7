//! Empty nested records consume the exact qualified Case root, not absence alone.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{StatePageWriter, StateWriteV1},
};
use oasis7_proto::memory_budget::ReservedValue;
pub struct OrganizationEmptyChildCreateV1<'a> {
    pub case: &'a str,
    pub record: OrganizationRelativeRecordV1<'a>,
    pub source: super::super::WorldRuleOrganizationScalarsRefV1<'a>,
    pub qualification: &'a ReservedValue<OrganizationRecordQualificationV1>,
    pub max_pending_node_bytes: usize,
}
pub fn create_qualified_organization_child_empty_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: OrganizationEmptyChildCreateV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    cells: &mut u64,
) -> Result<ReservedValue<OrganizationRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let owner = OrganizationRootKeyV1::Case(change.case);
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !matches!(
            change.record,
            OrganizationRelativeRecordV1::Grant(_) | OrganizationRelativeRecordV1::Recovery(_)
        ) || closure::profile_of(change.record) != change.source.profile()
            || !change.source.empty()
        {
            return Err(ValueTreeError::Invalid("organization empty child source/profile").into());
        }
        if !tx.uses_budget(write.memory) {
            return Err(ValueTreeError::Invalid("organization child writer pool").into());
        }
        let _operation = write
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let record = closure::local_record(change.record);
        let (outer, child) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let resolved = access::current(owner, change.qualification, tx, &mut records, error)?;
            let marker = record.overlay_path(RecordPathV1::Record, &mut records)?;
            if inner_tree::lookup(resolved.1, &marker.0, &mut records)?.is_some() {
                return Err(ValueTreeError::Invalid(
                    "organization empty child requires absent record",
                )
                .into());
            }
            resolved
        };
        let mut mutations = Vec::with_capacity(9);
        for (spec, payload) in closure::profile_of(change.record)
            .specs()
            .iter()
            .zip(super::super::payloads(change.source)?)
        {
            let Some(payload) = payload else {
                continue;
            };
            *cells = cells
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("organization child cells"))?;
            let row = {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                record.write_path(spec.path, payload, &mut combined)?
            };
            mutations.push(StateWriteV1 {
                key: row.field_key.as_bytes().to_vec(),
                before: None,
                after: Some(row.value_root),
            });
        }
        mutations.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            inner_tree::build(
                child,
                mutations,
                &mut records,
                change.max_pending_node_bytes,
            )?
        };
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            access::store(owner, &outer, &delta, &mut combined)?
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Authorization,
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let digest = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or(ValueTreeError::Invalid("organization child owner digest"))?;
        let proof = closure::from_typed(owner, digest, &delta.after, write.memory)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(proof)
    })
}
