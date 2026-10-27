//! Fixed ordered stage driver. Existing unknown attempts cannot be relaunched by a new HTTP request.
use super::*;
use crate::owner_setup_rpc::{OwnerSetupContinueRpc, OwnerSetupStageRpc};
use oasis7_custody_core::provisioning::*;
use wasm_bindgen::prelude::*;
#[wasm_bindgen(module = "/platform/provision_drive_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=runOwnerSetupStage)]
    async fn run_stage(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
pub(super) async fn continue_setup(
    state: &JsValue,
    env: &Env,
    rpc: OwnerSetupContinueRpc,
) -> Result<OwnerSetupProgressV1> {
    rpc.request.validate().map_err(|_| denied())?;
    setup_authority::enabled(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let draft = saved.state.draft();
    let config = super::super::super::Config::load(env)?;
    if config.directory.principal(&rpc.manager_session)? != draft.entry.manager
        || rpc.provision_id != draft.entry.provision_id
        || rpc.request.provision_id != draft.entry.provision_id
        || rpc.request.custody_account_id != draft.entry.custody_account_id
    {
        return Err(denied());
    }
    let id = &rpc.request.setup_id;
    let plan = store.owner_setup_plan(id).map_err(|_| denied())?;
    if plan.provision_id != draft.entry.provision_id {
        return Err(denied());
    }
    let progress = || {
        store
            .snapshot()
            .map_err(|_| denied())?
            .state
            .owner_setup_progress(id)
            .map_err(|_| denied())
    };
    if saved.state.provision().stage() == ProvisionStage::Prepared {
        return progress();
    }
    if saved
        .state
        .owner_setup_source(id)
        .is_none_or(|v| v.verification.is_none())
    {
        setup_dispatch::prepare_source(state, env, id, None).await?;
    }
    for stage in [
        OwnerSetupStageV1::CreateTask,
        OwnerSetupStageV1::PrepareGoal,
        OwnerSetupStageV1::PublishGoal,
        OwnerSetupStageV1::Delegation,
        OwnerSetupStageV1::Bind,
        OwnerSetupStageV1::FinalObservation,
    ] {
        let current = store.snapshot().map_err(|_| denied())?;
        if let Some(old) = current.state.owner_setup_outbox(id, stage) {
            if old
                .result
                .as_ref()
                .is_some_and(OwnerSetupResultV1::succeeded)
            {
                continue;
            }
            return progress();
        }
        store
            .reserve_owner_setup_stage(id, stage)
            .map_err(|_| denied())?;
        // No native or World I/O precedes the persisted stage budget and nonce.
        setup_dispatch::prepare_source(state, env, id, Some(stage)).await?;
        let raw = serde_json::to_string(&OwnerSetupStageRpc {
            request: rpc.request.clone(),
            stage,
        })?;
        if run_stage(env, &raw).await.is_err() {
            let original = store.owner_setup_outbox(id, stage).map_err(|_| denied())?;
            if matches!(
                original.operation.state(),
                oasis7_custody_core::operations::OperationState::Dispatching
                    | oasis7_custody_core::operations::OperationState::Unknown
            ) {
                store
                    .mark_owner_setup_unknown(id, stage)
                    .map_err(|_| denied())?;
            }
            return progress();
        }
        if store
            .owner_setup_outbox(id, stage)
            .map_err(|_| denied())?
            .result
            .as_ref()
            .is_none_or(|result| !result.succeeded())
        {
            return progress();
        }
    }
    progress()
}
