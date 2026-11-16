//! Fixed original control10 points to original JSON scalars.
use super::current::{CognitionProjectionAuxWorkV1, PaidValues, Scope};
use crate::{
    incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3,
    incremental_cognition_family,
    incremental_cognition_fields::CognitionControlFieldV1 as F,
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_utf8_output::read_text_value,
    incremental_world_rule_projection_delta::{Processing, json, spend},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, mem::size_of};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition projection control point")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Cognition projection control point")
}
struct Row {
    field: F,
    json: Option<ReservedValue<Vec<u8>>>,
}
pub(crate) struct CognitionControlProjectionRowsV1 {
    rows: [Option<Row>; 10],
    guard: PreparedGuard,
    roots: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl CognitionControlProjectionRowsV1 {
    pub(crate) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if self.roots != tx.parent_fence().parent_roots
            || !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub(crate) fn visit_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, Option<&'static str>, Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in self.rows.iter().flatten() {
            work(1, 0)?;
            let (root, member) = row.field.projection_envelope_path_v1();
            emit(root, member, row.json.as_deref().map(Vec::as_slice))?
        }
        Ok(())
    }
}
fn cell(work: &mut u64) -> Result<(), ValueTreeError> {
    *work = work.checked_sub(1).ok_or_else(limit)?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn capture_controls<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut CognitionProjectionAuxWorkV1<'_>,
    scope: Scope,
    live: &ExecutionFence,
) -> Result<CognitionControlProjectionRowsV1, WorldFieldOverlayErrorV1> {
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    if let Scope::QualifiedAll = scope {
        ctx.transaction.validate_bootstrap_read_only_v1(live)?
    }
    let memory = ctx.values.memory.clone();
    let p = Processing {
        bank: RefCell::new(&mut *work.processing),
    };
    let rootbytes = roots
        .state
        .iter()
        .try_fold(roots.world_id.len() + 4096, |n, root| {
            n.checked_add(root.node_root.len() + 128).ok_or_else(limit)
        })?;
    spend(
        &p,
        rootbytes.checked_mul(16).ok_or_else(limit)?,
        &mut ctx.cancelled,
    )?;
    parent.validate_binding(roots)?;
    let digest = roots.digest().map_err(|_| bad())?;
    if parent.allocation_class() != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(&memory)
        || ctx.transaction.parent_fence().parent_roots != digest
        || !ctx.transaction.uses_budget(&memory)
    {
        return Err(bad().into());
    }
    spend(
        &p,
        size_of::<CognitionControlProjectionRowsV1>() + 4096,
        &mut ctx.cancelled,
    )?;
    let fee = memory
        .reserve(
            size_of::<CognitionControlProjectionRowsV1>() + 4096,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut dirty = [false; 10];
    if let Scope::Current = scope {
        let stop = RefCell::new(&mut ctx.cancelled);
        for root in &roots.state {
            ctx.transaction.visit_pending_writes(
                root.namespace,
                work.pending_entries,
                || {
                    spend(&p, 0, &mut || (stop.borrow_mut())())
                        .map_err(WorldFieldOverlayErrorV1::from)
                },
                |key, before, after| {
                    spend(&p, key.len(), &mut || (stop.borrow_mut())())?;
                    if before == after || !key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                        return Ok(());
                    }
                    let parsed =
                        WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
                    for (index, field) in F::ALL.iter().copied().enumerate() {
                        let d = field.descriptor();
                        if parsed.owner == d.owner && parsed.name == d.name {
                            if parsed.namespace != root.namespace {
                                return Err(bad().into());
                            }
                            dirty[index] = true;
                            break;
                        }
                    }
                    Ok(())
                },
            )?;
        }
    } else {
        dirty.fill(true)
    }
    let values = PaidValues {
        source: ctx.values.source,
        p: &p,
    };
    let mut vr = ValueReadContext {
        source: &values,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut *ctx.values.io,
    };
    let mut records = InventoryValueReader {
        values: &mut vr,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let mut rows = std::array::from_fn(|_| None);
    for (index, field) in F::ALL.iter().copied().enumerate() {
        spend(&p, 0, &mut *records.cancelled)?;
        if !dirty[index] {
            continue;
        }
        let meta = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let key = records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
        cell(work.controls_remaining)?;
        let a = lookup(
            ctx.transaction,
            &meta,
            &mut records,
            &mut ctx.take_state_error,
        )?;
        cell(work.controls_remaining)?;
        let b = lookup(
            ctx.transaction,
            &key.0,
            &mut records,
            &mut ctx.take_state_error,
        )?;
        let value = match (a, b) {
            (None, None) => None,
            (Some(a), Some(b)) => {
                records.presence(&a, &meta, 0)?;
                cell(work.controls_remaining)?;
                incremental_cognition_family::read_control(field, &key, &b, &mut records)?;
                if field.unsigned() {
                    Some(json(
                        &records.unsigned(&b, &key.0, key.1)?,
                        &memory,
                        &p,
                        &mut *records.cancelled,
                    )?)
                } else {
                    let limits = MemberRecordLimitsV1 {
                        semantic_bytes: records.semantic_bytes,
                        payload_bytes: records.record_bytes,
                        record_bytes: records.record_bytes,
                    };
                    let header = read_member_record_header_v1(
                        &b,
                        &key.0,
                        records.values,
                        limits,
                        &mut *records.cancelled,
                    )?;
                    let n = usize::try_from(header.payload_bytes).map_err(|_| limit())?;
                    spend(
                        &p,
                        n.checked_mul(4)
                            .and_then(|n| n.checked_add(128))
                            .ok_or_else(limit)?,
                        &mut *records.cancelled,
                    )?;
                    let _text = memory
                        .reserve(
                            n.checked_add(size_of::<Vec<u8>>() + size_of::<String>())
                                .ok_or_else(limit)?,
                            MemoryClass::Ordinary,
                        )
                        .map_err(ValueTreeError::Memory)?;
                    let mut bytes = Vec::new();
                    bytes.try_reserve_exact(n).map_err(|_| limit())?;
                    if bytes.capacity() > n {
                        return Err(limit().into());
                    }
                    let observed = read_text_value(
                        &b,
                        &key.0,
                        records.values,
                        MemberReadProfileV1 {
                            codec: MemberPayloadCodecV1::Text,
                            semantic_bytes: key.1,
                            limits,
                        },
                        &mut bytes,
                        "Cognition projection control UTF8",
                        &mut *records.cancelled,
                    )?;
                    if observed != n as u64 || bytes.len() != n {
                        return Err(bad().into());
                    }
                    let text = String::from_utf8(bytes).map_err(|_| bad())?;
                    Some(json(&text, &memory, &p, &mut *records.cancelled)?)
                }
            }
            _ => return Err(bad().into()),
        };
        rows[index] = Some(Row { field, json: value });
    }
    drop(records);
    drop(vr);
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    if let Scope::QualifiedAll = scope {
        ctx.transaction.validate_bootstrap_read_only_v1(live)?
    }
    Ok(CognitionControlProjectionRowsV1 {
        rows,
        guard: ctx.transaction.guard(),
        roots: digest,
        memory,
        _fee: fee,
    })
}
