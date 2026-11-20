//! A locally saved candidate can avoid execution, never promote a sealed tail.
use super::*;

pub(super) fn checked_cached_candidate(
    anchor: &VerifiedFormalReadAnchorV1,
    recovery: Option<&VerifiedRecovery>,
    local: &Pending,
    remote: &Pending,
) -> Result<Option<FormalCandidateMaterialV1>, String> {
    let Some(candidate) = local.candidate.as_ref() else {
        return Ok(None);
    };
    if remote.candidate.is_some() {
        return Ok(None);
    }
    let original = recovery_runtime::pending_original_authority(anchor, recovery, remote)?;
    let eligible = match candidate.branch {
        FormalCompletionBranchV1::Normal => {
            original.definition() == anchor.authority().definition()
        }
        FormalCompletionBranchV1::ReplayFrozen => recovery.is_some_and(|context| {
            matches!(&context.body().tail, RecoveryTailV1::Frozen { .. })
                && context.successor().definition() == anchor.authority().definition()
        }),
        _ => false,
    };
    if !eligible {
        return Ok(None);
    }
    if candidate.frozen.as_ref() != remote.frozen_certificate.as_ref()
        || candidate.frozen_material.as_ref() != Some(&remote.material)
        || candidate
            .frozen
            .as_ref()
            .is_none_or(|certificate| certificate.reservation != remote.original)
    {
        return Err("cached candidate differs from authenticated original Frozen closure".into());
    }
    verify_record_material(&candidate.candidate.record, &candidate.material)
        .map_err(|e| e.to_string())?;
    verify_formal_candidate(
        &candidate.candidate,
        candidate.frozen.as_ref(),
        original,
        Some(anchor.head()),
        if candidate.branch == FormalCompletionBranchV1::Normal {
            None
        } else {
            recovery
        },
        candidate.branch,
    )
    .map_err(|e| e.to_string())?;
    Ok(Some(candidate.clone()))
}

pub(super) fn verify_qualified_commit(
    anchor: &VerifiedFormalReadAnchorV1,
    recovery: Option<&VerifiedRecovery>,
    proof: &FormalQualifiedCommitProofV1,
    expected_position: u64,
) -> Result<VerifiedFormalCommitV1, String> {
    use oasis7_consensus::{
        AuthorityCommitProof, AuthorityCommitVerifier, CommitCandidateRef,
        ControlledAuthorityCommitVerifier, TrustedCommitAnchor, VerifiedAuthorityCommit,
    };
    let (authority, recovery_ctx) = if proof.decision.branch == FormalCompletionBranchV1::Normal {
        (anchor.authority().clone(), None)
    } else {
        let context = recovery.ok_or("recovery proof lacks checked context")?;
        (context.original_authority().clone(), Some(context.clone()))
    };
    let trusted_anchor =
        TrustedCommitAnchor::controlled(authority, Some(anchor.head().clone()), recovery_ctx);
    let candidate = CommitCandidateRef::controlled(Some(expected_position), None);
    ControlledAuthorityCommitVerifier::qualified_only()
        .verify_authority_commit(
            &trusted_anchor,
            &candidate,
            &AuthorityCommitProof::controlled_qualified(proof.clone()),
        )
        .and_then(VerifiedAuthorityCommit::into_controlled_qualified)
        .map_err(|e| e.to_string())
}
