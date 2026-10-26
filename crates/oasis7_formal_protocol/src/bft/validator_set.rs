use std::collections::BTreeMap;

use oasis7_proto::distributed_bft::v2::{BFT_MAX_VALIDATORS_V2, BftScopeV2, BftValidatorV2};
use serde::Serialize;

pub const BFT_HEIGHT_SCHEDULE_RULESET_V2: &str = "height-seed-v2";
pub const BFT_VALIDATOR_CHANGE_RULESET_V2: &str = "comet-validator-change-priority-v1";
pub const BFT_EPOCH_MAPPING_RULESET_V2: &str = "epoch-multiply-minus-two-v1";
pub const BFT_MAX_ROUNDS_PER_HEIGHT_V2: u64 = 4096;
pub const BFT_MAX_ACTIONS_BYTES_V2: usize = 1_048_576;
pub const BFT_MAX_VALIDATORS_PER_TRANSITION_V2: usize = BFT_MAX_VALIDATORS_V2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BftValidatorSetV2 {
    scope: BftScopeV2,
    validators: BTreeMap<String, BftValidatorV2>,
    by_public_key: BTreeMap<String, String>,
    total_voting_power: u64,
    quorum_voting_power: u64,
    future_round_voting_power: u64,
    set_hash: String,
}

#[derive(Serialize)]
struct ValidatorSetHashPayloadV2<'a> {
    protocol_version: u16,
    proposer_ruleset: &'static str,
    change_ruleset: &'static str,
    scope: &'a BftScopeV2,
    validators: Vec<&'a BftValidatorV2>,
}

impl BftValidatorSetV2 {
    pub fn new(scope: BftScopeV2, validators: Vec<BftValidatorV2>) -> Result<Self, String> {
        scope.validate()?;
        if validators.is_empty() || validators.len() > BFT_MAX_VALIDATORS_V2 {
            return Err("BFT V2 validator set is empty or exceeds its bound".into());
        }
        let mut by_id = BTreeMap::new();
        let mut by_public_key = BTreeMap::new();
        let mut total_voting_power = 0u64;
        for validator in validators {
            validator.validate()?;
            if by_public_key
                .insert(
                    validator.public_key_hex.clone(),
                    validator.validator_id.clone(),
                )
                .is_some()
            {
                return Err("BFT V2 validator public keys must be unique".into());
            }
            if by_id
                .insert(validator.validator_id.clone(), validator)
                .is_some()
            {
                return Err("BFT V2 validator ids must be unique".into());
            }
        }
        for validator in by_id.values() {
            total_voting_power = total_voting_power
                .checked_add(validator.voting_power)
                .ok_or_else(|| "BFT V2 total voting power overflow".to_string())?;
        }
        let quorum_voting_power = quorum(total_voting_power)?;
        let future_round_voting_power = total_voting_power / 3 + 1;
        let encoded = serde_cbor::to_vec(&ValidatorSetHashPayloadV2 {
            protocol_version: 2,
            proposer_ruleset: BFT_HEIGHT_SCHEDULE_RULESET_V2,
            change_ruleset: BFT_VALIDATOR_CHANGE_RULESET_V2,
            scope: &scope,
            validators: by_id.values().collect(),
        })
        .map_err(|error| format!("encode BFT V2 validator set: {error}"))?;
        let digest = hash_domain("oasis7.bft.validator_set.v2.hash", &encoded)?;
        Ok(Self {
            scope,
            validators: by_id,
            by_public_key,
            total_voting_power,
            quorum_voting_power,
            future_round_voting_power,
            set_hash: hex::encode(digest),
        })
    }

    pub fn scope(&self) -> &BftScopeV2 {
        &self.scope
    }

    pub fn validators(&self) -> &BTreeMap<String, BftValidatorV2> {
        &self.validators
    }

    pub fn validator(&self, validator_id: &str) -> Option<&BftValidatorV2> {
        self.validators.get(validator_id)
    }

    pub fn validator_by_public_key(&self, public_key_hex: &str) -> Option<&BftValidatorV2> {
        self.by_public_key
            .get(public_key_hex)
            .and_then(|validator_id| self.validators.get(validator_id))
    }

    pub fn voting_power(&self, validator_id: &str) -> Option<u64> {
        self.validator(validator_id)
            .map(|validator| validator.voting_power)
    }

    pub fn total_voting_power(&self) -> u64 {
        self.total_voting_power
    }

    pub fn quorum_voting_power(&self) -> u64 {
        self.quorum_voting_power
    }

    pub fn future_round_voting_power(&self) -> u64 {
        self.future_round_voting_power
    }

    pub fn set_hash(&self) -> &str {
        &self.set_hash
    }

    pub fn validate_vote(
        &self,
        vote: &oasis7_proto::distributed_bft::v2::BftVoteV2,
    ) -> Result<u64, String> {
        if vote.active_validator_set_hash != self.set_hash || vote.scope != self.scope {
            return Err("BFT V2 vote scope or active set mismatch".into());
        }
        let validator = self
            .validator(&vote.validator_id)
            .ok_or_else(|| "BFT V2 vote signer is not in the active set".to_string())?;
        if validator.public_key_hex != vote.public_key_hex {
            return Err("BFT V2 vote key does not match its active validator".into());
        }
        vote.verify_signature()?;
        Ok(validator.voting_power)
    }

    pub fn validate_proposal_header(
        &self,
        header: &oasis7_proto::distributed_bft::v2::BftProposalHeaderV2,
    ) -> Result<(), String> {
        if header.scope != self.scope || header.active_validator_set_hash != self.set_hash {
            return Err("BFT V2 proposal scope or active set mismatch".into());
        }
        let validator = self
            .validator(&header.proposer_id)
            .ok_or_else(|| "BFT V2 proposer is not in the active set".to_string())?;
        if validator.public_key_hex != header.public_key_hex {
            return Err("BFT V2 proposer key does not match its active validator".into());
        }
        header.verify_signature()
    }

    pub(super) fn verify_transition_set(
        &self,
        transition: &oasis7_proto::distributed_bft::v2::BftValidatorTransitionV2,
    ) -> Result<Self, String> {
        if transition.old_validator_set_hash != self.set_hash {
            return Err("BFT V2 transition old set hash mismatch".into());
        }
        let next = Self::new(self.scope.clone(), transition.new_validators.clone())?;
        if transition.new_validator_set_hash != next.set_hash {
            return Err("BFT V2 transition new set hash mismatch".into());
        }
        Ok(next)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSchedulePolicyV2 {
    pub epoch_length_ticks: u64,
    pub mapping_ruleset: String,
    pub profile_id: String,
}

#[derive(Serialize)]
struct SchedulePolicyHashPayloadV2<'a> {
    protocol_version: u16,
    proposer_ruleset: &'static str,
    change_ruleset: &'static str,
    epoch_length_ticks: u64,
    mapping_ruleset: &'a str,
    profile_id: &'a str,
}

impl BftSchedulePolicyV2 {
    pub fn new(epoch_length_ticks: u64, profile_id: impl Into<String>) -> Result<Self, String> {
        let profile_id = profile_id.into();
        if epoch_length_ticks == 0 || profile_id.trim().is_empty() || profile_id.len() > 256 {
            return Err("BFT V2 epoch length or profile id is invalid".into());
        }
        Ok(Self {
            epoch_length_ticks,
            mapping_ruleset: BFT_EPOCH_MAPPING_RULESET_V2.to_string(),
            profile_id,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.epoch_length_ticks == 0
            || self.mapping_ruleset != BFT_EPOCH_MAPPING_RULESET_V2
            || self.profile_id.trim().is_empty()
            || self.profile_id.len() > 256
        {
            return Err("unsupported BFT V2 schedule policy".into());
        }
        Ok(())
    }

    pub fn hash(&self) -> Result<String, String> {
        self.validate()?;
        let encoded = serde_cbor::to_vec(&SchedulePolicyHashPayloadV2 {
            protocol_version: 2,
            proposer_ruleset: BFT_HEIGHT_SCHEDULE_RULESET_V2,
            change_ruleset: BFT_VALIDATOR_CHANGE_RULESET_V2,
            epoch_length_ticks: self.epoch_length_ticks,
            mapping_ruleset: &self.mapping_ruleset,
            profile_id: &self.profile_id,
        })
        .map_err(|error| format!("encode BFT V2 schedule policy: {error}"))?;
        Ok(hex::encode(hash_domain(
            "oasis7.bft.schedule_policy.v2.hash",
            &encoded,
        )?))
    }
}

pub(super) fn quorum(total: u64) -> Result<u64, String> {
    (total / 3)
        .checked_mul(2)
        .and_then(|base| base.checked_add((total % 3) * 2 / 3))
        .and_then(|base| base.checked_add(1))
        .ok_or_else(|| "BFT V2 quorum arithmetic overflow".into())
}

pub(super) fn hash_domain(domain: &str, encoded: &[u8]) -> Result<[u8; 32], String> {
    let domain_len = u64::try_from(domain.len())
        .map_err(|_| "BFT V2 domain length exceeds framing format".to_string())?;
    let total_len = 8usize
        .checked_add(domain.len())
        .and_then(|length| length.checked_add(encoded.len()))
        .ok_or_else(|| "BFT V2 hash input length overflow".to_string())?;
    if total_len > 2 * 1024 * 1024 {
        return Err("BFT V2 hash input exceeds protocol bound".into());
    }
    let mut bytes = Vec::with_capacity(total_len);
    bytes.extend_from_slice(&domain_len.to_be_bytes());
    bytes.extend_from_slice(domain.as_bytes());
    bytes.extend_from_slice(encoded);
    Ok(*blake3::hash(&bytes).as_bytes())
}
