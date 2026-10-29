use super::super::*;

#[derive(Debug, PartialEq)]
pub(super) enum WorldAgentView<'a> {
    WorldRuleCommitted {
        action_id: &'a u64,
        actor_id: &'a String,
        request_id: &'a String,
        command: &'a oasis7_agent_api::world_action::WorldRuleCommandV1,
        governance_epoch: &'a u64,
        world_binding: &'a Option<String>,
    },
    AgentRegistered {
        registration_contract_version: &'a u8,
        agent_id: &'a String,
        pos: &'a GeoPos,
    },
    AgentMoved {
        agent_id: &'a String,
        from: &'a GeoPos,
        to: &'a GeoPos,
    },
    AgentLocationAuthorityUpdated {
        authority: &'a AgentLocationAuthorityV1,
    },
    LocationAnchorUpdated {
        anchor: &'a LocationAnchorV1,
    },
    FactorySiteAuthorityUpdated {
        authority: &'a FactorySiteAuthorityV1,
    },
    FactoryConstructionPowerProfileUpdated {
        profile: &'a FactoryConstructionPowerProfileV1,
    },
    ActionAccepted {
        action_id: &'a ActionId,
        action_kind: &'a String,
        actor_id: &'a String,
        eta_ticks: &'a u64,
        notes: &'a Vec<String>,
    },
    ActionRejected {
        action_id: &'a ActionId,
        reason: &'a RejectReason,
    },
    AgentIntentProposed {
        intent: &'a AgentIntentV2,
    },
    AgentIntentSubmitted {
        intent: &'a AgentIntentV2,
    },
    AgentIntentAccepted {
        intent: &'a AgentIntentV2,
    },
    AgentIntentReplaced {
        intent: &'a AgentIntentV2,
    },
    AgentIntentTransitioned {
        intent: &'a AgentIntentV2,
    },
    Observation {
        observation: &'a Observation,
    },
    BodyAttributesUpdated {
        agent_id: &'a String,
        view: &'a BodyKernelView,
        reason: &'a String,
    },
    BodyAttributesRejected {
        agent_id: &'a String,
        reason: &'a String,
    },
    BodyInterfaceExpanded {
        agent_id: &'a String,
        slot_capacity: &'a u16,
        expansion_level: &'a u16,
        consumed_item_id: &'a String,
        new_slot_id: &'a String,
        slot_type: &'a BodySlotType,
    },
    BodyInterfaceExpandRejected {
        agent_id: &'a String,
        consumed_item_id: &'a String,
        reason: &'a String,
    },
}

impl<'a> WorldAgentView<'a> {
    pub(super) fn from_event(event: &'a DomainEvent) -> Self {
        match event {
            DomainEvent::WorldRuleCommitted {
                action_id,
                actor_id,
                request_id,
                command,
                governance_epoch,
                world_binding,
            } => Self::WorldRuleCommitted {
                action_id,
                actor_id,
                request_id,
                command,
                governance_epoch,
                world_binding,
            },
            DomainEvent::AgentRegistered {
                registration_contract_version,
                agent_id,
                pos,
            } => Self::AgentRegistered {
                registration_contract_version,
                agent_id,
                pos,
            },
            DomainEvent::AgentMoved { agent_id, from, to } => {
                Self::AgentMoved { agent_id, from, to }
            }
            DomainEvent::AgentLocationAuthorityUpdated { authority } => {
                Self::AgentLocationAuthorityUpdated { authority }
            }
            DomainEvent::LocationAnchorUpdated { anchor } => Self::LocationAnchorUpdated { anchor },
            DomainEvent::FactorySiteAuthorityUpdated { authority } => {
                Self::FactorySiteAuthorityUpdated { authority }
            }
            DomainEvent::FactoryConstructionPowerProfileUpdated { profile } => {
                Self::FactoryConstructionPowerProfileUpdated { profile }
            }
            DomainEvent::ActionAccepted {
                action_id,
                action_kind,
                actor_id,
                eta_ticks,
                notes,
            } => Self::ActionAccepted {
                action_id,
                action_kind,
                actor_id,
                eta_ticks,
                notes,
            },
            DomainEvent::ActionRejected { action_id, reason } => {
                Self::ActionRejected { action_id, reason }
            }
            DomainEvent::AgentIntentProposed { intent } => Self::AgentIntentProposed { intent },
            DomainEvent::AgentIntentSubmitted { intent } => Self::AgentIntentSubmitted { intent },
            DomainEvent::AgentIntentAccepted { intent } => Self::AgentIntentAccepted { intent },
            DomainEvent::AgentIntentReplaced { intent } => Self::AgentIntentReplaced { intent },
            DomainEvent::AgentIntentTransitioned { intent } => {
                Self::AgentIntentTransitioned { intent }
            }
            DomainEvent::Observation { observation } => Self::Observation { observation },
            DomainEvent::BodyAttributesUpdated {
                agent_id,
                view,
                reason,
            } => Self::BodyAttributesUpdated {
                agent_id,
                view,
                reason,
            },
            DomainEvent::BodyAttributesRejected { agent_id, reason } => {
                Self::BodyAttributesRejected { agent_id, reason }
            }
            DomainEvent::BodyInterfaceExpanded {
                agent_id,
                slot_capacity,
                expansion_level,
                consumed_item_id,
                new_slot_id,
                slot_type,
            } => Self::BodyInterfaceExpanded {
                agent_id,
                slot_capacity,
                expansion_level,
                consumed_item_id,
                new_slot_id,
                slot_type,
            },
            DomainEvent::BodyInterfaceExpandRejected {
                agent_id,
                consumed_item_id,
                reason,
            } => Self::BodyInterfaceExpandRejected {
                agent_id,
                consumed_item_id,
                reason,
            },
            _ => unreachable!("event was routed to the wrong world_agent view"),
        }
    }
}
