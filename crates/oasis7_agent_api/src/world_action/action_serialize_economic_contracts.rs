// Domain-scoped Action dispatch; payload field serialization is derived.
use super::{Action, ActionTag, payloads, serialize_action_envelope};
use serde::ser::{Error as _, Serializer};

pub(super) fn serialize<S>(action: &Action, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match action {
        Action::OpenEconomicContract {
            creator_agent_id,
            contract_id,
            counterparty_agent_id,
            fulfillment_kind,
            settlement_kind,
            settlement_amount,
            reputation_stake,
            expires_at,
            description,
        } => serialize_action_envelope(
            serializer,
            ActionTag::OpenEconomicContract,
            payloads::OpenEconomicContractPayload {
                creator_agent_id,
                contract_id,
                counterparty_agent_id,
                fulfillment_kind,
                settlement_kind,
                settlement_amount,
                reputation_stake,
                expires_at,
                description,
            },
        ),
        Action::AcceptEconomicContract {
            accepter_agent_id,
            contract_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::AcceptEconomicContract,
            payloads::AcceptEconomicContractPayload {
                accepter_agent_id,
                contract_id,
            },
        ),
        Action::SettleEconomicContract {
            operator_agent_id,
            contract_id,
            success,
            notes,
        } => serialize_action_envelope(
            serializer,
            ActionTag::SettleEconomicContract,
            payloads::SettleEconomicContractPayload {
                operator_agent_id,
                contract_id,
                success,
                notes,
            },
        ),
        _ => Err(S::Error::custom(
            "Action dispatched to wrong economic_contracts serializer",
        )),
    }
}
