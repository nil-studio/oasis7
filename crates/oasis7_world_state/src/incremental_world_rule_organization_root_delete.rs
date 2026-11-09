//! Finite closed-record deletion; Case and Grant are deliberately ineligible.
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
pub struct OrganizationClosedDeleteV1<'a> {
    pub owner: OrganizationRootKeyV1<'a>,
    pub record: OrganizationRelativeRecordV1<'a>,
    pub expected: super::super::WorldRuleOrganizationScalarsRefV1<'a>,
    pub qualification: &'a ReservedValue<OrganizationRecordQualificationV1>,
    pub max_pending_node_bytes: usize,
}
pub enum OrganizationClosedDeleteResultV1 {
    EstateDeleted,
    CaseUpdated(ReservedValue<OrganizationRecordQualificationV1>),
}
/// Checks every original closed column and Option absence. The parent proof
/// excludes unknown descendants, so no unrelated Case subtree must be scanned.
pub fn delete_qualified_organization_closed_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: OrganizationClosedDeleteV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    cells: &mut u64,
) -> Result<OrganizationClosedDeleteResultV1, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !matches!(
            (change.owner, change.record),
            (
                OrganizationRootKeyV1::Estate(_),
                OrganizationRelativeRecordV1::Estate
            ) | (
                OrganizationRootKeyV1::Case(_),
                OrganizationRelativeRecordV1::Recovery(_)
            )
        ) || closure::profile_of(change.record) != change.expected.profile()
        {
            return Err(ValueTreeError::Invalid("organization closed delete profile").into());
        }
        if !tx.uses_budget(write.memory) {
            return Err(ValueTreeError::Invalid("organization closed delete writer pool").into());
        }
        let _operation = write
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let record = closure::local_record(change.record);
        let mut mutations = Vec::with_capacity(10);
        let (outer, child) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let resolved =
                access::current(change.owner, change.qualification, tx, &mut records, error)?;
            for (spec, payload) in closure::profile_of(change.record)
                .specs()
                .iter()
                .zip(super::super::payloads(change.expected)?)
            {
                *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
                    "organization closed delete cells",
                ))?;
                let key = record.overlay_path(spec.path, &mut records)?;
                let before = inner_tree::lookup(resolved.1, &key.0, &mut records)?;
                match (payload, before) {
                    (Some(payload), Some(value)) => {
                        qualification::compare_payload(
                            payload,
                            &value,
                            &key.0,
                            key.1,
                            records.values,
                            records.record_bytes,
                            records.cancelled,
                        )?;
                        mutations.push(StateWriteV1 {
                            key: key.0.as_bytes().to_vec(),
                            before: Some(value),
                            after: None,
                        });
                    }
                    (None, None) => {}
                    _ => {
                        return Err(ValueTreeError::Invalid(
                            "organization closed delete expected presence",
                        )
                        .into());
                    }
                }
                if (records.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
            }
            resolved
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if matches!(change.record, OrganizationRelativeRecordV1::Estate) {
            tx.write(StateNamespaceV1::Authorization, outer.as_bytes(), None)?;
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(OrganizationClosedDeleteResultV1::EstateDeleted);
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
            access::store(change.owner, &outer, &delta, &mut combined)?
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
            .ok_or(ValueTreeError::Invalid(
                "organization closed delete owner digest",
            ))?;
        let proof = closure::from_typed(change.owner, digest, &delta.after, write.memory)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(OrganizationClosedDeleteResultV1::CaseUpdated(proof))
    })
}
