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
            ActionTag::TransferResource,
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
            ActionTag::CollectData,
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
            ActionTag::CollectDataAuthenticated,
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
            ActionTag::GrantDataAccess,
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
            ActionTag::RevokeDataAccess,
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
            ActionTag::RedeemPower,
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
            ActionTag::RedeemPowerSigned,
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
            ActionTag::ApplyNodePointsSettlementSigned,
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
