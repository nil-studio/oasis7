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
            ActionTag::WorldRule,
            payloads::WorldRulePayload {
                actor_id,
                request_id,
                command,
            },
        ),
        Action::WorldServiceIntent { request } => serialize_action_envelope(
            serializer,
            ActionTag::WorldServiceIntent,
            payloads::WorldServiceIntentPayload { request },
        ),
        Action::RegisterAgent { agent_id, pos } => serialize_action_envelope(
            serializer,
            ActionTag::RegisterAgent,
            payloads::RegisterAgentPayload { agent_id, pos },
        ),
        Action::MoveAgent { agent_id, to } => serialize_action_envelope(
            serializer,
            ActionTag::MoveAgent,
            payloads::MoveAgentPayload { agent_id, to },
        ),
        Action::QueryObservation { agent_id } => serialize_action_envelope(
            serializer,
            ActionTag::QueryObservation,
            payloads::QueryObservationPayload { agent_id },
        ),
        Action::EmitObservation { observation } => serialize_action_envelope(
            serializer,
            ActionTag::EmitObservation,
            payloads::EmitObservationPayload { observation },
        ),
        Action::BodyAction {
            agent_id,
            kind,
            payload,
        } => serialize_action_envelope(
            serializer,
            ActionTag::BodyAction,
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
            ActionTag::EmitBodyAttributes,
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
            ActionTag::ExpandBodyInterface,
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
