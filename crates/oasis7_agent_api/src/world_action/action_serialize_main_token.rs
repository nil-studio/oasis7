// Domain-scoped Action dispatch; payload field serialization is derived.
use super::{Action, ActionTag, payloads, serialize_action_envelope};
use serde::ser::{Error as _, Serializer};

pub(super) fn serialize<S>(action: &Action, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match action {
        Action::InitializeMainTokenGenesis { allocations } => serialize_action_envelope(
            serializer,
            ActionTag::InitializeMainTokenGenesis,
            payloads::InitializeMainTokenGenesisPayload { allocations },
        ),
        Action::ClaimMainTokenVesting {
            bucket_id,
            beneficiary,
            nonce,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ClaimMainTokenVesting,
            payloads::ClaimMainTokenVestingPayload {
                bucket_id,
                beneficiary,
                nonce,
            },
        ),
        Action::TransferMainToken {
            from_account_id,
            to_account_id,
            amount,
            nonce,
            asset_id,
            memo,
            chain_id,
            network_id,
            tx_version,
            tx_type,
            valid_until_unix_ms,
            max_fee,
            fee_asset_id,
            application_payload_hash,
            client_request_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::TransferMainToken,
            payloads::TransferMainTokenPayload {
                from_account_id,
                to_account_id,
                amount,
                nonce,
                asset_id,
                memo,
                chain_id,
                network_id,
                tx_version,
                tx_type,
                valid_until_unix_ms,
                max_fee,
                fee_asset_id,
                application_payload_hash,
                client_request_id,
            },
        ),
        Action::ApplyMainTokenEpochIssuance {
            epoch_index,
            actual_stake_ratio_bps,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ApplyMainTokenEpochIssuance,
            payloads::ApplyMainTokenEpochIssuancePayload {
                epoch_index,
                actual_stake_ratio_bps,
            },
        ),
        Action::SettleMainTokenFee { fee_kind, amount } => serialize_action_envelope(
            serializer,
            ActionTag::SettleMainTokenFee,
            payloads::SettleMainTokenFeePayload { fee_kind, amount },
        ),
        Action::UpdateMainTokenPolicy { proposal_id, next } => serialize_action_envelope(
            serializer,
            ActionTag::UpdateMainTokenPolicy,
            payloads::UpdateMainTokenPolicyPayload { proposal_id, next },
        ),
        Action::DistributeMainTokenTreasury {
            proposal_id,
            distribution_id,
            bucket_id,
            distributions,
        } => serialize_action_envelope(
            serializer,
            ActionTag::DistributeMainTokenTreasury,
            payloads::DistributeMainTokenTreasuryPayload {
                proposal_id,
                distribution_id,
                bucket_id,
                distributions,
            },
        ),
        _ => Err(S::Error::custom(
            "Action dispatched to wrong main_token serializer",
        )),
    }
}
