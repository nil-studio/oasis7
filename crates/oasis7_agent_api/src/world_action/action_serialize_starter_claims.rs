// Domain-scoped Action dispatch; payload field serialization is derived.
use super::{Action, ActionTag, payloads, serialize_action_envelope};
use serde::ser::{Error as _, Serializer};

pub(super) fn serialize<S>(action: &Action, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match action {
        Action::TopUpRestrictedStarterClaimLiveopsPool {
            controller_account_id,
            top_up_id,
            amount,
        } => serialize_action_envelope(
            serializer,
            ActionTag::TOP_UP_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
            payloads::TopUpRestrictedStarterClaimLiveopsPoolPayload {
                controller_account_id,
                top_up_id,
                amount,
            },
        ),
        Action::IssueRestrictedStarterClaimGrant {
            issuer_account_id,
            beneficiary_account_id,
            amount,
            issuance_reason,
            expires_at_epoch,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ISSUE_RESTRICTED_STARTER_CLAIM_GRANT,
            payloads::IssueRestrictedStarterClaimGrantPayload {
                issuer_account_id,
                beneficiary_account_id,
                amount,
                issuance_reason,
                expires_at_epoch,
            },
        ),
        Action::RevokeRestrictedStarterClaimGrant {
            issuer_account_id,
            beneficiary_account_id,
            revoke_reason,
        } => serialize_action_envelope(
            serializer,
            ActionTag::REVOKE_RESTRICTED_STARTER_CLAIM_GRANT,
            payloads::RevokeRestrictedStarterClaimGrantPayload {
                issuer_account_id,
                beneficiary_account_id,
                revoke_reason,
            },
        ),
        Action::ClaimAgent {
            claimer_agent_id,
            target_agent_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::CLAIM_AGENT,
            payloads::ClaimAgentPayload {
                claimer_agent_id,
                target_agent_id,
            },
        ),
        Action::ClaimStarterOc {
            agent_id,
            player_id,
            public_key,
        } => serialize_action_envelope(
            serializer,
            ActionTag::CLAIM_STARTER_OC,
            payloads::ClaimStarterOcPayload {
                agent_id,
                player_id,
                public_key,
            },
        ),
        Action::ReleaseAgentClaim {
            claimer_agent_id,
            target_agent_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::RELEASE_AGENT_CLAIM,
            payloads::ReleaseAgentClaimPayload {
                claimer_agent_id,
                target_agent_id,
            },
        ),
        _ => Err(S::Error::custom(
            "Action dispatched to wrong starter_claims serializer",
        )),
    }
}
