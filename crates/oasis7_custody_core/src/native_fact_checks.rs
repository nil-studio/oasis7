use super::*;
/// Typed native facts; no caller-selected signing domain or arbitrary bytes.
pub(crate) enum NativeFact<'a> {
    Challenge(&'a crate::native_binding::SignedNativeBindingChallengeV1),
    Receipt(&'a crate::native_binding::SignedNativeBindingReceiptV1),
    Status(&'a crate::native_binding::SignedManagedNativeSessionStatusV1),
    Statement(&'a crate::native_binding::SignedOwnerNativeBindingStatementV1),
}
pub(crate) fn registered_native_fact_checks(
    operation_id: &str,
    nonce: &str,
    context: &ApprovalAdmissionContext,
    fact: NativeFact<'_>,
    key_id: &str,
    public_key: &str,
    request_digest: &str,
) -> Result<ExactSignatureChecks> {
    let (actual_key, signature, bytes) = match fact {
        NativeFact::Challenge(v) => {
            v.validate().map_err(|_| ApprovalAdmissionError::Invalid)?;
            (&v.key_id, &v.signature, v.challenge.signing_bytes())
        }
        NativeFact::Receipt(v) => {
            v.validate().map_err(|_| ApprovalAdmissionError::Invalid)?;
            (&v.key_id, &v.signature, v.receipt.signing_bytes())
        }
        NativeFact::Statement(v) => {
            v.validate().map_err(|_| ApprovalAdmissionError::Invalid)?;
            (
                &v.statement.signer_key_id,
                &v.signature,
                v.statement.signing_bytes(),
            )
        }
        NativeFact::Status(v) => {
            v.validate().map_err(|_| ApprovalAdmissionError::Invalid)?;
            (&v.key_id, &v.signature, v.status.signing_bytes())
        }
    };
    if actual_key != key_id
        || public_key.len() != 64
        || !public_key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ApprovalAdmissionError::UnknownKey);
    }
    Ok(ExactSignatureChecks {
        operation_id: operation_id.into(),
        body_digest: request_digest.into(),
        context_digest: canonical_digest("oasis7.custody.native-stage-context.v1", context)?,
        verify_nonce: nonce.into(),
        verification_digest: None,
        checks: vec![SignatureCheck {
            key_id: key_id.into(),
            public_key_hex: public_key.into(),
            signature_base64url: signature.clone(),
            canonical_bytes: bytes.map_err(|_| ApprovalAdmissionError::Invalid)?,
        }],
    })
}

pub(crate) fn append_native_history_checks(
    mut source: ExactSignatureChecks,
    context: &ApprovalAdmissionContext,
    fact: NativeFact<'_>,
    key_id: &str,
    public_key: &str,
    request_digest: &str,
) -> Result<ExactSignatureChecks> {
    let native = registered_native_fact_checks(
        &source.operation_id,
        &source.verify_nonce,
        context,
        fact,
        key_id,
        public_key,
        request_digest,
    )?;
    source.checks.extend(native.checks);
    source.body_digest = canonical_digest(
        "oasis7.custody.native-stage-source-and-history.v1",
        &(source.body_digest, request_digest),
    )?;
    Ok(source)
}
