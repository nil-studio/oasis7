//! Public protocol proposal material; no private actions are serialized here.
use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3, BoundFormalBftHeaderV3, FormalBftExecutionCommitmentV1,
};
use oasis7_proto::distributed_bft::v3::{
    BftBlockHeaderV3, BftProposalHeaderV3, BftValidRoundEvidenceV3,
};
use serde::{Deserialize, Serialize};

const MAX_PUBLIC_PROPOSAL_BYTES: usize = 3 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftHeaderValueV3 {
    pub header: BftBlockHeaderV3,
    pub execution_commitment: FormalBftExecutionCommitmentV1,
}
impl BftHeaderValueV3 {
    pub fn block_id(&self) -> Result<String, String> {
        self.validate_binding()?;
        self.header.block_id()
    }
    pub fn validate_binding(&self) -> Result<BoundFormalBftHeaderV3, String> {
        BoundFormalBftHeaderV3::bind(self.header.clone(), self.execution_commitment.clone())
    }
    pub fn validate_for_schedule(
        &self,
        schedule: &BftHeaderHeightScheduleV3,
    ) -> Result<(), String> {
        self.validate_binding()?;
        schedule.validate_commitment(&self.execution_commitment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPublicProposalV3 {
    pub header: BftProposalHeaderV3,
    pub block: BftHeaderValueV3,
    pub valid_round_evidence: Option<BftValidRoundEvidenceV3>,
}
impl BftPublicProposalV3 {
    /// Binding also accepts the unsigned form used for a durable intent.
    /// Signature authentication is always performed by the admitted-set check.
    pub fn validate_binding(&self) -> Result<(), String> {
        self.header.signing_payload()?;
        if !self.header.signature_hex.is_empty() {
            self.header.validate()?;
        }
        self.block.validate_binding()?;
        let commitment = &self.block.execution_commitment;
        if self.header.scope != commitment.scope
            || self.header.height != commitment.height
            || self.header.active_validator_set_hash != commitment.active_validator_set_hash
            || self.header.schedule_policy_hash != commitment.schedule_policy_hash
            || self.header.block_id != self.block.block_id()?
            || self.header.valid_round
                != self.valid_round_evidence.as_ref().map(|e| e.proposal.round)
        {
            return Err("BFT V3 public proposal binding mismatch".into());
        }
        self.encoded_len()?;
        Ok(())
    }
    pub fn encoded_len(&self) -> Result<usize, String> {
        let bytes = serde_cbor::to_vec(self).map_err(|_| "encode BFT V3 public proposal")?;
        if bytes.len() > MAX_PUBLIC_PROPOSAL_BYTES {
            return Err("BFT V3 public proposal exceeds byte capacity".into());
        }
        Ok(bytes.len())
    }
}
