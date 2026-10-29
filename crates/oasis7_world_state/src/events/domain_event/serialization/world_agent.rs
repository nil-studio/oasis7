use super::*;

#[derive(serde::Serialize)]
#[serde(rename = "WorldRuleCommitted")]
struct WorldRuleCommittedPayload<'a> {
    action_id: &'a u64,
    actor_id: &'a String,
    request_id: &'a String,
    command: &'a oasis7_agent_api::world_action::WorldRuleCommandV1,
    governance_epoch: &'a u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    world_binding: &'a Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename = "AgentRegistered")]
struct AgentRegisteredPayload<'a> {
    #[serde(skip_serializing_if = "is_legacy_claim_contract_version")]
    registration_contract_version: &'a u8,
    agent_id: &'a String,
    pos: &'a GeoPos,
}

#[derive(serde::Serialize)]
#[serde(rename = "AgentMoved")]
struct AgentMovedPayload<'a> {
    agent_id: &'a String,
    from: &'a GeoPos,
    to: &'a GeoPos,
}

#[derive(serde::Serialize)]
#[serde(rename = "AgentLocationAuthorityUpdated")]
struct AgentLocationAuthorityUpdatedPayload<'a> {
    authority: &'a AgentLocationAuthorityV1,
}

#[derive(serde::Serialize)]
#[serde(rename = "LocationAnchorUpdated")]
struct LocationAnchorUpdatedPayload<'a> {
    anchor: &'a LocationAnchorV1,
}

#[derive(serde::Serialize)]
#[serde(rename = "FactorySiteAuthorityUpdated")]
struct FactorySiteAuthorityUpdatedPayload<'a> {
    authority: &'a FactorySiteAuthorityV1,
}

#[derive(serde::Serialize)]
#[serde(rename = "FactoryConstructionPowerProfileUpdated")]
struct FactoryConstructionPowerProfileUpdatedPayload<'a> {
    profile: &'a FactoryConstructionPowerProfileV1,
}

#[derive(serde::Serialize)]
#[serde(rename = "ActionAccepted")]
struct ActionAcceptedPayload<'a> {
    action_id: &'a ActionId,
    action_kind: &'a String,
    actor_id: &'a String,
    eta_ticks: &'a u64,
    notes: &'a Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename = "ActionRejected")]
struct ActionRejectedPayload<'a> {
    action_id: &'a ActionId,
    reason: &'a RejectReason,
}

#[derive(serde::Serialize)]
#[serde(rename = "AgentIntentProposed")]
struct AgentIntentProposedPayload<'a> {
    intent: &'a AgentIntentV2,
}

#[derive(serde::Serialize)]
#[serde(rename = "AgentIntentSubmitted")]
struct AgentIntentSubmittedPayload<'a> {
    intent: &'a AgentIntentV2,
}

#[derive(serde::Serialize)]
#[serde(rename = "AgentIntentAccepted")]
struct AgentIntentAcceptedPayload<'a> {
    intent: &'a AgentIntentV2,
}

#[derive(serde::Serialize)]
#[serde(rename = "AgentIntentReplaced")]
struct AgentIntentReplacedPayload<'a> {
    intent: &'a AgentIntentV2,
}

#[derive(serde::Serialize)]
#[serde(rename = "AgentIntentTransitioned")]
struct AgentIntentTransitionedPayload<'a> {
    intent: &'a AgentIntentV2,
}

#[derive(serde::Serialize)]
#[serde(rename = "Observation")]
struct ObservationPayload<'a> {
    observation: &'a Observation,
}

#[derive(serde::Serialize)]
#[serde(rename = "BodyAttributesUpdated")]
struct BodyAttributesUpdatedPayload<'a> {
    agent_id: &'a String,
    view: &'a BodyKernelView,
    reason: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "BodyAttributesRejected")]
struct BodyAttributesRejectedPayload<'a> {
    agent_id: &'a String,
    reason: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "BodyInterfaceExpanded")]
struct BodyInterfaceExpandedPayload<'a> {
    agent_id: &'a String,
    slot_capacity: &'a u16,
    expansion_level: &'a u16,
    consumed_item_id: &'a String,
    new_slot_id: &'a String,
    slot_type: &'a BodySlotType,
}

#[derive(serde::Serialize)]
#[serde(rename = "BodyInterfaceExpandRejected")]
struct BodyInterfaceExpandRejectedPayload<'a> {
    agent_id: &'a String,
    consumed_item_id: &'a String,
    reason: &'a String,
}

pub(super) fn serialize<S: serde::Serializer>(
    event: &DomainEvent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match event {
        DomainEvent::WorldRuleCommitted {
            action_id,
            actor_id,
            request_id,
            command,
            governance_epoch,
            world_binding,
        } => serialize_adjacent(
            serializer,
            Tag::WorldRuleCommitted,
            &WorldRuleCommittedPayload {
                action_id,
                actor_id,
                request_id,
                command,
                governance_epoch,
                world_binding,
            },
        ),
        DomainEvent::AgentRegistered {
            registration_contract_version,
            agent_id,
            pos,
        } => serialize_adjacent(
            serializer,
            Tag::AgentRegistered,
            &AgentRegisteredPayload {
                registration_contract_version,
                agent_id,
                pos,
            },
        ),
        DomainEvent::AgentMoved { agent_id, from, to } => serialize_adjacent(
            serializer,
            Tag::AgentMoved,
            &AgentMovedPayload { agent_id, from, to },
        ),
        DomainEvent::AgentLocationAuthorityUpdated { authority } => serialize_adjacent(
            serializer,
            Tag::AgentLocationAuthorityUpdated,
            &AgentLocationAuthorityUpdatedPayload { authority },
        ),
        DomainEvent::LocationAnchorUpdated { anchor } => serialize_adjacent(
            serializer,
            Tag::LocationAnchorUpdated,
            &LocationAnchorUpdatedPayload { anchor },
        ),
        DomainEvent::FactorySiteAuthorityUpdated { authority } => serialize_adjacent(
            serializer,
            Tag::FactorySiteAuthorityUpdated,
            &FactorySiteAuthorityUpdatedPayload { authority },
        ),
        DomainEvent::FactoryConstructionPowerProfileUpdated { profile } => serialize_adjacent(
            serializer,
            Tag::FactoryConstructionPowerProfileUpdated,
            &FactoryConstructionPowerProfileUpdatedPayload { profile },
        ),
        DomainEvent::ActionAccepted {
            action_id,
            action_kind,
            actor_id,
            eta_ticks,
            notes,
        } => serialize_adjacent(
            serializer,
            Tag::ActionAccepted,
            &ActionAcceptedPayload {
                action_id,
                action_kind,
                actor_id,
                eta_ticks,
                notes,
            },
        ),
        DomainEvent::ActionRejected { action_id, reason } => serialize_adjacent(
            serializer,
            Tag::ActionRejected,
            &ActionRejectedPayload { action_id, reason },
        ),
        DomainEvent::AgentIntentProposed { intent } => serialize_adjacent(
            serializer,
            Tag::AgentIntentProposed,
            &AgentIntentProposedPayload { intent },
        ),
        DomainEvent::AgentIntentSubmitted { intent } => serialize_adjacent(
            serializer,
            Tag::AgentIntentSubmitted,
            &AgentIntentSubmittedPayload { intent },
        ),
        DomainEvent::AgentIntentAccepted { intent } => serialize_adjacent(
            serializer,
            Tag::AgentIntentAccepted,
            &AgentIntentAcceptedPayload { intent },
        ),
        DomainEvent::AgentIntentReplaced { intent } => serialize_adjacent(
            serializer,
            Tag::AgentIntentReplaced,
            &AgentIntentReplacedPayload { intent },
        ),
        DomainEvent::AgentIntentTransitioned { intent } => serialize_adjacent(
            serializer,
            Tag::AgentIntentTransitioned,
            &AgentIntentTransitionedPayload { intent },
        ),
        DomainEvent::Observation { observation } => serialize_adjacent(
            serializer,
            Tag::Observation,
            &ObservationPayload { observation },
        ),
        DomainEvent::BodyAttributesUpdated {
            agent_id,
            view,
            reason,
        } => serialize_adjacent(
            serializer,
            Tag::BodyAttributesUpdated,
            &BodyAttributesUpdatedPayload {
                agent_id,
                view,
                reason,
            },
        ),
        DomainEvent::BodyAttributesRejected { agent_id, reason } => serialize_adjacent(
            serializer,
            Tag::BodyAttributesRejected,
            &BodyAttributesRejectedPayload { agent_id, reason },
        ),
        DomainEvent::BodyInterfaceExpanded {
            agent_id,
            slot_capacity,
            expansion_level,
            consumed_item_id,
            new_slot_id,
            slot_type,
        } => serialize_adjacent(
            serializer,
            Tag::BodyInterfaceExpanded,
            &BodyInterfaceExpandedPayload {
                agent_id,
                slot_capacity,
                expansion_level,
                consumed_item_id,
                new_slot_id,
                slot_type,
            },
        ),
        DomainEvent::BodyInterfaceExpandRejected {
            agent_id,
            consumed_item_id,
            reason,
        } => serialize_adjacent(
            serializer,
            Tag::BodyInterfaceExpandRejected,
            &BodyInterfaceExpandRejectedPayload {
                agent_id,
                consumed_item_id,
                reason,
            },
        ),
        _ => unreachable!("DomainEvent serializer group mismatch"),
    }
}
