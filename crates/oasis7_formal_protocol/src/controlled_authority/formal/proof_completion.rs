pub const PROOF_COMPLETE_SIGN_DOMAIN_V1: &str = "oasis7.formal.proof_complete.sign.v1";
use super::*;
/// Attests that this endpoint durably retained the complete dual-decision proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalProofCompleteBodyV1 {
    pub schema_version: u32,
    pub record_id: String,
    pub position: u64,
    pub decision_proof_digest: String,
    pub finalizing_authority_id: String,
    pub role: EndpointRole,
    pub instance_digest: String,
    pub branch: FormalCompletionBranchV1,
    pub recovery_certificate_digest: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalProofCompleteReceiptV1 {
    pub body: FormalProofCompleteBodyV1,
    pub signature_hex: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalQualifiedCommitProofV1 {
    pub schema_version: u32,
    pub decision: FormalDecisionProofV1,
    pub primary_complete: FormalProofCompleteReceiptV1,
    pub replica_complete: FormalProofCompleteReceiptV1,
}
pub fn formal_proof_complete_signing_bytes(
    body: &FormalProofCompleteBodyV1,
) -> Result<Vec<u8>, FormalError> {
    bytes(PROOF_COMPLETE_SIGN_DOMAIN_V1, body)
}
pub fn formal_qualified_proof_digest(
    proof: &FormalQualifiedCommitProofV1,
) -> Result<String, FormalError> {
    if proof.schema_version != 1 {
        return Err(invalid("formal qualified proof version"));
    }
    hash("oasis7.formal.qualified_proof_envelope.v1", proof)
}
#[derive(Debug, Clone)]
pub struct VerifiedFormalDecisionV1 {
    pub(super) record: CanonicalRecordV1,
    pub(super) finalizer: VerifiedAuthorityV1,
    pub(super) branch: FormalCompletionBranchV1,
    pub(super) recovery_certificate_digest: Option<String>,
    pub(super) decision_proof_digest: String,
}
impl VerifiedFormalDecisionV1 {
    pub fn record(&self) -> &CanonicalRecordV1 {
        &self.record
    }
    pub fn finalizing_authority(&self) -> &VerifiedAuthorityV1 {
        &self.finalizer
    }
    pub fn decision_proof_digest(&self) -> &str {
        &self.decision_proof_digest
    }
    pub fn complete_receipt_body(
        &self,
        role: EndpointRole,
    ) -> Result<FormalProofCompleteBodyV1, FormalError> {
        let binding = match role {
            EndpointRole::Primary => &self.finalizer.definition().primary,
            EndpointRole::Replica => &self.finalizer.definition().replica,
        };
        Ok(FormalProofCompleteBodyV1 {
            schema_version: 1,
            record_id: self.record.record_id()?,
            position: self.record.position,
            decision_proof_digest: self.decision_proof_digest.clone(),
            finalizing_authority_id: self.finalizer.authority_id()?,
            role,
            instance_digest: binding
                .instance
                .digest()
                .map_err(|error| invalid(error.to_string()))?,
            branch: self.branch,
            recovery_certificate_digest: self.recovery_certificate_digest.clone(),
        })
    }
}
pub fn verify_formal_qualified(
    proof: &FormalQualifiedCommitProofV1,
    authority: &VerifiedAuthorityV1,
    parent: Option<&VerifiedFormalCommitV1>,
    recovery: Option<&VerifiedRecovery>,
) -> Result<VerifiedFormalCommitV1, FormalError> {
    let decision = verify_formal_decision(&proof.decision, authority, parent, recovery)?;
    verify_formal_qualified_from_decision(proof, &decision)
}
pub fn verify_formal_qualified_from_decision(
    proof: &FormalQualifiedCommitProofV1,
    decision: &VerifiedFormalDecisionV1,
) -> Result<VerifiedFormalCommitV1, FormalError> {
    if proof.schema_version != 1
        || formal_decision_proof_digest(&proof.decision)? != decision.decision_proof_digest
    {
        return Err(invalid("qualified proof exact verified decision binding"));
    }
    for (receipt, role) in [
        (&proof.primary_complete, EndpointRole::Primary),
        (&proof.replica_complete, EndpointRole::Replica),
    ] {
        if receipt.body != decision.complete_receipt_body(role)? {
            return Err(invalid("qualified proof complete receipt context"));
        }
        let binding = match role {
            EndpointRole::Primary => &decision.finalizer.definition().primary,
            EndpointRole::Replica => &decision.finalizer.definition().replica,
        };
        verify(
            PROOF_COMPLETE_SIGN_DOMAIN_V1,
            &receipt.body,
            &receipt.signature_hex,
            &binding.instance.endpoint_public_key,
        )?;
    }
    let mut verified_record_id = [0u8; 32];
    hex::decode_to_slice(
        &proof.primary_complete.body.record_id,
        &mut verified_record_id,
    )
    .map_err(|_| invalid("verified commit cached record id"))?;
    Ok(VerifiedFormalCommitV1 {
        verified_record_id,
        record: decision.record.clone(),
        finalizing_authority_id: decision.finalizer.authority_id()?,
    })
}

#[path = "proof_genesis_bounded.rs"]
mod genesis_bounded;
pub use genesis_bounded::{
    PaidVerifiedGenesisCommitV1, PaidVerifiedGenesisDecisionV1, genesis_final_receipt_body_bounded,
    verify_genesis_decision_bounded, verify_genesis_qualified_bounded,
};
