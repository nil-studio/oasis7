//! Version 2 dynamic-set Tendermint-style wire protocol.
//!
//! V1 definitions remain unchanged. V2 uses distinct signing and block-ID
//! domains and is never a decode fallback for V1 material.

use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

use super::{
    BFT_MAX_ACTIONS_CBOR_BYTES_V1, BFT_MAX_IDENTITY_TEXT_BYTES_V1,
    BFT_MAX_SIGNED_MATERIAL_BYTES_V1, BFT_MAX_VALIDATORS_V1, domain_framed, hash_hex, public_key,
    signature, verifying_key,
};

pub const BFT_PROTOCOL_VERSION_V2: u16 = 2;
pub const BFT_PROPOSAL_SIGNING_DOMAIN_V2: &str = "oasis7.bft.proposal.v2.sign";
pub const BFT_VOTE_SIGNING_DOMAIN_V2: &str = "oasis7.bft.vote.v2.sign";
pub const BFT_BLOCK_ID_DOMAIN_V2: &str = "oasis7.bft.block_id.v2.hash";
pub const BFT_TRANSITION_DOMAIN_V2: &str = "oasis7.bft.validator_transition.v2.hash";
pub const BFT_MAX_ACTIONS_CBOR_BYTES_V2: usize = BFT_MAX_ACTIONS_CBOR_BYTES_V1;
pub const BFT_MAX_SIGNED_MATERIAL_BYTES_V2: usize = BFT_MAX_SIGNED_MATERIAL_BYTES_V1;
pub const BFT_MAX_VALIDATORS_V2: usize = BFT_MAX_VALIDATORS_V1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftScopeV2 {
    pub world_id: String,
    pub chain_id: String,
    pub genesis_hash: String,
}

impl BftScopeV2 {
    pub fn validate(&self) -> Result<(), String> {
        bounded_non_empty("world_id", &self.world_id, BFT_MAX_IDENTITY_TEXT_BYTES_V1)?;
        bounded_non_empty("chain_id", &self.chain_id, BFT_MAX_IDENTITY_TEXT_BYTES_V1)?;
        hash_hex("genesis_hash", &self.genesis_hash)
    }
}

/// Consensus identity is the public key. `validator_id` is an application
/// label and must not be used to carry proposer priority across key changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftValidatorV2 {
    pub validator_id: String,
    pub public_key_hex: String,
    pub voting_power: u64,
}

impl BftValidatorV2 {
    pub fn validate(&self) -> Result<(), String> {
        bounded_non_empty(
            "validator_id",
            &self.validator_id,
            BFT_MAX_IDENTITY_TEXT_BYTES_V1,
        )?;
        if self.voting_power == 0 {
            return Err("BFT V2 validator voting power must be positive".into());
        }
        public_key(&self.public_key_hex)?;
        Ok(())
    }
}

/// Candidate transition data committed by the h block. `source_commit_ref`
/// always names the already-committed parent; it can never name h's own QC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftValidatorTransitionV2 {
    pub source_commit_ref: String,
    pub source_state_root: String,
    pub source_profile: String,
    pub source_epoch: u64,
    pub target_epoch: u64,
    pub epoch_length_ticks: u64,
    pub mapping_ruleset_hash: String,
    pub scheduled_height: u64,
    pub activation_height: u64,
    pub old_validator_set_hash: String,
    pub new_validators: Vec<BftValidatorV2>,
    pub new_validator_set_hash: String,
    pub governance_action_digest: String,
    pub projected_registry_digest: String,
}

impl BftValidatorTransitionV2 {
    pub fn validate(&self) -> Result<(), String> {
        hash_hex("transition source_commit_ref", &self.source_commit_ref)?;
        hash_hex("transition source_state_root", &self.source_state_root)?;
        bounded_non_empty(
            "transition source_profile",
            &self.source_profile,
            BFT_MAX_IDENTITY_TEXT_BYTES_V1,
        )?;
        if self.target_epoch <= self.source_epoch {
            return Err("BFT V2 transition target epoch must be in the future".into());
        }
        if self.epoch_length_ticks == 0 {
            return Err("BFT V2 epoch length must be positive".into());
        }
        hash_hex(
            "transition mapping_ruleset_hash",
            &self.mapping_ruleset_hash,
        )?;
        let expected_activation = self
            .target_epoch
            .checked_mul(self.epoch_length_ticks)
            .ok_or_else(|| "BFT V2 activation height overflow".to_string())?;
        let expected_scheduled = expected_activation
            .checked_sub(2)
            .filter(|height| *height > 0)
            .ok_or_else(|| "BFT V2 transition has no positive scheduled height".to_string())?;
        if self.activation_height != expected_activation
            || self.scheduled_height != expected_scheduled
        {
            return Err("BFT V2 transition height does not match its epoch mapping".into());
        }
        hash_hex(
            "transition old_validator_set_hash",
            &self.old_validator_set_hash,
        )?;
        hash_hex(
            "transition new_validator_set_hash",
            &self.new_validator_set_hash,
        )?;
        hash_hex(
            "transition governance_action_digest",
            &self.governance_action_digest,
        )?;
        hash_hex(
            "transition projected_registry_digest",
            &self.projected_registry_digest,
        )?;
        if self.new_validators.is_empty() || self.new_validators.len() > BFT_MAX_VALIDATORS_V2 {
            return Err("BFT V2 transition validator count is outside its bound".into());
        }
        let mut previous_id: Option<&str> = None;
        for validator in &self.new_validators {
            validator.validate()?;
            if previous_id.is_some_and(|previous| previous >= validator.validator_id.as_str()) {
                return Err("BFT V2 transition validators must be sorted by unique id".into());
            }
            previous_id = Some(validator.validator_id.as_str());
        }
        Ok(())
    }

    pub fn transition_digest(&self) -> Result<String, String> {
        self.validate()?;
        let bytes = domain_framed(BFT_TRANSITION_DOMAIN_V2, self)?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    /// Construct the h+1 reference only after the h QC has been verified.
    pub fn committed_reference(
        &self,
        source_block_id: String,
    ) -> Result<BftValidatorTransitionRefV2, String> {
        self.validate()?;
        hash_hex("transition source_block_id", &source_block_id)?;
        Ok(BftValidatorTransitionRefV2 {
            source_block_id,
            transition_digest: self.transition_digest()?,
            scheduled_height: self.scheduled_height,
            activation_height: self.activation_height,
            old_validator_set_hash: self.old_validator_set_hash.clone(),
            new_validator_set_hash: self.new_validator_set_hash.clone(),
        })
    }
}

/// Reference carried by the h+1 block. It binds to the verified h block ID,
/// not to a self-referential h CommitRef.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftValidatorTransitionRefV2 {
    pub source_block_id: String,
    pub transition_digest: String,
    pub scheduled_height: u64,
    pub activation_height: u64,
    pub old_validator_set_hash: String,
    pub new_validator_set_hash: String,
}

impl BftValidatorTransitionRefV2 {
    pub fn validate(&self) -> Result<(), String> {
        hash_hex(
            "transition reference source_block_id",
            &self.source_block_id,
        )?;
        hash_hex("transition reference digest", &self.transition_digest)?;
        hash_hex(
            "transition reference old set hash",
            &self.old_validator_set_hash,
        )?;
        hash_hex(
            "transition reference new set hash",
            &self.new_validator_set_hash,
        )?;
        let scheduled_successor = self
            .scheduled_height
            .checked_add(2)
            .ok_or_else(|| "BFT V2 transition height overflow".to_string())?;
        if self.activation_height != scheduled_successor || self.scheduled_height == 0 {
            return Err("BFT V2 transition reference height mismatch".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftBlockV2 {
    pub protocol_version: u16,
    pub scope: BftScopeV2,
    pub height: u64,
    pub parent_commit_ref: String,
    pub authority_epoch: u64,
    pub active_validator_set_hash: String,
    pub schedule_policy_hash: String,
    pub manifest_hash: String,
    /// Manifest identity after execution; `manifest_hash` binds the parent.
    pub post_manifest_hash: String,
    pub actions_cbor: Vec<u8>,
    pub action_root: String,
    pub event_root: String,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub state_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<BftValidatorTransitionV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_ref: Option<BftValidatorTransitionRefV2>,
}

impl BftBlockV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol_version != BFT_PROTOCOL_VERSION_V2 || self.height == 0 {
            return Err("unsupported BFT V2 block version or height".into());
        }
        self.scope.validate()?;
        for (name, value) in [
            ("parent_commit_ref", self.parent_commit_ref.as_str()),
            (
                "active_validator_set_hash",
                self.active_validator_set_hash.as_str(),
            ),
            ("schedule_policy_hash", self.schedule_policy_hash.as_str()),
            ("manifest_hash", self.manifest_hash.as_str()),
            ("post_manifest_hash", self.post_manifest_hash.as_str()),
            ("action_root", self.action_root.as_str()),
            ("event_root", self.event_root.as_str()),
            ("execution_block_hash", self.execution_block_hash.as_str()),
            ("execution_state_root", self.execution_state_root.as_str()),
            ("state_root", self.state_root.as_str()),
        ] {
            hash_hex(name, value)?;
        }
        if self.actions_cbor.is_empty() || self.actions_cbor.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V2 {
            return Err("BFT V2 actions exceed protocol bound".into());
        }
        if self.transition.is_some() && self.transition_ref.is_some() {
            return Err("BFT V2 block cannot schedule and reference a transition together".into());
        }
        if let Some(transition) = &self.transition {
            transition.validate()?;
            if transition.scheduled_height != self.height
                || transition.source_commit_ref != self.parent_commit_ref
                || transition.old_validator_set_hash != self.active_validator_set_hash
            {
                return Err(
                    "BFT V2 candidate transition does not bind its parent and active set".into(),
                );
            }
        }
        if let Some(transition_ref) = &self.transition_ref {
            transition_ref.validate()?;
            let expected_height = transition_ref
                .scheduled_height
                .checked_add(1)
                .ok_or_else(|| "BFT V2 transition height overflow".to_string())?;
            if self.height != expected_height
                || transition_ref.source_block_id != self.parent_commit_ref
                || transition_ref.old_validator_set_hash != self.active_validator_set_hash
            {
                return Err("BFT V2 transition reference does not bind the h+1 block".into());
            }
        }
        Ok(())
    }

    pub fn block_id(&self) -> Result<String, String> {
        self.validate()?;
        let bytes = domain_framed(BFT_BLOCK_ID_DOMAIN_V2, self)?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BftVotePhaseV2 {
    Prevote,
    Precommit,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BftVoteValueV2 {
    Nil,
    Block { block_id: String },
}

impl BftVoteValueV2 {
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Block { block_id } = self {
            hash_hex("BFT V2 vote block_id", block_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftVoteV2 {
    pub protocol_version: u16,
    pub scope: BftScopeV2,
    pub height: u64,
    pub round: u64,
    pub phase: BftVotePhaseV2,
    pub active_validator_set_hash: String,
    pub schedule_policy_hash: String,
    pub value: BftVoteValueV2,
    pub validator_id: String,
    pub public_key_hex: String,
    pub signature_hex: String,
}

impl BftVoteV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        scope: BftScopeV2,
        height: u64,
        round: u64,
        phase: BftVotePhaseV2,
        active_validator_set_hash: String,
        schedule_policy_hash: String,
        value: BftVoteValueV2,
        validator_id: String,
        key: &SigningKey,
    ) -> Result<Self, String> {
        let mut vote = Self {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope,
            height,
            round,
            phase,
            active_validator_set_hash,
            schedule_policy_hash,
            value,
            validator_id,
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            signature_hex: String::new(),
        };
        vote.validate_unsigned()?;
        vote.signature_hex = hex::encode(key.sign(&vote.signing_payload()?).to_bytes());
        Ok(vote)
    }

    pub fn verify_signature(&self) -> Result<(), String> {
        self.validate_unsigned()?;
        let key = verifying_key(&self.public_key_hex)?;
        key.verify_strict(&self.signing_payload()?, &signature(&self.signature_hex)?)
            .map_err(|_| "BFT V2 vote signature verification failed".into())
    }

    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        self.validate_unsigned()?;
        domain_framed(
            BFT_VOTE_SIGNING_DOMAIN_V2,
            &VoteSigningPayloadV2 {
                protocol_version: self.protocol_version,
                scope: &self.scope,
                height: self.height,
                round: self.round,
                phase: self.phase,
                active_validator_set_hash: &self.active_validator_set_hash,
                schedule_policy_hash: &self.schedule_policy_hash,
                value: &self.value,
                validator_id: &self.validator_id,
                public_key_hex: &self.public_key_hex,
            },
        )
    }

    fn validate_unsigned(&self) -> Result<(), String> {
        if self.protocol_version != BFT_PROTOCOL_VERSION_V2 || self.height == 0 {
            return Err("unsupported BFT V2 vote version or height".into());
        }
        self.scope.validate()?;
        bounded_non_empty(
            "validator_id",
            &self.validator_id,
            BFT_MAX_IDENTITY_TEXT_BYTES_V1,
        )?;
        hash_hex("active_validator_set_hash", &self.active_validator_set_hash)?;
        hash_hex("schedule_policy_hash", &self.schedule_policy_hash)?;
        self.value.validate()?;
        public_key(&self.public_key_hex)?;
        Ok(())
    }
}

#[derive(Serialize)]
struct VoteSigningPayloadV2<'a> {
    protocol_version: u16,
    scope: &'a BftScopeV2,
    height: u64,
    round: u64,
    phase: BftVotePhaseV2,
    active_validator_set_hash: &'a str,
    schedule_policy_hash: &'a str,
    value: &'a BftVoteValueV2,
    validator_id: &'a str,
    public_key_hex: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftProposalHeaderV2 {
    pub protocol_version: u16,
    pub scope: BftScopeV2,
    pub height: u64,
    pub round: u64,
    pub active_validator_set_hash: String,
    pub schedule_policy_hash: String,
    pub proposer_id: String,
    pub block_id: String,
    pub valid_round: Option<u64>,
    pub public_key_hex: String,
    pub signature_hex: String,
}

impl BftProposalHeaderV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        scope: BftScopeV2,
        height: u64,
        round: u64,
        active_validator_set_hash: String,
        schedule_policy_hash: String,
        proposer_id: String,
        block_id: String,
        valid_round: Option<u64>,
        key: &SigningKey,
    ) -> Result<Self, String> {
        let mut header = Self {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope,
            height,
            round,
            active_validator_set_hash,
            schedule_policy_hash,
            proposer_id,
            block_id,
            valid_round,
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            signature_hex: String::new(),
        };
        header.validate_unsigned()?;
        header.signature_hex = hex::encode(key.sign(&header.signing_payload()?).to_bytes());
        Ok(header)
    }

    pub fn verify_signature(&self) -> Result<(), String> {
        self.validate_unsigned()?;
        verifying_key(&self.public_key_hex)?
            .verify_strict(&self.signing_payload()?, &signature(&self.signature_hex)?)
            .map_err(|_| "BFT V2 proposal signature verification failed".into())
    }

    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        self.validate_unsigned()?;
        domain_framed(
            BFT_PROPOSAL_SIGNING_DOMAIN_V2,
            &ProposalSigningPayloadV2 {
                protocol_version: self.protocol_version,
                scope: &self.scope,
                height: self.height,
                round: self.round,
                active_validator_set_hash: &self.active_validator_set_hash,
                schedule_policy_hash: &self.schedule_policy_hash,
                proposer_id: &self.proposer_id,
                block_id: &self.block_id,
                valid_round: self.valid_round,
                public_key_hex: &self.public_key_hex,
            },
        )
    }

    fn validate_unsigned(&self) -> Result<(), String> {
        if self.protocol_version != BFT_PROTOCOL_VERSION_V2 || self.height == 0 {
            return Err("unsupported BFT V2 proposal version or height".into());
        }
        self.scope.validate()?;
        hash_hex("active_validator_set_hash", &self.active_validator_set_hash)?;
        hash_hex("schedule_policy_hash", &self.schedule_policy_hash)?;
        bounded_non_empty(
            "proposer_id",
            &self.proposer_id,
            BFT_MAX_IDENTITY_TEXT_BYTES_V1,
        )?;
        hash_hex("proposal block_id", &self.block_id)?;
        if self.valid_round.is_some_and(|valid| valid >= self.round) {
            return Err("BFT V2 valid_round must be less than proposal round".into());
        }
        public_key(&self.public_key_hex)?;
        Ok(())
    }
}

#[derive(Serialize)]
struct ProposalSigningPayloadV2<'a> {
    protocol_version: u16,
    scope: &'a BftScopeV2,
    height: u64,
    round: u64,
    active_validator_set_hash: &'a str,
    schedule_policy_hash: &'a str,
    proposer_id: &'a str,
    block_id: &'a str,
    valid_round: Option<u64>,
    public_key_hex: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftValidRoundEvidenceV2 {
    pub proposal: BftProposalHeaderV2,
    pub prevotes: Vec<BftVoteV2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftProposalV2 {
    pub header: BftProposalHeaderV2,
    pub block: BftBlockV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_round_evidence: Option<BftValidRoundEvidenceV2>,
}

impl BftProposalV2 {
    pub fn validate_binding(&self) -> Result<(), String> {
        self.block.validate()?;
        if self.block.scope != self.header.scope
            || self.block.height != self.header.height
            || self.block.active_validator_set_hash != self.header.active_validator_set_hash
            || self.block.schedule_policy_hash != self.header.schedule_policy_hash
        {
            return Err("BFT V2 proposal block/header binding mismatch".into());
        }
        if self.block.block_id()? != self.header.block_id {
            return Err("BFT V2 proposal block ID mismatch".into());
        }
        match (self.header.valid_round, self.valid_round_evidence.as_ref()) {
            (None, None) => Ok(()),
            (Some(valid_round), Some(evidence)) if evidence.proposal.round == valid_round => {
                if evidence.prevotes.len() > BFT_MAX_VALIDATORS_V2 {
                    return Err("BFT V2 valid-round evidence exceeds validator bound".into());
                }
                Ok(())
            }
            (Some(_), _) => Err("BFT V2 proposal is missing matching valid-round evidence".into()),
            (None, Some(_)) => Err("BFT V2 proposal has unexpected valid-round evidence".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCommitCertificateV2 {
    pub proposal: Box<BftProposalV2>,
    pub precommits: Vec<BftVoteV2>,
}

impl BftCommitCertificateV2 {
    pub fn validate_wire_bounds(&self) -> Result<(), String> {
        self.proposal.validate_binding()?;
        if self.precommits.len() > BFT_MAX_VALIDATORS_V2 {
            return Err("BFT V2 commit certificate exceeds validator bound".into());
        }
        let encoded = serde_cbor::to_vec(self)
            .map_err(|error| format!("encode BFT V2 commit certificate: {error}"))?;
        if encoded.len() > BFT_MAX_SIGNED_MATERIAL_BYTES_V2 {
            return Err("BFT V2 commit certificate exceeds wire bound".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftHistoryRequestV2 {
    pub scope: BftScopeV2,
    pub active_validator_set_hash: String,
    pub schedule_policy_hash: String,
    pub after_height: u64,
    pub after_block_id: String,
}

impl BftHistoryRequestV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.scope.validate()?;
        hash_hex("history active set hash", &self.active_validator_set_hash)?;
        hash_hex("history schedule policy hash", &self.schedule_policy_hash)?;
        hash_hex("history after_block_id", &self.after_block_id)?;
        if self.after_height == 0 && self.after_block_id != self.scope.genesis_hash {
            return Err("BFT V2 history genesis cursor mismatch".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "message",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum BftMessageV2 {
    Proposal(Box<BftProposalV2>),
    Vote(BftVoteV2),
    Commit(BftCommitCertificateV2),
    HistoryRequest(BftHistoryRequestV2),
}

fn bounded_non_empty(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max {
        return Err(format!("BFT V2 {label} is empty or exceeds {max} bytes"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> BftScopeV2 {
        BftScopeV2 {
            world_id: "world-v2".into(),
            chain_id: "chain-v2".into(),
            genesis_hash: "11".repeat(32),
        }
    }

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn validator(seed: u8) -> BftValidatorV2 {
        BftValidatorV2 {
            validator_id: format!("validator-{seed}"),
            public_key_hex: hex::encode(key(seed).verifying_key().to_bytes()),
            voting_power: u64::from(seed),
        }
    }

    fn transition() -> BftValidatorTransitionV2 {
        BftValidatorTransitionV2 {
            source_commit_ref: "22".repeat(32),
            source_state_root: "33".repeat(32),
            source_profile: "controlled-bft".into(),
            source_epoch: 1,
            target_epoch: 3,
            epoch_length_ticks: 4,
            mapping_ruleset_hash: "44".repeat(32),
            scheduled_height: 10,
            activation_height: 12,
            old_validator_set_hash: "55".repeat(32),
            new_validators: vec![validator(8), validator(9)],
            new_validator_set_hash: "66".repeat(32),
            governance_action_digest: "77".repeat(32),
            projected_registry_digest: "88".repeat(32),
        }
    }

    fn block(height: u64) -> BftBlockV2 {
        BftBlockV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope: scope(),
            height,
            parent_commit_ref: if height == 10 {
                "22".repeat(32)
            } else {
                "99".repeat(32)
            },
            authority_epoch: 3,
            active_validator_set_hash: "55".repeat(32),
            schedule_policy_hash: "aa".repeat(32),
            manifest_hash: "bb".repeat(32),
            post_manifest_hash: "bc".repeat(32),
            actions_cbor: vec![0x80],
            action_root: "cc".repeat(32),
            event_root: "dd".repeat(32),
            execution_block_hash: "ee".repeat(32),
            execution_state_root: "ff".repeat(32),
            state_root: "ab".repeat(32),
            transition: None,
            transition_ref: None,
        }
    }

    #[test]
    fn v2_transition_maps_epoch_to_exact_positive_h_and_never_self_references_qc() {
        let mut block = block(10);
        let mut scheduled = transition();
        block.transition = Some(scheduled.clone());
        block
            .validate()
            .expect("h candidate binds parent and old set");
        let block_id_before_qc = block.block_id().expect("candidate block id");
        let reference = scheduled
            .committed_reference(block_id_before_qc.clone())
            .expect("QC-derived reference");
        assert_eq!(reference.source_block_id, block_id_before_qc);
        assert_eq!(reference.scheduled_height, 10);
        assert_eq!(reference.activation_height, 12);

        scheduled.source_commit_ref = "99".repeat(32);
        assert!(scheduled.validate().is_ok());
        block.transition = Some(scheduled);
        assert!(
            block.validate().is_err(),
            "transition must name the parent CRef"
        );

        let mut invalid = super::tests::block(0);
        invalid.transition = Some(BftValidatorTransitionV2 {
            scheduled_height: 0,
            activation_height: 2,
            target_epoch: 1,
            ..transition()
        });
        assert!(invalid.validate().is_err(), "genesis h=0 is unsupported");
    }

    #[test]
    fn v2_transition_reference_is_exactly_the_h_plus_one_old_set_proof() {
        let transition = transition();
        let reference = transition
            .committed_reference("99".repeat(32))
            .expect("reference");
        let mut h_plus_one = block(11);
        h_plus_one.parent_commit_ref = reference.source_block_id.clone();
        h_plus_one.transition_ref = Some(reference.clone());
        h_plus_one.validate().expect("h+1 still uses old set");

        let mut wrong_set = h_plus_one.clone();
        wrong_set.active_validator_set_hash = "66".repeat(32);
        assert!(wrong_set.validate().is_err());
        let mut wrong_parent = h_plus_one;
        wrong_parent.parent_commit_ref = "aa".repeat(32);
        assert!(wrong_parent.validate().is_err());
    }

    #[test]
    fn v2_votes_use_a_distinct_domain_and_bind_set_and_mapping_policy() {
        let signing_key = key(7);
        let vote = BftVoteV2::sign(
            scope(),
            12,
            0,
            BftVotePhaseV2::Prevote,
            "55".repeat(32),
            "aa".repeat(32),
            BftVoteValueV2::Block {
                block_id: "99".repeat(32),
            },
            "validator-7".into(),
            &signing_key,
        )
        .expect("signed V2 vote");
        vote.verify_signature().expect("valid V2 signature");
        let mut tampered = vote;
        tampered.active_validator_set_hash = "66".repeat(32);
        assert!(tampered.verify_signature().is_err());
    }

    #[test]
    fn v2_wire_rejects_unknown_protocol_versions_and_cross_version_payloads() {
        let mut value = serde_json::to_value(
            BftVoteV2::sign(
                scope(),
                1,
                0,
                BftVotePhaseV2::Precommit,
                "55".repeat(32),
                "aa".repeat(32),
                BftVoteValueV2::Nil,
                "validator-7".into(),
                &key(7),
            )
            .unwrap(),
        )
        .unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("protocol_version".into(), serde_json::json!(1));
        let decoded = serde_json::from_value::<BftVoteV2>(value.clone())
            .expect("same-shape message can be parsed before version validation");
        assert!(decoded.verify_signature().is_err());

        let mut v1_shape = value;
        v1_shape
            .as_object_mut()
            .unwrap()
            .remove("schedule_policy_hash");
        assert!(serde_json::from_value::<BftVoteV2>(v1_shape).is_err());
    }
}
