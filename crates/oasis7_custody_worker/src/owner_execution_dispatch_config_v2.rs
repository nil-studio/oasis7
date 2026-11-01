//! Pure fixed configuration for an existing paid callback; no Directory/RF IO.
use super::*;
use crate::execution_dispatch_native_pins_v2::NativeDispatchHistoryPinsV2;
use oasis7_custody_core::{
    approval::ApprovalContextV1, provisioning::AccountProvisionApprovalState,
};
pub(crate) struct ExecutionDispatchStaticConfigV2 {
    pub native: NativeDispatchHistoryPinsV2,
    pub context: ApprovalContextV1,
    pub world: oasis7_custody_core::approval::ApprovedWorldV1,
    pub namespace: String,
    pub instance_epoch: oasis7_custody_core::DecimalU64,
}
pub(crate) fn execution_dispatch_static_config_v2(
    env: &Env,
    state: &AccountProvisionApprovalState,
) -> Result<ExecutionDispatchStaticConfigV2> {
    let cfg = Config::load(env)?;
    let native = NativeDispatchHistoryPinsV2::load(env).map_err(|_| denied())?;
    let draft = state.draft();
    if cfg.directory.environment != draft.entry.environment {
        return Err(denied());
    }
    Ok(ExecutionDispatchStaticConfigV2 {
        native,
        context: ApprovalContextV1 {
            environment: cfg.directory.environment.clone(),
            custody_account_id: draft.entry.custody_account_id.clone(),
            audience: cfg.audience,
            policy_version: cfg.directory.version,
            security_version: state.provision().security_epoch(),
            service_epoch: cfg.service_epoch,
        },
        world: cfg.directory.world,
        namespace: cfg.directory.namespace,
        instance_epoch: cfg.directory.epoch,
    })
}
