//! Closed GEN proof construction. Borrowed original field mirrors measure
//! exact serializer output before any original DTO clone; the fixed verifier
//! is the only producer of the returned paid authenticated owner.
use super::*;
#[derive(Serialize)]
#[serde(rename = "FormalDecisionProofV1")]
struct DecisionBorrow<'a> {
    schema_version: u32,
    record: &'a CanonicalRecordV1,
    branch: FormalCompletionBranchV1,
    writer_signature_hex: &'a str,
    frozen: Option<&'a FormalFrozenCertificateV1>,
    primary: &'a FormalFinalReceiptV1,
    replica: &'a FormalFinalReceiptV1,
}
#[derive(Serialize)]
#[serde(rename = "FormalQualifiedCommitProofV1")]
struct QualifiedBorrow<'a> {
    schema_version: u32,
    decision: &'a FormalDecisionProofV1,
    primary_complete: &'a FormalProofCompleteReceiptV1,
    replica_complete: &'a FormalProofCompleteReceiptV1,
}
fn encode<T: Serialize>(
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>, FormalError> {
    crate::controlled_authority::formal::context_wire::encode_paid(
        value,
        LIMIT,
        ctx,
        "formal encoding exceeds byte bound",
        "GEN proof builder measurement",
    )
}
pub fn build_genesis_decision_bounded(
    candidate: &SignedCanonicalCandidateV1,
    context: &VerifiedCandidateContextV1,
    primary: &FormalFinalReceiptV1,
    replica: &FormalFinalReceiptV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidVerifiedGenesisDecisionV1, FormalError> {
    check_genesis_context(context, ctx)?;
    if !same(&candidate.record, context.record(), ctx)? {
        return Err(invalid("GEN builder exact authenticated record"));
    }
    let view = DecisionBorrow {
        schema_version: 1,
        record: &candidate.record,
        branch: FormalCompletionBranchV1::Normal,
        writer_signature_hex: &candidate.writer_signature_hex,
        frozen: None,
        primary,
        replica,
    };
    let wire = encode(&view, ctx)?;
    let layout = add(
        add(size_of::<FormalDecisionProofV1>(), wire.len())?,
        map_layout(candidate.record.artifact_roots.len())?,
    )?;
    let fee = ctx
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(|_| invalid("GEN decision builder clone admission"))?;
    charge(ctx, wire.len())?;
    ctx.consume_steps((candidate.record.artifact_roots.len() + 40) as u64)?;
    let value = FormalDecisionProofV1 {
        schema_version: 1,
        record: candidate.record.clone(),
        branch: FormalCompletionBranchV1::Normal,
        writer_signature_hex: candidate.writer_signature_hex.clone(),
        frozen: None,
        primary: primary.clone(),
        replica: replica.clone(),
    };
    check_record_copy(&value.record)?;
    let owned = fee.retain(value);
    let actual = encode(&*owned, ctx)?;
    charge(ctx, add(wire.len(), actual.len())?)?;
    if wire.as_slice() != actual.as_slice() {
        return Err(invalid("GEN decision borrowed serializer mismatch"));
    }
    verify_genesis_decision_bounded(&owned, candidate, context, ctx)
}
pub fn build_genesis_qualified_bounded(
    decision: &PaidVerifiedGenesisDecisionV1,
    candidate: &SignedCanonicalCandidateV1,
    context: &VerifiedCandidateContextV1,
    primary: &FormalProofCompleteReceiptV1,
    replica: &FormalProofCompleteReceiptV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidVerifiedGenesisCommitV1, FormalError> {
    decision.validate_context(ctx)?;
    check_genesis_context(context, ctx)?;
    let source = decision.decision();
    if source.frozen.is_some() {
        return Err(invalid("GEN builder frozen proof unsupported"));
    }
    let view = QualifiedBorrow {
        schema_version: 1,
        decision: source,
        primary_complete: primary,
        replica_complete: replica,
    };
    let wire = encode(&view, ctx)?;
    let layout = add(
        add(size_of::<FormalQualifiedCommitProofV1>(), wire.len())?,
        map_layout(source.record.artifact_roots.len())?,
    )?;
    let fee = ctx
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(|_| invalid("GEN qualified builder clone admission"))?;
    charge(ctx, wire.len())?;
    ctx.consume_steps((source.record.artifact_roots.len() + 50) as u64)?;
    let value = FormalQualifiedCommitProofV1 {
        schema_version: 1,
        decision: source.clone(),
        primary_complete: primary.clone(),
        replica_complete: replica.clone(),
    };
    check_record_copy(&value.decision.record)?;
    let owned = fee.retain(value);
    let actual = encode(&*owned, ctx)?;
    charge(ctx, add(wire.len(), actual.len())?)?;
    if wire.as_slice() != actual.as_slice() {
        return Err(invalid("GEN qualified borrowed serializer mismatch"));
    }
    verify_genesis_qualified_bounded(&owned, candidate, context, ctx)
}
#[cfg(test)]
#[path = "proof_genesis_build_tests.rs"]
mod tests;
