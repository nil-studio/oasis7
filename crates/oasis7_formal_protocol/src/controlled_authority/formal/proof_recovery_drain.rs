//! Qualification for a signed, independently installed recovery plan. The
//! helper never exposes the plan's parent as an ordinary reader trust anchor.
use super::*;

fn plan_parent(
    plan: &SignedRecoveryPlanV1,
    authority: &VerifiedAuthorityV1,
) -> Result<VerifiedFormalCommitV1, FormalError> {
    plan.verify(authority)?;
    if plan.body.intent.retained_context.is_some() {
        return Err(invalid(
            "old-epoch draining requires its actual active original cut",
        ));
    }
    let mut verified_record_id = [0; 32];
    hex::decode_to_slice(
        &plan.body.original_cut.record_id()?,
        &mut verified_record_id,
    )
    .map_err(|_| invalid("recovery original cut record id"))?;
    Ok(VerifiedFormalCommitV1 {
        verified_record_id,
        record: plan.body.original_cut.clone(),
        finalizing_authority_id: authority.authority_id()?,
    })
}

pub(crate) fn verify_recovery_drain_decision(
    plan: &SignedRecoveryPlanV1,
    authority: &VerifiedAuthorityV1,
    decision: &FormalDecisionProofV1,
) -> Result<(), FormalError> {
    let parent = plan_parent(plan, authority)?;
    verify_formal_decision(decision, authority, Some(&parent), None)?;
    Ok(())
}

pub(crate) fn verify_recovery_drained_qualified(
    plan: &SignedRecoveryPlanV1,
    authority: &VerifiedAuthorityV1,
    drained: &RecoveryDrainedProofV1,
) -> Result<FormalHeadRefV1, FormalError> {
    let parent = plan_parent(plan, authority)?;
    let proof = &drained.qualified;
    let checked = match &drained.fixed {
        RecoveryTailWitnessV1::Empty => {
            if proof.decision.record != *parent.record() {
                return Err(invalid(
                    "empty recovery must retain the exact qualified original cut",
                ));
            }
            verify_exact_plan_cut(
                proof,
                authority,
                plan.body.original_qualified_proof_digest.as_deref(),
            )?
        }
        RecoveryTailWitnessV1::Frozen(frozen)
        | RecoveryTailWitnessV1::Prepared { frozen, .. }
        | RecoveryTailWitnessV1::Decision { frozen, .. } => {
            if proof.decision.frozen.as_ref() != Some(frozen) {
                return Err(invalid("draining changed original Frozen certificate"));
            }
            if let RecoveryTailWitnessV1::Prepared { candidate, .. }
            | RecoveryTailWitnessV1::Decision { candidate, .. } = &drained.fixed
                && (proof.decision.record != candidate.record
                    || proof.decision.writer_signature_hex != candidate.writer_signature_hex)
            {
                return Err(invalid(
                    "draining changed already fixed candidate/signature",
                ));
            }
            verify_formal_qualified(proof, authority, Some(&parent), None)?
        }
    };
    if (!matches!(drained.fixed, RecoveryTailWitnessV1::Empty)
        && proof.decision.branch != FormalCompletionBranchV1::Normal)
        || checked.finalizing_authority_id() != authority.authority_id()?
    {
        return Err(invalid("draining must finish in the original epoch"));
    }
    Ok(checked.head())
}

fn verify_exact_plan_cut(
    proof: &FormalQualifiedCommitProofV1,
    authority: &VerifiedAuthorityV1,
    legacy_digest: Option<&str>,
) -> Result<VerifiedFormalCommitV1, FormalError> {
    if let Some(expected) = legacy_digest {
        if formal_qualified_proof_digest(proof)? != expected
            || proof.decision.schema_version != 1
            || !matches!(
                proof.decision.branch,
                FormalCompletionBranchV1::CompletePrepared | FormalCompletionBranchV1::ReplayFrozen
            )
        {
            return Err(invalid(
                "legacy original Qualified differs from issuer-verified proof bytes",
            ));
        }
        // The signed plan carries the digest produced only after the original
        // full writer/Frozen/recovery verifier. Keep every original byte pinned,
        // then independently recheck this active epoch's final/complete receipts.
        let recovery_digest = proof
            .decision
            .primary
            .body
            .recovery_certificate_digest
            .clone()
            .ok_or_else(|| invalid("legacy Qualified original recovery digest missing"))?;
        hash_hex(&recovery_digest)?;
        return verify_exact_final_receipts(
            proof,
            VerifiedCandidateContextV1 {
                record: proof.decision.record.clone(),
                finalizer: authority.clone(),
                branch: proof.decision.branch,
                recovery_certificate_digest: Some(recovery_digest),
            },
        );
    }
    if proof.decision.record.kind == FormalRecordKindV1::InitialActivation {
        return verify_formal_qualified(proof, authority, None, None);
    }
    if proof.schema_version != 1
        || proof.decision.schema_version != 1
        || proof.decision.branch != FormalCompletionBranchV1::Normal
    {
        return Err(invalid("recovery original cut Normal qualified proof"));
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
            .is_none_or(|start| candidate.record.position < start)
        {
            return Err(invalid("qualified original Normal authority window"));
        }
        let frozen = proof
            .decision
            .frozen
            .as_ref()
            .ok_or_else(|| invalid("recovery original cut Frozen certificate"))?;
        frozen.verify(authority.definition(), authority.activation_ref())?;
        frozen.matches_record(&candidate.record)?;
    }
    let context = VerifiedCandidateContextV1 {
        record: candidate.record,
        finalizer: authority.clone(),
        branch: FormalCompletionBranchV1::Normal,
        recovery_certificate_digest: None,
    };
    verify_exact_final_receipts(proof, context)
}

/// Restricted to provider-held native H ancestry, never an ordinary unpinned
/// reader constructor. The caller must establish the exact parent hash first.
pub(crate) fn verify_normal_qualified_from_authenticated_native_ancestry(
    proof: &FormalQualifiedCommitProofV1,
    authority: &VerifiedAuthorityV1,
) -> Result<VerifiedFormalCommitV1, FormalError> {
    verify_exact_plan_cut(proof, authority, None)
}

fn verify_exact_final_receipts(
    proof: &FormalQualifiedCommitProofV1,
    context: VerifiedCandidateContextV1,
) -> Result<VerifiedFormalCommitV1, FormalError> {
    let authority = &context.finalizer;
    for (receipt, role, binding) in [
        (
            &proof.decision.primary,
            EndpointRole::Primary,
            &authority.definition().primary,
        ),
        (
            &proof.decision.replica,
            EndpointRole::Replica,
            &authority.definition().replica,
        ),
    ] {
        if receipt.body != context.receipt_body(role)? {
            return Err(invalid("recovery original cut exact final receipt"));
        }
        verify(
            FINAL_RECEIPT_SIGN_DOMAIN_V1,
            &receipt.body,
            &receipt.signature_hex,
            &binding.instance.endpoint_public_key,
        )?;
    }
    let decision = VerifiedFormalDecisionV1 {
        record: context.record,
        finalizer: context.finalizer,
        branch: context.branch,
        recovery_certificate_digest: context.recovery_certificate_digest,
        decision_proof_digest: formal_decision_proof_digest(&proof.decision)?,
    };
    verify_formal_qualified_from_decision(proof, &decision)
}
