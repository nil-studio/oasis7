use super::*;

/// Native endpoint checkpoint revalidation only. The checked verdict binds
/// signed archive commitments to a policy-authorized provider statement for
/// that exact checkpoint. It does not establish freshness: native callers must
/// retain their authenticated RPC request/minimum or held monotonic guard checks.
/// Ordinary readers still require their independently configured external pin
/// and canonical activation/history verification.
pub fn restore_formal_commit_from_checkpoint(
    proof: &FormalQualifiedCommitProofV1,
    original_authority: &VerifiedAuthorityV1,
    recovery: Option<&VerifiedRecovery>,
    checkpoint: &NativeCheckpointVerdictV1,
) -> Result<VerifiedFormalCommitV1, FormalError> {
    if checkpoint.proof() != proof {
        return Err(invalid("native checkpoint exact archived proof binding"));
    }
    let parent = match (
        checkpoint.verified_parent_record(),
        checkpoint.verified_parent_finalizing_authority_id(),
    ) {
        (Some(record), Some(finalizer)) => {
            record.validate()?;
            super::crypto::hash_hex(finalizer)?;
            let mut verified_record_id = [0u8; 32];
            hex::decode_to_slice(&record.record_id()?, &mut verified_record_id)
                .map_err(|_| invalid("verified commit cached record id"))?;
            Some(VerifiedFormalCommitV1 {
                verified_record_id,
                record: record.clone(),
                finalizing_authority_id: finalizer.to_owned(),
            })
        }
        (None, None) => None,
        _ => {
            return Err(invalid(
                "native checkpoint complete original parent context required",
            ));
        }
    };
    let commit = verify_formal_qualified(proof, original_authority, parent.as_ref(), recovery)?;
    if &commit.head() != checkpoint.head()
        || commit.finalizing_authority_id() != checkpoint.authority_id()
    {
        return Err(invalid(
            "native checkpoint canonical head/finalizing authority binding",
        ));
    }
    Ok(commit)
}
