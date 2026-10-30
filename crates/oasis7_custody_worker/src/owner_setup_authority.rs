//! Setup authority remains independent of the expired Provision grant.
use super::*;
use crate::directory_config::DirectoryConfig;
use ed25519_dalek::VerifyingKey;
use oasis7_custody_core::approval::{
    ApprovedWorldV1, CustodySessionStatusV1, SignedApprovalVerificationV1,
};
use oasis7_custody_core::approval_admission::ApprovalAdmissionInput;

pub(super) fn enabled(env: &Env) -> Result<ApprovedWorldV1> {
    if env
        .var("CUSTODY_OWNER_SETUP_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(denied());
    }
    public_world(env)
}
/// Fixed public configuration only: archival does not require a live setup lane.
pub(super) fn public_world(env: &Env) -> Result<ApprovedWorldV1> {
    let config = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
    let raw: [u8; 32] = hex::decode(&config.world.service_public_key)
        .map_err(|_| denied())?
        .try_into()
        .map_err(|_| denied())?;
    if hex::encode(raw) != config.world.service_public_key
        || VerifyingKey::from_bytes(&raw)
            .map_err(|_| denied())?
            .is_weak()
    {
        return Err(denied());
    }
    Ok(config.world)
}
pub(super) async fn observe(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    input: ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
    inspection_only: bool,
) -> Result<authority::Observations> {
    let world = enabled(env)?;
    if saved.state.draft().plan.world_identity != world.world_id {
        return Err(denied());
    }
    authority::observe(env, saved.state.draft(), input, signed, inspection_only).await
}
/// Only Delegation/Bind/FinalObservation result callbacks whose local once reservation has
/// already committed can overlap these two independent readonly inputs.
pub(super) async fn observe_paid_result(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    id: &str,
    stage: oasis7_custody_core::provisioning::OwnerSetupStageV1,
) -> Result<authority::Observations> {
    use oasis7_custody_core::provisioning::{OWNER_SETUP_STAGE_TRANSPORT_COST, OwnerSetupStageV1};
    let world = enabled(env)?;
    let plan = saved.state.owner_setup_plan(id).ok_or_else(denied)?;
    let outbox = saved
        .state
        .owner_setup_outbox(id, stage)
        .ok_or_else(denied)?;
    // The snapshot was strictly restored; count its actual closed outboxes and
    // compare the public ledger projection without opening Core's private book.
    let mut spent = 8_u64;
    let mut cursor = Some(OwnerSetupStageV1::FinalObservation);
    while let Some(selected) = cursor {
        if saved.state.owner_setup_outbox(id, selected).is_some() {
            spent = spent
                .checked_add(OWNER_SETUP_STAGE_TRANSPORT_COST)
                .ok_or_else(denied)?;
        }
        cursor = selected.previous();
    }
    let remaining = saved
        .state
        .owner_setup_progress(id)
        .map_err(|_| denied())?
        .transport_remaining
        .0;
    if !matches!(
        stage,
        OwnerSetupStageV1::Delegation
            | OwnerSetupStageV1::Bind
            | OwnerSetupStageV1::FinalObservation
    ) || saved.state.frozen()
        || saved.state.draft().world_identity != world
        || plan.transport_limit.0.checked_sub(spent) != Some(remaining)
        || outbox.callback_claimed_at.is_none()
        || outbox.claimed_at.is_none()
        || outbox.result.is_some()
    {
        return Err(denied());
    }
    let original = saved.state.owner_setup_original(id).ok_or_else(denied)?;
    let signed = outbox.source.verification.as_ref().ok_or_else(denied)?;
    authority::observe_paid_owner_setup_result(
        env,
        saved.state.draft(),
        original.input().clone(),
        signed,
    )
    .await
}
pub(super) async fn observe_manager(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    input: ApprovalAdmissionInput,
    manager: CustodySessionStatusV1,
) -> Result<authority::Observations> {
    enabled(env)?;
    authority::observe_fresh_manager(env, saved.state.draft(), input, manager).await
}
pub(super) fn current(
    state: &JsValue,
    env: &Env,
    observed: &authority::Observations,
    require_business: bool,
) -> std::result::Result<NativeStageCurrent, JsValue> {
    let world = enabled(env).map_err(|_| JsValue::from_str("owner setup unavailable"))?;
    let current = authority::current(state, env, observed, require_business)?;
    if current.provision.approval.authority.security_version.0 == 0
        || current.provision.world_identity != world
    {
        return Err(JsValue::from_str("owner setup unavailable"));
    }
    Ok(current)
}

/// Management-only readonly port. It never fabricates a proof, consumes a nonce,
/// loads a business signing seed, or turns a basis into a stage permit.
pub(super) async fn management_read<R, F>(
    state: &JsValue,
    env: &Env,
    manager: &CustodySessionStatusV1,
    provision_id: &str,
    read: F,
) -> Result<R>
where
    F: FnOnce(
        &oasis7_custody_core::provisioning::AccountProvisionApprovalState,
        &oasis7_custody_core::provisioning::OwnerSetupManagementCurrent<'_>,
        u64,
    ) -> Result<R>,
{
    use crate::directory_rpc::{ProvisionGateRpc, ProvisionGateView};
    use oasis7_custody_core::{approval::ApprovalContextV1, provisioning::ProvisionPolicy};
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let draft = saved.state.draft();
    let config = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
    if config.principal(manager)? != draft.entry.manager || provision_id != draft.entry.provision_id
    {
        return Err(denied());
    }
    let started = clock()?;
    let gate = ProvisionGateRpc {
        manager_session: manager.clone(),
        provision_id: provision_id.into(),
        custody_account_id: draft.entry.custody_account_id.clone(),
    };
    let raw = crate::owner_directory::call_directory(
        env,
        "/internal/provision-gate",
        &serde_json::to_string(&gate)?,
        manager,
    )
    .await?;
    let gate: ProvisionGateView = serde_json::from_str(&raw).map_err(|_| denied())?;
    let (fence, fence_started) = super::super::super::fence(env).await?;
    // Everything below is synchronous, including the final Account resnapshot.
    let saved = OwnerProvisionApprovalStore::open(state.clone())
        .map_err(|_| denied())?
        .snapshot()
        .map_err(|_| denied())?;
    let config = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
    let now = clock()?;
    super::super::super::check_gate(&gate, saved.state.draft(), now)?;
    super::super::super::fresh(DecimalU64(started), now)?;
    super::super::super::fresh(fence_started, now)?;
    if config.principal(manager)? != saved.state.draft().entry.manager
        || provision_id != saved.state.draft().entry.provision_id
    {
        return Err(denied());
    }
    let value = |key| env.var(key).map(|v| v.to_string()).map_err(|_| denied());
    let service_epoch: DecimalU64 =
        serde_json::from_value(value("CUSTODY_SERVICE_EPOCH")?.into()).map_err(|_| denied())?;
    fence.check_epoch(service_epoch.0).map_err(|_| denied())?;
    let authority = ApprovalContextV1 {
        environment: config.environment.clone(),
        custody_account_id: saved.state.draft().entry.custody_account_id.clone(),
        audience: value("CUSTODY_NATIVE_AUDIENCE")?,
        policy_version: config.version,
        security_version: saved.state.provision().security_epoch(),
        service_epoch,
    };
    let current = oasis7_custody_core::provisioning::OwnerSetupManagementCurrent {
        manager,
        authority: &authority,
        policy: ProvisionPolicy {
            routes: &config.routes,
            managers: &config.managers,
            version: config.version.0,
        },
        world_identity: &config.world,
        native_identity_namespace: &config.namespace,
        fence: &fence,
        observed_at: started,
    };
    read(&saved.state, &current, now)
}
