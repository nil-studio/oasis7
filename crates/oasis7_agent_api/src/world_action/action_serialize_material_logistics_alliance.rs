// Domain-scoped Action dispatch; payload field serialization is derived.
use super::{Action, ActionTag, payloads, serialize_action_envelope};
use serde::ser::{Error as _, Serializer};

pub(super) fn serialize<S>(action: &Action, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match action {
        Action::TransferMaterial {
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            amount,
            distance_km,
            priority,
            route_id,
            route_ids,
            auto_reroute,
        } => serialize_action_envelope(
            serializer,
            ActionTag::TransferMaterial,
            payloads::TransferMaterialPayload {
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                amount,
                distance_km,
                priority,
                route_id,
                route_ids,
                auto_reroute,
            },
        ),
        Action::RegisterLogisticsRoute {
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            distance_km,
            priority,
            capacity_units,
            tariff_electricity_per_unit,
        } => serialize_action_envelope(
            serializer,
            ActionTag::RegisterLogisticsRoute,
            payloads::RegisterLogisticsRoutePayload {
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                distance_km,
                priority,
                capacity_units,
                tariff_electricity_per_unit,
            },
        ),
        Action::SetLogisticsRouteAvailability {
            requester_agent_id,
            route_id,
            available,
        } => serialize_action_envelope(
            serializer,
            ActionTag::SetLogisticsRouteAvailability,
            payloads::SetLogisticsRouteAvailabilityPayload {
                requester_agent_id,
                route_id,
                available,
            },
        ),
        Action::FormAlliance {
            proposer_agent_id,
            alliance_id,
            members,
            charter,
        } => serialize_action_envelope(
            serializer,
            ActionTag::FormAlliance,
            payloads::FormAlliancePayload {
                proposer_agent_id,
                alliance_id,
                members,
                charter,
            },
        ),
        Action::JoinAlliance {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::JoinAlliance,
            payloads::JoinAlliancePayload {
                operator_agent_id,
                alliance_id,
                member_agent_id,
            },
        ),
        Action::LeaveAlliance {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::LeaveAlliance,
            payloads::LeaveAlliancePayload {
                operator_agent_id,
                alliance_id,
                member_agent_id,
            },
        ),
        Action::DissolveAlliance {
            operator_agent_id,
            alliance_id,
            reason,
        } => serialize_action_envelope(
            serializer,
            ActionTag::DissolveAlliance,
            payloads::DissolveAlliancePayload {
                operator_agent_id,
                alliance_id,
                reason,
            },
        ),
        Action::DeclareWar {
            initiator_agent_id,
            war_id,
            aggressor_alliance_id,
            defender_alliance_id,
            objective,
            intensity,
        } => serialize_action_envelope(
            serializer,
            ActionTag::DeclareWar,
            payloads::DeclareWarPayload {
                initiator_agent_id,
                war_id,
                aggressor_alliance_id,
                defender_alliance_id,
                objective,
                intensity,
            },
        ),
        _ => Err(S::Error::custom(
            "Action dispatched to wrong material_logistics_alliance serializer",
        )),
    }
}
