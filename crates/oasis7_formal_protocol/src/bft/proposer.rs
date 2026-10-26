use std::collections::BTreeMap;

use super::{BFT_MAX_ROUNDS_PER_HEIGHT_V2, BftValidatorSetV2};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftProposerV2 {
    pub validator_id: String,
    pub public_key_hex: String,
    pub voting_power: u64,
}

/// Priority state is keyed by consensus public key. This preserves a continuing
/// member's priority by address and treats a key rotation as a new validator.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BftProposerPriorityV2 {
    #[serde(with = "super::priority_wire")]
    priorities: BTreeMap<String, i128>,
}

impl BftProposerPriorityV2 {
    pub fn new(set: &BftValidatorSetV2) -> Result<Self, String> {
        let mut value = Self {
            priorities: set
                .validators()
                .values()
                .map(|validator| (validator.public_key_hex.clone(), 0))
                .collect(),
        };
        value.center()?;
        Ok(value)
    }

    pub fn from_priorities(
        set: &BftValidatorSetV2,
        priorities: BTreeMap<String, i128>,
    ) -> Result<Self, String> {
        let value = Self { priorities };
        value.validate_set(set)?;
        Ok(value)
    }

    pub fn priorities(&self) -> &BTreeMap<String, i128> {
        &self.priorities
    }

    pub fn next_proposer(&mut self, set: &BftValidatorSetV2) -> Result<BftProposerV2, String> {
        self.validate_set(set)?;
        self.rescale(set)?;
        self.center()?;
        for validator in set.validators().values() {
            let priority = self
                .priorities
                .get_mut(&validator.public_key_hex)
                .ok_or_else(|| "BFT V2 proposer priority is missing a key".to_string())?;
            *priority = priority
                .checked_add(i128::from(validator.voting_power))
                .ok_or_else(|| "BFT V2 proposer priority overflow".to_string())?;
        }
        let mut winner: Option<&str> = None;
        for validator in set.validators().values() {
            let Some(current_key) = winner else {
                winner = Some(validator.public_key_hex.as_str());
                continue;
            };
            let candidate = self.priorities[&validator.public_key_hex];
            let current = self.priorities[current_key];
            if candidate > current
                || (candidate == current
                    && validator.public_key_hex.as_bytes() < current_key.as_bytes())
            {
                winner = Some(validator.public_key_hex.as_str());
            }
        }
        let winner_key = winner.ok_or_else(|| "BFT V2 validator set is empty".to_string())?;
        let winner_priority = self
            .priorities
            .get_mut(winner_key)
            .ok_or_else(|| "BFT V2 winner priority disappeared".to_string())?;
        *winner_priority = winner_priority
            .checked_sub(i128::from(set.total_voting_power()))
            .ok_or_else(|| "BFT V2 proposer priority overflow".to_string())?;
        let winner = set
            .validator_by_public_key(winner_key)
            .ok_or_else(|| "BFT V2 proposer disappeared".to_string())?;
        Ok(BftProposerV2 {
            validator_id: winner.validator_id.clone(),
            public_key_hex: winner.public_key_hex.clone(),
            voting_power: winner.voting_power,
        })
    }

    pub(super) fn after_round(
        set: &BftValidatorSetV2,
        start: &Self,
        round: u64,
    ) -> Result<Self, String> {
        if round > BFT_MAX_ROUNDS_PER_HEIGHT_V2 {
            return Err("BFT V2 proposer round capacity exceeded".into());
        }
        start.validate_set(set)?;
        let mut next = start.clone();
        for _ in 0..=round {
            next.next_proposer(set)?;
        }
        Ok(next)
    }

    /// Comet-style update is applied to the old schedule's h+1 seed at commit
    /// h, then exactly one new-set block increment prepares the h+2 seed.
    pub(super) fn after_validator_change(
        old_set: &BftValidatorSetV2,
        new_set: &BftValidatorSetV2,
        old_h_plus_one_seed: &Self,
    ) -> Result<Self, String> {
        old_h_plus_one_seed.validate_set(old_set)?;
        let removed_power = old_set
            .validators()
            .values()
            .filter(|old| {
                new_set
                    .validator_by_public_key(old.public_key_hex.as_str())
                    .is_none()
            })
            .try_fold(0u64, |sum, old| {
                sum.checked_add(old.voting_power)
                    .ok_or_else(|| "BFT V2 removed voting power overflow".to_string())
            })?;
        let updated_total_before_removals = new_set
            .total_voting_power()
            .checked_add(removed_power)
            .ok_or_else(|| "BFT V2 pre-removal voting power overflow".to_string())?;
        let initial_new_priority = i128::from(updated_total_before_removals)
            .checked_add(i128::from(updated_total_before_removals >> 3))
            .ok_or_else(|| "BFT V2 new-validator priority overflow".to_string())?
            .checked_neg()
            .ok_or_else(|| "BFT V2 new-validator priority overflow".to_string())?;
        let mut priorities = BTreeMap::new();
        for validator in new_set.validators().values() {
            let priority = old_h_plus_one_seed
                .priorities
                .get(&validator.public_key_hex)
                .copied()
                .unwrap_or(initial_new_priority);
            priorities.insert(validator.public_key_hex.clone(), priority);
        }
        let mut updated = Self { priorities };
        updated.rescale(new_set)?;
        updated.center()?;
        // Comet applies the validator change at h and advances the new set once
        // to produce the proposer priority used at h+2.
        updated.next_proposer(new_set)?;
        Ok(updated)
    }

    fn validate_set(&self, set: &BftValidatorSetV2) -> Result<(), String> {
        if self.priorities.len() != set.validators().len()
            || self
                .priorities
                .keys()
                .any(|key| set.validator_by_public_key(key.as_str()).is_none())
        {
            return Err("BFT V2 proposer priorities do not match active public keys".into());
        }
        Ok(())
    }

    fn rescale(&mut self, set: &BftValidatorSetV2) -> Result<(), String> {
        let max_distance = i128::from(set.total_voting_power())
            .checked_mul(2)
            .ok_or_else(|| "BFT V2 proposer priority scale overflow".to_string())?;
        let (minimum, maximum) = self.bounds()?;
        let distance = maximum
            .checked_sub(minimum)
            .ok_or_else(|| "BFT V2 proposer priority range overflow".to_string())?;
        if distance > max_distance {
            let ratio = distance
                .checked_div(max_distance)
                .and_then(|quotient| quotient.checked_add(i128::from(distance % max_distance != 0)))
                .ok_or_else(|| "BFT V2 proposer priority scale overflow".to_string())?;
            for priority in self.priorities.values_mut() {
                *priority /= ratio;
            }
        }
        Ok(())
    }

    fn center(&mut self) -> Result<(), String> {
        if self.priorities.is_empty() {
            return Err("BFT V2 proposer priority cannot be empty".into());
        }
        let sum = self.priorities.values().try_fold(0i128, |sum, priority| {
            sum.checked_add(*priority)
                .ok_or_else(|| "BFT V2 proposer priority sum overflow".to_string())
        })?;
        let average = sum / self.priorities.len() as i128;
        for priority in self.priorities.values_mut() {
            *priority = priority
                .checked_sub(average)
                .ok_or_else(|| "BFT V2 proposer priority centering overflow".to_string())?;
        }
        Ok(())
    }

    fn bounds(&self) -> Result<(i128, i128), String> {
        let mut values = self.priorities.values();
        let first = *values
            .next()
            .ok_or_else(|| "BFT V2 proposer priority cannot be empty".to_string())?;
        let mut min = first;
        let mut max = first;
        for value in values {
            min = min.min(*value);
            max = max.max(*value);
        }
        Ok((min, max))
    }
}
