use super::*;
pub(super) async fn basis(
    state: &JsValue,
    env: &Env,
    rpc: ReconcileBasisRpc,
) -> Result<OwnerSetupReconcileBasisV1> {
    rpc.request.validate().map_err(|_| denied())?;
    setup_authority::management_read(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        |state, c, t| {
            state
                .owner_setup_reconcile_basis(&rpc.request, c, t)
                .map_err(|_| denied())
        },
    )
    .await
}
pub(super) async fn prepare(
    state: &JsValue,
    env: &Env,
    rpc: ReconcilePrepareRpc,
) -> Result<OwnerSetupReconcilePrepareResponseV1> {
    rpc.request.validate().map_err(|_| denied())?;
    setup_authority::management_read(
        state,
        env,
        &rpc.manager_session,
        &rpc.provision_id,
        |state, c, t| {
            state
                .owner_setup_reconcile_prepare_request(&rpc.request, c, t)
                .map_err(|_| denied())
        },
    )
    .await
}
pub(super) async fn approve(
    state: &JsValue,
    env: &Env,
    rpc: ReconcileApproveRpc,
) -> Result<OwnerSetupReconcileProgressV1> {
    rpc.request.validate().map_err(|_| denied())?;
    setup_authority::enabled(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let config = super::super::super::super::Config::load(env)?;
    if config.directory.principal(&rpc.manager_session)? != saved.state.draft().entry.manager
        || rpc.provision_id != saved.state.draft().entry.provision_id
        || rpc.request.plan.provision_id != rpc.provision_id
        || rpc.request.input.request.context.custody_account_id
            != saved.state.draft().entry.custody_account_id
    {
        return Err(denied());
    }
    let id = rpc.request.input.operation_id.clone();
    if let Some(old) = saved.state.owner_setup_reconcile_original(&id) {
        if old.input() != &rpc.request.input
            || saved
                .state
                .owner_setup_reconciles()
                .get(&id)
                .is_none_or(|v| v.plan != rpc.request.plan)
        {
            return Err(denied());
        }
        if old.stage() == oasis7_custody_core::approval_admission::ApprovalAdmissionStage::Consumed
        {
            return saved
                .state
                .owner_setup_reconcile_progress(&id)
                .map_err(|_| denied());
        }
    }
    let before = setup_authority::observe_manager(
        env,
        &saved,
        rpc.request.input.clone(),
        rpc.manager_session,
    )
    .await?;
    let record = store
        .begin_owner_setup_reconcile(rpc.request.input.clone(), rpc.request.plan, || {
            setup_authority::current(state, env, &before, true)
        })
        .await
        .map_err(|_| denied())?;
    let raw = serde_json::to_string(&record.native_verify_request().map_err(|_| denied())?)?;
    let signed = serde_json::from_str(&super::super::super::super::text(
        super::super::super::super::verify_native(env, &raw)
            .await
            .map_err(|_| denied())?,
    )?)
    .map_err(|_| denied())?;
    let after = setup_authority::observe(env, &saved, rpc.request.input, &signed, false).await?;
    store
        .commit_owner_setup_reconcile(&id, signed, || {
            setup_authority::current(state, env, &after, true)
        })
        .await
        .map_err(|_| denied())?;
    store
        .snapshot()
        .map_err(|_| denied())?
        .state
        .owner_setup_reconcile_progress(&id)
        .map_err(|_| denied())
}
