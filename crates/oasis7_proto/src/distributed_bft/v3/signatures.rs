//! Independent V3 signatures over public header identities. No action payload,
//! execution preimage, protected-signer status or quorum authority is implied.
use super::super::v2::BftScopeV2;
use super::super::{
    BFT_MAX_IDENTITY_TEXT_BYTES_V1, bounded_non_empty, domain_framed, hash_hex, public_key,
    signature, verifying_key,
};
use super::BFT_PROTOCOL_VERSION_V3;
use ed25519_dalek::{Signer, SigningKey};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, SeqAccess, Visitor},
};
use std::{
    fmt,
    io::{self, Write},
};

pub const BFT_PROPOSAL_SIGNING_DOMAIN_V3: &str = "oasis7.bft.proposal.v3.sign";
pub const BFT_VOTE_SIGNING_DOMAIN_V3: &str = "oasis7.bft.vote.v3.sign";
pub const BFT_MAX_VALIDATORS_V3: usize = 256;
pub const BFT_MAX_SIGNED_MATERIAL_BYTES_V3: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BftVotePhaseV3 {
    Prevote,
    Precommit,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BftVoteValueV3 {
    Nil,
    Block { block_id: String },
}

impl BftVoteValueV3 {
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Block { block_id } = self {
            hash_hex("BFT V3 vote block_id", block_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftVoteV3 {
    pub protocol_version: u16,
    pub scope: BftScopeV2,
    pub height: u64,
    pub round: u64,
    pub phase: BftVotePhaseV3,
    pub active_validator_set_hash: String,
    pub schedule_policy_hash: String,
    pub value: BftVoteValueV3,
    pub validator_id: String,
    pub public_key_hex: String,
    pub signature_hex: String,
}

impl BftVoteV3 {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_unsigned()?;
        signature(&self.signature_hex)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        scope: BftScopeV2,
        height: u64,
        round: u64,
        phase: BftVotePhaseV3,
        active_validator_set_hash: String,
        schedule_policy_hash: String,
        value: BftVoteValueV3,
        validator_id: String,
        key: &SigningKey,
    ) -> Result<Self, String> {
        let mut vote = Self {
            protocol_version: BFT_PROTOCOL_VERSION_V3,
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
            .map_err(|_| "BFT V3 vote signature verification failed".into())
    }

    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        self.validate_unsigned()?;
        domain_framed(
            BFT_VOTE_SIGNING_DOMAIN_V3,
            &VoteSigningPayloadV3 {
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
        if self.protocol_version != BFT_PROTOCOL_VERSION_V3 || self.height == 0 {
            return Err("unsupported BFT V3 vote version or height".into());
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
struct VoteSigningPayloadV3<'a> {
    protocol_version: u16,
    scope: &'a BftScopeV2,
    height: u64,
    round: u64,
    phase: BftVotePhaseV3,
    active_validator_set_hash: &'a str,
    schedule_policy_hash: &'a str,
    value: &'a BftVoteValueV3,
    validator_id: &'a str,
    public_key_hex: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftProposalHeaderV3 {
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

impl BftProposalHeaderV3 {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_unsigned()?;
        signature(&self.signature_hex)?;
        Ok(())
    }

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
            protocol_version: BFT_PROTOCOL_VERSION_V3,
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
            .map_err(|_| "BFT V3 proposal signature verification failed".into())
    }

    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        self.validate_unsigned()?;
        domain_framed(
            BFT_PROPOSAL_SIGNING_DOMAIN_V3,
            &ProposalSigningPayloadV3 {
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
        if self.protocol_version != BFT_PROTOCOL_VERSION_V3 || self.height == 0 {
            return Err("unsupported BFT V3 proposal version or height".into());
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
            return Err("BFT V3 valid_round must be less than proposal round".into());
        }
        public_key(&self.public_key_hex)?;
        Ok(())
    }
}

#[derive(Serialize)]
struct ProposalSigningPayloadV3<'a> {
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
pub struct BftValidRoundEvidenceV3 {
    pub proposal: BftProposalHeaderV3,
    #[serde(deserialize_with = "bounded_votes")]
    pub prevotes: Vec<BftVoteV3>,
}

/// Public, header-only evidence. Binding is checked here; signer membership,
/// deterministic proposer selection and weighted quorum are formal verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftHeaderCommitCertificateV3 {
    pub proposal: BftProposalHeaderV3,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_round_evidence: Option<BftValidRoundEvidenceV3>,
    #[serde(deserialize_with = "bounded_votes")]
    pub precommits: Vec<BftVoteV3>,
}

impl BftHeaderCommitCertificateV3 {
    pub fn validate_binding(&self) -> Result<(), String> {
        self.proposal.validate()?;
        let header = &self.proposal;
        match (header.valid_round, &self.valid_round_evidence) {
            (None, None) => {}
            (Some(round), Some(evidence)) => {
                evidence.proposal.validate()?;
                let old = &evidence.proposal;
                if old.scope != header.scope
                    || old.height != header.height
                    || old.round != round
                    || old.block_id != header.block_id
                    || old.active_validator_set_hash != header.active_validator_set_hash
                    || old.schedule_policy_hash != header.schedule_policy_hash
                {
                    return Err("BFT V3 valid-round proposal context mismatch".into());
                }
                validate_votes(&evidence.prevotes, header, round, BftVotePhaseV3::Prevote)?;
            }
            _ => return Err("BFT V3 valid-round evidence presence mismatch".into()),
        }
        validate_votes(
            &self.precommits,
            header,
            header.round,
            BftVotePhaseV3::Precommit,
        )
    }

    pub fn validate_wire_bounds(&self) -> Result<(), String> {
        self.validate_binding()?;
        serde_cbor::to_writer(ByteBudget(BFT_MAX_SIGNED_MATERIAL_BYTES_V3), self)
            .map_err(|_| "BFT V3 certificate exceeds encoded bound".to_string())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_wire_bounds()?;
        serde_cbor::to_vec(self).map_err(|_| "encode BFT V3 certificate".into())
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > BFT_MAX_SIGNED_MATERIAL_BYTES_V3 {
            return Err("BFT V3 certificate exceeds encoded bound".into());
        }
        let value: Self = serde_cbor::from_slice(bytes)
            .map_err(|_| "decode bounded BFT V3 certificate".to_string())?;
        if value.canonical_bytes()? != bytes {
            return Err("noncanonical BFT V3 certificate".into());
        }
        Ok(value)
    }
}

fn validate_votes(
    votes: &[BftVoteV3],
    header: &BftProposalHeaderV3,
    round: u64,
    phase: BftVotePhaseV3,
) -> Result<(), String> {
    if votes.len() > BFT_MAX_VALIDATORS_V3 {
        return Err("BFT V3 vote count exceeds bound".into());
    }
    for vote in votes {
        vote.validate()?;
        if vote.scope != header.scope
            || vote.height != header.height
            || vote.round != round
            || vote.phase != phase
            || vote.active_validator_set_hash != header.active_validator_set_hash
            || vote.schedule_policy_hash != header.schedule_policy_hash
            || vote.value
                != (BftVoteValueV3::Block {
                    block_id: header.block_id.clone(),
                })
        {
            return Err("BFT V3 certificate vote context mismatch".into());
        }
    }
    Ok(())
}

fn bounded_votes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<BftVoteV3>, D::Error> {
    struct Votes;
    impl<'de> Visitor<'de> for Votes {
        type Value = Vec<BftVoteV3>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most 256 BFT V3 votes")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq
                .size_hint()
                .is_some_and(|size| size > BFT_MAX_VALIDATORS_V3)
            {
                return Err(de::Error::custom("BFT V3 vote count exceeds bound"));
            }
            let mut votes = Vec::new();
            while let Some(vote) = seq.next_element()? {
                if votes.len() == BFT_MAX_VALIDATORS_V3 {
                    return Err(de::Error::custom("BFT V3 vote count exceeds bound"));
                }
                votes.push(vote);
            }
            Ok(votes)
        }
    }
    deserializer.deserialize_seq(Votes)
}

struct ByteBudget(usize);
impl Write for ByteBudget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("BFT V3 byte bound"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> BftScopeV2 {
        BftScopeV2 {
            world_id: "v3-world".into(),
            chain_id: "v3-chain".into(),
            genesis_hash: "11".repeat(32),
        }
    }
    fn certificate() -> BftHeaderCommitCertificateV3 {
        let key = SigningKey::from_bytes(&[7; 32]);
        let proposal = BftProposalHeaderV3::sign(
            scope(),
            1,
            1,
            "22".repeat(32),
            "33".repeat(32),
            "validator".into(),
            "44".repeat(32),
            Some(0),
            &key,
        )
        .unwrap();
        let old = BftProposalHeaderV3::sign(
            scope(),
            1,
            0,
            "22".repeat(32),
            "33".repeat(32),
            "validator".into(),
            "44".repeat(32),
            None,
            &key,
        )
        .unwrap();
        let vote = |round, phase| {
            BftVoteV3::sign(
                scope(),
                1,
                round,
                phase,
                "22".repeat(32),
                "33".repeat(32),
                BftVoteValueV3::Block {
                    block_id: "44".repeat(32),
                },
                "validator".into(),
                &key,
            )
            .unwrap()
        };
        BftHeaderCommitCertificateV3 {
            proposal,
            valid_round_evidence: Some(BftValidRoundEvidenceV3 {
                proposal: old,
                prevotes: vec![vote(0, BftVotePhaseV3::Prevote)],
            }),
            precommits: vec![vote(1, BftVotePhaseV3::Precommit)],
        }
    }
    #[test]
    fn v3_signatures_bind_a_distinct_domain_and_complete_context() {
        let cert = certificate();
        cert.proposal.verify_signature().unwrap();
        cert.precommits[0].verify_signature().unwrap();
        let key = SigningKey::from_bytes(&[7; 32]);
        let old = super::super::super::v2::BftProposalHeaderV2::sign(
            scope(),
            1,
            1,
            "22".repeat(32),
            "33".repeat(32),
            "validator".into(),
            "44".repeat(32),
            Some(0),
            &key,
        )
        .unwrap();
        let mut transplanted = cert.proposal.clone();
        transplanted.signature_hex = old.signature_hex;
        assert!(transplanted.verify_signature().is_err());
        let old_vote = super::super::super::v2::BftVoteV2::sign(
            scope(),
            1,
            1,
            super::super::super::v2::BftVotePhaseV2::Precommit,
            "22".repeat(32),
            "33".repeat(32),
            super::super::super::v2::BftVoteValueV2::Block {
                block_id: "44".repeat(32),
            },
            "validator".into(),
            &key,
        )
        .unwrap();
        let mut transplanted_vote = cert.precommits[0].clone();
        transplanted_vote.signature_hex = old_vote.signature_hex;
        assert!(transplanted_vote.verify_signature().is_err());
        let mut changed = cert.precommits[0].clone();
        changed.schedule_policy_hash = "55".repeat(32);
        assert!(changed.verify_signature().is_err());
        changed = cert.precommits[0].clone();
        changed.protocol_version = 2;
        assert!(changed.verify_signature().is_err());
        let mut changed = cert.proposal.clone();
        changed.valid_round = None;
        assert!(changed.verify_signature().is_err());
    }
    #[test]
    fn v3_certificate_canonical_and_nested_binding_bounds() {
        let cert = certificate();
        let bytes = cert.canonical_bytes().unwrap();
        assert_eq!(
            BftHeaderCommitCertificateV3::decode_canonical(&bytes).unwrap(),
            cert
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(BftHeaderCommitCertificateV3::decode_canonical(&trailing).is_err());
        assert!(
            BftHeaderCommitCertificateV3::decode_canonical(&vec![
                0;
                BFT_MAX_SIGNED_MATERIAL_BYTES_V3
                    + 1
            ])
            .is_err()
        );
        let mut bad = cert.clone();
        bad.valid_round_evidence = None;
        assert!(bad.validate_wire_bounds().is_err());
        let mut bad = cert.clone();
        bad.valid_round_evidence
            .as_mut()
            .unwrap()
            .proposal
            .schedule_policy_hash = "55".repeat(32);
        assert!(bad.validate_wire_bounds().is_err());
        let mut bad = cert.clone();
        bad.valid_round_evidence.as_mut().unwrap().prevotes[0].phase = BftVotePhaseV3::Precommit;
        assert!(bad.validate_wire_bounds().is_err());
        let mut bad = cert.clone();
        bad.precommits[0].value = BftVoteValueV3::Nil;
        assert!(bad.validate_wire_bounds().is_err());
        let mut bad = cert.clone();
        bad.proposal.signature_hex = "A".repeat(128);
        assert!(bad.validate_wire_bounds().is_err());
        let mut at_limit = cert.clone();
        at_limit.precommits = vec![cert.precommits[0].clone(); BFT_MAX_VALIDATORS_V3];
        at_limit.validate_wire_bounds().unwrap(); // Duplicates are rejected by quorum verification, not shape validation.
        assert!(
            BftHeaderCommitCertificateV3::decode_canonical(&at_limit.canonical_bytes().unwrap())
                .is_ok()
        );
        at_limit.precommits.push(cert.precommits[0].clone());
        assert!(at_limit.validate_wire_bounds().is_err());
        assert!(
            serde_cbor::from_slice::<BftHeaderCommitCertificateV3>(
                &serde_cbor::to_vec(&at_limit).unwrap()
            )
            .is_err()
        );
        let mut bad = cert;
        bad.proposal.proposer_id = "x".repeat(BFT_MAX_IDENTITY_TEXT_BYTES_V1 + 1);
        assert!(bad.validate_wire_bounds().is_err());
    }
    #[test]
    fn v3_vote_sequence_rejects_an_oversized_declared_array_before_elements() {
        #[derive(Deserialize)]
        struct Votes(#[serde(deserialize_with = "bounded_votes")] Vec<BftVoteV3>);
        let error = serde_cbor::from_slice::<Votes>(&[0x99, 0x01, 0x01])
            .err()
            .unwrap();
        assert!(error.to_string().contains("vote count exceeds bound"));
        let empty = serde_cbor::from_slice::<Votes>(&[0x80]).unwrap();
        assert!(empty.0.is_empty());
    }
}
