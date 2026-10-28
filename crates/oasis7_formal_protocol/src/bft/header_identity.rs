//! Self-consistency binding only; deliberately distinct from verified certificates.
use oasis7_proto::distributed_bft::v3::BftBlockHeaderV3;

use super::FormalBftExecutionCommitmentV1;

/// Public execution preimage bound to a V3 header, without quorum authority.
/// No conversion to a V2 certificate or handoff target is provided.
#[derive(Debug, Clone)]
pub struct BoundFormalBftHeaderV3 {
    header: BftBlockHeaderV3,
    commitment: FormalBftExecutionCommitmentV1,
}

impl BoundFormalBftHeaderV3 {
    pub fn bind(
        header: BftBlockHeaderV3,
        commitment: FormalBftExecutionCommitmentV1,
    ) -> Result<Self, String> {
        header.validate()?;
        if commitment.digest()? != header.execution_commitment_hash {
            return Err("BFT public header execution commitment mismatch".into());
        }
        Ok(Self { header, commitment })
    }

    pub fn header(&self) -> &BftBlockHeaderV3 {
        &self.header
    }

    pub fn commitment(&self) -> &FormalBftExecutionCommitmentV1 {
        &self.commitment
    }
}
