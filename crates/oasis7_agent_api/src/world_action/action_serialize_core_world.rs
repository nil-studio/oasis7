// Domain-scoped Action dispatch; payload field serialization is derived.
use super::{Action, ActionTag, payloads, serialize_action_envelope};
use serde::ser::{Error as _, Serializer};

pub(super) fn serialize<S>(action: &Action, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match action {
        Action::WorldRule {
            actor_id,
            request_id,
            command,
        } => serialize_action_envelope(
            serializer,
            ActionTag::WORLD_RULE,
            payloads::WorldRulePayload {
                actor_id,
                request_id,
                command,
            },
        ),
        Action::WorldServiceIntent { request } => serialize_action_envelope(
            serializer,
            ActionTag::WORLD_SERVICE_INTENT,
            payloads::WorldServiceIntentPayload { request },
        ),
        Action::RegisterAgent { agent_id, pos } => serialize_action_envelope(
            serializer,
            ActionTag::REGISTER_AGENT,
            payloads::RegisterAgentPayload { agent_id, pos },
        ),
        Action::MoveAgent { agent_id, to } => serialize_action_envelope(
            serializer,
            ActionTag::MOVE_AGENT,
            payloads::MoveAgentPayload { agent_id, to },
        ),
        Action::QueryObservation { agent_id } => serialize_action_envelope(
            serializer,
            ActionTag::QUERY_OBSERVATION,
            payloads::QueryObservationPayload { agent_id },
        ),
        Action::EmitObservation { observation } => serialize_action_envelope(
            serializer,
            ActionTag::EMIT_OBSERVATION,
            payloads::EmitObservationPayload { observation },
        ),
        Action::BodyAction {
            agent_id,
            kind,
            payload,
        } => serialize_action_envelope(
            serializer,
            ActionTag::BODY_ACTION,
            payloads::BodyActionPayload {
                agent_id,
                kind,
                payload,
            },
        ),
        Action::EmitBodyAttributes {
            agent_id,
            view,
            reason,
        } => serialize_action_envelope(
            serializer,
            ActionTag::EMIT_BODY_ATTRIBUTES,
            payloads::EmitBodyAttributesPayload {
                agent_id,
                view,
                reason,
            },
        ),
        Action::ExpandBodyInterface {
            agent_id,
            interface_module_item_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::EXPAND_BODY_INTERFACE,
            payloads::ExpandBodyInterfacePayload {
                agent_id,
                interface_module_item_id,
            },
        ),
        _ => Err(S::Error::custom(
            "Action dispatched to wrong core_world serializer",
        )),
    }
}
