//! Versioned Tendermint-style consensus messages.
//!
//! This domain is intentionally separate from the legacy proposal,
//! attestation, and commit wire types. A BFT certificate cannot be represented
//! as a legacy commit message.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const BFT_PROTOCOL_VERSION_V1: u16 = 1;
pub const BFT_PROPOSAL_SIGNING_DOMAIN_V1: &str = "oasis7.bft.proposal.v1.sign";
pub const BFT_VOTE_SIGNING_DOMAIN_V1: &str = "oasis7.bft.vote.v1.sign";
pub const BFT_BLOCK_ID_DOMAIN_V1: &str = "oasis7.bft.block_id.v1.hash";
pub const BFT_MAX_ACTIONS_CBOR_BYTES_V1: usize = 1_048_576;
pub const BFT_MAX_SIGNED_MATERIAL_BYTES_V1: usize = 2_097_152;
pub const BFT_MAX_IDENTITY_TEXT_BYTES_V1: usize = 256;
pub const BFT_MAX_VALIDATORS_V1: usize = 256;
pub const BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1: usize = 4096;
pub const BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1: usize = 256 * 1024 * 1024;
pub const BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftScopeV1 {
    pub world_id: String,
    pub chain_id: String,
    pub genesis_hash: String,
}

impl BftScopeV1 {
    pub fn validate(&self) -> Result<(), String> {
        bounded_non_empty("world_id", &self.world_id, BFT_MAX_IDENTITY_TEXT_BYTES_V1)?;
        bounded_non_empty("chain_id", &self.chain_id, BFT_MAX_IDENTITY_TEXT_BYTES_V1)?;
        hash_hex("genesis_hash", &self.genesis_hash)
    }
}

/// The value ID covers the exact parent, manifest, actions and execution roots.
/// Round and proposal signatures deliberately do not affect this identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftBlockV1 {
    pub scope: BftScopeV1,
    pub height: u64,
    pub parent_commit_ref: String,
    pub authority_epoch: u64,
    pub manifest_hash: String,
    pub actions_cbor: Vec<u8>,
    pub action_root: String,
    pub event_root: String,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub state_root: String,
}

impl BftBlockV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.scope.validate()?;
        if self.height == 0 {
            return Err("BFT block height must be positive".into());
        }
        if self.actions_cbor.is_empty() || self.actions_cbor.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V1 {
            return Err(format!(
                "BFT actions encoding must be nonempty and at most {BFT_MAX_ACTIONS_CBOR_BYTES_V1} bytes"
            ));
        }
        for (name, value) in [
            ("parent_commit_ref", self.parent_commit_ref.as_str()),
            ("manifest_hash", self.manifest_hash.as_str()),
            ("action_root", self.action_root.as_str()),
            ("event_root", self.event_root.as_str()),
            ("execution_block_hash", self.execution_block_hash.as_str()),
            ("execution_state_root", self.execution_state_root.as_str()),
            ("state_root", self.state_root.as_str()),
        ] {
            hash_hex(name, value)?;
        }
        Ok(())
    }

    pub fn block_id(&self) -> Result<String, String> {
        self.validate()?;
        let bytes = domain_framed(
            BFT_BLOCK_ID_DOMAIN_V1,
            &BlockIdPayloadV1 {
                protocol_version: BFT_PROTOCOL_VERSION_V1,
                scope: &self.scope,
                height: self.height,
                parent_commit_ref: &self.parent_commit_ref,
                authority_epoch: self.authority_epoch,
                manifest_hash: &self.manifest_hash,
                actions_cbor: &self.actions_cbor,
                action_root: &self.action_root,
                event_root: &self.event_root,
                execution_block_hash: &self.execution_block_hash,
                execution_state_root: &self.execution_state_root,
                state_root: &self.state_root,
            },
        )?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

#[derive(Serialize)]
struct BlockIdPayloadV1<'a> {
    protocol_version: u16,
    scope: &'a BftScopeV1,
    height: u64,
    parent_commit_ref: &'a str,
    authority_epoch: u64,
    manifest_hash: &'a str,
    actions_cbor: &'a [u8],
    action_root: &'a str,
    event_root: &'a str,
    execution_block_hash: &'a str,
    execution_state_root: &'a str,
    state_root: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BftVotePhaseV1 {
    Prevote,
    Precommit,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BftVoteValueV1 {
    Nil,
    Block { block_id: String },
}

impl BftVoteValueV1 {
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Block { block_id } = self {
            hash_hex("vote block_id", block_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftVoteV1 {
    pub protocol_version: u16,
    pub scope: BftScopeV1,
    pub height: u64,
    pub round: u64,
    pub phase: BftVotePhaseV1,
    pub validator_set_hash: String,
    pub value: BftVoteValueV1,
    pub validator_id: String,
    pub public_key_hex: String,
    pub signature_hex: String,
}

impl BftVoteV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        scope: BftScopeV1,
        height: u64,
        round: u64,
        phase: BftVotePhaseV1,
        validator_set_hash: String,
        value: BftVoteValueV1,
        validator_id: String,
        key: &SigningKey,
    ) -> Result<Self, String> {
        let public_key_hex = hex::encode(key.verifying_key().to_bytes());
        let mut vote = Self {
            protocol_version: BFT_PROTOCOL_VERSION_V1,
            scope,
            height,
            round,
            phase,
            validator_set_hash,
            value,
            validator_id,
            public_key_hex,
            signature_hex: String::new(),
        };
        vote.validate_unsigned()?;
        vote.signature_hex = hex::encode(key.sign(&vote.signing_payload()?).to_bytes());
        Ok(vote)
    }

    pub fn verify_signature(&self) -> Result<(), String> {
        self.validate_unsigned()?;
        let key = verifying_key(&self.public_key_hex)?;
        let signature = signature(&self.signature_hex)?;
        key.verify_strict(&self.signing_payload()?, &signature)
            .map_err(|_| "BFT vote signature verification failed".to_string())
    }

    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        self.validate_unsigned()?;
        domain_framed(
            BFT_VOTE_SIGNING_DOMAIN_V1,
            &VoteSigningPayloadV1 {
                protocol_version: self.protocol_version,
                scope: &self.scope,
                height: self.height,
                round: self.round,
                phase: self.phase,
                validator_set_hash: &self.validator_set_hash,
                value: &self.value,
                validator_id: &self.validator_id,
                public_key_hex: &self.public_key_hex,
            },
        )
    }

    fn validate_unsigned(&self) -> Result<(), String> {
        if self.protocol_version != BFT_PROTOCOL_VERSION_V1 || self.height == 0 {
            return Err("unsupported BFT vote version or height".into());
        }
        self.scope.validate()?;
        bounded_non_empty(
            "validator_id",
            &self.validator_id,
            BFT_MAX_IDENTITY_TEXT_BYTES_V1,
        )?;
        hash_hex("validator_set_hash", &self.validator_set_hash)?;
        self.value.validate()?;
        public_key(&self.public_key_hex)?;
        Ok(())
    }
}

#[derive(Serialize)]
struct VoteSigningPayloadV1<'a> {
    protocol_version: u16,
    scope: &'a BftScopeV1,
    height: u64,
    round: u64,
    phase: BftVotePhaseV1,
    validator_set_hash: &'a str,
    value: &'a BftVoteValueV1,
    validator_id: &'a str,
    public_key_hex: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftProposalHeaderV1 {
    pub protocol_version: u16,
    pub scope: BftScopeV1,
    pub height: u64,
    pub round: u64,
    pub validator_set_hash: String,
    pub proposer_id: String,
    pub block_id: String,
    pub valid_round: Option<u64>,
    pub public_key_hex: String,
    pub signature_hex: String,
}

impl BftProposalHeaderV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        scope: BftScopeV1,
        height: u64,
        round: u64,
        validator_set_hash: String,
        proposer_id: String,
        block_id: String,
        valid_round: Option<u64>,
        key: &SigningKey,
    ) -> Result<Self, String> {
        let mut header = Self {
            protocol_version: BFT_PROTOCOL_VERSION_V1,
            scope,
            height,
            round,
            validator_set_hash,
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
        let key = verifying_key(&self.public_key_hex)?;
        let signature = signature(&self.signature_hex)?;
        key.verify_strict(&self.signing_payload()?, &signature)
            .map_err(|_| "BFT proposal signature verification failed".to_string())
    }

    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        self.validate_unsigned()?;
        domain_framed(
            BFT_PROPOSAL_SIGNING_DOMAIN_V1,
            &ProposalSigningPayloadV1 {
                protocol_version: self.protocol_version,
                scope: &self.scope,
                height: self.height,
                round: self.round,
                validator_set_hash: &self.validator_set_hash,
                proposer_id: &self.proposer_id,
                block_id: &self.block_id,
                valid_round: self.valid_round,
                public_key_hex: &self.public_key_hex,
            },
        )
    }

    fn validate_unsigned(&self) -> Result<(), String> {
        if self.protocol_version != BFT_PROTOCOL_VERSION_V1 || self.height == 0 {
            return Err("unsupported BFT proposal version or height".into());
        }
        self.scope.validate()?;
        hash_hex("validator_set_hash", &self.validator_set_hash)?;
        hash_hex("proposal block_id", &self.block_id)?;
        bounded_non_empty(
            "proposer_id",
            &self.proposer_id,
            BFT_MAX_IDENTITY_TEXT_BYTES_V1,
        )?;
        if self.valid_round.is_some_and(|valid| valid >= self.round) {
            return Err("BFT proposal valid_round must be less than proposal round".into());
        }
        public_key(&self.public_key_hex)?;
        Ok(())
    }
}

#[derive(Serialize)]
struct ProposalSigningPayloadV1<'a> {
    protocol_version: u16,
    scope: &'a BftScopeV1,
    height: u64,
    round: u64,
    validator_set_hash: &'a str,
    proposer_id: &'a str,
    block_id: &'a str,
    valid_round: Option<u64>,
    public_key_hex: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftValidRoundEvidenceV1 {
    pub proposal: BftProposalHeaderV1,
    pub prevotes: Vec<BftVoteV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftProposalV1 {
    pub header: BftProposalHeaderV1,
    pub block: BftBlockV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_round_evidence: Option<BftValidRoundEvidenceV1>,
}

impl BftProposalV1 {
    pub fn validate_binding(&self) -> Result<(), String> {
        self.block.validate()?;
        if self.block.scope != self.header.scope || self.block.height != self.header.height {
            return Err("BFT proposal block scope/height mismatch".into());
        }
        if self.block.block_id()? != self.header.block_id {
            return Err("BFT proposal block ID mismatch".into());
        }
        match (self.header.valid_round, self.valid_round_evidence.as_ref()) {
            (None, None) => Ok(()),
            (Some(valid_round), Some(proof)) if proof.proposal.round == valid_round => {
                if proof.prevotes.len() > BFT_MAX_VALIDATORS_V1 {
                    return Err("BFT valid-round evidence exceeds validator bound".into());
                }
                Ok(())
            }
            (Some(_), _) => Err("BFT proposal is missing matching valid-round evidence".into()),
            (None, Some(_)) => Err("BFT proposal has unexpected valid-round evidence".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCommitCertificateV1 {
    pub proposal: Box<BftProposalV1>,
    pub precommits: Vec<BftVoteV1>,
}

/// Unauthenticated bounded hint asking a peer for material after a cursor.
/// It grants no authority: responders must independently match the exact
/// cursor against their verified local history before returning one QC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftHistoryRequestV1 {
    pub scope: BftScopeV1,
    pub validator_set_hash: String,
    pub after_height: u64,
    pub after_block_id: String,
}

impl BftHistoryRequestV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.scope.validate()?;
        hash_hex(
            "history request validator_set_hash",
            &self.validator_set_hash,
        )?;
        hash_hex("history request after_block_id", &self.after_block_id)?;
        if self.after_height == 0 && self.after_block_id != self.scope.genesis_hash {
            return Err(
                "BFT history request genesis cursor must match the configured genesis hash".into(),
            );
        }
        Ok(())
    }
}

impl BftCommitCertificateV1 {
    pub fn validate_wire_bounds(&self) -> Result<(), String> {
        self.proposal.validate_binding()?;
        if self.precommits.len() > BFT_MAX_VALIDATORS_V1 {
            return Err("BFT commit certificate exceeds validator bound".into());
        }
        Ok(())
    }
}

/// New-format engineering consensus messages are carried in their own gossip
/// variant. They are never decoded as legacy PoS proposal/commit messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "message",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum BftMessageV1 {
    Proposal(Box<BftProposalV1>),
    Vote(BftVoteV1),
    Commit(BftCommitCertificateV1),
    HistoryRequest(BftHistoryRequestV1),
}

fn non_empty(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("BFT {label} cannot be empty"));
    }
    Ok(())
}

fn bounded_non_empty(label: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    non_empty(label, value)?;
    if value.len() > max_bytes {
        return Err(format!("BFT {label} exceeds {max_bytes} bytes"));
    }
    Ok(())
}

fn hash_hex(label: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("BFT {label} must be 32-byte hex"));
    }
    Ok(())
}

fn public_key(value: &str) -> Result<VerifyingKey, String> {
    if value.len() != 64 {
        return Err("BFT public key must be 32 bytes".into());
    }
    let bytes = hex::decode(value).map_err(|_| "BFT public key is not hex".to_string())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "BFT public key must be 32 bytes".to_string())?;
    if hex::encode(bytes) != value {
        return Err("BFT public key must use canonical lowercase hex".into());
    }
    let key =
        VerifyingKey::from_bytes(&bytes).map_err(|_| "BFT public key is invalid".to_string())?;
    if key.is_weak() {
        return Err("BFT weak Ed25519 public key".into());
    }
    Ok(key)
}

fn verifying_key(value: &str) -> Result<VerifyingKey, String> {
    public_key(value)
}

fn signature(value: &str) -> Result<Signature, String> {
    if value.len() != 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("BFT signature must be canonical lowercase hex64".into());
    }
    let bytes = hex::decode(value).map_err(|_| "BFT signature is not hex".to_string())?;
    Signature::from_slice(&bytes).map_err(|_| "BFT signature must be 64 bytes".to_string())
}

fn domain_framed<T: Serialize>(domain: &str, value: &T) -> Result<Vec<u8>, String> {
    let encoded =
        serde_cbor::to_vec(value).map_err(|err| format!("encode BFT domain payload: {err}"))?;
    let domain_len = u64::try_from(domain.len())
        .map_err(|_| "BFT domain length exceeds frame format".to_string())?;
    let total = 8usize
        .checked_add(domain.len())
        .and_then(|size| size.checked_add(encoded.len()))
        .ok_or_else(|| "BFT signed material length overflow".to_string())?;
    if total > BFT_MAX_SIGNED_MATERIAL_BYTES_V1 {
        return Err("BFT signed material exceeds protocol bound".into());
    }
    let mut framed = Vec::with_capacity(total);
    framed.extend_from_slice(&domain_len.to_be_bytes());
    framed.extend_from_slice(domain.as_bytes());
    framed.extend_from_slice(encoded.as_slice());
    Ok(framed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> BftScopeV1 {
        BftScopeV1 {
            world_id: "world-a".into(),
            chain_id: "chain-a".into(),
            genesis_hash: "11".repeat(32),
        }
    }

    fn signed_vote() -> BftVoteV1 {
        BftVoteV1::sign(
            scope(),
            1,
            0,
            BftVotePhaseV1::Prevote,
            "22".repeat(32),
            BftVoteValueV1::Nil,
            "validator-a".into(),
            &SigningKey::from_bytes(&[7; 32]),
        )
        .expect("signed vote")
    }

    #[test]
    fn domain_frame_has_stable_length_prefix_and_cbor_preimage() {
        let domain = "oasis7.bft.test.v1.golden";
        let framed = domain_framed(domain, &"x").expect("bounded frame");
        let mut expected = vec![0, 0, 0, 0, 0, 0, 0, 25];
        expected.extend_from_slice(domain.as_bytes());
        expected.extend_from_slice(&[0x61, b'x']);
        assert_eq!(framed, expected);

        let vote_payload = domain_framed(BFT_VOTE_SIGNING_DOMAIN_V1, &"x").unwrap();
        let proposal_payload = domain_framed(BFT_PROPOSAL_SIGNING_DOMAIN_V1, &"x").unwrap();
        assert_ne!(vote_payload, proposal_payload);
        assert_ne!(BFT_VOTE_SIGNING_DOMAIN_V1, BFT_BLOCK_ID_DOMAIN_V1);
    }

    #[test]
    fn strict_signature_rejects_phase_domain_mutation_and_noncanonical_hex() {
        let vote = signed_vote();
        vote.verify_signature().expect("original vote verifies");

        let mut wrong_phase = vote.clone();
        wrong_phase.phase = BftVotePhaseV1::Precommit;
        assert!(wrong_phase.verify_signature().is_err());

        let mut uppercase_key = vote.clone();
        uppercase_key.public_key_hex.make_ascii_uppercase();
        assert!(
            uppercase_key
                .verify_signature()
                .expect_err("uppercase key is noncanonical")
                .contains("canonical lowercase")
        );

        let mut uppercase_signature = vote;
        uppercase_signature.signature_hex.make_ascii_uppercase();
        assert!(
            uppercase_signature
                .verify_signature()
                .expect_err("uppercase signature is noncanonical")
                .contains("canonical lowercase")
        );
    }

    #[test]
    fn malformed_and_weak_ed25519_keys_are_rejected() {
        assert!(public_key("not-hex").is_err());
        assert!(
            public_key(&hex::encode([0u8; 32]))
                .expect_err("small-order key is rejected")
                .contains("weak")
        );
        assert!(public_key(&"00".repeat(31)).is_err());
    }

    #[test]
    fn signature_domains_and_wire_phase_are_not_interchangeable() {
        let vote = signed_vote();
        let proposal_domain = domain_framed(
            BFT_PROPOSAL_SIGNING_DOMAIN_V1,
            &VoteSigningPayloadV1 {
                protocol_version: vote.protocol_version,
                scope: &vote.scope,
                height: vote.height,
                round: vote.round,
                phase: vote.phase,
                validator_set_hash: &vote.validator_set_hash,
                value: &vote.value,
                validator_id: &vote.validator_id,
                public_key_hex: &vote.public_key_hex,
            },
        )
        .expect("bounded proposal-domain frame");
        assert_ne!(vote.signing_payload().unwrap(), proposal_domain);

        let block_domain = domain_framed(
            BFT_BLOCK_ID_DOMAIN_V1,
            &VoteSigningPayloadV1 {
                protocol_version: vote.protocol_version,
                scope: &vote.scope,
                height: vote.height,
                round: vote.round,
                phase: vote.phase,
                validator_set_hash: &vote.validator_set_hash,
                value: &vote.value,
                validator_id: &vote.validator_id,
                public_key_hex: &vote.public_key_hex,
            },
        )
        .expect("bounded block-domain frame");
        assert_ne!(vote.signing_payload().unwrap(), block_domain);
    }

    #[test]
    fn wire_structs_reject_unknown_fields() {
        let mut value = serde_json::to_value(signed_vote()).expect("serialize vote");
        value
            .as_object_mut()
            .expect("vote object")
            .insert("unexpected".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<BftVoteV1>(value).is_err());
    }

    #[test]
    fn history_request_is_a_bounded_non_authorizing_cursor_hint() {
        let request = BftHistoryRequestV1 {
            scope: scope(),
            validator_set_hash: "22".repeat(32),
            after_height: 0,
            after_block_id: "11".repeat(32),
        };
        request.validate().expect("valid genesis cursor");
        let message = BftMessageV1::HistoryRequest(request.clone());
        let encoded = serde_json::to_vec(&message).expect("encode bounded request");
        assert!(encoded.len() < 1024);
        assert_eq!(
            serde_json::from_slice::<BftMessageV1>(encoded.as_slice()).expect("decode request"),
            message
        );

        let mut wrong_genesis_cursor = request.clone();
        wrong_genesis_cursor.after_block_id = "33".repeat(32);
        assert!(wrong_genesis_cursor.validate().is_err());
        let mut malformed_hash = request;
        malformed_hash.validator_set_hash = "not-a-hash".into();
        assert!(malformed_hash.validate().is_err());
    }
}
