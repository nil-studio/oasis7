//! Explicit management continuation. Stages come only from Account originals.
use super::*;
use crate::native_runtime_rpc::NativeRuntimeCall;
use oasis7_custody_core::{approval::CustodySessionStatusV1, provisioning::*};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/platform/provision_drive_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = runNativeStage)]
    async fn run_native(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = runParticipantWorkflow)]
    async fn run_participant(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
async fn continue_participant(
    state: &JsValue,
    env: &Env,
    manager: &CustodySessionStatusV1,
) -> Result<()> {
    if env
        .var("CUSTODY_PARTICIPANT_PROVISION_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Ok(());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let approved = saved.state.accepted_approval().map_err(|_| denied())?;
    check_manager_reference(env, &saved, manager, approved.input())?;
    if saved.state.provision().stage() != ProvisionStage::NativeSessionReady {
        return Ok(());
    }
    let draft = saved.state.draft();
    let operation_id = crate::directory_config::digest(
        "oasis7.custody.first-participant-operation.v1",
        &(&draft.entry.custody_account_id, &draft.entry.provision_id),
    )?;
    if saved.state.participant_original(&operation_id).is_some() {
        return Ok(());
    }
    let request = ParticipantProvisionRequestV1 {
        api_version: 1,
        custody_account_id: draft.entry.custody_account_id.clone(),
        provision_id: draft.entry.provision_id.clone(),
        operation_id,
    };
    super::participant::start_participant_workflow(state, env, request.clone()).await?;
    // The workflow claim is permanent. Unknown replies cannot re-enter this send
    // under the same or another management ID; Account originals remain pending.
    let _reply = run_participant(env, &serde_json::to_string(&request)?).await;
    Ok(())
}
fn enabled(env: &Env) -> bool {
    env.var("CUSTODY_NATIVE_PROVISION_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        == Some("true")
}
fn operation(management: &str, account: &str, stage: &str) -> Result<String> {
    crate::directory_config::digest(
        "oasis7.custody.native-management-attempt.v1",
        &(management, account, stage),
    )
}
fn check_manager(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    manager: &CustodySessionStatusV1,
    input: &oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
) -> Result<()> {
    let config = super::super::Config::load(env)?;
    if config.directory.principal(manager)? != saved.state.draft().entry.manager {
        return Err(denied());
    }
    check_manager_reference(env, saved, manager, input)
}
// Entry authentication already checked the actual HTTP observation. Long
// workflows retain only its exact source identity; each new grant independently
// performs a fresh signed native inspection and current Directory/Fence checks.
fn check_manager_reference(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    manager: &CustodySessionStatusV1,
    input: &oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
) -> Result<()> {
    let config = super::super::Config::load(env)?;
    let proof = &input.approval.proof;
    let reference = crate::directory_rpc::NativeInspectReference {
        identity_namespace: manager.identity_namespace.clone(),
        identity_instance_epoch: manager.identity_instance_epoch,
        account_id: manager.account_id.clone(),
        session_ref: manager.session_ref.clone(),
        credential_ref: manager.credential_ref.clone(),
        credential_version: manager.credential_version,
        security_version: manager.security_version,
        policy_version: manager.policy_version,
        expires_at: manager.expires_at,
        idle_expires_at: manager.idle_expires_at,
    };
    if config
        .directory
        .inspection_reference_principal(&reference)?
        != saved.state.draft().entry.manager
        || manager.account_id != proof.manager_account_id
        || manager.session_ref != proof.manager_session_ref
        || manager.credential_ref != proof.origin_credential_id
        || manager.credential_version != proof.origin_credential_version
        || manager.security_version != proof.manager_security_version
        || manager.policy_version != proof.manager_policy_version
    {
        return Err(denied());
    }
    Ok(())
}
/// Native sends are never inferred from HTTP selectors. The fixed Identity actor
/// spends an Account read ticket and verifies live directory/source/RF before each permit.
pub(super) async fn continue_provision(
    state: &JsValue,
    env: &Env,
    manager: &CustodySessionStatusV1,
    management_operation: &str,
) -> Result<()> {
    if !enabled(env) {
        return Ok(());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    for _ in 0..3 {
        let saved = store.snapshot().map_err(|_| denied())?;
        let input = saved
            .state
            .accepted_approval()
            .map_err(|_| denied())?
            .input();
        check_manager_reference(env, &saved, manager, input)?;
        let draft = saved.state.draft();
        let binding_id = operation(
            management_operation,
            &draft.entry.custody_account_id,
            "binding",
        )?;
        let (stage, account_operation) = match saved.state.provision().stage() {
            ProvisionStage::RoleKeysReady => {
                let has_registration = saved.state.native_stages().values().any(|v| {
                    matches!(
                        v.result,
                        Some(NativeStageResultV1::RegistrationChallenge(_))
                    )
                });
                if has_registration {
                    (NativeStageKindV1::Binding, binding_id.clone())
                } else {
                    (
                        NativeStageKindV1::Registration,
                        operation(
                            management_operation,
                            &draft.entry.custody_account_id,
                            "registration",
                        )?,
                    )
                }
            }
            ProvisionStage::NativeRegistered => {
                // Initial login is one logical descriptor operation. An unknown or
                // explicit old login is never relaunched under a new management ID.
                if let Some(old) = saved
                    .state
                    .native_stages()
                    .values()
                    .find(|v| v.request.stage == NativeStageKindV1::Login)
                {
                    if old.result.is_none() {
                        super::reconcile::reconcile(
                            state,
                            &store,
                            env,
                            &NativeRuntimeCall::Provision {
                                request: old.request.clone(),
                            },
                            management_operation,
                        )
                        .await?;
                    }
                    return Ok(());
                }
                (
                    NativeStageKindV1::Login,
                    operation(
                        management_operation,
                        &draft.entry.custody_account_id,
                        "initial-login",
                    )?,
                )
            }
            ProvisionStage::NativeSessionReady => {
                return continue_participant(state, env, manager).await;
            }
            _ => return Ok(()),
        };
        let binding_id = if stage == NativeStageKindV1::Login {
            saved
                .state
                .native_stages()
                .values()
                .find_map(|v| match &v.result {
                    Some(NativeStageResultV1::BindingReceipt(v)) => {
                        Some(v.receipt.binding_operation_id.clone())
                    }
                    _ => None,
                })
                .ok_or_else(denied)?
        } else {
            binding_id
        };
        let request = NativeStageRequestV1 {
            api_version: 1,
            stage,
            custody_account_id: draft.entry.custody_account_id.clone(),
            provision_id: draft.entry.provision_id.clone(),
            operation_id: account_operation.clone(),
            binding_operation_id: binding_id,
        };
        if let Some(original) = saved.state.native_stages().get(&account_operation) {
            if original.request != request {
                return Err(denied());
            }
            // Existing management attempts are facts/pending originals, never new sends.
            // A distinct explicit management ID may purchase a fresh Register/B2 attempt.
            return Ok(());
        }
        let call = NativeRuntimeCall::Provision { request };
        let raw = serde_json::to_string(&call)?;
        let reply = run_native(env, &raw).await;
        let after = store.snapshot().map_err(|_| denied())?;
        let committed = after
            .state
            .native_stages()
            .get(&account_operation)
            .and_then(|v| v.result.as_ref());
        if committed.is_none() {
            return Ok(());
        } // unknown stays visible as pending, no resend
        // Only Account's committed facts advance the loop, including response loss.
        // A successful Identity JSON response alone never advances Account state.
        if let Ok(reply) = reply {
            let raw = super::super::text(reply)?;
            let returned: NativeStageResultV1 = serde_json::from_str(&raw).map_err(|_| denied())?;
            if committed != Some(&returned) {
                return Err(denied());
            }
        }
        if matches!(committed, Some(NativeStageResultV1::ExplicitLoginRequired)) {
            return Ok(());
        }
    }
    continue_participant(state, env, manager).await
}
fn managed_view(saved: &ProvisionApprovalSnapshot, id: &str) -> Result<ManagedLoginApprovalViewV1> {
    let task = saved.state.managed_login_task(id).ok_or_else(denied)?;
    let recovered = saved.state.session_status_recoveries().values().any(|v| {
        matches!(&v.funding, SessionStatusRecoveryFundingV1::ManagedLogin { operation_id } if operation_id == id)
            && v.result.is_some()
    });
    let stage = match &task.result {
        None if recovered => ManagedLoginStageV1::NativeSessionReady,
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
/// An explicit selector for one independently approved ManagedLogin operation.
/// Repeating a claimed/terminal selector does not create or replay a login send.
pub(super) async fn continue_managed_login(
    state: &JsValue,
    env: &Env,
    manager: &CustodySessionStatusV1,
    provision_id: &str,
    operation_id: &str,
) -> Result<ManagedLoginApprovalViewV1> {
    if !enabled(env) {
        return Err(denied());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let input = saved
        .state
        .managed_login_original(operation_id)
        .ok_or_else(denied)?
        .input();
    if saved.state.draft().entry.provision_id != provision_id {
        return Err(denied());
    }
    check_manager(env, &saved, manager, input)?;
    let task = saved
        .state
        .managed_login_task(operation_id)
        .ok_or_else(denied)?;
    let call = NativeRuntimeCall::ManagedLogin {
        custody_account_id: saved.state.draft().entry.custody_account_id.clone(),
        provision_id: provision_id.into(),
        operation_id: operation_id.into(),
    };
    if task.claimed_at.is_some() || task.result.is_some() {
        if task.result.is_none() {
            super::reconcile::reconcile(state, &store, env, &call, operation_id).await?;
        }
        return managed_view(&store.snapshot().map_err(|_| denied())?, operation_id);
    }
    let raw = serde_json::to_string(&call)?;
    // The fixed actor's GetPermit performs source/signature/Directory/RF checks;
    // the response itself cannot manufacture a task result or a Cookie.
    let reply = run_native(env, &raw).await;
    let after = store.snapshot().map_err(|_| denied())?;
    if let Ok(reply) = reply {
        let returned: NativeStageResultV1 =
            serde_json::from_str(&super::super::text(reply)?).map_err(|_| denied())?;
        if after
            .state
            .managed_login_task(operation_id)
            .and_then(|v| v.result.as_ref())
            != Some(&returned)
        {
            return Err(denied());
        }
    }
    managed_view(&after, operation_id)
}
