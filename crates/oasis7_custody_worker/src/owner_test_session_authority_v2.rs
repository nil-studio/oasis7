//! V2 observations select only the new map and actual complete V2 original.
use super::*;
fn enabled_v2(env: &Env) -> Result<()> {
    if env
        .var("CUSTODY_TEST_SESSION_V2_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(denied());
    }
    Ok(())
}
pub(crate) async fn observe_v2(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    input: ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
    inspection_only: bool,
) -> Result<authority::Observations> {
    enabled_v2(env)?;
    authority::observe_v2(env, saved.state.draft(), input, signed, inspection_only).await
}
pub(crate) async fn observe_manager_v2(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    input: ApprovalAdmissionInput,
    manager: CustodySessionStatusV1,
) -> Result<authority::Observations> {
    enabled_v2(env)?;
    authority::observe_manager_v2(env, saved.state.draft(), input, manager).await
}
pub(crate) fn current_v2(
    state: &JsValue,
    env: &Env,
    observed: &authority::Observations,
    business: bool,
) -> std::result::Result<NativeStageCurrent, JsValue> {
    enabled_v2(env).map_err(|_| JsValue::from_str("test session unavailable"))?;
    authority::current_v2(state, env, observed, business)
}
fn input_v2(
    saved: &ProvisionApprovalSnapshot,
    request: &TestSessionSelectorV1,
) -> Result<ApprovalAdmissionInput> {
    let draft = saved.state.draft();
    if request.api_version != 2
        || request.custody_account_id != draft.entry.custody_account_id
        || request.provision_id != draft.entry.provision_id
    {
        return Err(denied());
    }
    Ok(saved
        .state
        .test_session_v2_snapshot(&request.session_id)
        .map_err(|_| denied())?
        .input()
        .clone())
}
pub(crate) async fn runtime_observe_v2(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    request: &TestSessionSelectorV1,
    source: &str,
    inspection_only: bool,
) -> Result<authority::Observations> {
    enabled_v2(env)?;
    let input = input_v2(saved, request)?;
    let original = saved
        .state
        .test_session_v2_source(&request.session_id, source)
        .ok_or_else(denied)?;
    let signed = original
        .source
        .verification
        .as_ref()
        .or_else(|| {
            saved
                .state
                .test_session_v2_original(&request.session_id)
                .and_then(|r| r.verification())
        })
        .ok_or_else(denied)?;
    observe_v2(env, saved, input, signed, inspection_only).await
}
/// The fixed Account ticket was committed before this factory performs reads.
pub(crate) async fn runtime_observe_paid_factory_v2(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    request: &TestSessionSelectorV1,
    source: &str,
) -> Result<authority::Observations> {
    enabled_v2(env)?;
    let input = input_v2(saved, request)?;
    let original = saved
        .state
        .test_session_v2_source(&request.session_id, source)
        .ok_or_else(denied)?;
    let signed = original
        .source
        .verification
        .as_ref()
        .or_else(|| {
            saved
                .state
                .test_session_v2_original(&request.session_id)
                .and_then(|r| r.verification())
        })
        .ok_or_else(denied)?;
    authority::observe_paid_factory_v2(env, saved.state.draft(), input, signed).await
}

#[path = "owner_execution_prepare_authority_v2.rs"]
mod execution_prepare;
pub(crate) use execution_prepare::execution_prepare_observe_v2;
