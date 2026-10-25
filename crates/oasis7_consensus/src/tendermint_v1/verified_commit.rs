use std::collections::BTreeMap;

use oasis7_proto::distributed_bft::{BftBlockV1, BftCommitCertificateV1, BftScopeV1};

use super::engine::{BFT_MAX_ROUNDS_PER_HEIGHT_V1, verify_commit_certificate_v1};
use super::proposer::BftProposerPriorityV1;
use super::validator_set::BftValidatorSetV1;

/// A height-scoped proposer schedule. The fields are private so a later
/// height can only be reached through the next-height capability returned by
/// a verified commit certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftHeightProposerScheduleV1 {
    protocol_version: u16,
    scope: BftScopeV1,
    authority_epoch: u64,
    validator_set_hash: String,
    height: u64,
    parent_commit_ref: String,
    starting_priority: BftProposerPriorityV1,
}

impl BftHeightProposerScheduleV1 {
    /// Create the only initial schedule: height one, anchored to the configured
    /// genesis hash and the canonical initial proposer priority.
    pub fn genesis(set: &BftValidatorSetV1, authority_epoch: u64) -> Result<Self, String> {
        Ok(Self {
            protocol_version: oasis7_proto::distributed_bft::BFT_PROTOCOL_VERSION_V1,
            scope: set.scope().clone(),
            authority_epoch,
            validator_set_hash: set.set_hash().to_string(),
            height: 1,
            parent_commit_ref: set.scope().genesis_hash.clone(),
            starting_priority: BftProposerPriorityV1::new(set)?,
        })
    }

    pub fn scope(&self) -> &BftScopeV1 {
        &self.scope
    }

    pub fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }

    pub fn validator_set_hash(&self) -> &str {
        &self.validator_set_hash
    }

    pub fn height(&self) -> u64 {
        self.height
    }

    pub fn parent_commit_ref(&self) -> &str {
        &self.parent_commit_ref
    }

    pub fn starting_priority(&self) -> &BftProposerPriorityV1 {
        &self.starting_priority
    }

    fn validate_for_set(&self, set: &BftValidatorSetV1) -> Result<(), String> {
        if self.protocol_version != oasis7_proto::distributed_bft::BFT_PROTOCOL_VERSION_V1
            || &self.scope != set.scope()
            || self.validator_set_hash != set.set_hash()
            || self.height == 0
        {
            return Err("BFT proposer schedule scope, version, or set mismatch".into());
        }
        if self.starting_priority.priorities().len() != set.validators().len()
            || self
                .starting_priority
                .priorities()
                .keys()
                .any(|validator_id| !set.validators().contains_key(validator_id))
        {
            return Err("BFT proposer schedule priority does not match its set".into());
        }
        Ok(())
    }

    fn proposers_through(
        &self,
        set: &BftValidatorSetV1,
        round: u64,
    ) -> Result<(BTreeMap<u64, String>, BftProposerPriorityV1), String> {
        if round > BFT_MAX_ROUNDS_PER_HEIGHT_V1 {
            return Err(
                "BFT certificate exceeds per-height round capacity; preserve schedule state".into(),
            );
        }
        let mut priority = self.starting_priority.clone();
        let mut proposers = BTreeMap::new();
        for current_round in 0..=round {
            let proposer = priority.next_proposer(set)?;
            proposers.insert(current_round, proposer.validator_id);
        }
        Ok((proposers, priority))
    }
}

/// In-process capability proving that a certificate was validated against a
/// fixed set and the schedule derived from the complete preceding certificate
/// chain. The token is intentionally neither serializable nor constructible
/// from caller-supplied proposer maps.
#[derive(Debug, Clone)]
pub struct VerifiedBftCommitCertificateV1 {
    certificate: BftCommitCertificateV1,
    validator_set: BftValidatorSetV1,
    schedule: BftHeightProposerScheduleV1,
    next_height_schedule: BftHeightProposerScheduleV1,
    block_id: String,
}

impl VerifiedBftCommitCertificateV1 {
    pub fn certificate(&self) -> &BftCommitCertificateV1 {
        &self.certificate
    }

    pub fn validator_set(&self) -> &BftValidatorSetV1 {
        &self.validator_set
    }

    pub fn schedule(&self) -> &BftHeightProposerScheduleV1 {
        &self.schedule
    }

    pub fn next_height_schedule(&self) -> &BftHeightProposerScheduleV1 {
        &self.next_height_schedule
    }

    pub fn block(&self) -> &BftBlockV1 {
        &self.certificate.proposal.block
    }

    pub fn block_id(&self) -> &str {
        &self.block_id
    }
}

/// Verify a commit and return an opaque capability for an execution hook.
/// The only supported input for height > 1 is the schedule returned by the
/// immediately preceding verified certificate; cold recovery must therefore
/// replay a contiguous certificate chain from the configured genesis.
pub fn verify_commit_certificate_token_v1(
    set: &BftValidatorSetV1,
    schedule: &BftHeightProposerScheduleV1,
    certificate: &BftCommitCertificateV1,
) -> Result<VerifiedBftCommitCertificateV1, String> {
    schedule.validate_for_set(set)?;
    certificate.validate_wire_bounds()?;
    let block = &certificate.proposal.block;
    if block.height != schedule.height
        || block.parent_commit_ref != schedule.parent_commit_ref
        || block.authority_epoch != schedule.authority_epoch
        || block.scope != schedule.scope
        || certificate.proposal.header.validator_set_hash != schedule.validator_set_hash
    {
        return Err("BFT commit does not extend its exact height schedule".into());
    }

    let round = certificate.proposal.header.round;
    let (expected_proposers, priority_after_commit_round) =
        schedule.proposers_through(set, round)?;
    verify_commit_certificate_v1(set, &expected_proposers, certificate)?;
    let block_id = block.block_id()?;
    let next_height = schedule
        .height
        .checked_add(1)
        .ok_or_else(|| "BFT height overflow; preserve schedule state".to_string())?;
    let next_height_schedule = BftHeightProposerScheduleV1 {
        protocol_version: schedule.protocol_version,
        scope: schedule.scope.clone(),
        authority_epoch: schedule.authority_epoch,
        validator_set_hash: schedule.validator_set_hash.clone(),
        height: next_height,
        parent_commit_ref: block_id.clone(),
        starting_priority: priority_after_commit_round,
    };
    Ok(VerifiedBftCommitCertificateV1 {
        certificate: certificate.clone(),
        validator_set: set.clone(),
        schedule: schedule.clone(),
        next_height_schedule,
        block_id,
    })
}

#[cfg(test)]
#[path = "verified_commit_tests.rs"]
mod tests;
