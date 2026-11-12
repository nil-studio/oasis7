//! Read exactly one concrete body-state owner from the actual transaction.
//! Stable row ordinals are retained for later CAS, never reconstructed as keys.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::ExecutionFence,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    agent_state::{
        AgentBodyState, BodyModuleSlot, BodySlotType, CargoEntityEntry, CargoEntityKind,
    },
    incremental_agent_fields::AgentFieldV1 as Field,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1,
        AgentSubtreeStructureQualificationV1, AgentSubtreeValueV1 as Value,
        read_agent_subtree_scalar_overlay_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
};
use std::{
    io::{self, Write},
    mem::size_of,
};
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Current body interface typed record")
}
fn limit() -> BodyMutationError {
    ValueTreeError::ResourceLimited("body interface record layout").into()
}
fn charge(ctx: &mut FormalBoundedContextV1<'_>, n: u64) -> Result<(), BodyMutationError> {
    ctx.charge(1, n)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
struct LimitedText {
    bytes: Vec<u8>,
    cap: usize,
}
impl Write for LimitedText {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.cap)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "body interface text allowance",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(in crate::world) struct CurrentBodyInterfaceV1 {
    pub(in crate::world) body: AgentBodyState,
    pub(in crate::world) slots: Vec<u64>,
    pub(in crate::world) cargo: Vec<u64>,
    pub(in crate::world) slot_next: u64,
    pub(in crate::world) cargo_next: u64,
    fee: BodyInterfaceReadFeeV1,
    pub(in crate::world) layout_bytes: usize,
}
enum BodyInterfaceReadFeeV1 {
    Fresh(MemoryReservation),
    Prepaid(ReservedValue<()>),
}
fn scalar<M, R, E, C>(
    agent: &str,
    field: Field,
    ordinal: u64,
    q: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    max: u64,
    out: &mut impl Write,
    live: &ExecutionFence,
) -> Result<Option<Value>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    charge(common, max)?;
    if !common.memory().same_pool(context.values.memory) {
        return Err(bad());
    }
    let guard = context.transaction.guard();
    read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent,
                field,
                ordinal,
            },
            qualification: q,
        },
        context,
        max,
        out,
        &guard,
        live,
    )
    .map_err(overlay)
}
fn number<M, R, E, C>(
    agent: &str,
    field: Field,
    ordinal: u64,
    q: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Option<Value>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    scalar(
        agent,
        field,
        ordinal,
        q,
        context,
        common,
        64,
        &mut io::sink(),
        live,
    )
}
fn text<M, R, E, C>(
    agent: &str,
    field: Field,
    ordinal: u64,
    q: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    cap: usize,
    live: &ExecutionFence,
) -> Result<(Option<Value>, String), BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut out = LimitedText {
        bytes: Vec::with_capacity(cap),
        cap,
    };
    if out.bytes.capacity() > cap {
        return Err(limit());
    }
    let max = (cap as u64).checked_add(64).ok_or_else(limit)?;
    let value = scalar(
        agent, field, ordinal, q, context, common, max, &mut out, live,
    )?;
    let bytes = String::from_utf8(out.bytes).map_err(|_| bad())?;
    Ok((value, bytes))
}
fn u64_value(value: Option<Value>) -> Result<u64, BodyMutationError> {
    match value {
        Some(Value::U64(n)) => Ok(n),
        _ => Err(bad()),
    }
}
fn u16_value(value: Option<Value>) -> Result<u16, BodyMutationError> {
    match value {
        Some(Value::U16(n)) => Ok(n),
        _ => Err(bad()),
    }
}
fn i64_value(value: Option<Value>) -> Result<i64, BodyMutationError> {
    match value {
        Some(Value::I64(n)) => Ok(n),
        _ => Err(bad()),
    }
}
fn slot_type(value: Option<Value>) -> Result<BodySlotType, BodyMutationError> {
    match value {
        Some(Value::U8(0)) => Ok(BodySlotType::Power),
        Some(Value::U8(1)) => Ok(BodySlotType::Sensor),
        Some(Value::U8(2)) => Ok(BodySlotType::Mobility),
        Some(Value::U8(3)) => Ok(BodySlotType::Cognitive),
        Some(Value::U8(4)) => Ok(BodySlotType::Cargo),
        Some(Value::U8(5)) => Ok(BodySlotType::Universal),
        _ => Err(bad()),
    }
}
fn cargo_type(value: Option<Value>) -> Result<CargoEntityKind, BodyMutationError> {
    match value {
        Some(Value::U8(0)) => Ok(CargoEntityKind::Mineral),
        Some(Value::U8(1)) => Ok(CargoEntityKind::ModuleItem),
        Some(Value::U8(2)) => Ok(CargoEntityKind::InterfaceModuleItem),
        _ => Err(bad()),
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn read_current_body_interface_v1<M, R, E, C>(
    agent: &str,
    q: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    max_text_bytes: usize,
    prepaid: Option<ReservedValue<()>>,
    live: &ExecutionFence,
) -> Result<Option<CurrentBodyInterfaceV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        let capacity = number(agent, Field::SlotCapacity, 0, q, context, common, live)?;
        let Some(capacity) = capacity else {
            return Ok(None);
        };
        let capacity = u16_value(Some(capacity))?;
        let level = u16_value(number(
            agent,
            Field::ExpansionLevel,
            0,
            q,
            context,
            common,
            live,
        )?)?;
        let slot_count = u64_value(number(
            agent,
            Field::SlotCount,
            0,
            q,
            context,
            common,
            live,
        )?)?;
        let slot_next = u64_value(number(agent, Field::SlotNext, 0, q, context, common, live)?)?;
        let cargo_count = u64_value(number(
            agent,
            Field::CargoCount,
            0,
            q,
            context,
            common,
            live,
        )?)?;
        let cargo_next = u64_value(number(
            agent,
            Field::CargoNext,
            0,
            q,
            context,
            common,
            live,
        )?)?;
        if slot_count > slot_next || cargo_count > cargo_next {
            return Err(bad());
        }
        let sc = usize::try_from(slot_count).map_err(|_| limit())?;
        let cc = usize::try_from(cargo_count).map_err(|_| limit())?;
        // Every retained String reserves the explicit caller text maximum;
        // row/control cardinalities are authenticated, not guessed from DTOs.
        let texts = sc
            .checked_mul(2)
            .and_then(|n| n.checked_add(cc))
            .and_then(|n| n.checked_add(2))
            .ok_or_else(limit)?;
        let bytes = sc
            .checked_mul(size_of::<BodyModuleSlot>() + size_of::<u64>())
            .and_then(|n| {
                n.checked_add(cc.checked_mul(size_of::<CargoEntityEntry>() + size_of::<u64>())?)
            })
            .and_then(|n| n.checked_add(texts.checked_mul(max_text_bytes)?))
            .and_then(|n| n.checked_add(size_of::<CurrentBodyInterfaceV1>()))
            .ok_or_else(limit)?;
        let fee = match prepaid {
            Some(fee) => {
                if !fee.uses_budget(common.memory()) || fee.reserved_bytes() < bytes {
                    return Err(limit());
                }
                BodyInterfaceReadFeeV1::Prepaid(fee)
            }
            None => BodyInterfaceReadFeeV1::Fresh(
                common
                    .memory()
                    .reserve(bytes, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?,
            ),
        };
        let mut slots = Vec::with_capacity(sc);
        let mut slot_rows = Vec::with_capacity(sc);
        let mut cargo = Vec::with_capacity(cc);
        let mut cargo_rows = Vec::with_capacity(cc);
        if slots.capacity() > sc
            || slot_rows.capacity() > sc
            || cargo.capacity() > cc
            || cargo_rows.capacity() > cc
        {
            return Err(limit());
        }
        let (frame_tag, frame_kind) = text(
            agent,
            Field::FrameKind,
            0,
            q,
            context,
            common,
            max_text_bytes,
            live,
        )?;
        if !matches!(frame_tag, Some(Value::Text { .. })) {
            return Err(bad());
        }
        for ordinal in 0..slot_next {
            let (tag, id) = text(
                agent,
                Field::SlotId,
                ordinal,
                q,
                context,
                common,
                max_text_bytes,
                live,
            )?;
            if tag.is_none() {
                continue;
            }
            if !matches!(tag, Some(Value::Text { .. })) || slots.len() >= sc {
                return Err(bad());
            }
            let kind = slot_type(number(
                agent,
                Field::SlotType,
                ordinal,
                q,
                context,
                common,
                live,
            )?)?;
            let (tag, installed) = text(
                agent,
                Field::SlotInstalled,
                ordinal,
                q,
                context,
                common,
                max_text_bytes,
                live,
            )?;
            let installed = match tag {
                Some(Value::OptionalText(Some(_))) => Some(installed),
                Some(Value::OptionalText(None)) => None,
                _ => return Err(bad()),
            };
            let locked = match number(agent, Field::SlotLocked, ordinal, q, context, common, live)?
            {
                Some(Value::Bool(n)) => n,
                _ => return Err(bad()),
            };
            slots.push(BodyModuleSlot {
                slot_id: id,
                slot_type: kind,
                installed_module: installed,
                locked,
            });
            slot_rows.push(ordinal);
        }
        for ordinal in 0..cargo_next {
            let (tag, id) = text(
                agent,
                Field::CargoId,
                ordinal,
                q,
                context,
                common,
                max_text_bytes,
                live,
            )?;
            if tag.is_none() {
                continue;
            }
            if !matches!(tag, Some(Value::Text { .. })) || cargo.len() >= cc {
                return Err(bad());
            }
            let kind = cargo_type(number(
                agent,
                Field::CargoKind,
                ordinal,
                q,
                context,
                common,
                live,
            )?)?;
            let quantity = i64_value(number(
                agent,
                Field::CargoQuantity,
                ordinal,
                q,
                context,
                common,
                live,
            )?)?;
            let size = i64_value(number(
                agent,
                Field::CargoUnitSize,
                ordinal,
                q,
                context,
                common,
                live,
            )?)?;
            cargo.push(CargoEntityEntry {
                entity_id: id,
                entity_kind: kind,
                quantity,
                size_per_unit: size,
            });
            cargo_rows.push(ordinal);
        }
        if slots.len() != sc || cargo.len() != cc {
            return Err(bad());
        }
        charge(common, 0)?;
        Ok(Some(CurrentBodyInterfaceV1 {
            body: AgentBodyState {
                frame_kind,
                slot_capacity: capacity,
                expansion_level: level,
                slots,
                cargo_entries: cargo,
            },
            slots: slot_rows,
            cargo: cargo_rows,
            slot_next,
            cargo_next,
            fee,
            layout_bytes: bytes,
        }))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            context.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
