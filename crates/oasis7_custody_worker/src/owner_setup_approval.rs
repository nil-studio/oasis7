//! Fresh MFA consumption and independent setup budget are one Account SQL transition.
use super::*;
use crate::owner_setup_rpc::OwnerSetupApproveRpc;
use oasis7_custody_core::provisioning::OwnerSetupProgressV1;

pub(super) async fn approve(
    state: &JsValue,
    env: &Env,
    rpc: OwnerSetupApproveRpc,
) -> Result<OwnerSetupProgressV1> {
    rpc.request.validate().map_err(|_| denied())?;
    setup_authority::enabled(env)?;
    NativeConfig::load(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let draft = saved.state.draft();
    let request = rpc.request;
    let id = request.input.operation_id.clone();
    let config = super::super::super::Config::load(env)?;
    if config.directory.principal(&rpc.manager_session)? != draft.entry.manager
        || rpc.provision_id != draft.entry.provision_id
        || request.plan.provision_id != draft.entry.provision_id
        || request.input.request.context.custody_account_id != draft.entry.custody_account_id
    {
        return Err(denied());
    }
    if let Some(old) = saved.state.owner_setup_original(&id) {
        if old.input() != &request.input || saved.state.owner_setup_plan(&id) != Some(&request.plan)
        {
            return Err(denied());
        }
        if old.stage() == oasis7_custody_core::approval_admission::ApprovalAdmissionStage::Consumed
        {
            return saved.state.owner_setup_progress(&id).map_err(|_| denied());
        }
    }
    let before =
        setup_authority::observe_manager(env, &saved, request.input.clone(), rpc.manager_session)
            .await?;
    let pending = store
        .begin_owner_setup(request.input.clone(), request.plan, || {
            setup_authority::current(state, env, &before, true)
        })
        .await
        .map_err(|_| denied())?;
    let verify = pending.native_verify_request().map_err(|_| denied())?;
    let raw = serde_json::to_string(&verify)?;
    let signed = serde_json::from_str(&super::super::super::text(
        super::super::super::verify_native(env, &raw)
            .await
            .map_err(|_| denied())?,
    )?)
    .map_err(|_| denied())?;
    let after = setup_authority::observe(env, &saved, request.input, &signed, false).await?;
    store
        .commit_owner_setup(&id, signed, || {
            setup_authority::current(state, env, &after, true)
        })
        .await
        .map_err(|_| denied())?;
    store
        .snapshot()
        .map_err(|_| denied())?
        .state
        .owner_setup_progress(&id)
        .map_err(|_| denied())
}

pub(super) async fn basis(
    state: &JsValue,
    env: &Env,
    rpc: crate::owner_setup_rpc::OwnerSetupBasisRpc,
) -> Result<oasis7_custody_core::provisioning::OwnerSetupBasisV1> {
    rpc.request.validate().map_err(|_| denied())?;
    setup_authority::management_read(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        |state, current, now| state.owner_setup_basis(current, now).map_err(|_| denied()),
    )
    .await
}
pub(super) async fn prepare(
    state: &JsValue,
    env: &Env,
    rpc: crate::owner_setup_rpc::OwnerSetupPrepareRpc,
) -> Result<oasis7_custody_core::provisioning::OwnerSetupPrepareResponseV1> {
    rpc.request.validate().map_err(|_| denied())?;
    setup_authority::management_read(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        |state, current, now| {
            state
                .owner_setup_prepare_request(&rpc.request, current, now)
                .map_err(|_| denied())
        },
    )
    .await
}
