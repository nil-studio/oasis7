//! Fixed key preparation coordinator. Account owns attempts/budgets; vaults own secrets.
use super::*;
use crate::directory_rpc::{DriveRpc, ReadRpc, RolePermitRpc};
use oasis7_custody_core::{http::Role, provisioning::*, role_key_slot::RoleKeySlotScopeV1};

#[wasm_bindgen(module = "/platform/provision_drive_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = ensureIdentityKey)]
    async fn identity_key(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = ensureOwnerKey)]
    async fn owner_key(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = ensureExecutorKey)]
    async fn executor_key(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
struct Observations {
    manager: CustodySessionStatusV1,
    gate: ProvisionGateView,
    fence: RecoveryFence,
    fence_started: DecimalU64,
}
fn key_lane(env: &Env) -> Result<()> {
    if env
        .var("CUSTODY_PROVISION_KEYS_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(denied());
    }
    Config::load(env).map(|_| ())
}
fn source_session(
    input: &ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
) -> CustodySessionStatusV1 {
    // Only used for an already persisted inspection; the signature is rechecked
    // by the store before any permit can be returned or dispatch claimed.
    let proof = &input.approval.proof;
    let v = &signed.verification;
    CustodySessionStatusV1 {
        api_version: 1,
        identity_namespace: proof.identity_namespace.clone(),
        identity_instance_epoch: v.identity_instance_epoch,
        account_id: proof.manager_account_id.clone(),
        session_ref: proof.manager_session_ref.clone(),
        credential_ref: proof.origin_credential_id.clone(),
        credential_version: v.origin_credential_version,
        security_version: v.manager_security_version,
        policy_version: v.manager_policy_version,
        server_time: v.observed_at,
        expires_at: v.source_session_expires_at,
        idle_expires_at: v.source_session_idle_expires_at,
    }
}
async fn observe(
    env: &Env,
    draft: &AccountDraftV1,
    input: &ApprovalAdmissionInput,
    manager: CustodySessionStatusV1,
) -> Result<Observations> {
    let rpc = ApproveRpc {
        manager_session: manager.clone(),
        provision_id: draft.entry.provision_id.clone(),
        request: oasis7_custody_core::provisioning::ProvisionApproveRequestV1 {
            api_version: 1,
            input: input.clone(),
        },
    };
    let gate = gate(env, &rpc, draft).await?;
    let (fence, fence_started) = fence(env).await?;
    Ok(Observations {
        manager,
        gate,
        fence,
        fence_started,
    })
}
fn authority(
    env: &Env,
    draft: &AccountDraftV1,
    input: &ApprovalAdmissionInput,
    observed: &Observations,
) -> std::result::Result<ProvisionApprovalCurrent, JsValue> {
    current(
        env,
        draft,
        &observed.manager,
        input,
        &observed.gate,
        observed.fence,
        observed.fence_started,
    )
    .map_err(|_| JsValue::from_str("provision authority unavailable"))
}
fn attempt_id(domain: &str, operation: &str, account: &str) -> Result<String> {
    crate::directory_config::digest(domain, &(operation, account))
}
fn view(store: &OwnerProvisionApprovalStore, operation: &str) -> Result<ProvisionProgressViewV1> {
    let saved = store.snapshot().map_err(|_| denied())?;
    let state = saved.state;
    let draft = state.draft();
    let mut pending_operations = state
        .origin_inspections()
        .values()
        .filter(|v| v.verification.is_none())
        .map(|v| v.operation_id.clone())
        .collect::<Vec<_>>();
    pending_operations.extend(
        state
            .role_key_dispatches()
            .values()
            .filter(|v| v.result.is_none())
            .map(|v| v.operation_id.clone()),
    );
    pending_operations.sort();
    Ok(ProvisionProgressViewV1 {
        api_version: 1,
        provision_id: draft.entry.provision_id.clone(),
        custody_account_id: draft.entry.custody_account_id.clone(),
        operation_id: operation.into(),
        stage: state.provision().stage(),
        pending_operations,
        transport_remaining: DecimalU64(
            state
                .provision()
                .transport_remaining()
                .map_err(|_| denied())?,
        ),
    })
}
pub(crate) fn progress(
    state: &JsValue,
    env: &Env,
    rpc: ReadRpc,
) -> Result<ProvisionProgressViewV1> {
    let config = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
    let draft = read_pristine_draft(state).map_err(|_| denied())?;
    if config.principal(&rpc.manager_session)? != draft.entry.manager
        || rpc.provision_id != draft.entry.provision_id
    {
        return Err(denied());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let operation = store
        .snapshot()
        .map_err(|_| denied())?
        .state
        .accepted_operation_id()
        .ok_or_else(denied)?
        .to_owned();
    view(&store, &operation)
}

pub(crate) async fn advance(
    state: &JsValue,
    env: &Env,
    rpc: DriveRpc,
) -> Result<ProvisionProgressViewV1> {
    key_lane(env)?;
    rpc.request.validate().map_err(|_| denied())?;
    let draft = read_pristine_draft(state).map_err(|_| denied())?;
    let config = Config::load(env)?;
    if config.directory.principal(&rpc.manager_session)? != draft.entry.manager
        || rpc.provision_id != draft.entry.provision_id
    {
        return Err(denied());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let snapshot = store.snapshot().map_err(|_| denied())?;
    let input = snapshot
        .state
        .accepted_approval()
        .map_err(|_| denied())?
        .input()
        .clone();
    if rpc.manager_session.account_id != input.approval.proof.manager_account_id
        || rpc.manager_session.session_ref != input.approval.proof.manager_session_ref
    {
        return Err(denied());
    }
    let inspection_id = attempt_id(
        "oasis7.custody.provision-inspection-attempt.v1",
        &rpc.request.operation_id,
        &draft.entry.custody_account_id,
    )?;
    // Any existing attempt is history. It never reacquires transport rights.
    if snapshot
        .state
        .origin_inspections()
        .contains_key(&inspection_id)
        || snapshot.state.provision().stage() == ProvisionStage::RoleKeysReady
    {
        return view(&store, &rpc.request.operation_id);
    }
    store
        .reserve_origin_inspection(&inspection_id)
        .map_err(|_| denied())?;
    let before = observe(env, &draft, &input, rpc.manager_session.clone()).await?;
    let raw = store
        .claim_origin_inspection(&inspection_id, || authority(env, &draft, &input, &before))
        .map_err(|_| denied())?;
    let signed: SignedApprovalVerificationV1 = serde_json::from_str(&text(
        verify_native(env, &raw).await.map_err(|_| denied())?,
    )?)
    .map_err(|_| denied())?;
    let manager = source_session(&input, &signed);
    let mut observed = observe(env, &draft, &input, manager.clone()).await?;
    store
        .complete_origin_inspection(&inspection_id, signed.clone(), || {
            authority(env, &draft, &input, &observed)
        })
        .await
        .map_err(|_| denied())?;
    for (name, role) in [
        ("identity", Role::Identity),
        ("owner", Role::Owner),
        ("executor", Role::Executor),
    ] {
        if store
            .snapshot()
            .map_err(|_| denied())?
            .state
            .role_key_results()
            .contains_key(name)
        {
            continue;
        }
        let dispatch_id = attempt_id(
            "oasis7.custody.provision-role-attempt.v1",
            &format!("{}:{name}", rpc.request.operation_id),
            &draft.entry.custody_account_id,
        )?;
        store
            .claim_role_key_dispatch(&dispatch_id, role, &inspection_id, || {
                authority(env, &draft, &input, &observed)
            })
            .await
            .map_err(|_| denied())?;
        let saved = store.snapshot().map_err(|_| denied())?;
        let dispatch = saved
            .state
            .role_key_dispatches()
            .get(&dispatch_id)
            .ok_or_else(denied)?;
        let raw = &dispatch.request_bytes;
        let reply = match role {
            Role::Identity => identity_key(env, raw).await,
            Role::Owner => owner_key(env, raw).await,
            Role::Executor => executor_key(env, raw).await,
            Role::RecoveryFence => return Err(denied()),
        };
        let public: PublicRoleKey = match reply
            .ok()
            .and_then(|v| text(v).ok())
            .and_then(|raw| serde_json::from_str(&raw).ok())
        {
            Some(value) => value,
            None => return view(&store, &rpc.request.operation_id),
        };
        observed = observe(env, &draft, &input, manager.clone()).await?;
        store
            .record_dispatched_role_key_result(
                &dispatch_id,
                ProvisionRoleKeyResultV1 {
                    scope: dispatch.scope.clone(),
                    public_ref: public,
                },
                || authority(env, &draft, &input, &observed),
            )
            .map_err(|_| denied())?;
    }
    view(&store, &rpc.request.operation_id)
}

pub(crate) async fn verify_role_permit(
    state: &JsValue,
    env: &Env,
    rpc: RolePermitRpc,
) -> Result<ProvisionRoleKeyPermitViewV1> {
    key_lane(env)?;
    rpc.request.validate().map_err(|_| denied())?;
    let draft = read_pristine_draft(state).map_err(|_| denied())?;
    if rpc.request.custody_account_id != draft.entry.custody_account_id
        || rpc.request.provision_id != draft.entry.provision_id
    {
        return Err(denied());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    // Reserve the bounded read credit before any remote observation.
    let ticket = store
        .reserve_role_key_permit_read(&rpc.request.permit_id, rpc.role)
        .map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let dispatch = saved
        .state
        .role_key_dispatches()
        .values()
        .find(|v| v.permit_id == rpc.request.permit_id)
        .ok_or_else(denied)?;
    if RoleKeySlotScopeV1::from_task(
        saved
            .state
            .role_key_tasks()
            .get(match rpc.role {
                Role::Identity => "identity",
                Role::Owner => "owner",
                Role::Executor => "executor",
                Role::RecoveryFence => return Err(denied()),
            })
            .ok_or_else(denied)?,
    )
    .map_err(|_| denied())?
        != dispatch.scope
    {
        return Err(denied());
    }
    let inspection = saved
        .state
        .origin_inspections()
        .get(&dispatch.inspection_id)
        .ok_or_else(denied)?;
    let signed = inspection.verification.as_ref().ok_or_else(denied)?;
    let input = saved
        .state
        .accepted_approval()
        .map_err(|_| denied())?
        .input();
    let observed = observe(env, &draft, input, source_session(input, signed)).await?;
    let permit = store
        .authorize_reserved_role_key_permit_read(&rpc.request.permit_id, ticket, rpc.role, || {
            authority(env, &draft, input, &observed)
        })
        .await
        .map_err(|_| denied())?;
    permit
        .validate_for(&rpc.request, rpc.role)
        .map_err(|_| denied())?;
    Ok(permit)
}
