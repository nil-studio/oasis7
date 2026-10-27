//! Fixed separately approved Owner setup authority and Account outbox.
use super::*;
#[path = "owner_setup_approval.rs"]
mod setup_approval;
#[path = "owner_setup_authority.rs"]
mod setup_authority;
#[path = "owner_setup_dispatch.rs"]
mod setup_dispatch;

pub(crate) async fn approve_owner_setup(
    state: &JsValue,
    env: &Env,
    rpc: crate::owner_setup_rpc::OwnerSetupApproveRpc,
) -> Result<oasis7_custody_core::provisioning::OwnerSetupProgressV1> {
    setup_approval::approve(state, env, rpc).await
}

#[path = "owner_setup_drive.rs"]
mod setup_drive;
pub(crate) async fn continue_owner_setup(
    state: &JsValue,
    env: &Env,
    rpc: crate::owner_setup_rpc::OwnerSetupContinueRpc,
) -> Result<oasis7_custody_core::provisioning::OwnerSetupProgressV1> {
    setup_drive::continue_setup(state, env, rpc).await
}

pub(crate) async fn dispatch_owner_setup(
    state: &JsValue,
    env: &Env,
    rpc: crate::owner_setup_rpc::OwnerSetupRuntimeRpc,
    static_role: Role,
) -> Result<String> {
    setup_dispatch::dispatch(state, env, rpc, static_role).await
}

pub(crate) async fn basis_owner_setup(
    state: &JsValue,
    env: &Env,
    rpc: crate::owner_setup_rpc::OwnerSetupBasisRpc,
) -> Result<oasis7_custody_core::provisioning::OwnerSetupBasisV1> {
    setup_approval::basis(state, env, rpc).await
}
pub(crate) async fn prepare_owner_setup(
    state: &JsValue,
    env: &Env,
    rpc: crate::owner_setup_rpc::OwnerSetupPrepareRpc,
) -> Result<oasis7_custody_core::provisioning::OwnerSetupPrepareResponseV1> {
    setup_approval::prepare(state, env, rpc).await
}

#[path = "owner_setup_reconcile.rs"]
pub(crate) mod reconcile;
