//! Completed original Resource consent is historical; Current is checked separately.
use super::*;
use oasis7_custody_core::{approval::CustodyIntentV1, approval_admission::ApprovalAdmissionInput};
fn unavailable() -> StoreError {
    reject("resource_final_consent_unavailable")
}
fn is_resource(request: &ApprovalOptionsRequestV1) -> bool {
    matches!(&request.intent,CustodyIntentV1::OwnerSetupV2 {plan} if plan.selection.resource().is_some())
}
pub(super) fn historical(
    tx: &IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    input: &ApprovalAdmissionInput,
) -> Result<ApprovalRecord, StoreError> {
    if !is_resource(&input.request) {
        return Err(unavailable());
    }
    let proof = &input.approval.proof;
    let saved = tx
        .get::<ApprovalRecord>(&approval_key(&proof.approval_id))?
        .ok_or_else(unavailable)?
        .value;
    let completed: SignedCustodyApprovalV1 =
        serde_json::from_slice(saved.proof_bytes.as_deref().ok_or_else(unavailable)?)
            .map_err(|_| unavailable())?;
    if saved.complete_digest.is_none()
        || saved.request != input.request
        || completed != input.approval
        || saved.intent_digest != input.request.intent_digest().map_err(|_| unavailable())?
        || saved.model_budget_summary_contract != proof.model_budget_summary_contract
    {
        return Err(unavailable());
    }
    let old_configuration = saved
        .guarded_consent_provenance
        .as_ref()
        .and_then(|p| p.owner_resource_configuration_digest.as_deref());
    super::guarded_consent::require_guarded_consent_v2(
        tx,
        config,
        identity,
        &saved,
        super::guarded_consent::GuardedConsentUseV2::OwnerSetupResourceFinal,
        old_configuration,
    )?;
    input
        .request
        .validate_native_summary_contract(
            saved.model_budget_summary_contract.as_ref(),
            &saved.manager_account_id,
            Some(&hex::encode(config.key.verifying_key().to_bytes())),
            proof.issued_at.0,
        )
        .map_err(|_| unavailable())?;
    let request = ApprovalVerifyRequestV1 {
        approval: input.approval.clone(),
        proof_digest: input.approval.digest().map_err(|_| unavailable())?,
        expected_audience: proof.context.audience.clone(),
        expected_intent_digest: proof.intent_digest.clone(),
        purpose: VerificationPurposeV1::InspectOrigin,
        verify_nonce: "resource-history-only-v2".into(),
    };
    super::origin_observation::validate_verification_request(config, identity, &request)?;
    Ok(saved)
}
pub(super) fn current(
    tx: &IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    input: &ApprovalAdmissionInput,
    expected_configuration: &str,
    now: u64,
) -> Result<(), StoreError> {
    let saved = historical(tx, config, identity, input)?;
    super::guarded_consent::require_guarded_consent_v2(
        tx,
        config,
        identity,
        &saved,
        super::guarded_consent::GuardedConsentUseV2::OwnerSetupResourceFinal,
        Some(expected_configuration),
    )?;
    let (account, session) = management_session(
        tx,
        &saved.session_storage_key,
        identity.rp.identity_instance_epoch,
        now,
    )?;
    super::complete::check_record(
        &saved,
        &account,
        &session,
        &saved.session_storage_key,
        &saved.intent_digest,
    )?;
    let p = &input.approval.proof;
    let credential = tx
        .get::<CredentialRecord>(&credential_key(&p.passkey_credential_id))?
        .ok_or_else(unavailable)?
        .value;
    if p.context != input.request.context
        || p.approval_request_id != input.request.approval_request_id
        || p.manager_account_id != account.account_id
        || p.manager_session_ref != session.session_id
        || p.manager_policy_version.0 != account.policy_version
        || p.manager_security_version.0 != account.security_epoch
        || p.identity_instance_epoch.0 != identity.rp.identity_instance_epoch
        || p.origin_credential_id != session.origin_credential_id
        || p.origin_credential_version.0 != session.origin_credential_version
        || p.issued_at.0 > now
        || credential.account_id != account.account_id
        || credential.state != "active"
        || credential.version != p.passkey_credential_version.0
        || !matches!(
            &credential.public_material,
            CredentialMaterial::WebAuthn { .. }
        )
    {
        return Err(unavailable());
    }
    Ok(())
}
