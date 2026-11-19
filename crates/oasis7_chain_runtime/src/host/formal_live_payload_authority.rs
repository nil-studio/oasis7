//! Current authorization for formal submissions and retained signed originals.
use super::{
    World, WorldServicePayloadV1, agent_authority, agent_chat, authority, game_task, gameplay,
};

pub(super) fn validate_payload(
    world: &World,
    payload: &WorldServicePayloadV1,
) -> Result<(), String> {
    match payload {
        WorldServicePayloadV1::GameStarterClaim(signed) => {
            let binding = world
                .current_cognition_runtime_binding()
                .map_err(|e| format!("{e:?}"))?;
            oasis7_runtime::service_admission::starter::validate_owner(
                world,
                signed,
                &binding.branch_id,
                binding.reorg_epoch,
            )
        }
        WorldServicePayloadV1::GameParticipantRegister { owner, admission } => {
            let binding = world
                .current_cognition_runtime_binding()
                .map_err(|e| format!("{e:?}"))?;
            oasis7_runtime::service_admission::participant::validate_admission(
                world,
                owner,
                admission,
                &binding.branch_id,
                binding.reorg_epoch,
            )
        }
        WorldServicePayloadV1::GameTaskCreate(signed) => game_task::validate_owner(world, signed),
        WorldServicePayloadV1::GameGoalPublish(signed) => {
            game_task::validate_publish_owner(world, signed)
        }
        WorldServicePayloadV1::GameExecutorControl(signed) => {
            game_task::validate_executor_owner(world, signed)
        }
        WorldServicePayloadV1::GameNativeDelegation(signed) => {
            oasis7_runtime::service_admission::native_delegation::validate_owner(world, signed)
        }
        WorldServicePayloadV1::GameplayJson(bytes) => {
            gameplay::authenticated_action(world, bytes).map(|_| ())
        }
        WorldServicePayloadV1::SafeAlternative(signed) => {
            oasis7_runtime::service_admission::safe_alternative::validate_owner(world, signed)
        }
        WorldServicePayloadV1::AgentChat(request) => agent_chat::validate_owner(world, request),
        WorldServicePayloadV1::Cognition(signed) => {
            agent_authority::validate_cognition(world, signed)
        }
        WorldServicePayloadV1::FeedbackAck(signed) => {
            agent_authority::validate_feedback_ack(world, signed)
        }
        WorldServicePayloadV1::Delegation(signed) => {
            agent_authority::validate_delegation(world, signed)
        }
        WorldServicePayloadV1::Scheduler(signed) => {
            authority::verify_read_request("scheduler", signed)?;
            agent_authority::validate_agent_signer(
                world,
                &signed.request.agent_id,
                &signed.subject_public_key,
                signed.request.delegation_generation,
            )
        }
    }
}

pub(super) fn validate_lookup_payload(
    world: &World,
    payload: &WorldServicePayloadV1,
) -> Result<(), String> {
    if let WorldServicePayloadV1::GameNativeDelegation(signed) = payload {
        oasis7_runtime::service_admission::native_delegation::validate_receipt_owner(world, signed)
    } else {
        validate_payload(world, payload)
    }
}
