use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::v2::{
    BFT_PROTOCOL_VERSION_V2, BftBlockV2, BftProposalHeaderV2, BftProposalV2,
    BftValidRoundEvidenceV2,
};
use serde::{Deserialize, Serialize};

use super::BftConsensusMachineV2;

/// Exact unsigned V2 proposal decision reserved before the caller signs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftProposalIntentV2 {
    pub header: BftProposalHeaderV2,
    pub block: BftBlockV2,
    pub valid_round_evidence: Option<BftValidRoundEvidenceV2>,
}

impl BftProposalIntentV2 {
    fn unsigned_proposal(&self) -> BftProposalV2 {
        BftProposalV2 {
            header: self.header.clone(),
            block: self.block.clone(),
            valid_round_evidence: self.valid_round_evidence.clone(),
        }
    }
}

impl BftConsensusMachineV2 {
    /// Construct the exact proposal header and proof without signing.
    pub fn proposal_intent(
        &self,
        block: BftBlockV2,
        signer_id: &str,
    ) -> Result<BftProposalIntentV2, String> {
        if block.scope != *self.set.scope()
            || block.height != self.height
            || block.parent_commit_ref != self.schedule.parent_commit_ref()
            || block.active_validator_set_hash != self.set.set_hash()
            || block.schedule_policy_hash != self.schedule.policy_hash()
            || block.manifest_hash != self.schedule.parent_manifest_hash()
        {
            return Err("local BFT V2 proposal block differs from the current schedule".into());
        }
        self.schedule.validate_for_commit(&block)?;
        if signer_id != self.local_validator_id {
            return Err("BFT V2 proposer signer is not the local validator".into());
        }
        let validator = self
            .set
            .validator(signer_id)
            .ok_or_else(|| "local BFT V2 proposer is not in the active set".to_string())?;
        if self.current_proposer_id() != Some(signer_id) {
            return Err("local validator is not the deterministic BFT V2 proposer".into());
        }
        let block_id = block.block_id()?;
        let evidence = match self.valid_value.as_deref().zip(self.valid_round) {
            Some((valid_id, round)) if valid_id == block_id => {
                Some(self.valid_round_evidence(round, valid_id)?)
            }
            Some((valid_id, _)) if valid_id != block_id => {
                return Err("BFT V2 proposer must repropose its valid value".into());
            }
            _ => None,
        };
        let valid_round = evidence.as_ref().map(|proof| proof.proposal.round);
        let intent = BftProposalIntentV2 {
            header: BftProposalHeaderV2 {
                protocol_version: BFT_PROTOCOL_VERSION_V2,
                scope: block.scope.clone(),
                height: block.height,
                round: self.round,
                active_validator_set_hash: self.set.set_hash().to_string(),
                schedule_policy_hash: self.schedule.policy_hash().to_string(),
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

    /// Sign only the exact currently replayable proposal decision.
    pub fn sign_proposal_intent(
        &self,
        intent: &BftProposalIntentV2,
        key: &SigningKey,
    ) -> Result<BftProposalV2, String> {
        let expected =
            self.proposal_intent(intent.block.clone(), intent.header.proposer_id.as_str())?;
        if &expected != intent {
            return Err("BFT V2 proposal intent differs from deterministic replay".into());
        }
        let validator = self
            .set
            .validator(intent.header.proposer_id.as_str())
            .ok_or_else(|| "BFT V2 proposer is not in the active set".to_string())?;
        if validator.public_key_hex != hex::encode(key.verifying_key().to_bytes()) {
            return Err("BFT V2 proposal key does not match active validator".into());
        }
        let header = BftProposalHeaderV2::sign(
            intent.header.scope.clone(),
            intent.header.height,
            intent.header.round,
            intent.header.active_validator_set_hash.clone(),
            intent.header.schedule_policy_hash.clone(),
            intent.header.proposer_id.clone(),
            intent.header.block_id.clone(),
            intent.header.valid_round,
            key,
        )?;
        let proposal = BftProposalV2 {
            header,
            block: intent.block.clone(),
            valid_round_evidence: intent.valid_round_evidence.clone(),
        };
        proposal.validate_binding()?;
        self.validate_valid_round_evidence(&proposal)?;
        Ok(proposal)
    }
}
