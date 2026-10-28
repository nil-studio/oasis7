//! Fresh MFA is consumed once into a control-only preflight authority.
use super::*;
use oasis7_custody_core::time_units::UnixMillis;
pub(crate) async fn basis(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionBasisRpc,
) -> Result<TestSessionBasisV1> {
    session_authority::enabled(env)?;
    rpc.request.validate().map_err(|_| denied())?;
    let request = rpc.request;
    session_authority::management_read(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        &request.enrollment_id,
        |s, management, native_scope, enrollment, deployments, time| {
            s.test_session_basis(
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
pub(crate) async fn prepare(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionPrepareRpc,
) -> Result<TestSessionPrepareResponseV1> {
    session_authority::enabled(env)?;
    rpc.request.validate().map_err(|_| denied())?;
    let request = rpc.request;
    session_authority::management_read(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        &request.plan.enrollment_id,
        |s, management, native_scope, enrollment, deployments, time| {
            s.test_session_prepare_request(
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
    rpc: TestSessionApproveRpc,
) -> Result<TestSessionAuthorityV1> {
    session_authority::enabled(env)?;
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
    if let Some(old) = saved.state.test_session_original(&id) {
        if old.input() != &rpc.request.input {
            return Err(denied());
        }
        if old.stage() == oasis7_custody_core::approval_admission::ApprovalAdmissionStage::Consumed
        {
            return saved
                .state
                .test_sessions()
                .get(&id)
                .cloned()
                .ok_or_else(denied);
        }
    }
    let nonce = match saved.state.test_session_original(&id) {
        Some(old) => old.verify_nonce().to_owned(),
        None => crate::approval_crypto::verification_nonce().map_err(|_| denied())?,
    };
    let request = rpc.request;
    let admission = session_authority::management_read(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        &request.plan.enrollment_id,
        |s, management, native_scope, enrollment, deployments, time| {
            s.test_session_management_admission(
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
    let enrolled = session_authority::enrollment(env, &request.plan.enrollment_id).await?;
    let before = session_authority::observe_manager(
        env,
        &saved,
        admission.input.clone(),
        rpc.manager_session,
    )
    .await?;
    store
        .begin_test_session(admission.input.clone(), admission.termination, || {
            session_authority::approval_current(state, env, &before, &enrolled, started)
        })
        .await
        .map_err(|_| denied())?;
    // Charging and once-claim occur before the fixed native HTTP bridge.
    let verify = store
        .claim_test_session_approval(&id)
        .map_err(|_| denied())?;
    let signed: oasis7_custody_core::approval::SignedApprovalVerificationV1 =
        serde_json::from_str(&super::super::super::text(
            super::super::super::verify_native(env, &serde_json::to_string(&verify)?)
                .await
                .map_err(|_| denied())?,
        )?)
        .map_err(|_| denied())?;
    let started = clock()?;
    let enrolled = session_authority::enrollment(env, &request.plan.enrollment_id).await?;
    let after = session_authority::observe(env, &saved, admission.input, &signed, false).await?;
    store
        .consume_test_session(
            CommitTestSessionV1 {
                session_id: id,
                signed,
                source_nonce: crate::approval_crypto::verification_nonce().map_err(|_| denied())?,
            },
            || session_authority::approval_current(state, env, &after, &enrolled, started),
        )
        .await
        .map_err(|_| denied())
}

#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/test_session_rpc.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch,js_name=runTestSessionPreflight)]
    async fn run_preflight(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
async fn continue_authority(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionContinueRpc,
) -> Result<TestSessionAuthorityV1> {
    session_authority::enabled(env)?;
    rpc.request.validate().map_err(|_| denied())?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let id = &rpc.request.session_id;
    let s = saved.state.test_sessions().get(id).ok_or_else(denied)?;
    let config = super::super::super::Config::load(env)?;
    let proof = &s.input.approval.proof;
    if rpc.provision_id != saved.state.draft().entry.provision_id
        || config.directory.principal(&rpc.manager_session)? != saved.state.draft().entry.manager
        || proof.manager_account_id != rpc.manager_session.account_id
        || proof.manager_session_ref != rpc.manager_session.session_ref
        || proof.origin_credential_id != rpc.manager_session.credential_ref
    {
        return Err(denied());
    }
    if s.approved_at.is_some() || s.closing.is_some() || s.preflight_claimed_at.is_some() {
        return Ok(s.clone());
    }
    // Local prepayment precedes the first Directory/Fence/binding I/O.
    store
        .claim_test_session_preflight(id)
        .map_err(|_| denied())?;
    let observed =
        session_authority::observe_manager(env, &saved, s.input.clone(), rpc.manager_session)
            .await?;
    session_authority::current(state, env, &observed, true).map_err(|_| denied())?;
    let selector = TestSessionSelectorV1 {
        api_version: 1,
        custody_account_id: saved.state.draft().entry.custody_account_id.clone(),
        provision_id: rpc.provision_id,
        session_id: id.clone(),
    };
    run_preflight(env, &serde_json::to_string(&selector)?)
        .await
        .map_err(|_| denied())?;
    store
        .snapshot()
        .map_err(|_| denied())?
        .state
        .test_sessions()
        .get(id)
        .cloned()
        .ok_or_else(denied)
}

pub(crate) async fn approve(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionApproveRpc,
) -> Result<TestSessionProgressV1> {
    let id = rpc.request.input.operation_id.clone();
    approve_authority(state, env, rpc).await?;
    progress(state, &id)
}
pub(crate) async fn continue_session(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionContinueRpc,
) -> Result<TestSessionProgressV1> {
    let id = rpc.request.session_id.clone();
    continue_authority(state, env, rpc).await?;
    progress(state, &id)
}
fn progress(state: &JsValue, id: &str) -> Result<TestSessionProgressV1> {
    OwnerProvisionApprovalStore::open(state.clone())
        .map_err(|_| denied())?
        .snapshot()
        .map_err(|_| denied())?
        .state
        .test_session_progress(id, UnixMillis::new(clock()?))
        .map_err(|_| denied())
}

pub(crate) async fn invite(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionInviteRpc,
) -> Result<PairingInviteOriginalV1> {
    session_authority::enabled(env).map_err(|e| invite_diagnostic("enabled", e))?;
    rpc.request
        .validate()
        .map_err(|_| invite_diagnostic("request", denied()))?;
    session_authority::management_only(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        |_, current, _| {
            OwnerProvisionApprovalStore::open(state.clone())
                .map_err(|_| invite_diagnostic("store_open", denied()))?
                .create_test_session_invite(rpc.request, current)
                .map_err(|_| invite_diagnostic("store_transition", denied()))
        },
    )
    .await
    .map_err(|e| invite_diagnostic("metadata_or_store", e))
}

fn invite_diagnostic(stage: &str, error: worker::Error) -> worker::Error {
    #[cfg(feature = "approval-harness")]
    worker::console_warn!("test_session.invite.{}", stage);
    #[cfg(not(feature = "approval-harness"))]
    let _ = stage;
    error
}
