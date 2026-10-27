//! Public wire-3 certificates. Quorum verification does not execute private inputs.
use std::collections::BTreeSet;

use oasis7_proto::distributed_bft::{
    v2::{BftScopeV2, BftValidatorTransitionRefV2},
    v3::{BftBlockHeaderV3, BftHeaderCommitCertificateV3, BftProposalHeaderV3, BftVoteV3},
};
use serde::{Deserialize, Serialize};

use super::{
    BftProposerPriorityV2, BftSchedulePolicyV2, BftValidatorSetV2, BoundFormalBftHeaderV3,
    FormalBftExecutionCommitmentV1,
    formal_handoff::{canonical_bytes, decode_canonical, domain_digest},
    verified_commit::{BftCommitFields, BftHeightProposerScheduleV2},
};

pub const BFT_FORMAL_HEADER_CERTIFICATE_MAX_BYTES_V3: usize = 3 * 1024 * 1024;

/// Only the wire-3 qualified handoff and verified wire-3 successors construct
/// this capability. No public V2 schedule or engineering-genesis conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BftHeaderHeightScheduleV3 {
    inner: BftHeightProposerScheduleV2,
    activation_digest: String,
}

impl BftHeaderHeightScheduleV3 {
    pub(super) fn from_handoff(
        inner: BftHeightProposerScheduleV2,
        activation_digest: String,
    ) -> Self {
        Self {
            inner,
            activation_digest,
        }
    }

    pub fn scope(&self) -> &BftScopeV2 {
        self.inner.scope()
    }
    pub fn authority_epoch(&self) -> u64 {
        self.inner.authority_epoch()
    }
    pub fn policy(&self) -> &BftSchedulePolicyV2 {
        self.inner.policy()
    }
    pub fn policy_hash(&self) -> &str {
        self.inner.policy_hash()
    }
    pub fn active_set(&self) -> &BftValidatorSetV2 {
        self.inner.active_set()
    }
    pub fn height(&self) -> u64 {
        self.inner.height()
    }
    pub fn parent_commit_ref(&self) -> &str {
        self.inner.parent_commit_ref()
    }
    pub fn parent_tick(&self) -> u64 {
        self.inner.parent_tick()
    }
    pub fn parent_state_root(&self) -> &str {
        self.inner.parent_state_root()
    }
    pub fn parent_manifest_hash(&self) -> &str {
        self.inner.parent_manifest_hash()
    }
    pub fn starting_priority(&self) -> &BftProposerPriorityV2 {
        self.inner.starting_priority()
    }
    pub fn pending_transition(&self) -> Option<&BftValidatorTransitionRefV2> {
        self.inner.pending_transition()
    }
    pub fn last_activation_height(&self) -> u64 {
        self.inner.last_activation_height()
    }
    pub fn activation_digest(&self) -> &str {
        &self.activation_digest
    }
    pub fn expected_proposer_for_round(&self, round: u64) -> Result<String, String> {
        self.inner.expected_proposer_for_round(round)
    }

    pub fn digest(&self) -> Result<String, String> {
        Ok(domain_digest(
            "oasis7.bft.header_height_schedule.v3.digest",
            &canonical_bytes(
                self,
                BFT_FORMAL_HEADER_CERTIFICATE_MAX_BYTES_V3,
                "BFT header schedule",
            )?,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftCertifiedHeaderV3 {
    pub schema_version: u32,
    pub certificate: BftHeaderCommitCertificateV3,
    pub header: BftBlockHeaderV3,
    pub execution_commitment: FormalBftExecutionCommitmentV1,
}

impl FormalBftCertifiedHeaderV3 {
    pub fn new(
        certificate: BftHeaderCommitCertificateV3,
        header: BftBlockHeaderV3,
        execution_commitment: FormalBftExecutionCommitmentV1,
    ) -> Result<Self, String> {
        let value = Self {
            schema_version: 1,
            certificate,
            header,
            execution_commitment,
        };
        value.validate_binding()?;
        Ok(value)
    }

    pub fn validate_binding(&self) -> Result<BoundFormalBftHeaderV3, String> {
        if self.schema_version != 1 {
            return Err("unsupported formal BFT header certificate schema".into());
        }
        self.certificate.validate_wire_bounds()?;
        let bound =
            BoundFormalBftHeaderV3::bind(self.header.clone(), self.execution_commitment.clone())?;
        let proposal = &self.certificate.proposal;
        let commitment = bound.commitment();
        if self.header.block_id()? != proposal.block_id
            || commitment.scope != proposal.scope
            || commitment.height != proposal.height
            || commitment.active_validator_set_hash != proposal.active_validator_set_hash
            || commitment.schedule_policy_hash != proposal.schedule_policy_hash
        {
            return Err("formal BFT header certificate proposal binding mismatch".into());
        }
        Ok(bound)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_binding()?;
        canonical_bytes(
            self,
            BFT_FORMAL_HEADER_CERTIFICATE_MAX_BYTES_V3,
            "formal BFT header certificate",
        )
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
        let value: Self = decode_canonical(
            bytes,
            BFT_FORMAL_HEADER_CERTIFICATE_MAX_BYTES_V3,
            "formal BFT header certificate",
        )?;
        value.validate_binding()?;
        Ok(value)
    }
}

/// Authenticates the public commitment against the admitted schedule and QC.
/// It does not attest to World execution, durability, current permissions or freshness.
#[derive(Debug, Clone)]
pub struct VerifiedBftHeaderCertificateV3 {
    entry: FormalBftCertifiedHeaderV3,
    bound: BoundFormalBftHeaderV3,
    schedule: BftHeaderHeightScheduleV3,
    next_height_schedule: BftHeaderHeightScheduleV3,
    block_id: String,
}

impl VerifiedBftHeaderCertificateV3 {
    pub fn certified_header(&self) -> &FormalBftCertifiedHeaderV3 {
        &self.entry
    }
    pub fn certificate(&self) -> &BftHeaderCommitCertificateV3 {
        &self.entry.certificate
    }
    pub fn bound_header(&self) -> &BoundFormalBftHeaderV3 {
        &self.bound
    }
    pub fn block_id(&self) -> &str {
        &self.block_id
    }
    pub fn schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.schedule
    }
    pub fn next_height_schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.next_height_schedule
    }
}

pub fn verify_formal_bft_header_certificate_v3(
    schedule: &BftHeaderHeightScheduleV3,
    entry: &FormalBftCertifiedHeaderV3,
) -> Result<VerifiedBftHeaderCertificateV3, String> {
    let bound = entry.validate_binding()?;
    let commitment = bound.commitment();
    if commitment.activation_digest != schedule.activation_digest
        || commitment.parent_tick != schedule.parent_tick()
        || commitment.parent_semantic_state_root != schedule.parent_state_root()
    {
        return Err("BFT header certificate does not extend its admitted parent".into());
    }
    schedule
        .inner
        .validate_commit_fields(commit_fields(commitment))?;
    let certificate = &entry.certificate;
    let set = schedule.active_set();
    verify_proposal(schedule, &certificate.proposal)?;
    if let Some(evidence) = &certificate.valid_round_evidence {
        verify_proposal(schedule, &evidence.proposal)?;
        verify_vote_quorum(set, &evidence.prevotes)?;
    }
    verify_vote_quorum(set, &certificate.precommits)?;
    let block_id = entry.header.block_id()?;
    let next_inner = schedule
        .inner
        .successor_after_fields(commit_fields(commitment), block_id.clone())?;
    let next_height_schedule = BftHeaderHeightScheduleV3 {
        inner: next_inner,
        activation_digest: schedule.activation_digest.clone(),
    };
    Ok(VerifiedBftHeaderCertificateV3 {
        entry: entry.clone(),
        bound,
        schedule: schedule.clone(),
        next_height_schedule,
        block_id,
    })
}

fn verify_proposal(
    schedule: &BftHeaderHeightScheduleV3,
    proposal: &BftProposalHeaderV3,
) -> Result<(), String> {
    if proposal.scope != *schedule.scope()
        || proposal.height != schedule.height()
        || proposal.active_validator_set_hash != schedule.active_set().set_hash()
        || proposal.schedule_policy_hash != schedule.policy_hash()
        || proposal.proposer_id != schedule.expected_proposer_for_round(proposal.round)?
    {
        return Err("BFT V3 proposal does not match its deterministic schedule".into());
    }
    member_power(
        schedule.active_set(),
        &proposal.proposer_id,
        &proposal.public_key_hex,
    )?;
    proposal.verify_signature()
}

// Exact height/round/phase/value/scope/set/policy binding is checked by the
// certificate's bounded shape validator before any signature contributes power.
fn verify_vote_quorum(set: &BftValidatorSetV2, votes: &[BftVoteV3]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    let mut power = 0u64;
    for vote in votes {
        if !seen.insert(vote.validator_id.as_str()) {
            return Err("BFT V3 evidence repeats a validator".into());
        }
        let voting_power = member_power(set, &vote.validator_id, &vote.public_key_hex)?;
        vote.verify_signature()?;
        power = power
            .checked_add(voting_power)
            .ok_or_else(|| "BFT V3 evidence voting power overflow".to_string())?;
    }
    if power < set.quorum_voting_power() {
        return Err("BFT V3 evidence lacks quorum voting power".into());
    }
    Ok(())
}

fn member_power(set: &BftValidatorSetV2, id: &str, key: &str) -> Result<u64, String> {
    let validator = set
        .validator(id)
        .ok_or_else(|| "BFT V3 signer is not in the active set".to_string())?;
    if validator.public_key_hex != key {
        return Err("BFT V3 signer key does not match the active set".into());
    }
    Ok(validator.voting_power)
}

fn commit_fields(commitment: &FormalBftExecutionCommitmentV1) -> BftCommitFields<'_> {
    BftCommitFields {
        scope: &commitment.scope,
        height: commitment.height,
        parent_commit_ref: &commitment.parent_commit_ref,
        authority_epoch: commitment.authority_epoch,
        active_validator_set_hash: &commitment.active_validator_set_hash,
        schedule_policy_hash: &commitment.schedule_policy_hash,
        manifest_hash: &commitment.manifest_hash,
        post_manifest_hash: &commitment.post_manifest_hash,
        state_root: &commitment.state_root,
        transition: commitment.transition.as_ref(),
        transition_ref: commitment.transition_ref.as_ref(),
    }
}
