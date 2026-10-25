use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::{
    BftCommitCertificateV1, BftProposalHeaderV1, BftProposalV1, BftValidRoundEvidenceV1,
    BftVotePhaseV1, BftVoteV1, BftVoteValueV1,
};
use serde::{Deserialize, Serialize};

use super::vote_tally::VoteTally;
use super::{BFT_MAX_VALIDATORS_V1, BftProposerPriorityV1, BftValidatorSetV1};

mod proposal;
pub use proposal::BftProposalIntentV1;

pub const BFT_MAX_TRACKED_ROUNDS_V1: u64 = 64;
pub const BFT_MAX_ROUNDS_PER_HEIGHT_V1: u64 = 4096;
pub const BFT_MAX_EQUIVOCATIONS_V1: usize = BFT_MAX_VALIDATORS_V1;
pub const BFT_MAX_CACHED_VOTES_V1: usize =
    4 * BFT_MAX_VALIDATORS_V1 * (BFT_MAX_TRACKED_ROUNDS_V1 as usize + 1);
pub const BFT_MAX_CACHED_PROPOSAL_BYTES_V1: usize = 16 * 1024 * 1024;
pub const BFT_MAX_PROPOSAL_EQUIVOCATIONS_V1: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BftStepV1 {
    Propose,
    Prevote,
    Precommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BftTimeoutsV1 {
    pub propose_ms: u64,
    pub prevote_ms: u64,
    pub precommit_ms: u64,
    pub round_delta_ms: u64,
}

impl BftTimeoutsV1 {
    pub fn validate(self) -> Result<Self, String> {
        if self.propose_ms == 0
            || self.prevote_ms == 0
            || self.precommit_ms == 0
            || self.round_delta_ms == 0
        {
            return Err("BFT phase timeouts must be positive".into());
        }
        for timeout in [
            self.propose_ms,
            self.prevote_ms,
            self.precommit_ms,
            self.round_delta_ms,
        ] {
            i64::try_from(timeout).map_err(|_| "BFT phase timeout exceeds signed range")?;
        }
        Ok(self)
    }

    fn for_step(self, step: BftStepV1, round: u64) -> Result<u64, String> {
        let base = match step {
            BftStepV1::Propose => self.propose_ms,
            BftStepV1::Prevote => self.prevote_ms,
            BftStepV1::Precommit => self.precommit_ms,
        };
        let growth = self
            .round_delta_ms
            .checked_mul(round)
            .ok_or_else(|| "BFT round timeout growth overflow".to_string())?;
        base.checked_add(growth)
            .ok_or_else(|| "BFT round timeout overflow".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftVoteIntentV1 {
    pub height: u64,
    pub round: u64,
    pub phase: BftVotePhaseV1,
    pub value: BftVoteValueV1,
    pub validator_id: String,
}

impl BftVoteIntentV1 {
    pub fn sign(&self, set: &BftValidatorSetV1, key: &SigningKey) -> Result<BftVoteV1, String> {
        let validator = set
            .validator(self.validator_id.as_str())
            .ok_or_else(|| "local BFT signer is not in the active set".to_string())?;
        if validator.public_key_hex != hex::encode(key.verifying_key().to_bytes()) {
            return Err("local BFT signer key does not match validator set".into());
        }
        BftVoteV1::sign(
            set.scope().clone(),
            self.height,
            self.round,
            self.phase,
            set.set_hash().to_string(),
            self.value.clone(),
            self.validator_id.clone(),
            key,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BftConsensusActionV1 {
    SignVote(BftVoteIntentV1),
    Commit(BftCommitCertificateV1),
    RoundAdvanced { height: u64, round: u64 },
}

/// Deterministic Alg. 1 state for one fixed-set height. Signature decisions are
/// deliberately requested from the caller so it can durably reserve the exact
/// slot before exposing the signed vote.
#[derive(Debug, Clone)]
pub struct BftConsensusMachineV1 {
    set: BftValidatorSetV1,
    local_validator_id: String,
    timeouts: BftTimeoutsV1,
    height: u64,
    round: u64,
    step: BftStepV1,
    locked_value: Option<String>,
    locked_round: Option<u64>,
    valid_value: Option<String>,
    valid_round: Option<u64>,
    valid_round_proof: Option<BftValidRoundEvidenceV1>,
    height_start_priority: BftProposerPriorityV1,
    proposer_priority: BftProposerPriorityV1,
    round_proposers: BTreeMap<u64, String>,
    proposals: BTreeMap<u64, BftProposalV1>,
    votes: BTreeMap<(u64, BftVotePhaseV1), VoteTally>,
    future_round_voters: BTreeMap<u64, BTreeSet<String>>,
    local_votes: BTreeMap<(u64, BftVotePhaseV1), BftVoteV1>,
    prevote_quorum_timeout_armed: bool,
    precommit_quorum_timeout_armed: bool,
    commit_emitted: BTreeSet<(u64, String)>,
    proposal_equivocations: BTreeMap<(u64, String), (BftProposalHeaderV1, BftProposalHeaderV1)>,
    step_deadline_ms: Option<i64>,
}

impl BftConsensusMachineV1 {
    pub fn new(
        set: BftValidatorSetV1,
        local_validator_id: impl Into<String>,
        height: u64,
        proposer_priority: Option<BftProposerPriorityV1>,
        now_ms: i64,
        timeouts: BftTimeoutsV1,
    ) -> Result<Self, String> {
        if height == 0 {
            return Err("BFT machine height must be positive".into());
        }
        let local_validator_id = local_validator_id.into();
        if set.validator(&local_validator_id).is_none() {
            return Err("local BFT validator is not in the active set".into());
        }
        let timeouts = timeouts.validate()?;
        let mut proposer_priority = match proposer_priority {
            Some(priority) => priority,
            None => BftProposerPriorityV1::new(&set)?,
        };
        let height_start_priority = proposer_priority.clone();
        let proposer = proposer_priority.next_proposer(&set)?;
        let timeout = i64::try_from(timeouts.for_step(BftStepV1::Propose, 0)?)
            .map_err(|_| "BFT propose timeout exceeds signed clock range".to_string())?;
        let step_deadline_ms = now_ms
            .checked_add(timeout)
            .ok_or_else(|| "BFT propose deadline overflow".to_string())?;
        Ok(Self {
            set,
            local_validator_id,
            timeouts,
            height,
            round: 0,
            step: BftStepV1::Propose,
            locked_value: None,
            locked_round: None,
            valid_value: None,
            valid_round: None,
            valid_round_proof: None,
            height_start_priority,
            proposer_priority,
            round_proposers: BTreeMap::from([(0, proposer.validator_id)]),
            proposals: BTreeMap::new(),
            votes: BTreeMap::new(),
            future_round_voters: BTreeMap::new(),
            local_votes: BTreeMap::new(),
            prevote_quorum_timeout_armed: false,
            precommit_quorum_timeout_armed: false,
            proposal_equivocations: BTreeMap::new(),
            step_deadline_ms: Some(step_deadline_ms),
            commit_emitted: BTreeSet::new(),
        })
    }

    pub fn set(&self) -> &BftValidatorSetV1 {
        &self.set
    }

    pub fn height(&self) -> u64 {
        self.height
    }

    pub fn round(&self) -> u64 {
        self.round
    }

    pub fn step(&self) -> BftStepV1 {
        self.step
    }

    pub fn locked(&self) -> Option<(&str, u64)> {
        Some((self.locked_value.as_deref()?, self.locked_round?))
    }

    pub fn valid(&self) -> Option<(&str, u64)> {
        Some((self.valid_value.as_deref()?, self.valid_round?))
    }

    pub fn current_proposer_id(&self) -> Option<&str> {
        self.round_proposers.get(&self.round).map(String::as_str)
    }

    pub fn proposer_for_round(&self, round: u64) -> Option<&str> {
        self.round_proposers.get(&round).map(String::as_str)
    }

    pub fn proposal_for_round(&self, round: u64) -> Option<&BftProposalV1> {
        self.proposals.get(&round)
    }

    pub fn round_proposers(&self) -> &BTreeMap<u64, String> {
        &self.round_proposers
    }

    pub fn equivocation_evidence(&self) -> Vec<(&BftVoteV1, &BftVoteV1)> {
        self.votes
            .values()
            .flat_map(|tally| tally.evidence().map(|(first, second)| (first, second)))
            .collect()
    }

    pub fn proposal_equivocation_evidence(
        &self,
    ) -> impl Iterator<Item = (&BftProposalHeaderV1, &BftProposalHeaderV1)> {
        self.proposal_equivocations
            .values()
            .map(|(first, second)| (first, second))
    }

    pub fn step_deadline_ms(&self) -> Option<i64> {
        self.step_deadline_ms
    }

    pub fn vote_intent(
        &self,
        validator_id: impl Into<String>,
        phase: BftVotePhaseV1,
        value: BftVoteValueV1,
        round: u64,
    ) -> Result<Option<BftVoteIntentV1>, String> {
        let validator_id = validator_id.into();
        if validator_id != self.local_validator_id {
            return Err("BFT vote intent validator is not the local signer".into());
        }
        if self.set.validator(&validator_id).is_none() {
            return Err("local BFT validator is not in the active set".into());
        }
        if round != self.round {
            return Err("BFT local signing intent must target the current round".into());
        }
        let slot = (round, phase);
        if let Some(existing) = self.local_votes.get(&slot) {
            if existing.value != value {
                return Err("BFT local signing slot already contains a different value".into());
            }
            return Ok(None);
        }
        Ok(Some(BftVoteIntentV1 {
            height: self.height,
            round,
            phase,
            value,
            validator_id,
        }))
    }

    /// Record the exact vote only after the caller has persisted its signing
    /// decision and signature. Conflicting local bytes at a slot are fatal.
    pub fn record_local_vote(
        &mut self,
        vote: BftVoteV1,
        now_ms: i64,
    ) -> Result<Vec<BftConsensusActionV1>, String> {
        if vote.validator_id != self.local_validator_id || vote.round != self.round {
            return Err("local BFT vote identity or round is not current".into());
        }
        self.validate_vote_position(&vote)?;
        self.validate_local_vote_rule(&vote)?;
        let slot = (vote.round, vote.phase);
        if let Some(previous) = self.local_votes.get(&slot) {
            if previous != &vote {
                return Err("BFT local signing slot conflicts with persisted vote".into());
            }
        } else {
            self.local_votes.insert(slot, vote.clone());
        }
        self.receive_vote_inner(vote, false, now_ms)
    }

    fn validate_local_vote_rule(&self, vote: &BftVoteV1) -> Result<(), String> {
        match (&vote.phase, &vote.value) {
            (BftVotePhaseV1::Prevote, BftVoteValueV1::Block { block_id }) => {
                let proposal = self
                    .proposal_by_id(vote.round, block_id)
                    .ok_or_else(|| "local BFT prevote has no validated proposal".to_string())?;
                let safe = self.locked_value.is_none()
                    || self.locked_value.as_deref() == Some(block_id.as_str())
                    || proposal
                        .header
                        .valid_round
                        .zip(self.locked_round)
                        .is_some_and(|(valid_round, locked_round)| valid_round >= locked_round);
                if !safe {
                    return Err("local BFT prevote violates the lock/valid-round rule".into());
                }
            }
            (BftVotePhaseV1::Precommit, BftVoteValueV1::Block { block_id })
                if self.locked_value.as_deref() != Some(block_id.as_str())
                    || self.locked_round != Some(vote.round)
                    || self.proposal_by_id(vote.round, block_id).is_none() =>
            {
                return Err("local BFT precommit lacks the current-round lock".into());
            }
            _ => {}
        }
        Ok(())
    }

    pub fn receive_vote(
        &mut self,
        vote: BftVoteV1,
        now_ms: i64,
    ) -> Result<Vec<BftConsensusActionV1>, String> {
        self.receive_vote_inner(vote, true, now_ms)
    }

    fn receive_vote_inner(
        &mut self,
        vote: BftVoteV1,
        allow_future_advance: bool,
        now_ms: i64,
    ) -> Result<Vec<BftConsensusActionV1>, String> {
        self.validate_vote_position(&vote)?;
        let slot = (vote.round, vote.phase);
        let cached = self.cached_vote_count()?;
        let growth = self
            .votes
            .get(&slot)
            .map_or(1, |tally| tally.growth_for(&vote));
        if cached
            .checked_add(growth)
            .is_none_or(|count| count > BFT_MAX_CACHED_VOTES_V1)
        {
            return Err("BFT cached vote capacity exceeded".into());
        }
        let mut output = Vec::new();
        if vote.round > self.round && allow_future_advance {
            output.extend(self.observe_future_round_message(
                vote.round,
                vote.validator_id.as_str(),
                now_ms,
            )?);
        }
        let tally = self.votes.entry(slot).or_insert_with(VoteTally::new);
        let inserted = tally.insert(vote.clone())?;
        if !inserted {
            return Ok(output);
        }
        output.extend(self.on_tally_changed(vote.round, vote.phase, now_ms)?);
        Ok(output)
    }

    fn cached_vote_count(&self) -> Result<usize, String> {
        self.votes.values().try_fold(0usize, |total, tally| {
            total
                .checked_add(tally.message_count())
                .ok_or_else(|| "BFT cached vote count overflow".to_string())
        })
    }

    fn cached_proposal_bytes(&self) -> Result<usize, String> {
        self.proposals.values().try_fold(0usize, |total, proposal| {
            total
                .checked_add(proposal.block.actions_cbor.len())
                .ok_or_else(|| "BFT cached proposal byte count overflow".to_string())
        })
    }

    fn observe_future_round_message(
        &mut self,
        round: u64,
        validator_id: &str,
        now_ms: i64,
    ) -> Result<Vec<BftConsensusActionV1>, String> {
        if round <= self.round {
            return Ok(Vec::new());
        }
        if round > BFT_MAX_ROUNDS_PER_HEIGHT_V1 {
            return Err(
                "BFT per-height round capacity exceeded; stop and preserve signer state".into(),
            );
        }
        if round.saturating_sub(self.round) > BFT_MAX_TRACKED_ROUNDS_V1 {
            return Err("BFT future message round exceeds bounded window".into());
        }
        let voters = self.future_round_voters.entry(round).or_default();
        voters.insert(validator_id.to_string());
        let power =
            voters.iter().try_fold(0u64, |sum, id| {
                sum.checked_add(self.set.voting_power(id).ok_or_else(|| {
                    "BFT future-round set contains an unknown validator".to_string()
                })?)
                .ok_or_else(|| "BFT future-round voting power overflow".to_string())
            })?;
        if power >= self.set.future_round_voting_power() {
            self.advance_to_round(round, now_ms)
        } else {
            Ok(Vec::new())
        }
    }

    fn expected_proposer_for_round(&self, round: u64) -> Result<String, String> {
        if round > BFT_MAX_ROUNDS_PER_HEIGHT_V1 {
            return Err("BFT per-height round capacity exceeded".into());
        }
        if let Some(proposer) = self.round_proposers.get(&round) {
            return Ok(proposer.clone());
        }
        if round < self.round || round.saturating_sub(self.round) > BFT_MAX_TRACKED_ROUNDS_V1 {
            return Err("BFT proposer round is outside the retained window".into());
        }
        let mut priority = self.proposer_priority.clone();
        let mut selected = None;
        let mut next = self.round;
        while next < round {
            next = next
                .checked_add(1)
                .ok_or_else(|| "BFT proposer round overflow".to_string())?;
            selected = Some(priority.next_proposer(&self.set)?.validator_id);
        }
        selected.ok_or_else(|| "BFT proposer round has no selected proposer".into())
    }

    pub fn receive_proposal(
        &mut self,
        proposal: BftProposalV1,
        locally_validated: bool,
        now_ms: i64,
    ) -> Result<Vec<BftConsensusActionV1>, String> {
        proposal.validate_binding()?;
        self.set.validate_proposer(&proposal.header)?;
        if proposal.header.height != self.height {
            return Err("BFT proposal height is not current".into());
        }
        if proposal.header.round > self.round
            && proposal.header.round.saturating_sub(self.round) > BFT_MAX_TRACKED_ROUNDS_V1
        {
            return Err("BFT proposal round exceeds bounded window".into());
        }
        let expected_proposer = self.expected_proposer_for_round(proposal.header.round)?;
        if expected_proposer != proposal.header.proposer_id {
            return Err("BFT proposal does not come from deterministic proposer".into());
        }
        self.validate_valid_round_evidence(&proposal)?;
        if !locally_validated {
            return Err("BFT proposal candidate failed deterministic valid(v)".into());
        }

        let mut output = if proposal.header.round > self.round {
            self.observe_future_round_message(
                proposal.header.round,
                proposal.header.proposer_id.as_str(),
                now_ms,
            )?
        } else {
            Vec::new()
        };

        if let Some(previous) = self.proposals.get(&proposal.header.round) {
            if previous == &proposal {
                return Ok(output);
            }
            if self.proposal_equivocations.len() >= BFT_MAX_PROPOSAL_EQUIVOCATIONS_V1 {
                return Err("BFT proposal equivocation evidence capacity exceeded".into());
            }
            self.proposal_equivocations.insert(
                (proposal.header.round, proposal.header.proposer_id.clone()),
                (previous.header.clone(), proposal.header.clone()),
            );
            return Err("BFT proposer equivocated within height/round".into());
        }
        if self
            .cached_proposal_bytes()?
            .checked_add(proposal.block.actions_cbor.len())
            .is_none_or(|bytes| bytes > BFT_MAX_CACHED_PROPOSAL_BYTES_V1)
        {
            return Err("BFT cached proposal byte capacity exceeded".into());
        }
        let proposal_round = proposal.header.round;
        let proposal_id = proposal.header.block_id.clone();
        let valid_round = proposal.header.valid_round;
        let valid_round_proof = proposal.valid_round_evidence.clone();
        if valid_round.is_some_and(|round| round >= proposal_round) {
            return Err("BFT proposal valid round is not earlier than proposal round".into());
        }
        self.proposals.insert(proposal_round, proposal);
        if let Some(valid_round) = valid_round
            && self
                .valid_round
                .is_none_or(|known_round| valid_round > known_round)
        {
            self.valid_value = Some(proposal_id.clone());
            self.valid_round = Some(valid_round);
            self.valid_round_proof = valid_round_proof;
        }
        if proposal_round != self.round || self.step != BftStepV1::Propose {
            if proposal_round == self.round {
                output.extend(self.replay_round_tallies(proposal_round, now_ms)?);
            }
            return Ok(output);
        }
        let can_vote_for_value = self.locked_value.is_none()
            || self.locked_value.as_deref() == Some(proposal_id.as_str())
            || valid_round
                .zip(self.locked_round)
                .is_some_and(|(valid_round, locked_round)| locked_round <= valid_round);
        let value = if can_vote_for_value {
            BftVoteValueV1::Block {
                block_id: proposal_id,
            }
        } else {
            BftVoteValueV1::Nil
        };
        self.set_step(BftStepV1::Prevote, now_ms)?;
        output.extend(
            self.vote_intent(
                &self.local_validator_id,
                BftVotePhaseV1::Prevote,
                value,
                self.round,
            )?
            .map(BftConsensusActionV1::SignVote),
        );
        output.extend(self.replay_round_tallies(proposal_round, now_ms)?);
        Ok(output)
    }

    fn replay_round_tallies(
        &mut self,
        round: u64,
        now_ms: i64,
    ) -> Result<Vec<BftConsensusActionV1>, String> {
        let mut output = Vec::new();
        for phase in [BftVotePhaseV1::Prevote, BftVotePhaseV1::Precommit] {
            if self.votes.contains_key(&(round, phase)) {
                output.extend(self.on_tally_changed(round, phase, now_ms)?);
            }
        }
        Ok(output)
    }

    fn validate_valid_round_evidence(&self, proposal: &BftProposalV1) -> Result<(), String> {
        let (Some(valid_round), Some(evidence)) = (
            proposal.header.valid_round,
            proposal.valid_round_evidence.as_ref(),
        ) else {
            if proposal.header.valid_round.is_some() || proposal.valid_round_evidence.is_some() {
                return Err("BFT valid-round evidence is incomplete".into());
            }
            return Ok(());
        };
        if evidence.proposal.height != proposal.header.height
            || evidence.proposal.round != valid_round
            || evidence.proposal.block_id != proposal.header.block_id
        {
            return Err("BFT valid-round evidence does not prove this proposal value".into());
        }
        let expected_proposer = self.expected_proposer_for_round(valid_round)?;
        if expected_proposer != evidence.proposal.proposer_id {
            return Err("BFT valid-round proposal has wrong deterministic proposer".into());
        }
        self.set.validate_proposer(&evidence.proposal)?;
        let mut tally = VoteTally::new();
        let mut seen_signers = BTreeSet::new();
        for vote in evidence.prevotes.iter() {
            if vote.height != proposal.header.height
                || vote.round != valid_round
                || vote.phase != BftVotePhaseV1::Prevote
                || vote.value
                    != (BftVoteValueV1::Block {
                        block_id: proposal.header.block_id.clone(),
                    })
            {
                return Err("BFT valid-round evidence contains a mismatched prevote".into());
            }
            self.set.validate_vote(vote)?;
            if !seen_signers.insert(vote.validator_id.as_str()) {
                return Err("BFT valid-round evidence repeats a validator".into());
            }
            tally.insert(vote.clone())?;
        }
        let power = tally.power_for(
            &self.set,
            &BftVoteValueV1::Block {
                block_id: proposal.header.block_id.clone(),
            },
        )?;
        if power < self.set.quorum_voting_power() {
            return Err("BFT valid-round evidence lacks quorum prevotes".into());
        }
        Ok(())
    }

    fn validate_vote_position(&self, vote: &BftVoteV1) -> Result<u64, String> {
        if vote.height != self.height {
            return Err("BFT vote height is not current".into());
        }
        self.set.validate_vote(vote)
    }

    fn on_tally_changed(
        &mut self,
        round: u64,
        phase: BftVotePhaseV1,
        now_ms: i64,
    ) -> Result<Vec<BftConsensusActionV1>, String> {
        let tally = self
            .votes
            .get(&(round, phase))
            .cloned()
            .ok_or_else(|| "BFT vote tally disappeared".to_string())?;
        let mut output = Vec::new();
        self.arm_quorum_timeout_if_ready(round, phase, now_ms)?;
        if phase == BftVotePhaseV1::Prevote {
            let mut values = BTreeSet::new();
            for vote in tally.votes() {
                values.insert(vote.value.clone());
            }
            for value in values {
                if tally.power_for(&self.set, &value)? < self.set.quorum_voting_power() {
                    continue;
                }
                match &value {
                    BftVoteValueV1::Nil => {
                        if round == self.round && self.step == BftStepV1::Prevote {
                            self.set_step(BftStepV1::Precommit, now_ms)?;
                            output.extend(
                                self.vote_intent(
                                    &self.local_validator_id,
                                    BftVotePhaseV1::Precommit,
                                    BftVoteValueV1::Nil,
                                    round,
                                )?
                                .map(BftConsensusActionV1::SignVote),
                            );
                            if self.votes.contains_key(&(round, BftVotePhaseV1::Precommit)) {
                                output.extend(self.on_tally_changed(
                                    round,
                                    BftVotePhaseV1::Precommit,
                                    now_ms,
                                )?);
                            }
                        }
                    }
                    BftVoteValueV1::Block { block_id } => {
                        if self.proposal_by_id(round, block_id).is_none() {
                            continue;
                        }
                        if self
                            .valid_round
                            .is_none_or(|valid_round| round > valid_round)
                        {
                            self.valid_value = Some(block_id.clone());
                            self.valid_round = Some(round);
                            self.valid_round_proof = Some(BftValidRoundEvidenceV1 {
                                proposal: self
                                    .proposal_by_id(round, block_id)
                                    .ok_or_else(|| {
                                        "BFT valid-round proposal disappeared".to_string()
                                    })?
                                    .header
                                    .clone(),
                                prevotes: tally
                                    .votes()
                                    .filter(|vote| vote.value == value)
                                    .cloned()
                                    .collect(),
                            });
                        }
                        if round == self.round && self.step == BftStepV1::Prevote {
                            self.locked_value = Some(block_id.clone());
                            self.locked_round = Some(round);
                            self.set_step(BftStepV1::Precommit, now_ms)?;
                            output.extend(
                                self.vote_intent(
                                    &self.local_validator_id,
                                    BftVotePhaseV1::Precommit,
                                    value.clone(),
                                    round,
                                )?
                                .map(BftConsensusActionV1::SignVote),
                            );
                            if self.votes.contains_key(&(round, BftVotePhaseV1::Precommit)) {
                                output.extend(self.on_tally_changed(
                                    round,
                                    BftVotePhaseV1::Precommit,
                                    now_ms,
                                )?);
                            }
                        }
                    }
                }
            }
        } else if phase == BftVotePhaseV1::Precommit {
            let mut values = BTreeSet::new();
            for vote in tally.votes() {
                values.insert(vote.value.clone());
            }
            for value in values {
                if let BftVoteValueV1::Block { block_id } = &value
                    && tally.power_for(&self.set, &value)? >= self.set.quorum_voting_power()
                    && let Some(proposal) = self.proposal_by_id(round, block_id)
                {
                    if self.commit_emitted.contains(&(round, block_id.clone())) {
                        continue;
                    }
                    let precommits = tally
                        .votes()
                        .filter(|vote| vote.value == value)
                        .cloned()
                        .collect();
                    let certificate = BftCommitCertificateV1 {
                        proposal: Box::new(proposal.clone()),
                        precommits,
                    };
                    verify_commit_certificate_v1(&self.set, &self.round_proposers, &certificate)?;
                    self.commit_emitted.insert((round, block_id.clone()));
                    output.push(BftConsensusActionV1::Commit(certificate));
                }
            }
        }
        Ok(output)
    }

    fn proposal_by_id(&self, round: u64, block_id: &str) -> Option<&BftProposalV1> {
        self.proposals
            .get(&round)
            .filter(|proposal| proposal.header.block_id == block_id)
    }

    fn set_step(&mut self, step: BftStepV1, now_ms: i64) -> Result<(), String> {
        self.step = step;
        self.step_deadline_ms = None;
        match step {
            BftStepV1::Propose => {
                self.step_deadline_ms = Some(self.deadline_for_step(step, now_ms)?);
            }
            BftStepV1::Prevote => {
                self.arm_quorum_timeout_if_ready(self.round, BftVotePhaseV1::Prevote, now_ms)?
            }
            BftStepV1::Precommit => {
                self.arm_quorum_timeout_if_ready(self.round, BftVotePhaseV1::Precommit, now_ms)?
            }
        }
        Ok(())
    }

    fn deadline_for_step(&self, step: BftStepV1, now_ms: i64) -> Result<i64, String> {
        let timeout = i64::try_from(self.timeouts.for_step(step, self.round)?)
            .map_err(|_| "BFT phase timeout exceeds signed clock range".to_string())?;
        now_ms
            .checked_add(timeout)
            .ok_or_else(|| "BFT phase deadline overflow".to_string())
    }

    fn arm_quorum_timeout_if_ready(
        &mut self,
        round: u64,
        phase: BftVotePhaseV1,
        now_ms: i64,
    ) -> Result<(), String> {
        if round != self.round {
            return Ok(());
        }
        let (step, armed) = match phase {
            BftVotePhaseV1::Prevote => (BftStepV1::Prevote, self.prevote_quorum_timeout_armed),
            BftVotePhaseV1::Precommit => {
                (BftStepV1::Precommit, self.precommit_quorum_timeout_armed)
            }
        };
        if self.step != step || armed {
            return Ok(());
        }
        let Some(tally) = self.votes.get(&(round, phase)) else {
            return Ok(());
        };
        if tally.unique_power(&self.set)? < self.set.quorum_voting_power() {
            return Ok(());
        }
        match phase {
            BftVotePhaseV1::Prevote => self.prevote_quorum_timeout_armed = true,
            BftVotePhaseV1::Precommit => self.precommit_quorum_timeout_armed = true,
        }
        self.step_deadline_ms = Some(self.deadline_for_step(step, now_ms)?);
        Ok(())
    }

    pub fn on_timeout(
        &mut self,
        now_ms: i64,
        local_validator_id: &str,
    ) -> Result<Vec<BftConsensusActionV1>, String> {
        if local_validator_id != self.local_validator_id {
            return Err("BFT timeout local validator identity changed".into());
        }
        if self
            .step_deadline_ms
            .is_none_or(|deadline| now_ms < deadline)
        {
            return Ok(Vec::new());
        }
        match self.step {
            BftStepV1::Propose => {
                self.set_step(BftStepV1::Prevote, now_ms)?;
                let mut output = self
                    .vote_intent(
                        local_validator_id,
                        BftVotePhaseV1::Prevote,
                        BftVoteValueV1::Nil,
                        self.round,
                    )?
                    .map(BftConsensusActionV1::SignVote)
                    .into_iter()
                    .collect::<Vec<_>>();
                output.extend(self.replay_round_tallies(self.round, now_ms)?);
                Ok(output)
            }
            BftStepV1::Prevote => {
                self.set_step(BftStepV1::Precommit, now_ms)?;
                let mut output = self
                    .vote_intent(
                        local_validator_id,
                        BftVotePhaseV1::Precommit,
                        BftVoteValueV1::Nil,
                        self.round,
                    )?
                    .map(BftConsensusActionV1::SignVote)
                    .into_iter()
                    .collect::<Vec<_>>();
                if self
                    .votes
                    .contains_key(&(self.round, BftVotePhaseV1::Precommit))
                {
                    output.extend(self.on_tally_changed(
                        self.round,
                        BftVotePhaseV1::Precommit,
                        now_ms,
                    )?);
                }
                Ok(output)
            }
            BftStepV1::Precommit => {
                if self
                    .commit_emitted
                    .iter()
                    .any(|(round, _)| *round == self.round)
                {
                    return Ok(Vec::new());
                }
                self.advance_to_round(
                    self.round
                        .checked_add(1)
                        .ok_or_else(|| "BFT round overflow".to_string())?,
                    now_ms,
                )
            }
        }
    }

    fn valid_round_evidence(
        &self,
        round: u64,
        block_id: &str,
    ) -> Result<BftValidRoundEvidenceV1, String> {
        let proof = self
            .valid_round_proof
            .as_ref()
            .filter(|proof| proof.proposal.round == round && proof.proposal.block_id == block_id)
            .ok_or_else(|| "BFT valid-round proof is unavailable".to_string())?;
        Ok(proof.clone())
    }

    pub fn mark_commit_applied(
        &mut self,
        certificate: &BftCommitCertificateV1,
        now_ms: i64,
        timeouts: BftTimeoutsV1,
    ) -> Result<(), String> {
        let timeouts = timeouts.validate()?;
        if certificate.proposal.block.height != self.height {
            return Err("BFT certificate is not for current height".into());
        }
        verify_commit_certificate_v1(&self.set, &self.round_proposers, certificate)?;
        let next_height_priority = BftProposerPriorityV1::after_round(
            &self.set,
            &self.height_start_priority,
            certificate.proposal.header.round,
        )?;
        self.height = self
            .height
            .checked_add(1)
            .ok_or_else(|| "BFT height overflow".to_string())?;
        self.round = 0;
        self.step = BftStepV1::Propose;
        self.locked_value = None;
        self.locked_round = None;
        self.valid_value = None;
        self.valid_round = None;
        self.valid_round_proof = None;
        self.round_proposers.clear();
        self.proposals.clear();
        self.votes.clear();
        self.future_round_voters.clear();
        self.local_votes.clear();
        self.prevote_quorum_timeout_armed = false;
        self.precommit_quorum_timeout_armed = false;
        self.commit_emitted.clear();
        self.proposal_equivocations.clear();
        self.height_start_priority = next_height_priority.clone();
        self.proposer_priority = next_height_priority;
        self.timeouts = timeouts;
        let timeout = i64::try_from(timeouts.for_step(BftStepV1::Propose, 0)?)
            .map_err(|_| "BFT propose timeout exceeds signed clock range".to_string())?;
        self.step_deadline_ms = Some(
            now_ms
                .checked_add(timeout)
                .ok_or_else(|| "BFT propose deadline overflow".to_string())?,
        );
        let proposer = self.proposer_priority.next_proposer(&self.set)?;
        self.round_proposers.insert(0, proposer.validator_id);
        Ok(())
    }

    fn advance_to_round(
        &mut self,
        target_round: u64,
        now_ms: i64,
    ) -> Result<Vec<BftConsensusActionV1>, String> {
        if target_round <= self.round {
            return Ok(Vec::new());
        }
        let distance = target_round - self.round;
        if distance > BFT_MAX_TRACKED_ROUNDS_V1 {
            return Err("BFT target round exceeds bounded window".into());
        }
        if target_round > BFT_MAX_ROUNDS_PER_HEIGHT_V1 {
            return Err(
                "BFT per-height round capacity exceeded; stop and preserve signer state".into(),
            );
        }
        while self.round < target_round {
            self.round = self
                .round
                .checked_add(1)
                .ok_or_else(|| "BFT round overflow".to_string())?;
            let proposer = self.proposer_priority.next_proposer(&self.set)?;
            self.round_proposers
                .insert(self.round, proposer.validator_id);
        }
        let minimum_round = target_round.saturating_sub(BFT_MAX_TRACKED_ROUNDS_V1);
        self.proposals.retain(|round, _| *round >= minimum_round);
        self.votes.retain(|(round, _), _| *round >= minimum_round);
        self.local_votes
            .retain(|(round, _), _| *round >= minimum_round);
        self.commit_emitted
            .retain(|(round, _)| *round >= minimum_round);
        self.future_round_voters
            .retain(|round, _| *round >= target_round);
        self.prevote_quorum_timeout_armed = false;
        self.precommit_quorum_timeout_armed = false;
        self.set_step(BftStepV1::Propose, now_ms)?;
        let mut output = vec![BftConsensusActionV1::RoundAdvanced {
            height: self.height,
            round: self.round,
        }];
        if let Some(proposal) = self.proposals.remove(&target_round) {
            output.extend(self.receive_proposal(proposal, true, now_ms)?);
        }
        for phase in [BftVotePhaseV1::Prevote, BftVotePhaseV1::Precommit] {
            if self.votes.contains_key(&(target_round, phase)) {
                output.extend(self.on_tally_changed(target_round, phase, now_ms)?);
            }
        }
        Ok(output)
    }
}

/// Verify a commit certificate without changing local height/round. Each vote
/// list must contain at most one entry per validator identity.
pub fn verify_commit_certificate_v1(
    set: &BftValidatorSetV1,
    expected_proposers: &BTreeMap<u64, String>,
    certificate: &BftCommitCertificateV1,
) -> Result<(), String> {
    certificate.validate_wire_bounds()?;
    let proposal = certificate.proposal.as_ref();
    proposal.validate_binding()?;
    if expected_proposers.get(&proposal.header.round) != Some(&proposal.header.proposer_id) {
        return Err("BFT commit proposal has wrong deterministic proposer".into());
    }
    set.validate_proposer(&proposal.header)?;
    if proposal.block.scope != *set.scope() {
        return Err("BFT commit block scope mismatch".into());
    }
    if let Some(valid_round) = proposal.header.valid_round {
        let evidence = proposal
            .valid_round_evidence
            .as_ref()
            .ok_or_else(|| "BFT commit valid-round evidence missing".to_string())?;
        if evidence.proposal.round != valid_round
            || evidence.proposal.height != proposal.header.height
            || evidence.proposal.block_id != proposal.header.block_id
        {
            return Err("BFT commit valid-round evidence mismatch".into());
        }
        if expected_proposers.get(&valid_round) != Some(&evidence.proposal.proposer_id) {
            return Err("BFT valid-round evidence has wrong deterministic proposer".into());
        }
        set.validate_proposer(&evidence.proposal)?;
        let mut prevotes = VoteTally::new();
        let mut seen_prevote_signers = BTreeSet::new();
        for vote in evidence.prevotes.iter() {
            if vote.height != proposal.header.height
                || vote.round != valid_round
                || vote.phase != BftVotePhaseV1::Prevote
                || vote.value
                    != (BftVoteValueV1::Block {
                        block_id: proposal.header.block_id.clone(),
                    })
            {
                return Err("BFT commit valid-round prevote mismatch".into());
            }
            set.validate_vote(vote)?;
            if !seen_prevote_signers.insert(vote.validator_id.as_str()) {
                return Err("BFT valid-round evidence repeats a validator".into());
            }
            prevotes.insert(vote.clone())?;
        }
        if prevotes.power_for(
            set,
            &BftVoteValueV1::Block {
                block_id: proposal.header.block_id.clone(),
            },
        )? < set.quorum_voting_power()
        {
            return Err("BFT commit valid-round lacks quorum prevotes".into());
        }
    }
    let round = proposal.header.round;
    let block_id = proposal.header.block_id.clone();
    let mut precommits = VoteTally::new();
    let mut seen_precommit_signers = BTreeSet::new();
    for vote in certificate.precommits.iter() {
        if vote.height != proposal.header.height
            || vote.round != round
            || vote.phase != BftVotePhaseV1::Precommit
            || vote.value
                != (BftVoteValueV1::Block {
                    block_id: block_id.clone(),
                })
        {
            return Err("BFT commit certificate contains a mismatched precommit".into());
        }
        set.validate_vote(vote)?;
        if !seen_precommit_signers.insert(vote.validator_id.as_str()) {
            return Err("BFT commit certificate repeats a validator".into());
        }
        precommits.insert(vote.clone())?;
    }
    let power = precommits.power_for(set, &BftVoteValueV1::Block { block_id })?;
    if power < set.quorum_voting_power() {
        return Err("BFT commit certificate lacks quorum precommits".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
