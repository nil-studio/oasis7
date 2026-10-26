use std::collections::{BTreeMap, BTreeSet};

use oasis7_proto::distributed_bft::v2::{BftVoteV2, BftVoteValueV2};

use super::super::{BFT_MAX_EQUIVOCATIONS_V2, BftValidatorSetV2};

#[derive(Debug, Clone)]
pub(super) struct VoteTally {
    votes: BTreeMap<(String, BftVoteValueV2), BftVoteV2>,
    evidence: BTreeMap<String, (BftVoteV2, BftVoteV2)>,
}

impl VoteTally {
    pub(super) fn new() -> Self {
        Self {
            votes: BTreeMap::new(),
            evidence: BTreeMap::new(),
        }
    }

    pub(super) fn insert(&mut self, vote: BftVoteV2) -> Result<bool, String> {
        let key = (vote.validator_id.clone(), vote.value.clone());
        if let Some(previous) = self.votes.get(&key) {
            if previous == &vote {
                return Ok(false);
            }
            return Err("BFT validator supplied conflicting bytes for one vote value".into());
        }
        let prior_values = self
            .votes
            .keys()
            .filter(|(validator_id, _)| validator_id == &vote.validator_id)
            .count();
        if prior_values >= 2 {
            return Ok(false);
        }
        if prior_values > 0 && !self.evidence.contains_key(vote.validator_id.as_str()) {
            if self.evidence.len() >= BFT_MAX_EQUIVOCATIONS_V2 {
                return Err("BFT equivocation evidence capacity exceeded".into());
            }
            let first = self
                .votes
                .values()
                .find(|previous| previous.validator_id == vote.validator_id)
                .cloned()
                .ok_or_else(|| "BFT equivocation first vote disappeared".to_string())?;
            self.evidence
                .insert(vote.validator_id.clone(), (first, vote.clone()));
        }
        self.votes.insert(key, vote);
        Ok(true)
    }

    pub(super) fn power_for(
        &self,
        set: &BftValidatorSetV2,
        value: &BftVoteValueV2,
    ) -> Result<u64, String> {
        let mut power = 0u64;
        let mut counted = BTreeSet::new();
        for vote in self.votes.values() {
            if &vote.value == value && counted.insert(vote.validator_id.as_str()) {
                power = power
                    .checked_add(
                        set.voting_power(vote.validator_id.as_str())
                            .ok_or_else(|| "BFT tally contains unknown validator".to_string())?,
                    )
                    .ok_or_else(|| "BFT vote tally power overflow".to_string())?;
            }
        }
        Ok(power)
    }

    pub(super) fn unique_power(&self, set: &BftValidatorSetV2) -> Result<u64, String> {
        let mut power = 0u64;
        let mut counted = BTreeSet::new();
        for vote in self.votes.values() {
            if counted.insert(vote.validator_id.as_str()) {
                power = power
                    .checked_add(
                        set.voting_power(vote.validator_id.as_str())
                            .ok_or_else(|| "BFT tally contains unknown validator".to_string())?,
                    )
                    .ok_or_else(|| "BFT vote tally power overflow".to_string())?;
            }
        }
        Ok(power)
    }

    pub(super) fn growth_for(&self, vote: &BftVoteV2) -> usize {
        let key = (vote.validator_id.clone(), vote.value.clone());
        if self.votes.contains_key(&key) {
            return 0;
        }
        let prior_values = self
            .votes
            .keys()
            .filter(|(validator_id, _)| validator_id == &vote.validator_id)
            .count();
        if prior_values >= 2 {
            0
        } else {
            1 + usize::from(prior_values > 0 && !self.evidence.contains_key(&vote.validator_id))
        }
    }

    pub(super) fn message_count(&self) -> usize {
        self.votes.len().saturating_add(self.evidence.len())
    }

    pub(super) fn votes(&self) -> impl Iterator<Item = &BftVoteV2> {
        self.votes.values()
    }

    pub(super) fn evidence(&self) -> impl Iterator<Item = &(BftVoteV2, BftVoteV2)> {
        self.evidence.values()
    }
}
