//! Selected Current Supply columns, bound to a genuine retained record proof.
//! No materialization of unrelated authorization fields or dynamic maps.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current Supply lifecycle column source")
}
/// Actual Current outer presence only; this never creates record closure.
pub fn current_supply_lifecycle_record_present_v1<M, R, E, C>(
    owner: SupplyRootKeyV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary || !tx.uses_budget(ctx.values.memory) {
            return Err(bad().into());
        }
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let (_, present) = crate::incremental_world_rule_parent::read_parent_overlay(
            tx,
            &mut records,
            &mut ctx.take_state_error,
        )?;
        if !present {
            return Ok(false);
        }
        let key = owner
            .outer()
            .overlay_path(RecordPathV1::Record, &mut records)?;
        let value = crate::incremental_field_overlay::lookup(
            tx,
            &key.0,
            &mut records,
            &mut ctx.take_state_error,
        )?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(value.is_some())
    })
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_supply_lifecycle_scalar_v1<M, S, R, E, C>(
    request: SupplyScalarRequestV1<'_>,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: u64,
    output: &mut dyn std::io::Write,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<SupplyScalarValueV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            access::current(
                request.owner,
                request.qualification,
                tx,
                &mut records,
                &mut ctx.take_state_error,
            )?;
        }
        let mut read = WorldFieldReadContextV1 {
            root: request.qualification.root(),
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
        let result = read_qualified_supply_scalar_v1(request, &mut read, max, output)?;
        if (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(result)
    })
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_supply_lifecycle_member_v1<M, S, R, E, C>(
    request: SupplyMemberRequestV1<'_>,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<SupplyMemberValueV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            access::current(
                request.owner,
                request.qualification,
                tx,
                &mut records,
                &mut ctx.take_state_error,
            )?;
        }
        let mut read = WorldFieldReadContextV1 {
            root: request.qualification.root(),
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
        Ok(read_qualified_supply_member_v1(request, &mut read)?)
    })
}
/// Exact final CloseSupply fields. Call only after original allocation release
/// and actual provider material credit. No boolean here represents permission.
pub struct SupplyLifecycleCloseChangeV1<'a> {
    pub id: &'a str,
    pub qualification: &'a ReservedValue<SupplyRecordQualificationV1>,
    pub expected_active: bool,
    pub expected_escrow: i64,
    pub max_pending_node_bytes: usize,
}
pub fn close_current_supply_lifecycle_columns_v1<M, R, E, C, W, CW>(
    change: SupplyLifecycleCloseChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<SupplyRecordQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
{
    let owner = SupplyRootKeyV1::Authorization(change.id);
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(write.memory) || !change.qualification.uses_budget(write.memory) {
            return Err(bad().into());
        }
        let _phase = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        // Close changes only active and remaining escrow. The original revision,
        // crisis evidence and every dynamic member remain exactly unchanged.
        let fields = [
            (
                9,
                MemberPayloadRefV1::I64(change.expected_escrow),
                MemberPayloadRefV1::I64(0),
            ),
            (
                11,
                MemberPayloadRefV1::Bool(change.expected_active),
                MemberPayloadRefV1::Bool(false),
            ),
        ];
        let mut mutations = Vec::with_capacity(2);
        let (outer, child) = {
            let mut stopped = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut stopped,
            };
            let (outer, child) =
                access::current(owner, change.qualification, tx, &mut records, error)?;
            for (slot, before, _) in &fields {
                let key = owner
                    .relative()
                    .overlay_path(owner.spec(*slot).path, &mut records)?;
                let value = inner_tree::lookup(child, &key.0, &mut records)?.ok_or_else(bad)?;
                scalars::expected(*before, &value, &key, &mut records, 8)?;
                mutations.push(StateWriteV1 {
                    key: key.0.as_bytes().to_vec(),
                    before: Some(value),
                    after: None,
                });
            }
            (outer, child)
        };
        for ((slot, _, after), mutation) in fields.into_iter().zip(&mut mutations) {
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let row = owner
                .relative()
                .write_path(owner.spec(slot).path, after, &mut output)?;
            if row.field_key.as_bytes() != mutation.key.as_slice() {
                return Err(bad().into());
            }
            mutation.after = Some(row.value_root);
        }
        mutations.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = {
            let mut stopped = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut stopped,
            };
            inner_tree::build(
                child,
                mutations,
                &mut records,
                change.max_pending_node_bytes,
            )?
        };
        let row = {
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            access::store(owner, &outer, &delta, &mut output)?
        };
        tx.write(
            StateNamespaceV1::Authorization,
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let digest = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(bad)?;
        let proof = closure::from_typed(owner, digest, &delta.after, write.memory)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(proof)
    })
}

/// Qualify only the actually selected Current relative body when no retained
/// after-record proof is available. This is the original closed Supply codec,
/// never a scan of the Supply family or an invented authorization structure.
#[allow(clippy::too_many_arguments)]
pub fn qualify_current_supply_lifecycle_record_v1<M, S, R, E, C>(
    owner: SupplyRootKeyV1<'_>,
    retained: Option<ReservedValue<SupplyRecordQualificationV1>>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    parent: &ReservedValue<SupplyFamilyQualificationV1>,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<SupplyRecordQualificationV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || !parent.permits_class(MemoryClass::Ordinary)
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(bad().into());
        }
        let _phase = ctx
            .values
            .memory
            .reserve(RECORD_ROOT_BYTES + 8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let digest = roots.digest().map_err(|_| bad())?;
        parent.validate_binding(&digest)?;
        if digest != tx.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let (owner_digest, child) = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            let (_, present) = crate::incremental_world_rule_parent::read_parent_overlay(
                tx,
                &mut records,
                &mut ctx.take_state_error,
            )?;
            if !present {
                return Ok(None);
            }
            let key = owner
                .outer()
                .overlay_path(RecordPathV1::Record, &mut records)?;
            let Some(value) = crate::incremental_field_overlay::lookup(
                tx,
                &key.0,
                &mut records,
                &mut ctx.take_state_error,
            )?
            else {
                return Ok(None);
            };
            let mut bytes = [0u8; RECORD_ROOT_BYTES];
            read_member_record_profile_v1(
                &value,
                &key.0,
                records.values,
                MemberReadProfileV1 {
                    codec: MemberPayloadCodecV1::RecordRoot,
                    semantic_bytes: key.1,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: key.1,
                        payload_bytes: RECORD_ROOT_BYTES as u64,
                        record_bytes: records.record_bytes,
                    },
                },
                &mut std::io::sink(),
                &mut Cursor::new(bytes.as_mut_slice()),
                &mut *records.cancelled,
            )?;
            let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| bad())?;
            let owner_digest = WorldFieldKeyV1::from_bytes(key.0.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .semantic_key
                .ok_or_else(bad)?;
            let child = reference
                .validate_binding(
                    &owner.schema(),
                    &owner_digest,
                    StateNamespaceV1::Authorization,
                )
                .map_err(|_| bad())?
                .clone();
            (owner_digest, child)
        };
        if let Some(proof) = retained {
            if proof.uses_budget(ctx.values.memory)
                && proof.permits_class(MemoryClass::Ordinary)
                && proof.validate_binding(owner, &owner_digest, &child).is_ok()
            {
                if (ctx.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                return Ok(Some(proof));
            }
        }
        let mut read = WorldFieldReadContextV1 {
            root: &child,
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
        let proof = qualify_supply_body_v1(owner, owner_digest, &mut read, scan, cells, max)?;
        if (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
