// Domain-scoped Action dispatch; payload field serialization is derived.
use super::{Action, ActionTag, payloads, serialize_action_envelope};
use serde::ser::{Error as _, Serializer};

pub(super) fn serialize<S>(action: &Action, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match action {
        Action::OpenGovernanceProposal {
            proposer_agent_id,
            proposal_key,
            title,
            description,
            options,
            voting_window_ticks,
            quorum_weight,
            pass_threshold_bps,
        } => serialize_action_envelope(
            serializer,
            ActionTag::OpenGovernanceProposal,
            payloads::OpenGovernanceProposalPayload {
                proposer_agent_id,
                proposal_key,
                title,
                description,
                options,
                voting_window_ticks,
                quorum_weight,
                pass_threshold_bps,
            },
        ),
        Action::CastGovernanceVote {
            voter_agent_id,
            proposal_key,
            option,
            weight,
        } => serialize_action_envelope(
            serializer,
            ActionTag::CastGovernanceVote,
            payloads::CastGovernanceVotePayload {
                voter_agent_id,
                proposal_key,
                option,
                weight,
            },
        ),
        Action::ResolveCrisis {
            resolver_agent_id,
            crisis_id,
            strategy,
            success,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ResolveCrisis,
            payloads::ResolveCrisisPayload {
                resolver_agent_id,
                crisis_id,
                strategy,
                success,
            },
        ),
        Action::GrantMetaProgress {
            operator_agent_id,
            target_agent_id,
            track,
            points,
            achievement_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::GrantMetaProgress,
            payloads::GrantMetaProgressPayload {
                operator_agent_id,
                target_agent_id,
                track,
                points,
                achievement_id,
            },
        ),
        Action::UpdateGameplayPolicy {
            operator_agent_id,
            electricity_tax_bps,
            data_tax_bps,
            power_trade_fee_bps,
            max_open_contracts_per_agent,
            blocked_agents,
            forbidden_location_ids,
        } => serialize_action_envelope(
            serializer,
            ActionTag::UpdateGameplayPolicy,
            payloads::UpdateGameplayPolicyPayload {
                operator_agent_id,
                electricity_tax_bps,
                data_tax_bps,
                power_trade_fee_bps,
                max_open_contracts_per_agent,
                blocked_agents,
                forbidden_location_ids,
            },
        ),
        Action::UpdateRestrictedStarterClaimAdminRegistry {
            controller_account_id,
            next_admin_account_ids,
        } => serialize_action_envelope(
            serializer,
            ActionTag::UpdateRestrictedStarterClaimAdminRegistry,
            payloads::UpdateRestrictedStarterClaimAdminRegistryPayload {
                controller_account_id,
                next_admin_account_ids,
            },
        ),
        Action::SubmitGovernanceValidatorAdmission {
            controller_account_id,
            candidate_id,
            node_id,
            finality_signer_public_key,
            stake,
            operator_owner,
            public_manifest_hash,
        } => serialize_action_envelope(
            serializer,
            ActionTag::SubmitGovernanceValidatorAdmission,
            payloads::SubmitGovernanceValidatorAdmissionPayload {
                controller_account_id,
                candidate_id,
                node_id,
                finality_signer_public_key,
                stake,
                operator_owner,
                public_manifest_hash,
            },
        ),
        Action::ApproveGovernanceValidatorAdmission {
            controller_account_id,
            candidate_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ApproveGovernanceValidatorAdmission,
            payloads::ApproveGovernanceValidatorAdmissionPayload {
                controller_account_id,
                candidate_id,
            },
        ),
        Action::ActivateGovernanceValidatorAdmission {
            controller_account_id,
            candidate_id,
            activation_epoch,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ActivateGovernanceValidatorAdmission,
            payloads::ActivateGovernanceValidatorAdmissionPayload {
                controller_account_id,
                candidate_id,
                activation_epoch,
            },
        ),
        Action::RevokeGovernanceValidatorAdmission {
            controller_account_id,
            candidate_id,
            node_id,
            reason,
        } => serialize_action_envelope(
            serializer,
            ActionTag::RevokeGovernanceValidatorAdmission,
            payloads::RevokeGovernanceValidatorAdmissionPayload {
                controller_account_id,
                candidate_id,
                node_id,
                reason,
            },
        ),
        _ => Err(S::Error::custom(
            "Action dispatched to wrong governance serializer",
        )),
    }
}
