//! Restricted qualification from an opaque canonical-root HistoricalCut fact.
//! Parent continuity is delegated by the profile2 exact-parent-cut contract;
//! signatures and exact completion bodies remain independently checked here.
use super::super::business_history::VerifiedHistoricalCutInclusionV1;
use super::*;
pub(crate) fn verify_historical_qualified_from_inclusion(
    fact: &VerifiedHistoricalCutInclusionV1,
) -> Result<VerifiedFormalCommitV1, FormalError> {
    let proof = &fact.cut().parent_qualified;
    let authority = fact.authority();
    if proof.schema_version != 1
        || proof.decision.schema_version != 1
        || proof.decision.branch != FormalCompletionBranchV1::Normal
        || fact.cut().parent_record != proof.decision.record
    {
        return Err(invalid("historical delegated exact Normal qualified proof"));
    }
    if proof.decision.record.kind == FormalRecordKindV1::InitialActivation {
        return verify_formal_qualified(proof, authority, None, None);
    }
    let candidate = SignedCanonicalCandidateV1 {
        record: proof.decision.record.clone(),
        writer_signature_hex: proof.decision.writer_signature_hex.clone(),
    };
    candidate.verify_original(authority)?;
    if matches!(
        candidate.record.kind,
        FormalRecordKindV1::RecoveryActivation { .. }
    ) {
        super::super::recovery_activation::verify_recovery_activation_identity(
            &candidate.record,
            authority,
            proof.decision.frozen.as_ref(),
        )?;
    } else {
        if authority
            .first_normal_position
            .is_none_or(|position| candidate.record.position < position)
        {
            return Err(invalid("historical delegated Normal authority window"));
        }
        let frozen = proof
            .decision
            .frozen
            .as_ref()
            .ok_or_else(|| invalid("historical execution original Frozen certificate"))?;
        frozen.verify(authority.definition(), authority.activation_ref())?;
        frozen.matches_record(&candidate.record)?;
    }
    let context = VerifiedCandidateContextV1 {
        record: candidate.record,
        finalizer: authority.clone(),
        branch: FormalCompletionBranchV1::Normal,
        recovery_certificate_digest: None,
    };
    for (receipt, role) in [
        (&proof.decision.primary, EndpointRole::Primary),
        (&proof.decision.replica, EndpointRole::Replica),
    ] {
        if receipt.body != context.receipt_body(role)? {
            return Err(invalid("historical exact dual final receipt context"));
        }
        let binding = match role {
            EndpointRole::Primary => &authority.definition().primary,
            EndpointRole::Replica => &authority.definition().replica,
        };
        verify(
            "oasis7.formal.final_receipt.sign.v1",
            &receipt.body,
            &receipt.signature_hex,
            &binding.instance.endpoint_public_key,
        )?;
    }
    let decision = VerifiedFormalDecisionV1 {
        record: context.record,
        finalizer: context.finalizer,
        branch: context.branch,
        recovery_certificate_digest: None,
        decision_proof_digest: formal_decision_proof_digest(&proof.decision)?,
    };
    verify_formal_qualified_from_decision(proof, &decision)
}
