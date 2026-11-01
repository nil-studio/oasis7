//! Native-owned basis producer; never supplied by the browser or model.
use super::*;
use oasis7_custody_core::approval::{
    NativeFullSessionProxyProfileV2, NativeFullSessionProxySummaryContractV2,
    NativeModelProxyBasisV2,
};
use oasis7_custody_core::provisioning::ModelBudgetApprovalContractV1;
pub(crate) trait ModelProxyBasisAuthority: Send + Sync {
    fn public_metadata(&self) -> Value;
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
