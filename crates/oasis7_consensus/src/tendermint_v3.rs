//! Qualified-H header-only protocol state and protected signing decisions.
//! Application execution and private candidate retention belong to the Node.
mod engine;
mod peer_message;
mod public_proposal;
mod signing_journal;

pub use engine::{
    BFT_MAX_CACHED_PROPOSAL_BYTES_V3, BFT_MAX_CACHED_VOTES_V3, BFT_MAX_EQUIVOCATIONS_V3,
    BFT_MAX_PROPOSAL_EQUIVOCATIONS_V3, BftConsensusActionV3, BftConsensusMachineV3,
    BftProposalIntentV3, BftStepV3, BftVoteIntentV3,
};
pub use peer_message::*;
pub use public_proposal::{BftHeaderValueV3, BftPublicProposalV3};
pub type BftProposalV3 = BftPublicProposalV3;
pub use oasis7_formal_protocol::bft::{
    BFT_MAX_ROUNDS_PER_HEIGHT_V2 as BFT_MAX_ROUNDS_PER_HEIGHT_V3, BftHeaderHeightScheduleV3,
    BftProposerPriorityV2, BftValidatorSetV2, FormalBftCertifiedHeaderV3,
    FormalBftExecutionCommitmentV1, VerifiedBftHeaderCertificateV3,
    verify_formal_bft_header_certificate_v3 as verify_commit_certificate_token_v3,
};
pub use signing_journal::{
    BFT_MAX_SIGNING_JOURNAL_BYTES_V3, BFT_MAX_SIGNING_JOURNAL_EVENTS_V3, BftSigningJournalEventV3,
    BftSigningJournalV3, complete_reserved_bft_proposal_v3, complete_reserved_bft_vote_v3,
    replay_bft_signing_journal_v3, replay_bft_signing_journal_with_effects_v3,
    verify_bft_signing_journal_transition_v3, verify_reserved_bft_commit_transition_v3,
};

fn verify_commit_certificate_v3(
    schedule: &BftHeaderHeightScheduleV3,
    certificate: &FormalBftCertifiedHeaderV3,
) -> Result<(), String> {
    verify_commit_certificate_token_v3(schedule, certificate).map(|_| ())
}

fn validate_proposal_header_v3(
    set: &BftValidatorSetV2,
    header: &oasis7_proto::distributed_bft::v3::BftProposalHeaderV3,
) -> Result<u64, String> {
    if header.scope != *set.scope() || header.active_validator_set_hash != set.set_hash() {
        return Err("BFT V3 proposal differs from its active set".into());
    }
    let validator = set
        .validator(&header.proposer_id)
        .ok_or("unknown BFT V3 proposer")?;
    if validator.public_key_hex != header.public_key_hex {
        return Err("BFT V3 proposer key differs from active set".into());
    }
    header.verify_signature()?;
    Ok(validator.voting_power)
}

fn validate_vote_v3(
    set: &BftValidatorSetV2,
    vote: &oasis7_proto::distributed_bft::v3::BftVoteV3,
) -> Result<u64, String> {
    if vote.scope != *set.scope() || vote.active_validator_set_hash != set.set_hash() {
        return Err("BFT V3 vote differs from its active set".into());
    }
    let validator = set
        .validator(&vote.validator_id)
        .ok_or("unknown BFT V3 voter")?;
    if validator.public_key_hex != vote.public_key_hex {
        return Err("BFT V3 voter key differs from active set".into());
    }
    vote.verify_signature()?;
    Ok(validator.voting_power)
}

#[cfg(test)]
mod tests;
