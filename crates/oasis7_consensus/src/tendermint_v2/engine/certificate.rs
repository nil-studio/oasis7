use std::collections::BTreeSet;

use oasis7_proto::distributed_bft::v2::{BftCommitCertificateV2, BftVotePhaseV2, BftVoteValueV2};

use super::super::BFT_MAX_ROUNDS_PER_HEIGHT_V2;
use super::super::verified_commit::BftHeightProposerScheduleV2;

/// Validate a complete V2 commit certificate against the exact height schedule.
/// This checks protocol/QC lineage only; it does not validate Runtime execution.
pub fn verify_commit_certificate_v2(
    schedule: &BftHeightProposerScheduleV2,
    certificate: &BftCommitCertificateV2,
) -> Result<(), String> {
    certificate.validate_wire_bounds()?;
    let set = schedule.active_set();
    let proposal = certificate.proposal.as_ref();
    proposal.validate_binding()?;
    let header = &proposal.header;
    if header.round > BFT_MAX_ROUNDS_PER_HEIGHT_V2
        || schedule.expected_proposer_for_round(header.round)? != header.proposer_id
    {
        return Err("BFT V2 commit proposal has the wrong deterministic proposer".into());
    }
    set.validate_proposal_header(header)?;
    if proposal.block.scope != *set.scope()
        || proposal.block.active_validator_set_hash != set.set_hash()
        || proposal.block.schedule_policy_hash != schedule.policy_hash()
    {
        return Err("BFT V2 proposal is outside its scheduled scope or policy".into());
    }

    if let Some(valid_round) = header.valid_round {
        if valid_round > BFT_MAX_ROUNDS_PER_HEIGHT_V2
            || schedule.expected_proposer_for_round(valid_round)?
                != proposal
                    .valid_round_evidence
                    .as_ref()
                    .ok_or_else(|| "BFT V2 valid-round evidence is missing".to_string())?
                    .proposal
                    .proposer_id
        {
            return Err("BFT V2 valid-round proposal has the wrong proposer".into());
        }
        let evidence = proposal
            .valid_round_evidence
            .as_ref()
            .ok_or_else(|| "BFT V2 valid-round evidence is missing".to_string())?;
        if evidence.proposal.height != header.height
            || evidence.proposal.round != valid_round
            || evidence.proposal.block_id != header.block_id
        {
            return Err("BFT V2 valid-round evidence does not bind the proposal".into());
        }
        set.validate_proposal_header(&evidence.proposal)?;
        let mut seen = BTreeSet::new();
        let mut power = 0u64;
        let expected_value = BftVoteValueV2::Block {
            block_id: header.block_id.clone(),
        };
        for vote in &evidence.prevotes {
            if vote.height != header.height
                || vote.round != valid_round
                || vote.phase != BftVotePhaseV2::Prevote
                || vote.value != expected_value
            {
                return Err("BFT V2 valid-round prevote does not match block".into());
            }
            if !seen.insert(vote.validator_id.as_str()) {
                return Err("BFT V2 valid-round evidence repeats a validator".into());
            }
            power = power
                .checked_add(set.validate_vote(vote)?)
                .ok_or_else(|| "BFT V2 valid-round voting power overflow".to_string())?;
        }
        if power < set.quorum_voting_power() {
            return Err("BFT V2 valid-round prevotes lack quorum".into());
        }
    }

    let mut seen = BTreeSet::new();
    let mut power = 0u64;
    let expected_value = BftVoteValueV2::Block {
        block_id: header.block_id.clone(),
    };
    for vote in &certificate.precommits {
        if vote.height != header.height
            || vote.round != header.round
            || vote.phase != BftVotePhaseV2::Precommit
            || vote.value != expected_value
        {
            return Err("BFT V2 commit certificate contains a mismatched precommit".into());
        }
        if !seen.insert(vote.validator_id.as_str()) {
            return Err("BFT V2 commit certificate repeats a validator".into());
        }
        power = power
            .checked_add(set.validate_vote(vote)?)
            .ok_or_else(|| "BFT V2 precommit voting power overflow".to_string())?;
    }
    if power < set.quorum_voting_power() {
        return Err("BFT V2 commit certificate lacks quorum precommits".into());
    }
    Ok(())
}
