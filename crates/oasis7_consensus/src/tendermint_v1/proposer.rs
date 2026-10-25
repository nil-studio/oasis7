use std::collections::BTreeMap;

use super::BftValidatorSetV1;
use super::engine::BFT_MAX_ROUNDS_PER_HEIGHT_V1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftProposerV1 {
    pub validator_id: String,
    pub public_key_hex: String,
    pub voting_power: u64,
}

/// Deterministic Comet-style weighted round-robin proposer priorities.
/// Priorities are carried across rounds and heights by the caller's WAL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftProposerPriorityV1 {
    priorities: BTreeMap<String, i128>,
}

impl BftProposerPriorityV1 {
    pub fn new(set: &BftValidatorSetV1) -> Result<Self, String> {
        let mut this = Self {
            priorities: set
                .validators()
                .keys()
                .map(|validator_id| (validator_id.clone(), 0))
                .collect(),
        };
        this.center()?;
        Ok(this)
    }

    pub fn priorities(&self) -> &BTreeMap<String, i128> {
        &self.priorities
    }

    pub fn from_priorities(
        set: &BftValidatorSetV1,
        priorities: BTreeMap<String, i128>,
    ) -> Result<Self, String> {
        let value = Self { priorities };
        value.validate_set(set)?;
        Ok(value)
    }

    pub fn next_proposer(&mut self, set: &BftValidatorSetV1) -> Result<BftProposerV1, String> {
        self.validate_set(set)?;
        self.rescale(set)?;
        self.center()?;
        for validator in set.validators().values() {
            let priority = self
                .priorities
                .get_mut(&validator.validator_id)
                .ok_or_else(|| "BFT proposer priority is missing a validator".to_string())?;
            *priority = priority
                .checked_add(i128::from(validator.voting_power))
                .ok_or_else(|| "BFT proposer priority overflow".to_string())?;
        }
        let mut winner: Option<&str> = None;
        for validator in set.validators().values() {
            let Some(current_id) = winner else {
                winner = Some(validator.validator_id.as_str());
                continue;
            };
            let current = set
                .validator(current_id)
                .ok_or_else(|| "BFT proposer winner disappeared".to_string())?;
            let candidate_priority = self.priorities[&validator.validator_id];
            let current_priority = self.priorities[current_id];
            if candidate_priority > current_priority
                || (candidate_priority == current_priority
                    && validator.public_key_hex.as_bytes() < current.public_key_hex.as_bytes())
            {
                winner = Some(validator.validator_id.as_str());
            }
        }
        let winner_id = winner.ok_or_else(|| "BFT validator set is empty".to_string())?;
        let winner_power = set
            .total_voting_power()
            .try_into()
            .map_err(|_| "BFT total voting power exceeds proposer arithmetic".to_string())?;
        let winner_priority = self
            .priorities
            .get_mut(winner_id)
            .ok_or_else(|| "BFT proposer winner priority disappeared".to_string())?;
        *winner_priority = winner_priority
            .checked_sub(winner_power)
            .ok_or_else(|| "BFT proposer priority overflow".to_string())?;
        let validator = set
            .validator(winner_id)
            .ok_or_else(|| "BFT selected proposer disappeared".to_string())?;
        Ok(BftProposerV1 {
            validator_id: validator.validator_id.clone(),
            public_key_hex: validator.public_key_hex.clone(),
            voting_power: validator.voting_power,
        })
    }

    /// Reconstruct the height's priority after selecting every proposer from
    /// round zero through `round`. Callers must bound the round before use.
    pub(super) fn after_round(
        set: &BftValidatorSetV1,
        starting_priority: &Self,
        round: u64,
    ) -> Result<Self, String> {
        if round > BFT_MAX_ROUNDS_PER_HEIGHT_V1 {
            return Err("BFT proposer round capacity exceeded".into());
        }
        starting_priority.validate_set(set)?;
        let mut priority = starting_priority.clone();
        for _ in 0..=round {
            priority.next_proposer(set)?;
        }
        Ok(priority)
    }

    fn validate_set(&self, set: &BftValidatorSetV1) -> Result<(), String> {
        if self.priorities.len() != set.validators().len()
            || self
                .priorities
                .keys()
                .any(|validator_id| !set.validators().contains_key(validator_id))
        {
            return Err("BFT proposer priorities do not match the fixed validator set".into());
        }
        Ok(())
    }

    fn rescale(&mut self, set: &BftValidatorSetV1) -> Result<(), String> {
        let total = i128::from(set.total_voting_power());
        let max_distance = total
            .checked_mul(2)
            .ok_or_else(|| "BFT proposer priority scale overflow".to_string())?;
        let (minimum, maximum) = self.priority_bounds()?;
        let distance = maximum
            .checked_sub(minimum)
            .ok_or_else(|| "BFT proposer priority range overflow".to_string())?;
        if distance > max_distance {
            let ratio = distance / max_distance;
            for priority in self.priorities.values_mut() {
                *priority /= ratio;
            }
        }
        Ok(())
    }

    fn center(&mut self) -> Result<(), String> {
        if self.priorities.is_empty() {
            return Err("BFT proposer priorities cannot be empty".into());
        }
        let sum = self.priorities.values().try_fold(0i128, |sum, priority| {
            sum.checked_add(*priority)
                .ok_or_else(|| "BFT proposer priority sum overflow".to_string())
        })?;
        let average = sum / self.priorities.len() as i128;
        for priority in self.priorities.values_mut() {
            *priority = priority
                .checked_sub(average)
                .ok_or_else(|| "BFT proposer priority centering overflow".to_string())?;
        }
        Ok(())
    }

    fn priority_bounds(&self) -> Result<(i128, i128), String> {
        let mut priorities = self.priorities.values();
        let first = *priorities
            .next()
            .ok_or_else(|| "BFT proposer priorities cannot be empty".to_string())?;
        let mut minimum = first;
        let mut maximum = first;
        for priority in priorities {
            minimum = minimum.min(*priority);
            maximum = maximum.max(*priority);
        }
        Ok((minimum, maximum))
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::tendermint_v1::BftValidatorV1;
    use oasis7_proto::distributed_bft::BftScopeV1;

    fn set(powers: &[u64], keys: &[[u8; 32]]) -> BftValidatorSetV1 {
        BftValidatorSetV1::new(
            BftScopeV1 {
                world_id: "w".into(),
                chain_id: "c".into(),
                genesis_hash: "11".repeat(32),
            },
            powers
                .iter()
                .zip(keys)
                .enumerate()
                .map(|(index, (power, seed))| BftValidatorV1 {
                    validator_id: format!("v{index}"),
                    public_key_hex: hex::encode(
                        SigningKey::from_bytes(seed).verifying_key().to_bytes(),
                    ),
                    voting_power: *power,
                })
                .collect(),
        )
        .expect("valid validator set")
    }

    #[test]
    fn weighted_quorum_and_proposer_sequence_are_checked_and_deterministic() {
        let keys = [[1; 32], [2; 32], [3; 32]];
        let set = set(&[1, 2, 3], &keys);
        assert_eq!(set.total_voting_power(), 6);
        assert_eq!(set.quorum_voting_power(), 5);
        assert_eq!(set.future_round_voting_power(), 3);

        let mut priorities = BftProposerPriorityV1::new(&set).unwrap();
        let sequence = (0..6)
            .map(|_| {
                priorities
                    .next_proposer(&set)
                    .expect("weighted proposer")
                    .validator_id
            })
            .collect::<Vec<_>>();
        assert_eq!(sequence, ["v2", "v1", "v0", "v2", "v1", "v2"]);
    }

    #[test]
    fn same_priority_tie_breaks_on_raw_public_key_bytes() {
        let keys = [[9; 32], [1; 32]];
        let set = set(&[1, 1], &keys);
        let mut priority = BftProposerPriorityV1::new(&set).unwrap();
        assert_eq!(priority.next_proposer(&set).unwrap().validator_id, "v1");
    }

    #[test]
    fn rejects_duplicate_keys_zero_power_and_total_overflow() {
        let key = SigningKey::from_bytes(&[1; 32]);
        let public_key_hex = hex::encode(key.verifying_key().to_bytes());
        let scope = BftScopeV1 {
            world_id: "w".into(),
            chain_id: "c".into(),
            genesis_hash: "11".repeat(32),
        };
        let validator = |id: &str, voting_power| BftValidatorV1 {
            validator_id: id.into(),
            public_key_hex: public_key_hex.clone(),
            voting_power,
        };
        assert!(
            BftValidatorSetV1::new(scope.clone(), vec![validator("a", 1), validator("b", 1)])
                .unwrap_err()
                .contains("public keys must be unique")
        );
        assert!(
            BftValidatorSetV1::new(scope.clone(), vec![validator("a", 0)])
                .unwrap_err()
                .contains("must be positive")
        );
        let second_key = SigningKey::from_bytes(&[2; 32]);
        let mut second = validator("b", 1);
        second.public_key_hex = hex::encode(second_key.verifying_key().to_bytes());
        let mut first = validator("a", u64::MAX);
        first.public_key_hex = hex::encode(key.verifying_key().to_bytes());
        assert!(
            BftValidatorSetV1::new(scope, vec![first, second])
                .unwrap_err()
                .contains("total voting power overflow")
        );
    }
}
