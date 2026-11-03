//! Exact point scalar changes, without reading unrelated Text or collections.
use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_world_rule_organization::ReadLimits,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{StatePageWriter, StateWriteV1},
};
use oasis7_proto::memory_budget::ReservedValue;
pub struct OrganizationFieldChangeV1<'a> {
    pub request: OrganizationFieldRequestV1<'a>,
    pub expected: OrganizationFieldRefV1<'a>,
    pub after: OrganizationFieldRefV1<'a>,
    pub max_current_payload_bytes: u64,
    pub max_pending_node_bytes: usize,
}
pub fn change_qualified_organization_field_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: OrganizationFieldChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<OrganizationRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let request = change.request;
    let owner = request.owner;
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !matches!(
            (owner, request.record),
            (
                OrganizationRootKeyV1::Case(_),
                OrganizationRelativeRecordV1::Case
                    | OrganizationRelativeRecordV1::Grant(_)
                    | OrganizationRelativeRecordV1::Recovery(_)
            ) | (
                OrganizationRootKeyV1::Estate(_),
                OrganizationRelativeRecordV1::Estate
            )
        ) {
            return Err(ValueTreeError::Invalid("organization field owner/record profile").into());
        }
        let span = request.field.span(request.record)?;
        let old = change.expected.payloads(&span)?;
        let new = change.after.payloads(&span)?;
        if !tx.uses_budget(write.memory) {
            return Err(ValueTreeError::Invalid("organization field writer pool").into());
        }
        let _operation = write
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut compare = Compare::new(change.expected.text().unwrap_or("").as_bytes());
        let (outer, child, keys, roots) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let (outer, child) =
                access::current(owner, request.qualification, tx, &mut records, error)?;
            let record = closure::local_record(request.record);
            let marker = record.overlay_path(RecordPathV1::Record, &mut records)?;
            let marker_root = inner_tree::lookup(child, &marker.0, &mut records)?.ok_or(
                ValueTreeError::Invalid("organization scalar mutation requires record"),
            )?;
            super::super::marker(
                &marker_root,
                &marker.0,
                marker.1,
                records.values,
                records.record_bytes,
                records.cancelled,
            )?;
            let specs = closure::profile_of(request.record).specs();
            let mut keys = std::array::from_fn::<_, 2, _>(|_| None);
            let mut roots = [None; 2];
            for (offset, spec) in specs[span.slot..span.slot + span.count].iter().enumerate() {
                let key = record.overlay_path(spec.path, &mut records)?;
                roots[offset] = inner_tree::lookup(child, &key.0, &mut records)?;
                keys[offset] = Some(key);
            }
            let value = fields_read::resolved(
                &span,
                &keys,
                &roots,
                records.values,
                ReadLimits {
                    record_bytes: records.record_bytes,
                    max_payload: change.max_current_payload_bytes,
                },
                &mut compare,
                records.cancelled,
            )?;
            if value != change.expected.value() || !compare.is_equal() {
                return Err(ValueTreeError::Invalid("organization scalar field expected").into());
            }
            (outer, child, keys, roots)
        };
        let same = change.expected == change.after;
        // Text equality may scan a long target; cancellation is checked after
        // comparison even when this is the final/no-op field contribution.
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if same {
            return Ok(None);
        }
        let specs = closure::profile_of(request.record).specs();
        let mut mutations = Vec::with_capacity(span.count);
        for offset in 0..span.count {
            let unchanged = super::super::overlay::equal(old[offset], new[offset]);
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if unchanged {
                continue;
            }
            let after = if let Some(payload) = new[offset] {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                let row = closure::local_record(request.record).write_path(
                    specs[span.slot + offset].path,
                    payload,
                    &mut combined,
                )?;
                if row.field_key != keys[offset].as_ref().unwrap().0 {
                    return Err(
                        ValueTreeError::Invalid("organization scalar field identity").into(),
                    );
                }
                Some(row.value_root)
            } else {
                None
            };
            mutations.push(StateWriteV1 {
                key: keys[offset].as_ref().unwrap().0.as_bytes().to_vec(),
                before: roots[offset],
                after,
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
            .ok_or(ValueTreeError::Invalid("organization scalar owner digest"))?;
        let proof = closure::from_typed(owner, digest, &delta.after, write.memory)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
