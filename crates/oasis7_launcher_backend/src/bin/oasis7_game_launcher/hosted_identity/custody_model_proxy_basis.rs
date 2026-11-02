//! Native-owned basis producer; never supplied by the browser or model.
use super::*;
use oasis7_custody_core::approval::{
    NativeFullSessionProxyProfileV2, NativeFullSessionProxySummaryContractV2,
    NativeModelProxyBasisV2,
};
use oasis7_custody_core::provisioning::ModelBudgetApprovalContractV1;
pub(crate) trait ModelProxyBasisAuthority: Send + Sync {
    fn public_metadata(&self) -> Value;
    fn active_committed(&self, _signal: ActiveModelSignal) {}
    fn first_reservation_query(
        &self,
        _input: &oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
        _owner: &ModelProxyOwnerBinding,
        _now: u64,
    ) -> Result<
        oasis7_custody_core::native_model_proxy_query_v2::SignedNativeModelReservationQueryV2,
        String,
    > {
        Err("Native model reserve query unsupported".into())
    }

    fn prepare(
        &self,
        request: &ApprovalOptionsRequestV1,
        account: &str,
        session: &str,
        now: u64,
    ) -> Result<NativeModelProxyBasisV2, String>;
    fn validate_saved(
        &self,
        basis: &NativeModelProxyBasisV2,
        request: &ApprovalOptionsRequestV1,
        account: &str,
        session: &str,
        now: u64,
    ) -> Result<(), String>;
}
fn proxy_requested(request: &ApprovalOptionsRequestV1) -> bool {
    match &request.intent {
        CustodyIntentV1::TestSessionV2 { plan, .. } => plan.model_budget.as_ref(),
        CustodyIntentV1::OwnerSetupV2 { plan } => plan.model_budget.as_ref(),
        _ => None,
    }
    .is_some_and(|v| {
        v.approval_contract == ModelBudgetApprovalContractV1::NativeCustodyModelProxyV2
    })
}
pub(super) fn options_contract(
    service: &IdentityService,
    request: &ApprovalOptionsRequestV1,
    account: &str,
    session: &str,
    now: u64,
) -> Result<Option<NativeCustodySummaryContractV1>, StoreError> {
    let config = service
        .custody
        .as_ref()
        .ok_or_else(|| reject("custody_disabled"))?;
    let enabled = match &request.intent {
        CustodyIntentV1::TestSessionV2 { .. } => config.full_session_v2_summary_contract_enabled,
        _ => config.model_budget_summary_contract_enabled,
    };
    if !proxy_requested(request) {
        return if enabled {
            request
                .expected_model_budget_summary_contract()
                .map_err(|_| reject("invalid_request"))
        } else {
            Ok(None)
        };
    }
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &request.intent else {
        return Err(reject("model_proxy_unsupported"));
    };
    if plan.model_budget.as_ref().is_none_or(|budget| {
        budget.native_public_key != hex::encode(config.key.verifying_key().as_bytes())
    }) {
        return Err(reject("model_proxy_native_key_mismatch"));
    }
    if !enabled {
        return Err(reject("model_proxy_unsupported"));
    }
    let authority = service
        .model_proxy_basis
        .as_ref()
        .ok_or_else(|| reject("model_proxy_unsupported"))?;
    let basis = authority
        .prepare(request, account, session, now)
        .map_err(|_| reject("model_proxy_basis_unavailable"))?;
    basis
        .validate_request(request, account, now)
        .map_err(|_| reject("model_proxy_basis_mismatch"))?;
    let Some(NativeCustodySummaryContractV1::FullTestSessionV2(base)) = request
        .expected_model_budget_summary_contract()
        .map_err(|_| reject("invalid_request"))?
    else {
        return Err(reject("model_proxy_unsupported"));
    };
    let basis_digest = basis.digest().map_err(|_| StoreError::Corrupt)?;
    Ok(Some(
        NativeCustodySummaryContractV1::FullTestSessionProxyV2(Box::new(
            NativeFullSessionProxySummaryContractV2 {
                api_version: 2,
                contract: NativeFullSessionProxyProfileV2::FullTestSessionProxyV2,
                full_session: Box::new(base),
                basis: Box::new(basis),
                basis_digest,
            },
        )),
    ))
}
pub(super) fn recheck_before_sign(
    service: &IdentityService,
    saved: &ApprovalRecord,
    now: u64,
) -> Result<(), StoreError> {
    if !proxy_requested(&saved.request) {
        return Ok(());
    }
    let Some(NativeCustodySummaryContractV1::FullTestSessionProxyV2(contract)) =
        saved.model_budget_summary_contract.as_ref()
    else {
        return Err(reject("model_proxy_basis_missing"));
    };
    if contract.basis_digest != contract.basis.digest().map_err(|_| StoreError::Corrupt)? {
        return Err(reject("model_proxy_basis_mismatch"));
    }
    service
        .model_proxy_basis
        .as_ref()
        .ok_or_else(|| reject("model_proxy_unsupported"))?
        .validate_saved(
            &contract.basis,
            &saved.request,
            &saved.manager_account_id,
            &saved.manager_session_ref,
            now,
        )
        .map_err(|_| reject("model_proxy_basis_not_current"))
}

impl IdentityService {
    pub(in crate::hosted_identity) fn model_proxy_capability(&self) -> Value {
        let Some(authority) = &self.model_proxy_basis else {
            return serde_json::json!({"api_version":2,"available":false});
        };
        let mut public = authority.public_metadata();
        if let Some(config) = &self.custody {
            public["native_public_key"] =
                serde_json::json!(hex::encode(config.key.verifying_key().as_bytes()));
        }
        public
    }
    pub(crate) fn attach_model_proxy_basis(
        &mut self,
        authority: std::sync::Arc<dyn ModelProxyBasisAuthority>,
    ) -> Result<(), String> {
        if self.model_proxy_basis.is_some() {
            return Err("model proxy authority already attached".into());
        }
        self.model_proxy_basis = Some(authority);
        Ok(())
    }
}

/// Local Native proof-owner binding. No SID/cookie/key is exported or accepted
/// from the floor caller. Only a completed actual custody ceremony can create it.
pub(crate) struct ModelProxyOwnerBinding {
    account_id: String,
    session_ref: String,
    approved_input_digest: String,
}
impl ModelProxyOwnerBinding {
    pub(crate) fn account_id(&self) -> &str {
        &self.account_id
    }
    pub(crate) fn session_ref(&self) -> &str {
        &self.session_ref
    }
    pub(crate) fn approved_input_digest(&self) -> &str {
        &self.approved_input_digest
    }
}
impl IdentityService {
    pub(crate) fn model_proxy_owner_binding(
        &mut self,
        input: &oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
        now: u64,
    ) -> Result<ModelProxyOwnerBinding, StoreError> {
        if !self.clock.observe(now) {
            return Err(reject("clock_unhealthy"));
        }
        if !proxy_requested(&input.request) {
            return Err(reject("model_proxy_unsupported"));
        }
        let config = self
            .custody
            .as_ref()
            .ok_or_else(|| reject("custody_unavailable"))?;
        if !config.full_session_v2_summary_contract_enabled {
            return Err(reject("model_proxy_disabled"));
        }
        // Ordinary runtime transactions verify an existing external guard.
        // Only new options may initialize it; missing provenance never upgrades.
        let saved = self.store.transaction(|tx| {
            let record = tx
                .get::<ApprovalRecord>(&approval_key(&input.approval.proof.approval_id))?
                .ok_or_else(|| reject("approval_not_found"))?
                .value;
            let (account, session) = management_session(
                tx,
                &record.session_storage_key,
                self.config.rp.identity_instance_epoch,
                now,
            )?;
            if record.request != input.request
                || record.intent_digest
                    != input
                        .request
                        .intent_digest()
                        .map_err(|_| StoreError::Corrupt)?
                || record.manager_account_id != account.account_id
                || record.manager_session_ref != session.session_id
            {
                return Err(reject("model_proxy_owner_mismatch"));
            }
            let proof: SignedCustodyApprovalV1 = serde_json::from_slice(
                record
                    .proof_bytes
                    .as_ref()
                    .ok_or_else(|| reject("model_proxy_consent_missing"))?,
            )
            .map_err(|_| StoreError::Corrupt)?;
            if proof != input.approval {
                return Err(reject("model_proxy_consent_mismatch"));
            }
            super::guarded_consent::require_guarded_consent_v2(
                tx,
                config,
                &self.config,
                &record,
                super::guarded_consent::GuardedConsentUseV2::NativeModelProxy,
                None,
            )?;
            super::complete::check_record(
                &record,
                &account,
                &session,
                &record.session_storage_key,
                &record.intent_digest,
            )?;
            let p = &proof.proof;
            let passkey = tx
                .get::<CredentialRecord>(&credential_key(&p.passkey_credential_id))?
                .ok_or_else(|| reject("model_proxy_passkey_unavailable"))?
                .value;
            if record.complete_digest.is_none()
                || p.context != input.request.context
                || p.approval_request_id != input.request.approval_request_id
                || p.manager_account_id != account.account_id
                || p.manager_session_ref != session.session_id
                || p.manager_policy_version.0 != account.policy_version
                || p.manager_security_version.0 != account.security_epoch
                || p.identity_instance_epoch.0 != self.config.rp.identity_instance_epoch
                || p.origin_credential_id != session.origin_credential_id
                || p.origin_credential_version.0 != session.origin_credential_version
                || p.issued_at.0 > now
                || passkey.account_id != account.account_id
                || passkey.state != "active"
                || passkey.version != p.passkey_credential_version.0
                || !matches!(passkey.public_material, CredentialMaterial::WebAuthn { .. })
            {
                return Err(reject("model_proxy_origin_changed"));
            }
            // Pure signature/binding verifier only: no ordinary SDK quota,
            // authorization callback, Source signature or outbound request.
            super::origin_observation::validate_verification_request(
                config,
                &self.config,
                &ApprovalVerifyRequestV1 {
                    approval: proof.clone(),
                    proof_digest: proof.digest().map_err(|_| StoreError::Corrupt)?,
                    expected_audience: input.request.context.audience.clone(),
                    expected_intent_digest: record.intent_digest.clone(),
                    purpose: VerificationPurposeV1::InspectOrigin,
                    verify_nonce: p.nonce.clone(),
                },
            )?;
            input
                .request
                .validate_native_summary_contract(
                    p.model_budget_summary_contract.as_ref(),
                    &account.account_id,
                    Some(&hex::encode(config.key.verifying_key().as_bytes())),
                    p.issued_at.0,
                )
                .map_err(|_| reject("model_proxy_summary_mismatch"))?;
            Ok(record)
        })?;
        recheck_before_sign(self, &saved, now)?;
        Ok(ModelProxyOwnerBinding {
            account_id: saved.manager_account_id,
            session_ref: saved.manager_session_ref,
            approved_input_digest: input
                .approved_test_session_v2_digest()
                .map_err(|_| StoreError::Corrupt)?,
        })
    }
}

#[path = "custody_model_proxy_first_query.rs"]
mod first_query;

/// Only the actual committed Native registration path constructs this signal.
/// It is local scheduling data, never a Current/dispatch/floor capability.
pub(crate) struct ActiveModelSignal {
    input: oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
}
impl ActiveModelSignal {
    pub(in crate::hosted_identity::custody) fn from_registered_input(
        input: oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
    ) -> Self {
        Self { input }
    }
    pub(crate) fn into_input(
        self,
    ) -> oasis7_custody_core::approval_admission::ApprovalAdmissionInput {
        self.input
    }
}
