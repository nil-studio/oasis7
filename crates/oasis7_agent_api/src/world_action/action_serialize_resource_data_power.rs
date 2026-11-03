// Domain-scoped Action dispatch; payload field serialization is derived.
use super::{Action, ActionTag, payloads, serialize_action_envelope};
use serde::ser::{Error as _, Serializer};

pub(super) fn serialize<S>(action: &Action, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match action {
        Action::TransferResource {
            from_agent_id,
            to_agent_id,
            kind,
            amount,
        } => serialize_action_envelope(
            serializer,
            ActionTag::TRANSFER_RESOURCE,
            payloads::TransferResourcePayload {
                from_agent_id,
                to_agent_id,
                kind,
                amount,
            },
        ),
        Action::CollectData {
            collector_agent_id,
            electricity_cost,
            data_amount,
        } => serialize_action_envelope(
            serializer,
            ActionTag::COLLECT_DATA,
            payloads::CollectDataPayload {
                collector_agent_id,
                electricity_cost,
                data_amount,
            },
        ),
        Action::CollectDataAuthenticated {
            collector_agent_id,
            electricity_cost,
            data_amount,
            player_id,
            public_key,
            nonce,
            signature,
        } => serialize_action_envelope(
            serializer,
            ActionTag::COLLECT_DATA_AUTHENTICATED,
            payloads::CollectDataAuthenticatedPayload {
                collector_agent_id,
                electricity_cost,
                data_amount,
                player_id,
                public_key,
                nonce,
                signature,
            },
        ),
        Action::GrantDataAccess {
            owner_agent_id,
            grantee_agent_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::GRANT_DATA_ACCESS,
            payloads::GrantDataAccessPayload {
                owner_agent_id,
                grantee_agent_id,
            },
        ),
        Action::RevokeDataAccess {
            owner_agent_id,
            grantee_agent_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::REVOKE_DATA_ACCESS,
            payloads::RevokeDataAccessPayload {
                owner_agent_id,
                grantee_agent_id,
            },
        ),
        Action::RedeemPower {
            node_id,
            target_agent_id,
            redeem_credits,
            nonce,
        } => serialize_action_envelope(
            serializer,
            ActionTag::REDEEM_POWER,
            payloads::RedeemPowerPayload {
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
            },
        ),
        Action::RedeemPowerSigned {
            node_id,
            target_agent_id,
            redeem_credits,
            nonce,
            signer_node_id,
            signature,
        } => serialize_action_envelope(
            serializer,
            ActionTag::REDEEM_POWER_SIGNED,
            payloads::RedeemPowerSignedPayload {
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                signer_node_id,
                signature,
            },
        ),
        Action::ApplyNodePointsSettlementSigned {
            report,
            signer_node_id,
            mint_records,
        } => serialize_action_envelope(
            serializer,
            ActionTag::APPLY_NODE_POINTS_SETTLEMENT_SIGNED,
            payloads::ApplyNodePointsSettlementSignedPayload {
                report,
                signer_node_id,
                mint_records,
            },
        ),
        _ => Err(S::Error::custom(
            "Action dispatched to wrong resource_data_power serializer",
        )),
    }
}
