//! Account approval consumer. Fixed private bindings only; production stays opt-in and fenced.
use crate::{
    directory_config::{DirectoryConfig, clock, denied},
    directory_rpc::{ApproveRpc, ProvisionGateRpc, ProvisionGateView},
    owner_account::read_pristine_draft,
    provision_approval_store::{OwnerProvisionApprovalStore, ProvisionApprovalCurrent},
};
use oasis7_custody_core::{
    DecimalU64,
    approval::{ApprovalContextV1, CustodySessionStatusV1, SignedApprovalVerificationV1},
    approval_admission::{
        ApprovalAdmissionContext, ApprovalAdmissionInput, ApprovalAdmissionStage,
        ApprovalSourceVersions,
    },
    provisioning::{AccountDraftV1, ProvisionApprovalViewV1},
    recovery::RecoveryFence,
};
use std::collections::{BTreeMap, BTreeSet};
use wasm_bindgen::prelude::*;
use worker::{Env, Result};

#[wasm_bindgen(module = "/platform/provision_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = verifyNativeApproval)]
    async fn verify_native(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = readRecoveryFence)]
    async fn read_fence(env: &Env) -> std::result::Result<JsValue, JsValue>;
}
const MAX_BYTES: usize = 32768;
const MAX_OBSERVATION_AGE: u64 = 5000;
struct Config {
    directory: DirectoryConfig,
    audience: String,
    service_epoch: DecimalU64,
    pins: BTreeMap<String, String>,
}
impl Config {
    fn load(env: &Env) -> Result<Self> {
        if env
            .var("CUSTODY_PROVISION_APPROVAL_ENABLED")
            .map(|v| v.to_string())
            .ok()
            .as_deref()
            != Some("true")
        {
            return Err(denied());
        }
        let directory = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
        let value = |key| env.var(key).map(|v| v.to_string()).map_err(|_| denied());
        let audience = value("CUSTODY_NATIVE_AUDIENCE")?;
        let key_id = value("CUSTODY_NATIVE_APPROVAL_KEY_ID")?;
        let public = value("CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY")?;
        let service_epoch: DecimalU64 =
            serde_json::from_value(value("CUSTODY_SERVICE_EPOCH")?.into()).map_err(|_| denied())?;
        let key: [u8; 32] = hex::decode(&public)
            .map_err(|_| denied())?
            .try_into()
            .map_err(|_| denied())?;
        if service_epoch.0 == 0
            || audience.is_empty()
            || audience.len() > 256
            || key_id.is_empty()
            || key_id.len() > 256
            || hex::encode(key) != public
        {
            return Err(denied());
        }
        Ok(Self {
            directory,
            audience,
            service_epoch,
            pins: BTreeMap::from([(key_id, public)]),
        })
    }
}
fn text(raw: JsValue) -> Result<String> {
    let raw = raw.as_string().ok_or_else(denied)?;
    if raw.len() > MAX_BYTES {
        return Err(denied());
    }
    Ok(raw)
}
fn fresh(observed_at: DecimalU64, now: u64) -> Result<()> {
    if observed_at.0 > now || now - observed_at.0 > MAX_OBSERVATION_AGE {
        return Err(denied());
    }
    Ok(())
}
async fn gate(env: &Env, rpc: &ApproveRpc, draft: &AccountDraftV1) -> Result<ProvisionGateView> {
    let request = ProvisionGateRpc {
        manager_session: rpc.manager_session.clone(),
        provision_id: rpc.provision_id.clone(),
        custody_account_id: draft.entry.custody_account_id.clone(),
    };
    let raw = serde_json::to_string(&request)?;
    let raw = crate::owner_directory::call_directory(
        env,
        "/internal/provision-gate",
        &raw,
        &rpc.manager_session,
    )
    .await?;
    if raw.len() > MAX_BYTES {
        return Err(denied());
    }
    let view: ProvisionGateView = serde_json::from_str(&raw).map_err(|_| denied())?;
    check_gate(&view, draft, clock()?)?;
    Ok(view)
}
fn check_gate(view: &ProvisionGateView, draft: &AccountDraftV1, now: u64) -> Result<()> {
    fresh(view.observed_at, now)?;
    if view.api_version != 1
        || view.draft != *draft
        || view.directory_revision.len() != 64
        || !view
            .directory_revision
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(denied());
    }
    Ok(())
}
async fn fence(env: &Env) -> Result<(RecoveryFence, DecimalU64)> {
    let fence: RecoveryFence =
        serde_json::from_str(&text(read_fence(env).await.map_err(|_| denied())?)?)
            .map_err(|_| denied())?;
    Ok((fence, DecimalU64(clock()?)))
}
fn current(
    env: &Env,
    draft: &AccountDraftV1,
    session: &CustodySessionStatusV1,
    input: &ApprovalAdmissionInput,
    gate: &ProvisionGateView,
    fence: RecoveryFence,
    fence_observed: DecimalU64,
) -> Result<ProvisionApprovalCurrent> {
    let config = Config::load(env)?;
    let now = clock()?;
    check_gate(gate, draft, now)?;
    fresh(fence_observed, now)?;
    fence
        .check_epoch(config.service_epoch.0)
        .map_err(|_| denied())?;
    let principal = config.directory.principal(session)?;
    let proof = &input.approval.proof;
    if draft.entry.manager != principal
        || draft.entry.provision_id
            != match &input.request.intent {
                oasis7_custody_core::approval::CustodyIntentV1::Provision {
                    provision_id, ..
                } => provision_id.clone(),
                _ => return Err(denied()),
            }
        || proof.manager_account_id != session.account_id
        || proof.manager_session_ref != session.session_ref
        || proof.origin_credential_id != session.credential_ref
        || proof.origin_credential_version != session.credential_version
        || proof.manager_security_version != session.security_version
        || proof.manager_policy_version != session.policy_version
    {
        return Err(denied());
    }
    let allowed_managers = config
        .directory
        .managers
        .iter()
        .filter(|m| m.environments.contains(&config.directory.environment))
        .map(|m| m.principal.native_account_id.clone())
        .collect::<BTreeSet<_>>();
    let authority = ApprovalContextV1 {
        environment: config.directory.environment.clone(),
        custody_account_id: draft.entry.custody_account_id.clone(),
        audience: config.audience,
        policy_version: config.directory.version,
        security_version: gate.security_epoch,
        service_epoch: config.service_epoch,
    };
    let approval = ApprovalAdmissionContext {
        identity_namespace: config.directory.namespace.clone(),
        identity_instance_epoch: config.directory.epoch,
        authority,
        allowed_managers,
        source: ApprovalSourceVersions {
            manager_account_id: session.account_id.clone(),
            manager_session_ref: session.session_ref.clone(),
            origin_credential_id: session.credential_ref.clone(),
            origin_credential_version: session.credential_version,
            passkey_credential_id: proof.passkey_credential_id.clone(),
            passkey_credential_version: proof.passkey_credential_version,
            manager_policy_version: session.policy_version,
            manager_security_version: session.security_version,
            session_expires_at: session.expires_at,
            session_idle_expires_at: session.idle_expires_at,
        },
        native_key_pins: config.pins,
        fence,
        frozen: false,
        handoff_until: DecimalU64(
            proof
                .expires_at
                .0
                .min(draft.plan.expires_at.0)
                .min(session.expires_at.0)
                .min(session.idle_expires_at.0),
        ),
    };
    Ok(ProvisionApprovalCurrent {
        approval,
        routes: config.directory.routes,
        managers: config.directory.managers,
        policy_version: config.directory.version,
        world_identity: config.directory.world,
        native_identity_namespace: config.directory.namespace,
    })
}

pub(crate) async fn approve(
    state: &JsValue,
    env: &Env,
    rpc: ApproveRpc,
) -> Result<ProvisionApprovalViewV1> {
    let _config = Config::load(env)?;
    rpc.request.validate().map_err(|_| denied())?;
    let draft = read_pristine_draft(state).map_err(|_| denied())?;
    if rpc.provision_id != draft.entry.provision_id {
        return Err(denied());
    }
    let gate_before = gate(env, &rpc, &draft).await?;
    let (fence_before, fence_time_before) = fence(env).await?;
    let input = rpc.request.input.clone();
    let initial = current(
        env,
        &draft,
        &rpc.manager_session,
        &input,
        &gate_before,
        fence_before,
        fence_time_before,
    )?;
    let store = OwnerProvisionApprovalStore::initialize(state.clone(), draft.clone(), initial)
        .map_err(|_| denied())?;
    let pending = store
        .begin(input.clone(), || {
            current(
                env,
                &draft,
                &rpc.manager_session,
                &input,
                &gate_before,
                fence_before,
                fence_time_before,
            )
            .map_err(|_| JsValue::from_str("current approval authority unavailable"))
        })
        .await
        .map_err(|_| denied())?;
    if pending.stage() != ApprovalAdmissionStage::Consumed {
        let request = pending.native_verify_request().map_err(|_| denied())?;
        let raw = serde_json::to_string(&request)?;
        if raw.len() > MAX_BYTES {
            return Err(denied());
        }
        let signed: SignedApprovalVerificationV1 = serde_json::from_str(&text(
            verify_native(env, &raw).await.map_err(|_| denied())?,
        )?)
        .map_err(|_| denied())?;
        let gate_after = gate(env, &rpc, &draft).await?;
        let (fence_after, fence_time_after) = fence(env).await?;
        store
            .commit(&input.operation_id, signed, || {
                current(
                    env,
                    &draft,
                    &rpc.manager_session,
                    &input,
                    &gate_after,
                    fence_after,
                    fence_time_after,
                )
                .map_err(|_| JsValue::from_str("current approval authority unavailable"))
            })
            .await
            .map_err(|_| denied())?;
    }
    let snapshot = store.snapshot().map_err(|_| denied())?;
    Ok(ProvisionApprovalViewV1 {
        api_version: 1,
        provision_id: draft.entry.provision_id,
        custody_account_id: draft.entry.custody_account_id,
        operation_id: input.operation_id,
        stage: snapshot.state.provision().stage(),
    })
}
