//! Fresh MFA is consumed once into a control-only preflight authority.
use super::*;
use oasis7_custody_core::time_units::UnixMillis;
pub(crate) async fn basis_v2(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionBasisRpcV2,
) -> Result<TestSessionBasisV2> {
    enabled_v2(env)?;
    rpc.request.validate().map_err(|_| denied())?;
    let request = rpc.request;
    session_authority::management_read(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        &request.enrollment_id,
        |s, management, native_scope, enrollment, deployments, time| {
            s.test_session_basis_v2(
                &request,
                &TestSessionManagementCurrent {
                    management,
                    native_scope,
                    enrollment,
                    deployments,
                },
                UnixMillis::new(time),
            )
            .map_err(|_| denied())
        },
    )
    .await
}
pub(crate) async fn prepare_v2(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionPrepareRpcV2,
) -> Result<oasis7_custody_core::approval::ApprovalOptionsRequestV1> {
    enabled_v2(env)?;
    rpc.request.validate().map_err(|_| denied())?;
    let request = rpc.request;
    session_authority::management_read(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        &request.plan.enrollment_id,
        |s, management, native_scope, enrollment, deployments, time| {
            s.test_session_prepare_request_v2(
                &request,
                &TestSessionManagementCurrent {
                    management,
                    native_scope,
                    enrollment,
                    deployments,
                },
                UnixMillis::new(time),
            )
            .map_err(|_| denied())
        },
    )
    .await
}
async fn approve_authority(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionApproveRpcV2,
) -> Result<TestSessionAuthorityV2> {
    enabled_v2(env)?;
    rpc.request.validate().map_err(|_| denied())?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let config = super::super::super::Config::load(env)?;
    if config.directory.principal(&rpc.manager_session)? != saved.state.draft().entry.manager
        || rpc.provision_id != saved.state.draft().entry.provision_id
    {
        return Err(denied());
    }
    let id = rpc.request.input.operation_id.clone();
    if let Some(old) = saved.state.test_session_v2_original(&id) {
        if old.input() != &rpc.request.input {
            return Err(denied());
        }
        if old.stage() == oasis7_custody_core::approval_admission::ApprovalAdmissionStage::Consumed
        {
            return saved
                .state
                .test_session_v2_snapshot(&id)
                .cloned()
                .map_err(|_| denied());
        }
    }
    let nonce = match saved.state.test_session_v2_original(&id) {
        Some(old) => old.verify_nonce().to_owned(),
        None => crate::approval_crypto::verification_nonce().map_err(|_| denied())?,
    };
    let request = rpc.request;
    let admission = session_authority::management_read(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        enrollment_id_v2(&request)?,
        |s, management, native_scope, enrollment, deployments, time| {
            s.test_session_management_admission_v2(
                &request,
                &nonce,
                &TestSessionManagementCurrent {
                    management,
                    native_scope,
                    enrollment,
                    deployments,
                },
                UnixMillis::new(time),
            )
            .map_err(|_| denied())
        },
    )
    .await?;
    let started = clock()?;
    let enrolled = session_authority::enrollment(env, enrollment_id_v2(&request)?).await?;
    let before = session_authority::observe_manager_v2(
        env,
        &saved,
        admission.input.clone(),
        rpc.manager_session,
    )
    .await?;
    store
        .begin_test_session_v2(admission.input.clone(), || {
            session_authority::approval_current_v2(state, env, &before, &enrolled, started)
        })
        .await
        .map_err(|_| denied())?;
    // Charging and once-claim occur before the fixed native HTTP bridge.
    let verify = store
        .claim_test_session_v2_approval(&id)
        .map_err(|_| denied())?;
    let signed: oasis7_custody_core::approval::SignedApprovalVerificationV1 =
        serde_json::from_str(&super::super::super::text(
            super::super::super::verify_native(env, &serde_json::to_string(&verify)?)
                .await
                .map_err(|_| denied())?,
        )?)
        .map_err(|_| denied())?;
    let started = clock()?;
    let enrolled = session_authority::enrollment(env, enrollment_id_v2(&request)?).await?;
    let after = session_authority::observe_v2(env, &saved, admission.input, &signed, false).await?;
    store
        .consume_test_session_v2(
            CommitTestSessionV2 {
                session_id: id,
                signed,
                source_nonce: crate::approval_crypto::verification_nonce().map_err(|_| denied())?,
            },
            || session_authority::approval_current_v2(state, env, &after, &enrolled, started),
        )
        .await
        .map_err(|_| denied())
}

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
fn enrollment_id_v2(request: &TestSessionApproveRequestV2) -> Result<&str> {
    let oasis7_custody_core::approval::CustodyIntentV1::TestSessionV2 { plan, .. } =
        &request.input.request.intent
    else {
        return Err(denied());
    };
    Ok(&plan.enrollment_id)
}
pub(crate) async fn approve_v2(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionApproveRpcV2,
) -> Result<TestSessionApprovalProgressV2> {
    let id = rpc.request.input.operation_id.clone();
    approve_authority(state, env, rpc).await?;
    OwnerProvisionApprovalStore::open(state.clone())
        .map_err(|_| denied())?
        .snapshot()
        .map_err(|_| denied())?
        .state
        .test_session_v2_approval_progress(&id)
        .map_err(|_| denied())
}

#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/test_session_rpc.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch,js_name=runTestSessionPreflight)]
    async fn run_preflight_v2(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
pub(crate) async fn continue_v2(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionContinueRpcV2,
) -> Result<TestSessionActivationProgressV2> {
    enabled_v2(env)?;
    rpc.request.validate().map_err(|_| denied())?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let id = &rpc.request.session_id;
    let session = saved
        .state
        .test_session_v2_snapshot(id)
        .map_err(|_| denied())?;
    let config = super::super::super::Config::load(env)?;
    let proof = &session.input().approval.proof;
    if rpc.provision_id != saved.state.draft().entry.provision_id
        || config.directory.principal(&rpc.manager_session)? != saved.state.draft().entry.manager
        || proof.manager_account_id != rpc.manager_session.account_id
        || proof.manager_session_ref != rpc.manager_session.session_ref
        || proof.origin_credential_id != rpc.manager_session.credential_ref
    {
        return Err(denied());
    }
    if session.approved_at().is_none()
        && session.closing_metadata().is_none()
        && session.preflight_claimed_at().is_none()
    {
        store
            .claim_test_session_v2_preflight(id)
            .map_err(|_| denied())?;
        let observed = session_authority::observe_manager_v2(
            env,
            &saved,
            session.input().clone(),
            rpc.manager_session,
        )
        .await?;
        session_authority::current_v2(state, env, &observed, true).map_err(|_| denied())?;
        let selector = TestSessionSelectorV1 {
            api_version: 2,
            custody_account_id: saved.state.draft().entry.custody_account_id.clone(),
            provision_id: rpc.provision_id.clone(),
            session_id: id.clone(),
        };
        run_preflight_v2(env, &serde_json::to_string(&selector)?)
            .await
            .map_err(|_| denied())?;
    }
    let current = store.snapshot().map_err(|_| denied())?;
    let session = current
        .state
        .test_session_v2_snapshot(id)
        .map_err(|_| denied())?;
    Ok(TestSessionActivationProgressV2 {
        api_version: 2,
        custody_account_id: current.state.draft().entry.custody_account_id.clone(),
        provision_id: rpc.provision_id,
        session_id: id.clone(),
        activated_at: session.approved_at(),
        preflight_pending: session.approved_at().is_none()
            && session.preflight_claimed_at().is_some(),
        closing: session.closing_metadata().is_some(),
    })
}
