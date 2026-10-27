use super::*;
use wasm_bindgen::prelude::*;
#[wasm_bindgen(module = "/platform/provision_drive_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=runOwnerSetupReconcile)]
    async fn run_attempt(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
pub(super) async fn continue_attempt(
    state: &JsValue,
    env: &Env,
    rpc: ReconcileContinueRpc,
) -> Result<OwnerSetupReconcileProgressV1> {
    rpc.request.validate().map_err(|_| denied())?;
    setup_authority::enabled(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let config = super::super::super::super::Config::load(env)?;
    if config.directory.principal(&rpc.manager_session)? != saved.state.draft().entry.manager
        || rpc.provision_id != saved.state.draft().entry.provision_id
    {
        return Err(denied());
    }
    let id = &rpc.request.reconcile_id;
    let attempt = &rpc.request.attempt_id;
    let original = store.reconcile_approval(id).map_err(|_| denied())?;
    if original.plan.provision_id != rpc.provision_id {
        return Err(denied());
    }
    let progress = || {
        store
            .snapshot()
            .map_err(|_| denied())?
            .state
            .owner_setup_reconcile_progress(id)
            .map_err(|_| denied())
    };
    if original.attempts.contains_key(attempt) {
        return progress();
    }
    reconcile_authority::prepare_source(state, env, id, None).await?;
    store
        .reserve_owner_setup_reconcile_attempt(id, attempt)
        .map_err(|_| denied())?;
    reconcile_authority::prepare_source(state, env, id, Some(attempt)).await?;
    let selector = ReconcileSelector {
        custody_account_id: saved.state.draft().entry.custody_account_id.clone(),
        provision_id: rpc.provision_id,
        request: rpc.request.clone(),
    };
    // A failed/unknown single workflow keeps its attempt and budget. Only a new
    // explicitly requested paid attempt may repeat these readonly observations.
    let _response = run_attempt(env, &serde_json::to_string(&selector)?).await;
    progress()
}
