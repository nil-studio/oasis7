//! Closed parent-key scan plus actual transaction additions/deletions.
//! The result is paid current data, never logistics or publication authority.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_key_read::{OriginalKeyKind, OriginalSemanticKeyProbe},
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_record_root::RECORD_ROOT_BYTES,
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    cell::RefCell,
    io::{self, Write},
    mem::size_of,
};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("current logistics route scan")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("current logistics route scan")
}
/// Only this fixed Ordinary producer can construct prepaid scan storage.
/// Capacity is a finite resource allowance, never a statement of cardinality.
pub struct CurrentLogisticsRouteScanReservationV1 {
    memory: MemoryBudget,
    capacity: usize,
    max_id_bytes: usize,
    fee: MemoryReservation,
}
pub fn reserve_current_logistics_route_scan_v1(
    qualification: &ReservedValue<LogisticsFamilyQualificationV1>,
    memory: &MemoryBudget,
    capacity: usize,
    max_id_bytes: usize,
) -> Result<CurrentLogisticsRouteScanReservationV1, WorldFieldOverlayErrorV1> {
    if !qualification.uses_budget(memory)
        || qualification.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    let item = max_id_bytes
        .checked_add(size_of::<ReservedValue<String>>())
        .ok_or_else(limited)?;
    let layout = capacity
        .checked_mul(
            size_of::<ReservedValue<String>>()
                .checked_add(item)
                .ok_or_else(limited)?,
        )
        .and_then(|n| n.checked_add(size_of::<ReservedValue<Vec<ReservedValue<String>>>>()))
        .and_then(|n| n.checked_add(size_of::<CurrentLogisticsRouteScanReservationV1>()))
        .ok_or_else(limited)?;
    let fee = memory
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(CurrentLogisticsRouteScanReservationV1 {
        memory: memory.clone(),
        capacity,
        max_id_bytes,
        fee,
    })
}
struct LimitedBytes {
    bytes: Vec<u8>,
    limit: usize,
    hit_limit: bool,
}
impl Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.hit_limit = true;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "logistics route id allowance",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn decode_id<R: ValueNodeReader, C: FnMut() -> bool>(
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_id_bytes: usize,
    fee: MemoryReservation,
) -> Result<ReservedValue<String>, ValueTreeError> {
    // The existing original decoder validates both the outer semantic ID and
    // the exact child-root binding; the embedded RouteId is deliberately not
    // substituted for that original map key.
    family::reference(LogisticsRecordKindV1::Route, key, value, records)?;
    let mut output = LimitedBytes {
        bytes: Vec::with_capacity(max_id_bytes),
        limit: max_id_bytes,
        hit_limit: false,
    };
    if output.bytes.capacity() > max_id_bytes {
        return Err(limited());
    }
    let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Text, &mut output);
    let mut scratch = [0; RECORD_ROOT_BYTES];
    let result = read_member_record_v1(
        value,
        key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: records.record_bytes,
        },
        &mut probe,
        &mut &mut scratch[..],
        &mut *records.cancelled,
    );
    let probe_error = probe.error();
    let framing = probe.finish();
    drop(probe);
    if output.hit_limit {
        return Err(limited());
    }
    if let Some(reason) = probe_error {
        return Err(ValueTreeError::Invalid(reason));
    }
    result?;
    framing?;
    let text = String::from_utf8(output.bytes).map_err(|_| invalid())?;
    Ok(fee.retain(text))
}
fn route_key(raw: &[u8]) -> Result<Option<WorldFieldKeyV1>, ValueTreeError> {
    if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    if binding.name != LogisticsRecordKindV1::Route.field_name() {
        return Ok(None);
    }
    let key = WorldFieldKeyV1::new(
        LogisticsRecordKindV1::Route.descriptor()?,
        binding.kind,
        binding.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != raw || binding.namespace != StateNamespaceV1::Business {
        return Err(invalid());
    }
    Ok((binding.kind == WorldFieldRecordKindV1::Entry).then_some(key))
}
/// Enumerate every currently present route exactly once: parent entries are
/// looked up in the actual overlay (so deletes disappear and replacements are
/// read), then only genuine before-absent additions are included. No unbounded
/// set or caller assertion of emptiness is used. All IDs retain their fees.
#[allow(clippy::too_many_arguments)]
pub fn read_current_logistics_route_ids_v1<M, P, R, E, C>(
    roots: &RootSetV1,
    qualification: &ReservedValue<LogisticsFamilyQualificationV1>,
    source: &P,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    pending_remaining: &mut usize,
    max_id_bytes: usize,
) -> Result<ReservedValue<Vec<ReservedValue<String>>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let reservation = (|| {
        qualification.validate_binding(&roots.digest().map_err(|_| invalid())?)?;
        let mut additions = 0usize;
        context.transaction.visit_pending_writes(
            StateNamespaceV1::Business,
            pending_remaining,
            || {
                if (context.cancelled)() {
                    Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled))
                } else {
                    Ok(())
                }
            },
            |raw, before, after| {
                if before.is_none() && after.is_some() && route_key(raw)?.is_some() {
                    // The qualified parent necessarily contains its format
                    // marker, so a before-absent Entry cannot be that marker.
                    additions = additions.checked_add(1).ok_or_else(limited)?;
                }
                Ok::<(), WorldFieldOverlayErrorV1>(())
            },
        )?;
        let capacity = usize::try_from(qualification.record_counts()[0])
            .map_err(|_| limited())?
            .checked_add(additions)
            .ok_or_else(limited)?;
        reserve_current_logistics_route_scan_v1(
            qualification,
            context.values.memory,
            capacity,
            max_id_bytes,
        )
    })();
    match reservation {
        Ok(reservation) => read_current_logistics_route_ids_with_reservation_v1(
            roots,
            qualification,
            source,
            context,
            scan,
            pending_remaining,
            reservation,
        ),
        Err(error) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                context.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            Err(error)
        }
    }
}
/// Same exact Current scan using storage prepaid by the fixed Ordinary
/// reservation producer, including failure paths after an Accepted audit.
#[allow(clippy::too_many_arguments)]
pub fn read_current_logistics_route_ids_with_reservation_v1<M, P, R, E, C>(
    roots: &RootSetV1,
    qualification: &ReservedValue<LogisticsFamilyQualificationV1>,
    source: &P,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    pending_remaining: &mut usize,
    reservation: CurrentLogisticsRouteScanReservationV1,
) -> Result<ReservedValue<Vec<ReservedValue<String>>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        let memory = context.values.memory;
        if context.values.class != MemoryClass::Ordinary
            || !context.transaction.uses_budget(memory)
            || !qualification.uses_budget(memory)
            || qualification.allocation_class() != MemoryClass::Ordinary
            || source.allocation_class() != Some(MemoryClass::Ordinary)
            || source
                .allocation_budget()
                .is_none_or(|pool| !pool.same_pool(memory))
        {
            return Err(invalid().into());
        }
        qualification.validate_binding(&roots.digest().map_err(|_| invalid())?)?;
        for root in &roots.state {
            if context.transaction.parent_state_root(root.namespace) != root {
                return Err(invalid().into());
            }
        }
        if !reservation.memory.same_pool(memory) {
            return Err(invalid().into());
        }
        let count = qualification.record_counts()[0];
        let CurrentLogisticsRouteScanReservationV1 {
            capacity,
            max_id_bytes,
            fee: mut paid,
            ..
        } = reservation;
        let item_bytes = max_id_bytes
            .checked_add(size_of::<ReservedValue<String>>())
            .ok_or_else(limited)?;
        let layout = capacity
            .checked_mul(size_of::<ReservedValue<String>>())
            .and_then(|n| n.checked_add(size_of::<ReservedValue<Vec<ReservedValue<String>>>>()))
            .ok_or_else(limited)?;
        let list_fee = paid.split(layout).map_err(|_| invalid())?;
        let mut ids = Vec::with_capacity(capacity);
        if ids.capacity() > capacity {
            return Err(limited().into());
        }
        // Use the existing original format key, not an assumed hash/literal.
        let cancelled = RefCell::new(&mut context.cancelled);
        let marker = {
            let mut cancellation = cancelled.borrow_mut();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut **cancellation,
            };
            container::overlay(
                LogisticsRecordKindV1::Route.sample(),
                context.transaction,
                &mut records,
                &mut context.take_state_error,
            )?;
            records
                .key(
                    LogisticsRecordKindV1::Route.descriptor()?,
                    WorldFieldRecordKindV1::Entry,
                    &(RECORD_FORMAT_KEY, ()),
                )?
                .0
        };
        let mut observed = 0u64;
        let root = roots.state_root(StateNamespaceV1::Business);
        let scanned = StateScanContext {
            source,
            memory,
            class: MemoryClass::Ordinary,
            io: scan,
        }
        .visit_entries(
            root,
            |entry| {
                let Some(key) = route_key(&entry.key)? else {
                    return Ok(());
                };
                if key == marker {
                    return Ok(());
                }
                observed = observed.checked_add(1).ok_or_else(limited)?;
                let current = context
                    .transaction
                    .read(StateNamespaceV1::Business, key.as_bytes())?;
                if let Some(value) = current {
                    let mut cancellation = cancelled.borrow_mut();
                    let mut records = InventoryValueReader {
                        values: &mut context.values,
                        key_work: &mut *context.key_work,
                        semantic_bytes: context.semantic_bytes,
                        record_bytes: context.record_bytes,
                        cancelled: &mut **cancellation,
                    };
                    if ids.len() == capacity {
                        return Err(WorldFieldOverlayErrorV1::from(limited()));
                    }
                    let item_fee = paid.split(item_bytes).map_err(|_| invalid())?;
                    ids.push(decode_id(
                        &key,
                        &value,
                        &mut records,
                        max_id_bytes,
                        item_fee,
                    )?);
                }
                Ok::<(), WorldFieldOverlayErrorV1>(())
            },
            || (cancelled.borrow_mut())(),
        );
        if let Some(error) = (context.take_state_error)() {
            return Err(error.into());
        }
        match scanned {
            Ok(_) => {}
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e).into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
            Err(StateScanError::ResourceLimited) => return Err(limited().into()),
            _ => return Err(invalid().into()),
        }
        if observed != count {
            return Err(invalid().into());
        }
        // visit_pending_writes borrows the transaction immutably. Decode only
        // the before-absent values it yields; these are exactly additions not
        // visited by the immutable parent tree, including same-tx registrations.
        context.transaction.visit_pending_writes(
            StateNamespaceV1::Business,
            pending_remaining,
            || {
                if (cancelled.borrow_mut())() {
                    Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled))
                } else {
                    Ok(())
                }
            },
            |raw, before, after| {
                if before.is_some() {
                    return Ok(());
                }
                let Some(value) = after else {
                    return Ok(());
                };
                let Some(key) = route_key(raw)? else {
                    return Ok(());
                };
                if key == marker {
                    return Ok(());
                }
                let mut cancellation = cancelled.borrow_mut();
                let mut records = InventoryValueReader {
                    values: &mut context.values,
                    key_work: &mut *context.key_work,
                    semantic_bytes: context.semantic_bytes,
                    record_bytes: context.record_bytes,
                    cancelled: &mut **cancellation,
                };
                if ids.len() == capacity {
                    return Err(WorldFieldOverlayErrorV1::from(limited()));
                }
                let item_fee = paid.split(item_bytes).map_err(|_| invalid())?;
                ids.push(decode_id(
                    &key,
                    &value,
                    &mut records,
                    max_id_bytes,
                    item_fee,
                )?);
                Ok(())
            },
        )?;
        Ok(list_fee.retain(ids))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            context.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
