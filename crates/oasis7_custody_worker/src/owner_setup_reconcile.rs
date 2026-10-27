//! Independent paid reconciliation exposes only original lookup and fixed readonly observations.
use super::*;
use crate::owner_setup_reconcile_rpc::*;
use oasis7_custody_core::provisioning::*;
#[path = "owner_setup_reconcile_approval.rs"]
mod reconcile_approval;
#[path = "owner_setup_reconcile_authority.rs"]
mod reconcile_authority;
#[path = "owner_setup_reconcile_dispatch.rs"]
mod reconcile_dispatch;
#[path = "owner_setup_reconcile_drive.rs"]
mod reconcile_drive;
pub(crate) async fn basis(
    state: &JsValue,
    env: &Env,
    rpc: ReconcileBasisRpc,
) -> Result<OwnerSetupReconcileBasisV1> {
    reconcile_approval::basis(state, env, rpc).await
}
pub(crate) async fn prepare(
    state: &JsValue,
    env: &Env,
    rpc: ReconcilePrepareRpc,
) -> Result<OwnerSetupReconcilePrepareResponseV1> {
    reconcile_approval::prepare(state, env, rpc).await
}
pub(crate) async fn approve(
    state: &JsValue,
    env: &Env,
    rpc: ReconcileApproveRpc,
) -> Result<OwnerSetupReconcileProgressV1> {
    reconcile_approval::approve(state, env, rpc).await
}
pub(crate) async fn continue_attempt(
    state: &JsValue,
    env: &Env,
    rpc: ReconcileContinueRpc,
) -> Result<OwnerSetupReconcileProgressV1> {
    reconcile_drive::continue_attempt(state, env, rpc).await
}
pub(crate) async fn dispatch(
    state: &JsValue,
    env: &Env,
    rpc: ReconcileRuntimeRpc,
    role: Role,
) -> Result<String> {
    reconcile_dispatch::dispatch(state, env, rpc, role).await
}
