//! Child of custody: the saved human consent is necessary, never sufficient
//! for the separately authenticated managed-SID registration operation.
use super::*;
use oasis7_custody_core::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, SignedAccountConsumedPendingV2,
};

pub(super) fn validate_saved_full_consent(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    pending: &SignedAccountConsumedPendingV2,
    issuer_pin: &AccountMachineIssuerPinV2,
    issuer_floor: &AccountMachineIssuerFloorV2,
    now: u64,
) -> Result<(), StoreError> {
    pending
        .verify_issuer(issuer_pin, issuer_floor, now)
        .map_err(|_| unavailable())?;
    validate_saved_full_input(
        tx,
        config,
        identity,
        &pending.certificate.input,
        &pending.certificate.intent_digest,
    )
}

/// Exact completed protected human ceremony; not Current or a paid HTTP permit.
pub(super) fn validate_saved_full_input(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    input: &oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
    intent_digest: &str,
) -> Result<(), StoreError> {
    if !config.full_session_v2_summary_contract_enabled {
        return Err(unavailable());
    }
    let proof = &input.approval.proof;
    let row = tx
        .get::<ApprovalRecord>(&approval_key(&proof.approval_id))?
        .ok_or_else(unavailable)?;
    let saved = row.value;
    let configuration = saved
        .guarded_consent_provenance
        .as_ref()
        .and_then(|p| p.cleanup_configuration_digest.as_deref());
    super::guarded_consent::require_guarded_consent_v2(
        tx,
        config,
        identity,
        &saved,
        super::guarded_consent::GuardedConsentUseV2::TermWideCleanup,
        configuration,
    )?;
    let completed: SignedCustodyApprovalV1 =
        serde_json::from_slice(saved.proof_bytes.as_deref().ok_or_else(unavailable)?)
            .map_err(|_| unavailable())?;
    // No expiry refresh: Native's current origin/SID checks are separate. This
    // comparison is the exact persisted completed FullTestSessionV2 ceremony.
    if saved.complete_digest.is_none()
        || saved.request != input.request
        || completed != input.approval
        || saved.intent_digest != intent_digest
        || saved.manager_account_id != proof.manager_account_id
        || saved.manager_session_ref != proof.manager_session_ref
        || saved.origin_credential_id != proof.origin_credential_id
        || saved.origin_credential_version != proof.origin_credential_version.0
        || saved.policy_version != proof.manager_policy_version.0
        || saved.security_epoch != proof.manager_security_version.0
        || saved.model_budget_summary_contract != proof.model_budget_summary_contract
        || proof.key_id != config.key_id
        || proof.context.audience != config.audience
        || proof.context.environment != identity.namespace.environment_id
        || proof.identity_namespace != identity.namespace.key()
        || proof.identity_instance_epoch.0 != identity.rp.identity_instance_epoch
    {
        return Err(unavailable());
    }
    input
        .request
        .validate_native_summary_contract(
            saved.model_budget_summary_contract.as_ref(),
            &saved.manager_account_id,
            Some(&hex::encode(config.key.verifying_key().to_bytes())),
            proof.issued_at.0,
        )
        .map_err(|_| unavailable())?;
    let signature: [u8; 64] = URL_SAFE_NO_PAD
        .decode(&completed.signature)
        .map_err(|_| unavailable())?
        .try_into()
        .map_err(|_| unavailable())?;
    config
        .key
        .verifying_key()
        .verify_strict(
            &proof.signing_bytes().map_err(|_| unavailable())?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| unavailable())?;
    Ok(())
}
fn unavailable() -> StoreError {
    StoreError::Rejected("cleanup_full_consent_unavailable".into())
}

pub(super) fn expand_saved_pending_reference(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    reference: &oasis7_custody_core::native_cleanup_v2::AccountConsumedPendingReferenceV2,
    issuer_pin: &AccountMachineIssuerPinV2,
    issuer_floor: &AccountMachineIssuerFloorV2,
    now: u64,
) -> Result<SignedAccountConsumedPendingV2, StoreError> {
    let row = tx
        .get::<ApprovalRecord>(&approval_key(&reference.approval_id))?
        .ok_or_else(unavailable)?;
    let completed: SignedCustodyApprovalV1 =
        serde_json::from_slice(row.value.proof_bytes.as_deref().ok_or_else(unavailable)?)
            .map_err(|_| unavailable())?;
    let input = oasis7_custody_core::approval_admission::ApprovalAdmissionInput {
        operation_id: reference.session_id.clone(),
        request: row.value.request,
        approval: completed,
    };
    let expanded = reference.expand(input).map_err(|_| unavailable())?;
    validate_saved_full_consent(
        tx,
        config,
        identity,
        &expanded,
        issuer_pin,
        issuer_floor,
        now,
    )?;
    Ok(expanded)
}
