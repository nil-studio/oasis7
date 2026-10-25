use std::collections::{BTreeMap, BTreeSet};

use oasis7_proto::distributed_bft::{
    BFT_MAX_IDENTITY_TEXT_BYTES_V1, BFT_PROTOCOL_VERSION_V1, BftProposalHeaderV1, BftScopeV1,
    BftVoteV1,
};
use serde::Serialize;

pub use oasis7_proto::distributed_bft::BFT_MAX_VALIDATORS_V1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftValidatorV1 {
    pub validator_id: String,
    pub public_key_hex: String,
    pub voting_power: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftValidatorSetV1 {
    scope: BftScopeV1,
    validators: BTreeMap<String, BftValidatorV1>,
    total_voting_power: u64,
    quorum_voting_power: u64,
    future_round_voting_power: u64,
    set_hash: String,
}

#[derive(Serialize)]
struct ValidatorSetHashPayload<'a> {
    protocol_version: u16,
    scope: &'a BftScopeV1,
    validators: Vec<&'a BftValidatorV1>,
}

impl BftValidatorSetV1 {
    pub fn new(scope: BftScopeV1, validators: Vec<BftValidatorV1>) -> Result<Self, String> {
        scope.validate()?;
        if validators.is_empty() {
            return Err("BFT validator set cannot be empty".into());
        }
        if validators.len() > BFT_MAX_VALIDATORS_V1 {
            return Err(format!(
                "BFT validator set exceeds {BFT_MAX_VALIDATORS_V1} members"
            ));
        }
        let mut by_id = BTreeMap::new();
        let mut public_keys = BTreeSet::new();
        let mut total_voting_power = 0u64;
        for validator in validators {
            if validator.validator_id.trim().is_empty()
                || validator.validator_id.len() > BFT_MAX_IDENTITY_TEXT_BYTES_V1
            {
                return Err("BFT validator id must be nonempty and bounded".into());
            }
            if validator.voting_power == 0 {
                return Err(format!(
                    "BFT validator {} voting power must be positive",
                    validator.validator_id
                ));
            }
            let public_key = decode_public_key(&validator.public_key_hex)?;
            if !public_keys.insert(public_key) {
                return Err("BFT validator public keys must be unique".into());
            }
            if by_id
                .insert(validator.validator_id.clone(), validator)
                .is_some()
            {
                return Err("BFT validator ids must be unique".into());
            }
        }
        for validator in by_id.values() {
            total_voting_power = total_voting_power
                .checked_add(validator.voting_power)
                .ok_or_else(|| "BFT total voting power overflow".to_string())?;
        }
        let quorum_voting_power = (total_voting_power / 3)
            .checked_mul(2)
            .and_then(|base| base.checked_add((total_voting_power % 3) * 2 / 3))
            .and_then(|base| base.checked_add(1))
            .ok_or_else(|| "BFT quorum arithmetic overflow".to_string())?;
        let future_round_voting_power = total_voting_power / 3 + 1;
        let encoded = serde_cbor::to_vec(&ValidatorSetHashPayload {
            protocol_version: BFT_PROTOCOL_VERSION_V1,
            scope: &scope,
            validators: by_id.values().collect(),
        })
        .map_err(|err| format!("encode BFT validator set: {err}"))?;
        let domain = "oasis7.bft.validator_set.v1.hash";
        let framed_len = 8usize
            .checked_add(domain.len())
            .and_then(|size| size.checked_add(encoded.len()))
            .ok_or_else(|| "BFT validator set frame length overflow".to_string())?;
        if framed_len > 256 * 1024 {
            return Err("BFT validator set exceeds protocol bound".into());
        }
        let mut payload = Vec::with_capacity(framed_len);
        payload.extend_from_slice(&(domain.len() as u64).to_be_bytes());
        payload.extend_from_slice(domain.as_bytes());
        payload.extend_from_slice(encoded.as_slice());
        let set_hash = blake3::hash(&payload).to_hex().to_string();
        Ok(Self {
            scope,
            validators: by_id,
            total_voting_power,
            quorum_voting_power,
            future_round_voting_power,
            set_hash,
        })
    }

    pub fn scope(&self) -> &BftScopeV1 {
        &self.scope
    }

    pub fn validators(&self) -> &BTreeMap<String, BftValidatorV1> {
        &self.validators
    }

    pub fn total_voting_power(&self) -> u64 {
        self.total_voting_power
    }

    pub fn quorum_voting_power(&self) -> u64 {
        self.quorum_voting_power
    }

    /// The first integer voting power strictly greater than one third.
    pub fn future_round_voting_power(&self) -> u64 {
        self.future_round_voting_power
    }

    pub fn set_hash(&self) -> &str {
        &self.set_hash
    }

    pub fn validator(&self, validator_id: &str) -> Option<&BftValidatorV1> {
        self.validators.get(validator_id)
    }

    pub fn voting_power(&self, validator_id: &str) -> Option<u64> {
        self.validator(validator_id)
            .map(|validator| validator.voting_power)
    }

    pub fn validate_vote(&self, vote: &BftVoteV1) -> Result<u64, String> {
        if vote.scope != self.scope || vote.validator_set_hash != self.set_hash {
            return Err("BFT vote scope or validator set mismatch".into());
        }
        let validator = self
            .validators
            .get(vote.validator_id.as_str())
            .ok_or_else(|| "BFT vote signer is not in the active validator set".to_string())?;
        if validator.public_key_hex != vote.public_key_hex {
            return Err("BFT vote public key does not match validator identity".into());
        }
        vote.verify_signature()?;
        Ok(validator.voting_power)
    }

    pub fn validate_proposer(&self, header: &BftProposalHeaderV1) -> Result<(), String> {
        if header.scope != self.scope || header.validator_set_hash != self.set_hash {
            return Err("BFT proposal scope or validator set mismatch".into());
        }
        let validator = self
            .validators
            .get(header.proposer_id.as_str())
            .ok_or_else(|| "BFT proposal signer is not in the active validator set".to_string())?;
        if validator.public_key_hex != header.public_key_hex {
            return Err("BFT proposal key does not match validator identity".into());
        }
        header.verify_signature()
    }
}

fn decode_public_key(value: &str) -> Result<[u8; 32], String> {
    let bytes =
        hex::decode(value).map_err(|_| "BFT validator public key is not hex".to_string())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "BFT validator public key must be 32 bytes".to_string())?;
    if hex::encode(bytes) != value {
        return Err("BFT validator public key must use canonical lowercase hex".into());
    }
    let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes)
        .map_err(|_| "BFT validator public key is invalid".to_string())?;
    if key.is_weak() {
        return Err("BFT validator public key is weak".into());
    }
    Ok(bytes)
}
