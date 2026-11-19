//! A single transaction-bound snapshot of lifecycle selectors.
//!
//! The accepted index is only a seed for unchanged shipment rows. Supply and
//! Emergency selectors are reopened from the actual transaction, and shipment
//! rows named by the transaction's original semantic keys are reopened through
//! the existing overlay codec. This is data only; callers still use the
//! selected-record readers and their original business predicates.
use super::*;
use crate::{
    incremental_field_member::{FieldOwner, MemberRecordLimitsV1, read_member_record_key_v1},
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
    incremental_typed_record_codec::TypedRecordReadWorkV1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    incremental_world_rule_parent::descriptor as world_rules_descriptor,
    incremental_world_rule_shipments::{
        WorldRuleShipmentValueV1, read_world_rule_shipment_overlay_v1,
    },
    incremental_world_rule_supply_root::open_current_supply_lifecycle_directory_v1,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    io::{self, Write},
    mem::{align_of, size_of},
    sync::Arc,
};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Current WorldRules lifecycle directory")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current WorldRules lifecycle directory")
}
fn add(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_add(b).ok_or_else(limited)
}

fn canonical_world_rules_entry(raw: &[u8]) -> Result<Option<WorldFieldKeyV1>, ValueTreeError> {
    const DOMAIN: &[u8] = b"oasis7.iws.world-field-key.v1\0";
    if !raw.starts_with(DOMAIN) {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    if parsed.owner != FieldOwner::WorldState || parsed.name != "world_rules" {
        return Ok(None);
    }
    if parsed.namespace != StateNamespaceV1::Authorization {
        return Err(invalid());
    }
    let key = WorldFieldKeyV1::new(
        world_rules_descriptor().map_err(|_| invalid())?,
        parsed.kind,
        parsed.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != raw {
        return Err(invalid());
    }
    match parsed.kind {
        WorldFieldRecordKindV1::Metadata => Ok(None),
        WorldFieldRecordKindV1::Entry => Ok(Some(key)),
        _ => Err(invalid()),
    }
}

fn shipment_id(semantic: &[u8]) -> Result<Option<u64>, ValueTreeError> {
    let decoded = read_semantic_key_v1(semantic).map_err(ValueTreeError::Invalid)?;
    let SemanticKeyRefV1::Pair { left, right } = decoded else {
        return Ok(None);
    };
    let (branch, id, column) = match left.decode().map_err(ValueTreeError::Invalid)? {
        SemanticKeyRefV1::Text(branch) => (
            branch,
            right.decode().map_err(ValueTreeError::Invalid)?,
            None,
        ),
        SemanticKeyRefV1::Pair { left, right: id } => {
            let SemanticKeyRefV1::Text(branch) = left.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Ok(None);
            };
            let SemanticKeyRefV1::Text(column) = right.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Ok(None);
            };
            (
                branch,
                id.decode().map_err(ValueTreeError::Invalid)?,
                Some(column),
            )
        }
        _ => return Ok(None),
    };
    if branch != "supply_shipments" {
        return Ok(None);
    }
    let SemanticKeyRefV1::Unsigned(id) = id else {
        return Err(invalid());
    };
    match column {
        None => Ok(Some(id)),
        Some(
            "authorization_id" | "batch_id" | "beneficiary_id" | "lineage_id" | "input_amount"
            | "delivered_amount" | "settled",
        ) => Ok(Some(id)),
        _ => Err(invalid()),
    }
}

fn debit(
    work: &mut WorldRulesFamilyWorkV1,
    records: u64,
    bytes: usize,
) -> Result<(), ValueTreeError> {
    work.outer_records_remaining = work
        .outer_records_remaining
        .checked_sub(records)
        .ok_or_else(limited)?;
    work.remainder.cells_remaining = work
        .remainder
        .cells_remaining
        .checked_sub(u64::try_from(bytes).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    Ok(())
}

/// Growable exact-capacity output that retains its reservation with the text.
struct PaidTextOutput {
    bytes: Vec<u8>,
    fee: MemoryReservation,
    memory: MemoryBudget,
    max: usize,
}
impl PaidTextOutput {
    fn new(memory: &MemoryBudget, max: usize) -> Result<Self, ValueTreeError> {
        Ok(Self {
            bytes: Vec::new(),
            fee: memory.reserve(0, MemoryClass::Ordinary)?,
            memory: memory.clone(),
            max,
        })
    }
    fn into_text(self) -> Result<(String, MemoryReservation), ValueTreeError> {
        let text = String::from_utf8(self.bytes).map_err(|_| invalid())?;
        if text.capacity() != self.fee.bytes() {
            return Err(invalid());
        }
        Ok((text, self.fee))
    }
}
impl Write for PaidTextOutput {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let next = self
            .bytes
            .len()
            .checked_add(input.len())
            .filter(|n| *n <= self.max)
            .ok_or_else(|| io::Error::other("Current shipment text bound"))?;
        if next > self.bytes.capacity() {
            let mut fee = self
                .memory
                .reserve(next - self.bytes.capacity(), MemoryClass::Ordinary)
                .map_err(|_| io::Error::other("Current shipment text budget"))?;
            self.bytes
                .try_reserve_exact(next - self.bytes.len())
                .map_err(|_| io::Error::other("Current shipment text allocation"))?;
            if self.bytes.capacity() != next {
                return Err(io::Error::other("Current shipment text capacity"));
            }
            self.fee.merge(&mut fee).map_err(io::Error::other)?;
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn row_fee(bytes: usize) -> Result<usize, ValueTreeError> {
    add(
        size_of::<LifecycleSupplyShipmentV1>(),
        add(
            bytes,
            size_of::<Arc<ReservedValue<LifecycleSupplyShipmentV1>>>()
                + 2 * size_of::<usize>()
                + align_of::<LifecycleSupplyShipmentV1>(),
        )?,
    )
}

fn read_current_shipment<M, R, E, C>(
    id: u64,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_payload: usize,
) -> Result<Option<Arc<ReservedValue<LifecycleSupplyShipmentV1>>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let memory = ctx.values.memory.clone();
    let mut a = PaidTextOutput::new(&memory, max_payload)?;
    let mut b = PaidTextOutput::new(&memory, max_payload)?;
    let mut c = PaidTextOutput::new(&memory, max_payload)?;
    let mut d = PaidTextOutput::new(&memory, max_payload)?;
    let value = {
        let mut outputs: [&mut dyn Write; 4] = [&mut a, &mut b, &mut c, &mut d];
        read_world_rule_shipment_overlay_v1(id, ctx, max_payload as u64, &mut outputs)?
    };
    let (lengths, input_amount, delivered_amount, settled) = match value {
        WorldRuleShipmentValueV1::RulesAbsent | WorldRuleShipmentValueV1::ShipmentAbsent => {
            return Ok(None);
        }
        WorldRuleShipmentValueV1::Value {
            identity_utf8_bytes,
            input_amount,
            delivered_amount,
            settled,
        } => (identity_utf8_bytes, input_amount, delivered_amount, settled),
    };
    let (authorization_id, fa) = a.into_text()?;
    let (batch_id, fb) = b.into_text()?;
    let (beneficiary_id, fc) = c.into_text()?;
    let (lineage_id, fd) = d.into_text()?;
    let fields = [authorization_id, batch_id, beneficiary_id, lineage_id];
    if fields
        .iter()
        .zip(lengths)
        .any(|(text, expected)| text.len() != usize::try_from(expected).unwrap_or(usize::MAX))
    {
        return Err(invalid().into());
    }
    // Four output Strings are moved into the row without cloning their text.
    let [authorization_id, batch_id, beneficiary_id, lineage_id] = fields;
    let row = LifecycleSupplyShipmentV1 {
        outer_id: id,
        value: crate::world_rules::SupplyShipmentV1 {
            authorization_id,
            batch_id,
            beneficiary_id,
            lineage_id,
            input_amount,
            delivered_amount,
            settled,
        },
    };
    let bytes = [fa.bytes(), fb.bytes(), fc.bytes(), fd.bytes()]
        .into_iter()
        .try_fold(0usize, add)?;
    let layout = row_fee(bytes)?.checked_sub(bytes).ok_or_else(limited)?;
    let mut fee = memory
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut a = fa;
    let mut b = fb;
    let mut c = fc;
    let mut d = fd;
    for text_fee in [&mut a, &mut b, &mut c, &mut d] {
        fee.merge(text_fee).map_err(ValueTreeError::Invalid)?;
    }
    Ok(Some(Arc::new(fee.retain(row))))
}

fn collect_changed_shipments<M, R, E, C>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut WorldRulesFamilyWorkV1,
    pending_entries: &mut usize,
) -> Result<(Vec<u64>, MemoryReservation), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use std::cell::RefCell;
    let memory = ctx.values.memory.clone();
    let max_semantic = usize::try_from(ctx.semantic_bytes)
        .unwrap_or(usize::MAX)
        .min(usize::try_from(work.max_semantic_bytes).unwrap_or(usize::MAX));
    let cells = RefCell::new(&mut work.remainder.cells_remaining);
    let records = RefCell::new(&mut work.outer_records_remaining);
    let stop = RefCell::new(&mut ctx.cancelled);
    let mut count = 0usize;
    ctx.transaction.visit_pending_writes(
        StateNamespaceV1::Authorization,
        pending_entries,
        || -> Result<(), WorldFieldOverlayErrorV1> {
            if (stop.borrow_mut())() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(())
        },
        |raw, _, _| {
            if let Some(key) = canonical_world_rules_entry(raw)? {
                let mut remaining = records.borrow_mut();
                **remaining = (**remaining).checked_sub(1).ok_or_else(limited)?;
                let mut cells = cells.borrow_mut();
                **cells = (**cells)
                    .checked_sub(u64::try_from(raw.len()).map_err(|_| limited())?)
                    .ok_or_else(limited)?;
                let _ = key;
                count = count.checked_add(1).ok_or_else(limited)?;
            }
            Ok::<_, WorldFieldOverlayErrorV1>(())
        },
    )?;
    let bytes = count
        .checked_mul(size_of::<u64>())
        .and_then(|n| n.checked_add(size_of::<Vec<u64>>() + 64))
        .ok_or_else(limited)?;
    let fee = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut ids = Vec::new();
    ids.try_reserve_exact(count).map_err(|_| limited())?;
    if ids.capacity() != count {
        return Err(invalid().into());
    }
    let mut seen = 0usize;
    ctx.transaction.visit_pending_writes(
        StateNamespaceV1::Authorization,
        pending_entries,
        || -> Result<(), WorldFieldOverlayErrorV1> {
            if (stop.borrow_mut())() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(())
        },
        |raw, before, after| {
            let Some(key) = canonical_world_rules_entry(raw)? else {
                return Ok(());
            };
            seen = seen.checked_add(1).ok_or_else(limited)?;
            if seen > count {
                return Err(invalid().into());
            }
            {
                let mut remaining = records.borrow_mut();
                **remaining = (**remaining).checked_sub(1).ok_or_else(limited)?;
                let mut cells = cells.borrow_mut();
                **cells = (**cells)
                    .checked_sub(u64::try_from(raw.len()).map_err(|_| limited())?)
                    .ok_or_else(limited)?;
            }
            let value = after.as_ref().or(before.as_ref()).ok_or_else(invalid)?;
            let semantic_fee = memory
                .reserve(
                    max_semantic
                        .checked_add(size_of::<Vec<u8>>() + 64)
                        .ok_or_else(limited)?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let mut semantic = Vec::new();
            semantic
                .try_reserve_exact(max_semantic)
                .map_err(|_| limited())?;
            if semantic.capacity() != max_semantic {
                return Err(invalid().into());
            }
            let mut cancelled = || (stop.borrow_mut())();
            read_member_record_key_v1(
                value,
                &key,
                &mut ctx.values,
                MemberRecordLimitsV1 {
                    semantic_bytes: u64::try_from(max_semantic).map_err(|_| limited())?,
                    payload_bytes: ctx.record_bytes,
                    record_bytes: ctx.record_bytes,
                },
                &mut semantic,
                &mut cancelled,
            )?;
            drop(semantic_fee);
            if let Some(id) = shipment_id(&semantic)? {
                ids.push(id);
            }
            Ok::<_, WorldFieldOverlayErrorV1>(())
        },
    )?;
    if seen != count {
        return Err(invalid().into());
    }
    if (stop.borrow_mut())() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let n = ids.len();
    let comparisons = n
        .checked_mul(if n <= 1 {
            0
        } else {
            usize::BITS as usize - (n - 1).leading_zeros() as usize
        })
        .ok_or_else(limited)?;
    let cmp_bytes = comparisons
        .checked_mul(size_of::<u64>())
        .ok_or_else(limited)?;
    let mut records_left = records.borrow_mut();
    **records_left = (**records_left)
        .checked_sub(u64::try_from(comparisons).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    let mut cells_left = cells.borrow_mut();
    **cells_left = (**cells_left)
        .checked_sub(u64::try_from(cmp_bytes).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    drop(records_left);
    drop(cells_left);
    ids.sort_unstable();
    ids.dedup();
    Ok((ids, fee))
}

/// One read-only Current lifecycle snapshot. The strict guard is captured only
/// after all source and selected-row reads have completed.
pub struct CurrentWorldRuleLifecycleDirectoryV1 {
    parent: [u8; 32],
    guard: PreparedGuard,
    memory: MemoryBudget,
    supply:
        ReservedValue<crate::incremental_world_rule_supply_root::CurrentSupplyLifecycleDirectoryV1>,
    emergency_source: ReservedValue<CurrentEmergencyLifecycleSourceV1>,
    emergency: ReservedValue<CurrentEmergencyLifecycleDirectoryV1>,
    shipments: Vec<Arc<ReservedValue<LifecycleSupplyShipmentV1>>>,
    _shipments_fee: MemoryReservation,
}
impl CurrentWorldRuleLifecycleDirectoryV1 {
    pub fn supply(
        &self,
    ) -> &ReservedValue<crate::incremental_world_rule_supply_root::CurrentSupplyLifecycleDirectoryV1>
    {
        &self.supply
    }
    pub fn emergency_source(&self) -> &ReservedValue<CurrentEmergencyLifecycleSourceV1> {
        &self.emergency_source
    }
    pub fn emergency(&self) -> &ReservedValue<CurrentEmergencyLifecycleDirectoryV1> {
        &self.emergency
    }
    /// Keep selectors and their exact Current source paired at the call site.
    pub fn emergency_scope(
        &self,
    ) -> (
        &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
        &ReservedValue<CurrentEmergencyLifecycleDirectoryV1>,
    ) {
        (&self.emergency_source, &self.emergency)
    }
    pub fn supply_shipments(&self) -> impl ExactSizeIterator<Item = &LifecycleSupplyShipmentV1> {
        self.shipments.iter().map(|row| &***row)
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        roots: &RootSetV1,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !tx.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || self.parent != tx.parent_fence().parent_roots
            || self.parent != roots.digest().map_err(|_| invalid())?
            || !self.supply.uses_budget(memory)
            || !self.emergency.uses_budget(memory)
            || !self.emergency_source.uses_budget(memory)
            || self.shipments.iter().any(|row| !row.uses_budget(memory))
        {
            return Err(invalid().into());
        }
        Ok(())
    }
}

/// Reconcile accepted selectors with this transaction's actual Current.
/// Only transaction-touched shipment identities are reopened; untouched rows
/// remain shared from the genuine accepted parent index.
#[allow(clippy::too_many_arguments)]
pub fn open_current_world_rule_lifecycle_directory_v1<M, S, R, E, C>(
    parent: &ReservedValue<WorldRulesQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    world_work: &mut WorldRulesFamilyWorkV1,
    typed_work: &mut TypedRecordReadWorkV1,
    pending_entries: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentWorldRuleLifecycleDirectoryV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || !parent.uses_budget(ctx.values.memory)
            || parent.allocation_class() != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        parent
            .validate_binding(roots)
            .map_err(WorldFieldOverlayErrorV1::from)?;
        let digest = roots.digest().map_err(|_| invalid())?;
        if digest != ctx.transaction.parent_fence().parent_roots {
            return Err(invalid().into());
        }
        let parent_index = parent.lifecycle_index();
        parent_index.check_budget(ctx.values.memory, MemoryClass::Ordinary)?;

        let emergency_bind_guard = ctx.transaction.guard();
        let emergency_source = bind_current_emergency_lifecycle_v1(
            parent.emergency(),
            roots,
            state,
            ctx,
            scan,
            typed_work,
            &emergency_bind_guard,
            live,
        )?;
        let emergency_project_guard = ctx.transaction.guard();
        let emergency = project_current_emergency_lifecycle_v1(
            &emergency_source,
            parent.emergency(),
            roots,
            state,
            ctx,
            scan,
            typed_work,
            &emergency_project_guard,
            live,
        )?;
        emergency.validate(ctx.transaction, roots, ctx.values.memory, live)?;
        let supply_guard = ctx.transaction.guard();
        let supply = open_current_supply_lifecycle_directory_v1(
            parent.supply(),
            roots,
            state,
            ctx,
            scan,
            &mut world_work.remainder.cells_remaining,
            pending_entries,
            world_work.remainder.max_payload_bytes,
            &supply_guard,
            live,
        )?;
        supply.validate(ctx.transaction, ctx.values.memory, live)?;

        let (changed, _changed_fee) = collect_changed_shipments(ctx, world_work, pending_entries)?;
        let parent_rows = &parent_index.supply_shipments;
        let maximum = parent_rows
            .len()
            .checked_add(changed.len())
            .ok_or_else(limited)?;
        let shipment_slot_bytes = maximum
            .checked_mul(size_of::<Arc<ReservedValue<LifecycleSupplyShipmentV1>>>())
            .and_then(|n| {
                n.checked_add(size_of::<Vec<Arc<ReservedValue<LifecycleSupplyShipmentV1>>>>() + 64)
            })
            .ok_or_else(limited)?;
        let shipment_fee = ctx
            .values
            .memory
            .reserve(shipment_slot_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut shipments = Vec::new();
        shipments
            .try_reserve_exact(maximum)
            .map_err(|_| limited())?;
        if shipments.capacity() != maximum {
            return Err(invalid().into());
        }
        let mut old_at = 0usize;
        let mut changed_at = 0usize;
        let max_payload =
            usize::try_from(world_work.remainder.max_payload_bytes).map_err(|_| limited())?;
        while old_at < parent_rows.len() || changed_at < changed.len() {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let old_id = parent_rows.get(old_at).map(|row| row.outer_id);
            let changed_id = changed.get(changed_at).copied();
            match (old_id, changed_id) {
                (Some(old), Some(new)) if old < new => {
                    let row = &parent_rows[old_at];
                    let bytes = row.value.authorization_id.len()
                        + row.value.batch_id.len()
                        + row.value.beneficiary_id.len()
                        + row.value.lineage_id.len();
                    debit(world_work, 1, bytes)?;
                    shipments.push(Arc::clone(row));
                    old_at += 1;
                }
                (Some(old), Some(new)) if old == new => {
                    debit(world_work, 1, size_of::<u64>())?;
                    if let Some(row) = read_current_shipment(new, ctx, max_payload)? {
                        let bytes = row.value.authorization_id.len()
                            + row.value.batch_id.len()
                            + row.value.beneficiary_id.len()
                            + row.value.lineage_id.len();
                        debit(world_work, 0, bytes)?;
                        shipments.push(row);
                    }
                    old_at += 1;
                    changed_at += 1;
                }
                (_, Some(new)) => {
                    debit(world_work, 1, size_of::<u64>())?;
                    if let Some(row) = read_current_shipment(new, ctx, max_payload)? {
                        if !supply.rules_present() {
                            return Err(invalid().into());
                        }
                        let bytes = row.value.authorization_id.len()
                            + row.value.batch_id.len()
                            + row.value.beneficiary_id.len()
                            + row.value.lineage_id.len();
                        debit(world_work, 0, bytes)?;
                        shipments.push(row);
                    }
                    changed_at += 1;
                }
                (Some(_), None) => {
                    let row = &parent_rows[old_at];
                    let bytes = row.value.authorization_id.len()
                        + row.value.batch_id.len()
                        + row.value.beneficiary_id.len()
                        + row.value.lineage_id.len();
                    debit(world_work, 1, bytes)?;
                    shipments.push(Arc::clone(row));
                    old_at += 1;
                }
                (None, None) => break,
            }
        }
        if !supply.rules_present() && !shipments.is_empty() {
            return Err(invalid().into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(
                size_of::<CurrentWorldRuleLifecycleDirectoryV1>() + 512,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let final_guard = ctx.transaction.guard();
        ctx.transaction.validate_guard(&final_guard, live)?;
        Ok(fee.retain(CurrentWorldRuleLifecycleDirectoryV1 {
            parent: digest,
            guard: final_guard,
            memory: ctx.values.memory.clone(),
            supply,
            emergency_source,
            emergency,
            shipments,
            _shipments_fee: shipment_fee,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
