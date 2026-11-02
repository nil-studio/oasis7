//! Existing Native current-origin observation transaction body, shared with
//! the explicitly authenticated machine mode. It does not consume new MFA.
use super::*;
pub(super) fn observe_origin_current(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    request: &ApprovalVerifyRequestV1,
    now: u64,
) -> Result<ApprovalVerificationV1, StoreError> {
    let proof = &request.approval.proof;
    let epoch = identity.rp.identity_instance_epoch;
    consume_quota(tx, "custody-verify", "adapter", now, 60_000, 300)?;
    revocation::overflow::require_participant(tx, &proof.manager_account_id)?;
    let saved = tx
        .get::<ApprovalRecord>(&approval_key(&proof.approval_id))?
        .ok_or_else(|| reject("approval_history_unavailable"))?
        .value;
    let original: SignedCustodyApprovalV1 =
        serde_json::from_slice(saved.proof_bytes.as_deref().ok_or(StoreError::Corrupt)?)
            .map_err(|_| StoreError::Corrupt)?;
    if original != request.approval {
        return Err(reject("approval_original_mismatch"));
    }
    // A new Resource consume cannot arm from an older unprotected metadata ceremony.
    // InspectOrigin remains available to independently authorized original-history paths.
    if request.purpose == VerificationPurposeV1::ConsumeApproval
        && matches!(&saved.request.intent,
        CustodyIntentV1::OwnerSetupV2 {plan} if plan.selection.resource().is_some())
    {
        super::guarded_consent::verify_existing_guarded_consent_v2(tx, config, identity, &saved)?;
    }
    let source_session = tx.get::<AccountSession>(&saved.session_storage_key)?;
    let passkey = tx.get::<CredentialRecord>(&credential_key(&proof.passkey_credential_id))?;
    let account = tx.get::<AccountRecord>(&format!("account:{}", proof.manager_account_id))?;
    let origin_credential =
        tx.get::<CredentialRecord>(&credential_key(&proof.origin_credential_id))?;
    let origin_active =
        valid_session(tx, &saved.session_storage_key, epoch, now).is_ok_and(|(a, s)| {
            a.policy_version == proof.manager_policy_version.0
                && a.security_epoch == proof.manager_security_version.0
                && s.session_id == proof.manager_session_ref
                && s.origin_credential_id == proof.origin_credential_id
                && s.origin_credential_version == proof.origin_credential_version.0
        }) && passkey.as_ref().is_some_and(|c| {
            c.value.account_id == proof.manager_account_id
                && c.value.state == "active"
                && c.value.version == proof.passkey_credential_version.0
                && matches!(c.value.public_material, CredentialMaterial::WebAuthn { .. })
        });
    let origin_state = if source_session.is_none()
        || account.is_none()
        || passkey.is_none()
        || origin_credential.is_none()
    {
        OriginStateV1::Unavailable
    } else if origin_active {
        OriginStateV1::Active
    } else {
        OriginStateV1::Revoked
    };
    let proof_state = if proof.expires_at.0 <= now {
        ProofStateV1::Expired
    } else {
        ProofStateV1::Active
    };
    if request.purpose == VerificationPurposeV1::ConsumeApproval
        && (proof_state != ProofStateV1::Active || origin_state != OriginStateV1::Active)
    {
        return Err(reject("approval_expired_or_revoked"));
    }
    Ok(ApprovalVerificationV1 {
        version: 1,
        key_id: config.key_id.clone(),
        identity_namespace: identity.namespace.key(),
        purpose: request.purpose,
        proof_digest: request.proof_digest.clone(),
        approval_request_id: proof.approval_request_id.clone(),
        audience: proof.context.audience.clone(),
        intent_digest: proof.intent_digest.clone(),
        verify_nonce: request.verify_nonce.clone(),
        manager_security_version: DecimalU64(
            account.as_ref().map_or(0, |a| a.value.security_epoch),
        ),
        manager_policy_version: DecimalU64(account.as_ref().map_or(0, |a| a.value.policy_version)),
        origin_credential_version: DecimalU64(origin_credential.map_or(0, |c| c.value.version)),
        identity_instance_epoch: DecimalU64(epoch),
        credential_version: DecimalU64(passkey.map_or(0, |c| c.value.version)),
        observed_at: DecimalU64(now),
        proof_expires_at: proof.expires_at,
        source_session_expires_at: DecimalU64(
            source_session.as_ref().map_or(0, |s| s.value.expires_at),
        ),
        source_session_idle_expires_at: DecimalU64(
            source_session
                .as_ref()
                .map_or(0, |s| s.value.idle_expires_at),
        ),
        proof_state,
        origin_state,
    })
}

pub(super) fn validate_verification_request(
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    request: &ApprovalVerifyRequestV1,
) -> Result<(), StoreError> {
    let proof = &request.approval.proof;
    if request.verify_nonce.is_empty()
        || request.verify_nonce.len() > 256
        || proof.version != 1
        || proof.key_id != config.key_id
        || proof.identity_namespace != identity.namespace.key()
        || request.expected_audience != config.audience
        || proof.context.audience != request.expected_audience
        || proof.context.environment != identity.namespace.environment_id
        || proof.intent_digest != request.expected_intent_digest
        || request.approval.digest().map_err(|_| StoreError::Corrupt)? != request.proof_digest
    {
        return Err(reject("approval_binding_mismatch"));
    }
    let signature = URL_SAFE_NO_PAD
        .decode(&request.approval.signature)
        .ok()
        .and_then(|v| Signature::from_slice(&v).ok())
        .ok_or_else(|| reject("approval_signature_invalid"))?;
    config
        .key
        .verifying_key()
        .verify_strict(
            &proof.signing_bytes().map_err(|_| StoreError::Corrupt)?,
            &signature,
        )
        .map_err(|_| reject("approval_signature_invalid"))?;
    Ok(())
}
