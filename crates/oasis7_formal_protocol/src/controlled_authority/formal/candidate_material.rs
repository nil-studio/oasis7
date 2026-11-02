//! Shared existing candidate/material carrier; no endpoint or trust constructor.
use super::{FormalCompletionBranchV1, FormalFrozenCertificateV1, SignedCanonicalCandidateV1};
use crate::controlled_authority::replicated_protocol::ClosedRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormalCandidateMaterialV1 {
    pub candidate: SignedCanonicalCandidateV1,
    pub material: ClosedRecord,
    pub frozen: Option<FormalFrozenCertificateV1>,
    pub frozen_material: Option<ClosedRecord>,
    pub branch: FormalCompletionBranchV1,
}
