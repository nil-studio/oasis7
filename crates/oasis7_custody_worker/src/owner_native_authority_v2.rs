//! Explicit V2 source observations; ordinary V1 entrypoints keep rejecting V2.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Profile {
    OwnerSetup,
    TestSession,
}
pub(super) fn profile(env: &Env, input: &ApprovalAdmissionInput) -> Result<Profile> {
    use oasis7_custody_core::approval::CustodyIntentV1;
    let (profile, flag) = match &input.request.intent {
        CustodyIntentV1::OwnerSetupV2 { plan } => {
            plan.validate().map_err(|_| denied())?;
            (Profile::OwnerSetup, "CUSTODY_OWNER_SETUP_V2_ENABLED")
        }
        CustodyIntentV1::TestSessionV2 { plan, .. } => {
            plan.validate().map_err(|_| denied())?;
            (Profile::TestSession, "CUSTODY_TEST_SESSION_V2_ENABLED")
        }
        _ => return Err(denied()),
    };
    if env.var(flag).ok().map(|v| v.to_string()).as_deref() != Some("true") {
        return Err(denied());
    }
    Ok(profile)
}
pub(super) fn current_v2(
    state: &JsValue,
    env: &Env,
    observed: &Observations,
    require_business: bool,
) -> std::result::Result<NativeStageCurrent, JsValue> {
    let expected = profile(env, &observed.input)
        .map_err(|_| JsValue::from_str("native authority unavailable"))?;
    if observed.custody_v2 != Some(expected) {
        return Err(JsValue::from_str("native authority unavailable"));
    }
    build_current(state, env, observed, require_business)
        .map_err(|_| JsValue::from_str("native authority unavailable"))
}

pub(super) async fn observe_v2(
    env: &Env,
    draft: &oasis7_custody_core::provisioning::AccountDraftV1,
    input: ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
    inspection_only: bool,
) -> Result<Observations> {
    let profile = profile(env, &input)?;
    let mut result = observe_impl(env, draft, input, signed, inspection_only).await?;
    result.custody_v2 = Some(profile);
    Ok(result)
}
pub(super) async fn observe_manager_v2(
    env: &Env,
    draft: &oasis7_custody_core::provisioning::AccountDraftV1,
    input: ApprovalAdmissionInput,
    manager: oasis7_custody_core::approval::CustodySessionStatusV1,
) -> Result<Observations> {
    let profile = profile(env, &input)?;
    let mut result = observe_fresh_manager_impl(env, draft, input, manager).await?;
    result.custody_v2 = Some(profile);
    Ok(result)
}
pub(super) async fn observe_paid_factory_v2(
    env: &Env,
    draft: &oasis7_custody_core::provisioning::AccountDraftV1,
    input: ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
) -> Result<Observations> {
    let profile = profile(env, &input)?;
    let mut result = observe_paid_test_session_factory_impl(env, draft, input, signed).await?;
    result.custody_v2 = Some(profile);
    Ok(result)
}

pub(super) async fn observe_paid_owner_setup_result_v2(
    env: &Env,
    draft: &oasis7_custody_core::provisioning::AccountDraftV1,
    input: ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
) -> Result<Observations> {
    let actual = profile(env, &input)?;
    if actual != Profile::OwnerSetup {
        return Err(denied());
    }
    let mut result = super::observe_paid_owner_setup_result(env, draft, input, signed).await?;
    result.custody_v2 = Some(actual);
    Ok(result)
}
