//! Fresh positive-epoch ManagedLogin approval. Never revives the old Provision grant.
use super::*;
use crate::directory_rpc::ManagedLoginApproveRpc;
use oasis7_custody_core::provisioning::{ManagedLoginApprovalViewV1, ManagedLoginStageV1};

fn view(store: &OwnerProvisionApprovalStore, id: &str) -> Result<ManagedLoginApprovalViewV1> {
    let saved = store.snapshot().map_err(|_| denied())?;
    let task = saved.state.managed_login_task(id).ok_or_else(denied)?;
    let stage = match &task.result {
        None => ManagedLoginStageV1::Approved,
        Some(NativeStageResultV1::SessionStatus(_)) => ManagedLoginStageV1::NativeSessionReady,
        Some(NativeStageResultV1::ExplicitLoginRequired) => {
            ManagedLoginStageV1::ExplicitLoginRequired
        }
        _ => return Err(denied()),
    };
    Ok(ManagedLoginApprovalViewV1 {
        api_version: 1,
        provision_id: saved.state.draft().entry.provision_id.clone(),
        custody_account_id: saved.state.draft().entry.custody_account_id.clone(),
        operation_id: id.into(),
        stage,
    })
}
pub(super) async fn approve_managed_login(
    state: &JsValue,
    env: &Env,
    rpc: ManagedLoginApproveRpc,
) -> Result<ManagedLoginApprovalViewV1> {
    rpc.request.validate().map_err(|_| denied())?;
    NativeConfig::load(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let draft = saved.state.draft();
    let input = rpc.request.input;
    let config = super::super::Config::load(env)?;
    if config.directory.principal(&rpc.manager_session)? != draft.entry.manager
        || rpc.provision_id != draft.entry.provision_id
        || input.request.context.custody_account_id != draft.entry.custody_account_id
    {
        return Err(denied());
    }
    if let Some(original) = saved.state.managed_login_original(&input.operation_id) {
        if original.input() != &input {
            return Err(denied());
        }
        if saved
            .state
            .managed_login_task(&input.operation_id)
            .is_some()
        {
            // The exact already-consumed fact, with no new native verification, SID or budget.
            return view(&store, &input.operation_id);
        }
    }
    let before =
        authority::observe_fresh_manager(env, draft, input.clone(), rpc.manager_session).await?;
    let record = store
        .begin_managed_login(input.clone(), || {
            authority::current(state, env, &before, true)
        })
        .await
        .map_err(|_| denied())?;
    let request = record.native_verify_request().map_err(|_| denied())?;
    let raw = serde_json::to_string(&request)?;
    let signed = serde_json::from_str(&super::super::text(
        super::super::verify_native(env, &raw)
            .await
            .map_err(|_| denied())?,
    )?)
    .map_err(|_| denied())?;
    let after = authority::observe(env, draft, input.clone(), &signed, false).await?;
    // This consumes the original proof and creates its independent 64-hop task in
    // the one Account SQL/audit transition, after strict async and final exact checks.
    store
        .commit_managed_login(&input.operation_id, signed, || {
            authority::current(state, env, &after, true)
        })
        .await
        .map_err(|_| denied())?;
    view(&store, &input.operation_id)
}
