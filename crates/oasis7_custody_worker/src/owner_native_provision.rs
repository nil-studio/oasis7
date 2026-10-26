//! Owner-side native binding statement construction from registered Account originals.
use crate::{
    directory_config::{clock, denied},
    provision_approval_store::{
        OwnerProvisionApprovalStore, ProvisionApprovalSnapshot, native::NativeStageCurrent,
    },
};
use oasis7_custody_core::{
    DecimalU64,
    http::Role,
    native_binding::{
        OwnerNativeBindingStatementV1, SignedNativeBindingChallengeV1,
        SignedOwnerNativeBindingStatementV1,
    },
    provisioning::{
        NativeStageKindV1, NativeStagePermitViewV1, NativeStageRequestV1, NativeStageResultV1,
    },
};
use wasm_bindgen::JsValue;
use worker::{Env, Result};

#[path = "owner_native_config.rs"]
mod config;
use config::NativeConfig;
#[path = "owner_native_authority.rs"]
mod authority;
#[path = "owner_native_dispatch.rs"]
mod dispatch;
#[path = "owner_native_managed_approval.rs"]
mod managed_approval;
pub(crate) async fn approve_managed_login(
    state: &JsValue,
    env: &Env,
    rpc: crate::directory_rpc::ManagedLoginApproveRpc,
) -> Result<oasis7_custody_core::provisioning::ManagedLoginApprovalViewV1> {
    managed_approval::approve_managed_login(state, env, rpc).await
}

#[path = "owner_native_drive.rs"]
mod native_drive;
pub(crate) async fn continue_provision(
    state: &JsValue,
    env: &Env,
    manager: &oasis7_custody_core::approval::CustodySessionStatusV1,
    operation_id: &str,
) -> Result<()> {
    native_drive::continue_provision(state, env, manager, operation_id).await
}
pub(crate) async fn continue_managed_login(
    state: &JsValue,
    env: &Env,
    manager: &oasis7_custody_core::approval::CustodySessionStatusV1,
    provision_id: &str,
    operation_id: &str,
) -> Result<oasis7_custody_core::provisioning::ManagedLoginApprovalViewV1> {
    native_drive::continue_managed_login(state, env, manager, provision_id, operation_id).await
}

pub(crate) async fn dispatch(
    state: &JsValue,
    env: &Env,
    rpc: crate::native_runtime_rpc::NativeRuntimeRpc,
) -> Result<String> {
    dispatch::dispatch(state, env, rpc).await
}

/// The caller supplies observations from the fixed Directory/Fence readers. This
/// Rust-only closure is never assembled from an HTTP authority object.
pub(super) async fn issue_binding_statement<F>(
    state: &JsValue,
    env: &Env,
    request: &NativeStageRequestV1,
    challenge: SignedNativeBindingChallengeV1,
    reserved_ticket: u8,
    current: F,
) -> Result<SignedOwnerNativeBindingStatementV1>
where
    F: Fn() -> std::result::Result<NativeStageCurrent, JsValue>,
{
    let config = NativeConfig::load(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let original = store.native_stage_original(request).map_err(|_| denied())?;
    if request.stage != NativeStageKindV1::Binding {
        return Err(denied());
    }
    if let Some(statement) = original.binding_statement {
        // Historical evidence, including its original deadline. No resigning or
        // fresh permit is inferred from this readonly recovery path.
        return Ok(statement);
    }
    let checked = || {
        let trusted = current()?;
        if trusted.scope.http_origin != config.http_origin
            || trusted.scope.native_root_key_id != config.native_root_key_id
            || trusted.scope.native_root_public_key != config.native_root_public_key
            || trusted.scope.owner_binding_pin != config.owner_pin
        {
            return Err(JsValue::from_str("native authority unavailable"));
        }
        Ok(trusted)
    };
    // Authenticate the native challenge before constructing any Owner statement.
    store
        .record_native_binding_challenge(&request.operation_id, challenge, &checked)
        .await
        .map_err(|_| denied())?;
    let permit = store
        .authorize_native_stage_read(&request.operation_id, reserved_ticket, &checked)
        .await
        .map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let statement = binding_statement(&saved, &permit, &config, clock()?)?;
    let signed = config.sign(statement, clock()?)?;
    // This rechecks strict Rust/WebCrypto signatures and final authority/time in
    // the same SQL transition that retains the immutable statement and audit.
    store
        .register_native_binding_statement(&request.operation_id, signed, &checked)
        .await
        .map_err(|_| denied())
}

/// Only the fixed Account coordinator calls this after consuming an oracle read.
/// The signed output still has to pass the Account's exact signature checks and
/// final SQL authority recheck before it can leave the coordinator.
fn binding_statement(
    saved: &ProvisionApprovalSnapshot,
    permit: &NativeStagePermitViewV1,
    config: &NativeConfig,
    now: u64,
) -> Result<OwnerNativeBindingStatementV1> {
    let state = &saved.state;
    let original = state
        .native_stages()
        .get(&permit.attempt_id)
        .ok_or_else(denied)?;
    if permit.request.stage != NativeStageKindV1::Binding
        || original.request != permit.request
        || original.descriptor != permit.descriptor
        || original.claimed_at.is_none()
        || original.result.is_some()
        || original.request_bytes.as_deref() != Some(permit.request_bytes.as_str())
        || original.nonce != permit.nonce
        || permit.target_security_epoch != state.provision().security_epoch()
    {
        return Err(denied());
    }
    let challenge = if let Some(challenge) = &original.binding_challenge {
        &challenge.challenge
    } else {
        state
            .native_stages()
            .values()
            .find_map(|stage| match stage.result.as_ref() {
                Some(NativeStageResultV1::RegistrationChallenge(value))
                    if value.challenge.challenge_id == original.request.binding_operation_id =>
                {
                    Some(&value.challenge)
                }
                _ => None,
            })
            .ok_or_else(denied)?
    };
    // A service-level pin cannot alias any of this account's role identities.
    if [Role::Owner, Role::Identity, Role::Executor]
        .iter()
        .filter_map(|role| state.provision().role_key(*role))
        .any(|key| {
            key.public_key == config.owner_pin.public_key || key.key_id == config.owner_pin.key_id
        })
    {
        return Err(denied());
    }
    let draft = state.draft();
    let expires_at = permit
        .expires_at
        .0
        .min(draft.plan.expires_at.0)
        .min(challenge.expires_at.0);
    let statement = OwnerNativeBindingStatementV1 {
        challenge: challenge.clone(),
        environment: draft.entry.environment.clone(),
        custody_account_id: draft.entry.custody_account_id.clone(),
        provision_id: draft.entry.provision_id.clone(),
        approved_plan_digest: draft.entry.plan_digest.clone(),
        login_key_ref: original.descriptor.identity_key.key_id.clone(),
        binding_operation_id: original.request.binding_operation_id.clone(),
        permit_id: permit.attempt_id.clone(),
        source_revision: permit.source_revision.clone(),
        account_authority_revision: permit.authority_revision.clone(),
        service_epoch: config.owner_pin.service_epoch,
        service_identity: config.owner_pin.service_identity.clone(),
        signer_key_id: config.owner_pin.key_id.clone(),
        authority_observed_at: permit.observed_at,
        expires_at: DecimalU64(expires_at),
    };
    config
        .owner_pin
        .validate_statement(&statement, now)
        .map_err(|_| denied())?;
    Ok(statement)
}
