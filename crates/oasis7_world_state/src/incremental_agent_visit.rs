use super::*;
use crate::{
    AgentCell, AgentState, DomainEvent,
    agent_cell::{AgentActivityStatus, AgentActivityV1, AgentIntentV2},
    agent_state::{
        AgentBodyState, BodyModuleSlot, BodySlotType, CargoEntityEntry, CargoEntityKind,
    },
    incremental_agent_body::{AgentBodyKeyV1, AgentBodyValueV1},
    incremental_event_fields::MailboxKeyV1,
};
use oasis7_agent_api::{
    world_action::{GeoPos, ResourceKind},
    world_entity::RobotBodySpec,
    world_resource::ResourceStock,
};

pub trait AgentCellFieldVisitor {
    type Error;
    fn scalar(
        &mut self,
        field: AgentFieldV1,
        ordinal: u64,
        value: AgentScalarRefV1<'_>,
    ) -> Result<(), Self::Error>;
    fn body(&mut self, value: &AgentBodyValueV1) -> Result<(), Self::Error>;
    fn mailbox_frontier(&mut self, count: u64) -> Result<(), Self::Error>;
    fn mailbox(&mut self, ordinal: u64, event: &DomainEvent) -> Result<(), Self::Error>;
}
#[derive(Debug)]
pub enum AgentVisitError<E> {
    Invalid(&'static str),
    Sink(E),
}
/// Borrowed genesis lowering, never an ordinary mutation exporter. Vec order
/// is preserved using initial ordinals; duplicate semantic IDs are not merged.
pub fn visit_agent_cell<S: AgentCellFieldVisitor>(
    agent_id: &str,
    cell: &AgentCell,
    sink: &mut S,
) -> Result<(), AgentVisitError<S::Error>> {
    use AgentFieldV1 as F;
    use AgentScalarRefV1 as V;
    let AgentCell {
        state,
        mailbox,
        last_active,
        activity,
        intent,
    } = cell;
    let AgentState {
        agent_id: stored_id,
        pos,
        body,
        resources,
        body_view,
        body_state,
    } = state;
    if stored_id != agent_id || intent.as_ref().is_some_and(|i| i.agent_id != agent_id) {
        return Err(AgentVisitError::Invalid("agent identity mismatch"));
    }
    AgentBodyKeyV1::new(agent_id).map_err(AgentVisitError::Invalid)?;
    MailboxKeyV1::new(agent_id, Some(0)).map_err(AgentVisitError::Invalid)?;
    macro_rules! emit {
        ($field:ident,$value:expr) => {
            sink.scalar(F::$field, 0, $value)
                .map_err(AgentVisitError::Sink)?
        };
    }
    emit!(Identity, V::Text(agent_id));
    let GeoPos { x_cm, y_cm, z_cm } = pos;
    emit!(PositionX, V::I64(*x_cm));
    emit!(PositionY, V::I64(*y_cm));
    emit!(PositionZ, V::I64(*z_cm));
    let RobotBodySpec { kind, height_cm } = body;
    emit!(BodyKind, V::Text(kind));
    emit!(BodyHeight, V::I64(*height_cm));
    let ResourceStock { amounts } = resources;
    for (kind, amount) in amounts {
        let ordinal = match kind {
            ResourceKind::Electricity => 0,
            ResourceKind::Data => 1,
        };
        sink.scalar(F::ResourceAmount, ordinal, V::I64(*amount))
            .map_err(AgentVisitError::Sink)?;
    }
    sink.body(&AgentBodyValueV1 {
        body_view: body_view.clone(),
        last_active: *last_active,
    })
    .map_err(AgentVisitError::Sink)?;
    let AgentBodyState {
        frame_kind,
        slot_capacity,
        slots,
        expansion_level,
        cargo_entries,
    } = body_state;
    emit!(FrameKind, V::Text(frame_kind));
    emit!(SlotCapacity, V::U16(*slot_capacity));
    emit!(ExpansionLevel, V::U16(*expansion_level));
    emit!(SlotCount, V::U64(slots.len() as u64));
    emit!(SlotNext, V::U64(slots.len() as u64));
    for (ordinal, slot) in slots.iter().enumerate() {
        let BodyModuleSlot {
            slot_id,
            slot_type,
            installed_module,
            locked,
        } = slot;
        let kind = match slot_type {
            BodySlotType::Power => 0,
            BodySlotType::Sensor => 1,
            BodySlotType::Mobility => 2,
            BodySlotType::Cognitive => 3,
            BodySlotType::Cargo => 4,
            BodySlotType::Universal => 5,
        };
        for (field, value) in [
            (F::SlotId, V::Text(slot_id)),
            (F::SlotType, V::U8(kind)),
            (
                F::SlotInstalled,
                V::OptionalText(installed_module.as_deref()),
            ),
            (F::SlotLocked, V::Bool(*locked)),
        ] {
            sink.scalar(field, ordinal as u64, value)
                .map_err(AgentVisitError::Sink)?;
        }
    }
    emit!(CargoCount, V::U64(cargo_entries.len() as u64));
    emit!(CargoNext, V::U64(cargo_entries.len() as u64));
    for (ordinal, entry) in cargo_entries.iter().enumerate() {
        let CargoEntityEntry {
            entity_id,
            entity_kind,
            quantity,
            size_per_unit,
        } = entry;
        let kind = match entity_kind {
            CargoEntityKind::Mineral => 0,
            CargoEntityKind::ModuleItem => 1,
            CargoEntityKind::InterfaceModuleItem => 2,
        };
        for (field, value) in [
            (F::CargoId, V::Text(entity_id)),
            (F::CargoKind, V::U8(kind)),
            (F::CargoQuantity, V::I64(*quantity)),
            (F::CargoUnitSize, V::I64(*size_per_unit)),
        ] {
            sink.scalar(field, ordinal as u64, value)
                .map_err(AgentVisitError::Sink)?;
        }
    }
    emit!(ActivityPresent, V::Bool(activity.is_some()));
    if let Some(AgentActivityV1 {
        status,
        updated_at,
        operation_kind,
        operation_id,
        target_id,
        reason_code,
        reason_summary,
    }) = activity
    {
        let status = match status {
            AgentActivityStatus::Idle => 0,
            AgentActivityStatus::Executing => 1,
            AgentActivityStatus::Blocked => 2,
            AgentActivityStatus::Waiting => 3,
            AgentActivityStatus::Unavailable => 4,
        };
        emit!(ActivityStatus, V::U8(status));
        emit!(ActivityUpdated, V::U64(*updated_at));
        emit!(
            ActivityOperationKind,
            V::OptionalText(operation_kind.as_deref())
        );
        emit!(ActivityOperationId, V::OptionalU64(*operation_id));
        emit!(ActivityTarget, V::OptionalText(target_id.as_deref()));
        emit!(ActivityReasonCode, V::OptionalText(reason_code.as_deref()));
        emit!(
            ActivityReasonSummary,
            V::OptionalText(reason_summary.as_deref())
        );
    }
    emit!(IntentPresent, V::Bool(intent.is_some()));
    if let Some(AgentIntentV2 {
        schema_version,
        agent_id,
        intent_id,
        kind,
        summary,
        target_id,
        effect_intent_id,
        intent_tick,
        world_id,
        reorg_epoch,
        authority_scope,
        status,
        source,
        logical_time,
        event_seq,
        updated_at,
        receipt_ref,
        reason_code,
        reason_summary,
        replaced_by,
        actor_id,
        request_digest,
    }) = intent
    {
        emit!(IntentSchema, V::U32(*schema_version));
        emit!(IntentAgent, V::Text(agent_id));
        emit!(IntentId, V::Text(intent_id));
        emit!(IntentKind, V::Text(kind));
        emit!(IntentSummary, V::Text(summary));
        emit!(IntentTarget, V::OptionalText(target_id.as_deref()));
        emit!(IntentEffect, V::OptionalText(effect_intent_id.as_deref()));
        emit!(IntentTick, V::OptionalU64(*intent_tick));
        emit!(IntentWorld, V::OptionalText(world_id.as_deref()));
        emit!(IntentReorg, V::OptionalU64(*reorg_epoch));
        emit!(
            IntentAuthorityScope,
            V::OptionalText(authority_scope.as_deref())
        );
        emit!(IntentStatus, V::Text(status));
        emit!(IntentSource, V::Text(source));
        emit!(IntentLogicalTime, V::U64(*logical_time));
        emit!(IntentEventSeq, V::U64(*event_seq));
        emit!(IntentUpdated, V::U64(*updated_at));
        emit!(IntentReceipt, V::OptionalText(receipt_ref.as_deref()));
        emit!(IntentReasonCode, V::OptionalText(reason_code.as_deref()));
        emit!(
            IntentReasonSummary,
            V::OptionalText(reason_summary.as_deref())
        );
        emit!(IntentReplacedBy, V::OptionalText(replaced_by.as_deref()));
        emit!(IntentActor, V::Text(actor_id));
        emit!(IntentRequestDigest, V::Text(request_digest));
    }
    sink.mailbox_frontier(mailbox.len() as u64)
        .map_err(AgentVisitError::Sink)?;
    for (index, event) in mailbox.iter().enumerate() {
        sink.mailbox(index as u64, event)
            .map_err(AgentVisitError::Sink)?;
    }
    Ok(())
}
