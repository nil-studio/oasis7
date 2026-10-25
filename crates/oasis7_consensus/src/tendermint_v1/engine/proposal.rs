use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::{
    BFT_PROTOCOL_VERSION_V1, BftBlockV1, BftProposalHeaderV1, BftProposalV1,
    BftValidRoundEvidenceV1,
};
use serde::{Deserialize, Serialize};

use super::BftConsensusMachineV1;

/// Exact unsigned proposal decision that must be durably reserved before the
/// proposer invokes its signing key. It includes the complete block and any
/// valid-round proof so restart cannot choose a different slot value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftProposalIntentV1 {
    pub header: BftProposalHeaderV1,
    pub block: BftBlockV1,
    pub valid_round_evidence: Option<BftValidRoundEvidenceV1>,
}

impl BftProposalIntentV1 {
    fn unsigned_proposal(&self) -> BftProposalV1 {
        BftProposalV1 {
            header: self.header.clone(),
            block: self.block.clone(),
            valid_round_evidence: self.valid_round_evidence.clone(),
        }
    }
}

impl BftConsensusMachineV1 {
    /// Construct the precise header and evidence without producing a
    /// signature. The caller must persist this exact value before signing.
    pub fn proposal_intent(
        &self,
        block: BftBlockV1,
        signer_id: &str,
    ) -> Result<BftProposalIntentV1, String> {
        if block.scope != *self.set.scope() {
            return Err("local BFT proposal block scope does not match validator set".into());
        }
        if block.height != self.height {
            return Err("local BFT proposal block height is not current".into());
        }
        if signer_id != self.local_validator_id {
            return Err("BFT proposer signer is not the local validator".into());
        }
        let validator = self
            .set
            .validator(signer_id)
            .ok_or_else(|| "local BFT proposer is not in the active set".to_string())?;
        if self.current_proposer_id() != Some(signer_id) {
            return Err("local validator is not the deterministic BFT proposer".into());
        }
        let block_id = block.block_id()?;
        let evidence = match self.valid_value.as_deref().zip(self.valid_round) {
            Some((valid_id, round)) if valid_id == block_id => {
                Some(self.valid_round_evidence(round, valid_id)?)
            }
            Some((valid_id, _)) if valid_id != block_id => {
                return Err("BFT proposer must repropose its valid value".into());
            }
            _ => None,
        };
        let valid_round = evidence.as_ref().map(|proof| proof.proposal.round);
        let intent = BftProposalIntentV1 {
            header: BftProposalHeaderV1 {
                protocol_version: BFT_PROTOCOL_VERSION_V1,
                scope: block.scope.clone(),
                height: block.height,
                round: self.round,
                validator_set_hash: self.set.set_hash().to_string(),
                proposer_id: signer_id.to_string(),
                block_id,
                valid_round,
                public_key_hex: validator.public_key_hex.clone(),
                signature_hex: String::new(),
            },
            block,
            valid_round_evidence: evidence,
        };
        let unsigned = intent.unsigned_proposal();
        unsigned.validate_binding()?;
        self.validate_valid_round_evidence(&unsigned)?;
        Ok(intent)
    }

    /// Sign only a decision that can be reconstructed byte-for-byte from the
    /// current machine state. Callers persist `intent` before invoking this.
    pub fn sign_proposal_intent(
        &self,
        intent: &BftProposalIntentV1,
        key: &SigningKey,
    ) -> Result<BftProposalV1, String> {
        let expected =
            self.proposal_intent(intent.block.clone(), intent.header.proposer_id.as_str())?;
        if &expected != intent {
            return Err(
                "BFT proposal intent differs from the current deterministic decision".into(),
            );
        }
        let validator = self
            .set
            .validator(intent.header.proposer_id.as_str())
            .ok_or_else(|| "local BFT proposer is not in the active set".to_string())?;
        if validator.public_key_hex != hex::encode(key.verifying_key().to_bytes()) {
            return Err("local BFT proposal key does not match validator set".into());
        }
        let header = BftProposalHeaderV1::sign(
            intent.header.scope.clone(),
            intent.header.height,
            intent.header.round,
            intent.header.validator_set_hash.clone(),
            intent.header.proposer_id.clone(),
            intent.header.block_id.clone(),
            intent.header.valid_round,
            key,
        )?;
        let proposal = BftProposalV1 {
            header,
            block: intent.block.clone(),
            valid_round_evidence: intent.valid_round_evidence.clone(),
        };
        proposal.validate_binding()?;
        self.validate_valid_round_evidence(&proposal)?;
        Ok(proposal)
    }

    /// Consensus-unit-test convenience. Runtime signing must reserve the
    /// returned intent in its durable WAL before calling the signer.
    #[cfg(test)]
    pub(crate) fn make_proposal(
        &self,
        block: BftBlockV1,
        signer_id: &str,
        key: &SigningKey,
    ) -> Result<BftProposalV1, String> {
        let intent = self.proposal_intent(block, signer_id)?;
        self.sign_proposal_intent(&intent, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tendermint_v1::{
        BftProposerPriorityV1, BftTimeoutsV1, BftValidatorSetV1, BftValidatorV1,
    };
    use oasis7_proto::distributed_bft::BftScopeV1;

    fn fixture() -> (BftConsensusMachineV1, BftBlockV1, String, SigningKey) {
        let keys = (1..=4)
            .map(|seed| SigningKey::from_bytes(&[seed; 32]))
            .collect::<Vec<_>>();
        let scope = BftScopeV1 {
            world_id: "proposal-intent-world".into(),
            chain_id: "proposal-intent-chain".into(),
            genesis_hash: "11".repeat(32),
        };
        let set = BftValidatorSetV1::new(
            scope.clone(),
            keys.iter()
                .enumerate()
                .map(|(index, key)| BftValidatorV1 {
                    validator_id: format!("v{index}"),
                    public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                    voting_power: 1,
                })
                .collect(),
        )
        .expect("valid validator set");
        let proposer = BftProposerPriorityV1::new(&set)
            .expect("priority")
            .next_proposer(&set)
            .expect("proposer");
        let signer_index = proposer.validator_id[1..]
            .parse::<usize>()
            .expect("validator index");
        let machine = BftConsensusMachineV1::new(
            set,
            proposer.validator_id.clone(),
            1,
            None,
            0,
            BftTimeoutsV1 {
                propose_ms: 10,
                prevote_ms: 10,
                precommit_ms: 10,
                round_delta_ms: 5,
            },
        )
        .expect("machine");
        let block = BftBlockV1 {
            scope,
            height: 1,
            parent_commit_ref: "22".repeat(32),
            authority_epoch: 1,
            manifest_hash: "33".repeat(32),
            actions_cbor: vec![0x80],
            action_root: "44".repeat(32),
            event_root: "55".repeat(32),
            execution_block_hash: "66".repeat(32),
            execution_state_root: "77".repeat(32),
            state_root: "88".repeat(32),
        };
        (
            machine,
            block,
            proposer.validator_id,
            keys[signer_index].clone(),
        )
    }

    #[test]
    fn proposal_intent_is_unsigned_durable_and_only_signs_exact_replay() {
        let (machine, block, signer_id, key) = fixture();
        let intent = machine
            .proposal_intent(block, signer_id.as_str())
            .expect("proposal intent");
        assert!(intent.header.signature_hex.is_empty());
        let persisted = serde_cbor::to_vec(&intent).expect("encode intent");
        let restored: BftProposalIntentV1 =
            serde_cbor::from_slice(persisted.as_slice()).expect("decode intent");
        assert_eq!(restored, intent);

        let first = machine
            .sign_proposal_intent(&restored, &key)
            .expect("sign exact intent");
        let replay = machine
            .sign_proposal_intent(&restored, &key)
            .expect("re-sign exact intent after restart");
        assert_eq!(first, replay);

        let mut changed = restored;
        changed.block.state_root = "99".repeat(32);
        assert!(machine.sign_proposal_intent(&changed, &key).is_err());
        let wrong_key = SigningKey::from_bytes(&[31; 32]);
        assert!(machine.sign_proposal_intent(&intent, &wrong_key).is_err());
    }
}
