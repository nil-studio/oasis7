//! Request construction only; the real native UV authority must approve this new intent.
//! This harness never imports a proof, changes Account state, or extends old Provision rights.
use super::*;
use crate::provision_approval_store::ProvisionApprovalSnapshot;
use oasis7_custody_core::provisioning::NativeStageResultV1;
use serde::Deserialize;

pub(super) async fn handle(mut request: Request) -> Result<Response> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        snapshot: ProvisionApprovalSnapshot,
    }
    let input: Input = request.json().await?;
    let state = &input.snapshot.state;
    let now = js_sys::Date::now() as u64;
    if now < state.draft().plan.expires_at.0
        || state.provision().security_epoch().0 != 1
        || state.bootstrap_activation().is_none()
    {
        return Err(failure(
            "fixture requires expired Provision and real stored activation",
        ));
    }
    let blocked = state
        .native_stages()
        .values()
        .find(|v| matches!(v.result, Some(NativeStageResultV1::ExplicitLoginRequired)))
        .ok_or_else(|| failure("missing explicit login requirement"))?;
    let binding = state
        .native_stages()
        .values()
        .find_map(|v| match &v.result {
            Some(NativeStageResultV1::BindingReceipt(v)) => Some(&v.receipt),
            _ => None,
        })
        .ok_or_else(|| failure("missing native binding"))?;
    let mut context = state
        .accepted_approval()
        .map_err(failure)?
        .input()
        .request
        .context
        .clone();
    context.security_version = state.provision().security_epoch();
    let request = ApprovalOptionsRequestV1 {
        approval_request_id: crate::approval_crypto::verification_nonce().map_err(failure)?,
        context,
        intent: CustodyIntentV1::ManagedLogin {
            native_account_id: binding.native_account_id.clone(),
            login_key_ref: binding.login_key_ref.clone(),
            native_identity_namespace: binding.identity_namespace.clone(),
            blocked_operation_ref: blocked.request.operation_id.clone(),
            login_request_id: crate::approval_crypto::verification_nonce().map_err(failure)?,
            transport_limit: DecimalU64(64),
            deadline: DecimalU64(
                now.checked_add(60_000)
                    .ok_or_else(|| failure("clock overflow"))?,
            ),
        },
    };
    request.validate(now).map_err(failure)?;
    Response::from_json(
        &json!({"request":request,"expired_provision_deadline":state.draft().plan.expires_at,"target_security_epoch":state.provision().security_epoch(),"requires_fresh_native_uv":true}),
    )
}
