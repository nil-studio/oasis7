//! Native-specific current authority. Historical references only preflight a charged inspection.
use super::super::{Config, check_gate, fence, fresh, gate};
use super::*;
use crate::{
    directory_rpc::{ApproveRpc, NativeInspectReference, ProvisionInspectGateRpc},
    provision_approval_store::{ProvisionApprovalCurrent, native::NativeStageScopeCurrent},
};
use oasis7_custody_core::{
    approval::{ApprovalContextV1, CustodySessionStatusV1, SignedApprovalVerificationV1},
    approval_admission::{
        ApprovalAdmissionContext, ApprovalAdmissionInput, ApprovalSourceVersions,
    },
    recovery::RecoveryFence,
};
use std::collections::BTreeSet;

pub(super) struct Observations {
    input: ApprovalAdmissionInput,
    manager: CustodySessionStatusV1,
    gate: crate::directory_rpc::ProvisionGateView,
    fence: RecoveryFence,
    fence_started: DecimalU64,
    inspection_only: bool,
}
fn manager(
    input: &ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
) -> CustodySessionStatusV1 {
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
fn reference(session: &CustodySessionStatusV1) -> NativeInspectReference {
    NativeInspectReference {
        identity_namespace: session.identity_namespace.clone(),
        identity_instance_epoch: session.identity_instance_epoch,
        account_id: session.account_id.clone(),
        session_ref: session.session_ref.clone(),
        credential_ref: session.credential_ref.clone(),
        credential_version: session.credential_version,
        security_version: session.security_version,
        policy_version: session.policy_version,
        expires_at: session.expires_at,
        idle_expires_at: session.idle_expires_at,
    }
}
pub(super) fn original_source(
    saved: &ProvisionApprovalSnapshot,
    call: &crate::native_runtime_rpc::NativeRuntimeCall,
) -> Result<(ApprovalAdmissionInput, SignedApprovalVerificationV1)> {
    use crate::native_runtime_rpc::NativeRuntimeCall;
    let state = &saved.state;
    match call {
        NativeRuntimeCall::Provision { request } => {
            let approved = state.accepted_approval().map_err(|_| denied())?;
            let input = approved.input().clone();
            let signed = state
                .native_stages()
                .get(&request.operation_id)
                .and_then(|s| state.origin_inspections().get(&s.inspection_id))
                .and_then(|i| i.verification.clone())
                .or_else(|| approved.verification().cloned())
                .ok_or_else(denied)?;
            Ok((input, signed))
        }
        NativeRuntimeCall::ManagedLogin { operation_id, .. } => {
            let approved = state
                .managed_login_original(operation_id)
                .ok_or_else(denied)?;
            Ok((
                approved.input().clone(),
                approved.verification().cloned().ok_or_else(denied)?,
            ))
        }
    }
}
pub(super) async fn observe(
    env: &Env,
    draft: &oasis7_custody_core::provisioning::AccountDraftV1,
    input: ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
    inspection_only: bool,
) -> Result<Observations> {
    let manager = manager(&input, signed);
    let gate = if inspection_only {
        let rpc = ProvisionInspectGateRpc {
            manager_reference: reference(&manager),
            provision_id: draft.entry.provision_id.clone(),
            custody_account_id: draft.entry.custody_account_id.clone(),
        };
        let raw = crate::owner_directory::call_inspection_directory(env, &rpc).await?;
        let view = serde_json::from_str(&raw).map_err(|_| denied())?;
        check_gate(&view, draft, clock()?)?;
        view
    } else {
        let rpc = ApproveRpc {
            manager_session: manager.clone(),
            provision_id: draft.entry.provision_id.clone(),
            request: oasis7_custody_core::provisioning::ProvisionApproveRequestV1 {
                api_version: 1,
                input: input.clone(),
            },
        };
        gate(env, &rpc, draft).await?
    };
    let (fence, fence_started) = fence(env).await?;
    Ok(Observations {
        input,
        manager,
        gate,
        fence,
        fence_started,
        inspection_only,
    })
}
/// The HTTP/service boundary supplies real fresh manager metadata; no native verification is fabricated.
pub(super) async fn observe_fresh_manager(
    env: &Env,
    draft: &oasis7_custody_core::provisioning::AccountDraftV1,
    input: ApprovalAdmissionInput,
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
        input,
        manager,
        gate,
        fence,
        fence_started,
        inspection_only: false,
    })
}
/// Final synchronous closure re-reads actual target state. It never edits the signed source context.
pub(super) fn current(
    state: &JsValue,
    env: &Env,
    observed: &Observations,
    require_business: bool,
) -> std::result::Result<NativeStageCurrent, JsValue> {
    build_current(state, env, observed, require_business)
        .map_err(|_| JsValue::from_str("native authority unavailable"))
}
fn build_current(
    state: &JsValue,
    env: &Env,
    observed: &Observations,
    require_business: bool,
) -> Result<NativeStageCurrent> {
    if require_business && observed.inspection_only {
        return Err(denied());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let draft = saved.state.draft();
    let target = saved.state.provision().security_epoch();
    if target.0 > 1
        || (target.0 == 1 && saved.state.bootstrap_activation().is_none())
        || saved.state.frozen()
    {
        return Err(denied());
    }
    let config = Config::load(env)?;
    let native = NativeConfig::load(env)?;
    let now = clock()?;
    check_gate(&observed.gate, draft, now)?;
    fresh(observed.fence_started, now)?;
    observed
        .fence
        .check_epoch(config.service_epoch.0)
        .map_err(|_| denied())?;
    let principal = if observed.inspection_only {
        config
            .directory
            .inspection_reference_principal(&reference(&observed.manager))?
    } else {
        config.directory.principal(&observed.manager)?
    };
    let proof = &observed.input.approval.proof;
    let session = &observed.manager;
    if principal != draft.entry.manager
        || proof.manager_account_id != session.account_id
        || proof.manager_session_ref != session.session_ref
        || proof.origin_credential_id != session.credential_ref
        || proof.origin_credential_version != session.credential_version
        || proof.manager_policy_version != session.policy_version
        || proof.manager_security_version != session.security_version
    {
        return Err(denied());
    }
    let authority = ApprovalContextV1 {
        environment: config.directory.environment.clone(),
        custody_account_id: draft.entry.custody_account_id.clone(),
        audience: config.audience,
        policy_version: config.directory.version,
        security_version: target,
        service_epoch: config.service_epoch,
    };
    // A fresh ManagedLogin is a separate positive-epoch authorization. Its own
    // deadline/proof and current source bound it; an expired Provision plan grants
    // no new rights and must not suppress this independently approved recovery.
    let authority_deadline = match &observed.input.request.intent {
        oasis7_custody_core::approval::CustodyIntentV1::ManagedLogin { deadline, .. } => {
            deadline.0.min(proof.expires_at.0)
        }
        // The book independently bounds initial proof consumption. A consumed
        // fixed setup plan retains its signed deadline; every stage still needs
        // its own fresh origin inspection and current Directory/Fence evidence.
        oasis7_custody_core::approval::CustodyIntentV1::OwnerSetup { deadline, .. } => deadline.0,
        oasis7_custody_core::approval::CustodyIntentV1::OwnerSetupReconcile { plan } => {
            plan.deadline.0
        }
        // This is only the signed source-observation ceiling. Each sibling
        // purpose independently enforces its own immutable deadline in Core.
        oasis7_custody_core::approval::CustodyIntentV1::TestSession {
            write_deadline,
            read_until,
            cleanup_until,
            ..
        } => write_deadline.0.max(read_until.0).max(cleanup_until.0),
        oasis7_custody_core::approval::CustodyIntentV1::Provision { .. } => draft.plan.expires_at.0,
    };
    let approval = ApprovalAdmissionContext {
        identity_namespace: config.directory.namespace.clone(),
        identity_instance_epoch: config.directory.epoch,
        authority,
        allowed_managers: config
            .directory
            .managers
            .iter()
            .filter(|m| m.environments.contains(&config.directory.environment))
            .map(|m| m.principal.native_account_id.clone())
            .collect::<BTreeSet<_>>(),
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
        fence: observed.fence,
        frozen: false,
        handoff_until: DecimalU64(
            authority_deadline
                .min(session.expires_at.0)
                .min(session.idle_expires_at.0),
        ),
    };
    Ok(NativeStageCurrent {
        provision: ProvisionApprovalCurrent {
            approval,
            routes: config.directory.routes,
            managers: config.directory.managers,
            policy_version: config.directory.version,
            world_identity: config.directory.world,
            native_identity_namespace: config.directory.namespace,
        },
        scope: NativeStageScopeCurrent {
            http_origin: native.http_origin,
            native_root_key_id: native.native_root_key_id,
            native_root_public_key: native.native_root_public_key,
            owner_binding_pin: native.owner_pin,
        },
    })
}
