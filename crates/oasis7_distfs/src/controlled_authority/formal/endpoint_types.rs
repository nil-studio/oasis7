use super::*;
use crate::controlled_authority::replicated_protocol::{ClosedRecord, EndpointRole};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormalCandidateMaterialV1 {
    pub candidate: SignedCanonicalCandidateV1,
    pub material: ClosedRecord,
    pub frozen: Option<FormalFrozenCertificateV1>,
    pub frozen_material: Option<ClosedRecord>,
    pub branch: FormalCompletionBranchV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormalPrepareReceiptBodyV1 {
    pub final_receipt: FormalFinalReceiptBodyV1,
    pub candidate_material_digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormalPrepareReceiptV1 {
    pub body: FormalPrepareReceiptBodyV1,
    pub signature_hex: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormalRequestStateV1 {
    Frozen {
        input: SignedFormalFrozenV1,
        material: ClosedRecord,
        receipt: Option<FormalFrozenReceiptV1>,
        certificate: Option<FormalFrozenCertificateV1>,
    },
    Prepared {
        candidate: FormalCandidateMaterialV1,
        receipt: Option<FormalPrepareReceiptV1>,
    },
    Decided {
        candidate: FormalCandidateMaterialV1,
        prepare: FormalPrepareReceiptV1,
        receipt: FormalFinalReceiptV1,
    },
    Ready {
        candidate: FormalCandidateMaterialV1,
        prepare: FormalPrepareReceiptV1,
        decision: FormalDecisionProofV1,
        complete: Option<FormalProofCompleteReceiptV1>,
    },
    Qualified {
        candidate: FormalCandidateMaterialV1,
        proof: FormalQualifiedCommitProofV1,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data_closure: Option<String>,
    },
}
impl FormalRequestStateV1 {
    /// Local signed-checkpoint reference, not a physical closure permit.
    pub fn local_data_closure_reference(&self) -> Option<&str> {
        match self {
            Self::Qualified { data_closure, .. } => data_closure.as_deref(),
            _ => None,
        }
    }
    pub fn request_digest(&self) -> &str {
        match self {
            Self::Frozen { input, .. } => &input.body.request_digest,
            Self::Prepared { candidate, .. }
            | Self::Decided { candidate, .. }
            | Self::Ready { candidate, .. }
            | Self::Qualified { candidate, .. } => &candidate.candidate.record.request_digest,
        }
    }
    pub fn position(&self) -> u64 {
        match self {
            Self::Frozen { input, .. } => input.body.next_position,
            Self::Prepared { candidate, .. }
            | Self::Decided { candidate, .. }
            | Self::Ready { candidate, .. }
            | Self::Qualified { candidate, .. } => candidate.candidate.record.position,
        }
    }
    pub fn candidate(&self) -> Option<&FormalCandidateMaterialV1> {
        match self {
            Self::Frozen { .. } => None,
            Self::Prepared { candidate, .. }
            | Self::Decided { candidate, .. }
            | Self::Ready { candidate, .. }
            | Self::Qualified { candidate, .. } => Some(candidate),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormalQueryOutcomeV1 {
    NotRecorded,
    Recorded(Box<FormalRequestStateV1>),
}
pub fn formal_prepare_receipt_signing_bytes(
    body: &FormalPrepareReceiptBodyV1,
) -> Result<Vec<u8>, FormalError> {
    super::crypto::bytes("oasis7.formal.prepare_receipt.sign.v1", body)
}
pub(in crate::controlled_authority) const CANDIDATE_MATERIAL_DOMAIN_V1: &str =
    "oasis7.formal.candidate_material.v1";
pub fn formal_candidate_material_digest(
    candidate: &FormalCandidateMaterialV1,
) -> Result<String, FormalError> {
    super::crypto::hash(CANDIDATE_MATERIAL_DOMAIN_V1, candidate)
}
pub fn verify_formal_prepare_receipt(
    receipt: &FormalPrepareReceiptV1,
    candidate: &FormalCandidateMaterialV1,
    context: &VerifiedCandidateContextV1,
    role: EndpointRole,
) -> Result<(), FormalError> {
    if receipt.body.final_receipt != context.receipt_body(role)?
        || receipt.body.candidate_material_digest != formal_candidate_material_digest(candidate)?
    {
        return Err(super::crypto::invalid(
            "formal prepare receipt candidate/context",
        ));
    }
    let definition = context.finalizing_authority().definition();
    let key = match role {
        EndpointRole::Primary => &definition.primary.instance.endpoint_public_key,
        EndpointRole::Replica => &definition.replica.instance.endpoint_public_key,
    };
    super::crypto::verify(
        "oasis7.formal.prepare_receipt.sign.v1",
        &receipt.body,
        &receipt.signature_hex,
        key,
    )
}
