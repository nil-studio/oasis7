//! Selected actual Current request receipts. Data, never rule authority.
use crate::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_columns_read::{ColumnCellV1, ColumnReadProfileV1, read_column_row_v1},
    incremental_typed_record_codec::{RecoveredTypedRecordV1, recover_typed_record_body_v1},
    incremental_world_field_key::WorldFieldRecordKindV1,
    incremental_world_rule_composite::{
        WorldRuleCompositeBranchV1, WorldRuleCompositeReadWorkV1, WorldRuleCompositeWriteWorkV1,
    },
    incremental_world_rule_composite_family::WorldRuleCompositeQualificationV1,
    incremental_world_rule_composite_overlay::{
        WorldRuleCompositeOverlayV1, bind_current_world_rule_branch_v1,
    },
    incremental_world_rule_receipts::{WorldRuleRequestEditV1, patch_world_rule_requests_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StatePageWriter, StateRootV1, StateScanBudget,
    },
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{collections::BTreeMap, mem::size_of};
type Requests = BTreeMap<String, crate::world_rules::WorldRuleReceiptV1>;
#[path = "incremental_world_rule_observation_registry.rs"]
mod observation_registry;
pub use observation_registry::{
    CurrentObservationRegistryEntryV2, insert_current_observation_registry_v2,
    read_current_observation_registry_v2,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current world-rule request source")
}
fn debit_key(
    bytes: usize,
    work: &mut crate::incremental_semantic_key::SemanticKeyBudget,
) -> Result<(), ValueTreeError> {
    work.bytes_remaining = work
        .bytes_remaining
        .checked_sub(u64::try_from(bytes).map_err(|_| bad())?)
        .ok_or(ValueTreeError::ResourceLimited(
            "world-rule request metadata bytes",
        ))?;
    Ok(())
}
pub struct CurrentWorldRuleRequestV1 {
    overlay: ReservedValue<WorldRuleCompositeOverlayV1<Requests>>,
    key: String,
    actor: String,
    request: String,
    length: u64,
    length_value: [u8; 32],
    value: Option<RecoveredTypedRecordV1<crate::world_rules::WorldRuleReceiptV1>>,
    memory: MemoryBudget,
    identity: PreparedGuard,
    _phase: MemoryReservation,
}
impl CurrentWorldRuleRequestV1 {
    pub fn value(&self) -> Option<&crate::world_rules::WorldRuleReceiptV1> {
        self.value.as_ref().map(RecoveredTypedRecordV1::value)
    }
    /// Retained original Receipt decoder allocations, without detaching fees.
    pub fn retained_allocation_bytes_v1(
        &self,
        steps: &mut u64,
        cancelled: impl FnMut() -> bool,
    ) -> Result<usize, ValueTreeError> {
        match &self.value {
            Some(value) => value.retained_allocation_bytes_v1(steps, cancelled),
            None => Ok(0),
        }
    }
}
fn point<M, R, E, C>(
    body: &StateRootV1,
    entry: Option<&str>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_heap: usize,
) -> Result<
    Option<(
        [u8; 32],
        ReservedValue<crate::incremental_record_columns_read::ColumnRowV1>,
    )>,
    WorldFieldOverlayErrorV1,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let p = WorldRuleCompositeBranchV1::Requests.profile();
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let (key, _) = match entry {
        Some(id) => records.key(p.field, WorldFieldRecordKindV1::Entry, &("entry", id))?,
        None => records.key(p.field, WorldFieldRecordKindV1::Entry, &"_length")?,
    };
    let Some(value) = crate::incremental_record_tree::lookup(body, &key, &mut records, p.labels)?
    else {
        return Ok(None);
    };
    let bytes = key
        .as_bytes()
        .len()
        .checked_add(size_of::<StateEntryV1>())
        .ok_or_else(bad)?;
    let _fee = records
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let row = read_column_row_v1(
        ColumnReadProfileV1 {
            field: p.field,
            schema: p.schema,
            groups: &[],
            reference_schema: p.reference_schema,
        },
        &StateEntryV1 {
            namespace: body.namespace,
            key: key.as_bytes().to_vec(),
            value,
        },
        &mut records,
        max_heap,
    )?;
    Ok(Some((value, row)))
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_world_rule_request_v1<M, S, R, E, C>(
    actor: &str,
    request: &str,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Requests>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentWorldRuleRequestV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let n = actor
            .len()
            .checked_add(request.len())
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| {
                n.checked_add(
                    64 + size_of::<CurrentWorldRuleRequestV1>()
                        + size_of::<blake3::Hasher>()
                        + 64
                        + size_of::<usize>(),
                )
            })
            .ok_or_else(bad)?;
        let phase = ctx
            .values
            .memory
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        debit_key(
            actor
                .len()
                .checked_add(request.len())
                .and_then(|n| n.checked_add("oasis7.world-rule.request.v1".len() + 16))
                .ok_or_else(bad)?,
            ctx.key_work,
        )?;
        let key = crate::world_rules::request_key(actor, request);
        let overlay =
            bind_current_world_rule_branch_v1(parent, roots, state, ctx, scan, work, guard, live)?;
        let body = overlay.body();
        let (length_value, length_row) =
            point(body, None, ctx, work.max_tree_heap)?.ok_or_else(bad)?;
        let ColumnCellV1::U64(length) = &length_row.cell else {
            return Err(bad().into());
        };
        let length = *length;
        let value = match point(body, Some(&key), ctx, work.max_tree_heap)? {
            None => None,
            Some((_, row)) => {
                let ColumnCellV1::Root(child) = &row.cell else {
                    return Err(bad().into());
                };
                let mut read = WorldFieldReadContextV1 {
                    root: child,
                    state,
                    values: ValueReadContext {
                        source: ctx.values.source,
                        memory: ctx.values.memory,
                        class: ctx.values.class,
                        io: &mut *ctx.values.io,
                    },
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    take_state_error: &mut ctx.take_state_error,
                    cancelled: &mut ctx.cancelled,
                };
                Some(recover_typed_record_body_v1(
                    child,
                    &WorldRuleCompositeBranchV1::Requests.profile(),
                    &mut read,
                    scan,
                    work,
                )?)
            }
        };
        // The entry guard was checked before this owner's authenticated reads.
        // Those reads may extend the read-set; retain transaction identity and
        // the live fence while minting the new source at the resulting cut.
        ctx.transaction.validate_transaction_identity(guard)?;
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                size_of::<CurrentWorldRuleRequestV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(CurrentWorldRuleRequestV1 {
            overlay,
            key,
            actor: actor.to_owned(),
            request: request.to_owned(),
            length,
            length_value,
            value,
            memory: ctx.values.memory.clone(),
            identity: ctx.transaction.guard(),
            _phase: phase,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
/// Only insert the exact observed absent request. Existing receipts remain an
/// exact-retry decision for the fixed business owner and are not overwritten.
#[allow(clippy::too_many_arguments)]
pub fn insert_current_world_rule_request_v1<M, S, R, E, C, W, CW>(
    source: &ReservedValue<CurrentWorldRuleRequestV1>,
    receipt: &crate::world_rules::WorldRuleReceiptV1,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Requests>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut WorldRuleCompositeReadWorkV1,
    write_work: &mut WorldRuleCompositeWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<WorldRuleCompositeOverlayV1<Requests>>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        ctx.transaction
            .validate_transaction_identity(&source.identity)?;
        debit_key(
            source
                .actor
                .len()
                .checked_add(source.request.len())
                .and_then(|n| n.checked_add(receipt.actor_id.len()))
                .and_then(|n| n.checked_add(receipt.request_id.len()))
                .ok_or_else(bad)?,
            ctx.key_work,
        )?;
        if !source.uses_budget(ctx.values.memory)
            || !source.memory.same_pool(ctx.values.memory)
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || source.value.is_some()
            || receipt.actor_id != source.actor
            || receipt.request_id != source.request
        {
            return Err(bad().into());
        }
        let after = source.length.checked_add(1).ok_or_else(bad)?;
        let edits = [
            WorldRuleRequestEditV1::Count {
                expected: source.length_value,
                after,
            },
            WorldRuleRequestEditV1::Put {
                key: &source.key,
                expected: None,
                receipt,
            },
        ];
        patch_world_rule_requests_v1(
            parent,
            roots,
            Some(&source.overlay),
            &edits,
            state,
            ctx,
            write,
            scan,
            read_work,
            write_work,
            guard,
            live,
        )
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
