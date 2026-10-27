//! Fixed-role participant coordinator; no caller selects the role or forwarding origin.
use super::*;
use crate::participant_runtime_rpc::{ParticipantRuntimeAction, ParticipantRuntimeRpc};
use oasis7_custody_core::provisioning::ParticipantStepV1;

fn role_allowed(action: &ParticipantRuntimeAction, role: Role) -> bool {
    match action {
        ParticipantRuntimeAction::GetPermit {
            step: ParticipantStepV1::SignBundle,
        }
        | ParticipantRuntimeAction::PublishBundle { .. } => role == Role::Owner,
        _ => role == Role::Identity,
    }
}

use crate::provision_approval_store::native::NativeStageScopeCurrent;
use oasis7_custody_core::provisioning::{
    ParticipantCallbackV1, ParticipantNoncesV1, ParticipantOriginalV1,
    ParticipantProvisionRequestV1,
};
fn scope(config: &NativeConfig) -> NativeStageScopeCurrent {
    NativeStageScopeCurrent {
        http_origin: config.http_origin.clone(),
        native_root_key_id: config.native_root_key_id.clone(),
        native_root_public_key: config.native_root_public_key.clone(),
        owner_binding_pin: config.owner_pin.clone(),
    }
}
fn source(
    saved: &ProvisionApprovalSnapshot,
    original: &ParticipantOriginalV1,
) -> Result<(
    oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
    oasis7_custody_core::approval::SignedApprovalVerificationV1,
)> {
    let approved = saved.state.accepted_approval().map_err(|_| denied())?;
    let signed = original
        .source_inspection
        .verification
        .clone()
        .or_else(|| approved.verification().cloned())
        .ok_or_else(denied)?;
    Ok((approved.input().clone(), signed))
}
async fn observe(
    state: &JsValue,
    env: &Env,
    request: &ParticipantProvisionRequestV1,
    inspection_only: bool,
) -> Result<authority::Observations> {
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let original = store.participant_original(request).map_err(|_| denied())?;
    let (input, signed) = source(&saved, &original)?;
    participant_authority::observe(env, &saved, input, &signed, inspection_only).await
}
async fn prepare_source(
    state: &JsValue,
    env: &Env,
    request: &ParticipantProvisionRequestV1,
) -> Result<()> {
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let original = store.participant_original(request).map_err(|_| denied())?;
    if original.source_inspection.verification.is_some() {
        return Ok(());
    }
    if original.source_inspection.claimed_at.is_some() {
        return Err(denied());
    }
    let before = observe(state, env, request, true).await?;
    let verify = store
        .claim_participant_source(&request.operation_id, || {
            participant_authority::current(state, env, &before, false)
        })
        .map_err(|_| denied())?;
    let raw = serde_json::to_string(&verify)?;
    let signed = serde_json::from_str(&super::super::super::text(
        super::super::super::verify_native(env, &raw)
            .await
            .map_err(|_| denied())?,
    )?)
    .map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let approved = saved.state.accepted_approval().map_err(|_| denied())?;
    let after =
        participant_authority::observe(env, &saved, approved.input().clone(), &signed, false)
            .await?;
    store
        .complete_participant_source(&request.operation_id, signed, || {
            participant_authority::current(state, env, &after, true)
        })
        .await
        .map_err(|_| denied())?;
    store
        .claim_participant_workflow(&request.operation_id, || {
            participant_authority::current(state, env, &after, true)
        })
        .await
        .map_err(|_| denied())
}
fn step_key(step: ParticipantStepV1) -> &'static str {
    match step {
        ParticipantStepV1::Policy => "policy",
        ParticipantStepV1::SignBundle => "sign_bundle",
        ParticipantStepV1::Authorize => "authorize",
        ParticipantStepV1::Submit => "submit",
        ParticipantStepV1::OwnerView => "owner_view",
    }
}
pub(super) async fn dispatch(
    state: &JsValue,
    env: &Env,
    rpc: ParticipantRuntimeRpc,
    static_role: Role,
) -> Result<String> {
    rpc.request.validate().map_err(|_| denied())?;
    if !role_allowed(&rpc.action, static_role) {
        return Err(denied());
    }
    participant_authority::enabled(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let draft = saved.state.draft();
    if rpc.request.custody_account_id != draft.entry.custody_account_id
        || rpc.request.provision_id != draft.entry.provision_id
    {
        return Err(denied());
    }
    let id = &rpc.request.operation_id;
    let original = store
        .participant_original(&rpc.request)
        .map_err(|_| denied())?;
    if original.workflow_claimed_at.is_none() || original.result.is_some() {
        return Err(denied());
    }
    match rpc.action {
        ParticipantRuntimeAction::GetPermit { step } => {
            let ticket = store.reserve_participant_read(id).map_err(|_| denied())?;
            let observed = observe(state, env, &rpc.request, false).await?;
            let original = store
                .participant_original(&rpc.request)
                .map_err(|_| denied())?;
            if !original.claims.contains_key(step_key(step)) {
                store
                    .claim_participant_step(id, step, || {
                        participant_authority::current(state, env, &observed, true)
                    })
                    .await
                    .map_err(|_| denied())?;
            }
            let permit = store
                .authorize_participant_read(id, ticket, step, || {
                    participant_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&permit)?)
        }
        ParticipantRuntimeAction::PublishBundle { candidate } => {
            let ticket = store.reserve_participant_read(id).map_err(|_| denied())?;
            let observed = observe(state, env, &rpc.request, false).await?;
            let bundle = store
                .publish_participant_bundle(id, ticket, candidate, || {
                    participant_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&bundle)?)
        }
        ParticipantRuntimeAction::RecordPolicy { original, session } => {
            store
                .reserve_participant_callback(id, ParticipantCallbackV1::Policy)
                .map_err(|_| denied())?;
            let observed = observe(state, env, &rpc.request, false).await?;
            store
                .record_participant_policy(id, original, session, || {
                    participant_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok("{}".into())
        }
        ParticipantRuntimeAction::RecordAdmission { admission, session } => {
            store
                .reserve_participant_callback(id, ParticipantCallbackV1::Admission)
                .map_err(|_| denied())?;
            let observed = observe(state, env, &rpc.request, false).await?;
            store
                .record_participant_admission(id, admission, session, || {
                    participant_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok("{}".into())
        }
        ParticipantRuntimeAction::RecordResult { result, session } => {
            store
                .reserve_participant_callback(id, ParticipantCallbackV1::Final)
                .map_err(|_| denied())?;
            let observed = observe(state, env, &rpc.request, false).await?;
            let receipt = store
                .complete_participant(id, *result, session, || {
                    participant_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&receipt)?)
        }
    }
}

/// This is called once by the fixed public continue coordinator, before invoking
/// Identity. An existing incomplete workflow is unknown and must not be rerun.
pub(super) async fn start_workflow(
    state: &JsValue,
    env: &Env,
    request: ParticipantProvisionRequestV1,
) -> Result<()> {
    request.validate().map_err(|_| denied())?;
    participant_authority::enabled(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let draft = saved.state.draft();
    if request.custody_account_id != draft.entry.custody_account_id
        || request.provision_id != draft.entry.provision_id
        || saved
            .state
            .participant_original(&request.operation_id)
            .is_some()
    {
        return Err(denied());
    }
    let config = NativeConfig::load(env)?;
    let nonce = || crate::approval_crypto::verification_nonce().map_err(|_| denied());
    store
        .reserve_participant_provision(
            request.clone(),
            ParticipantNoncesV1 {
                source: nonce()?,
                policy: nonce()?,
                owner_view: nonce()?,
                session: nonce()?,
            },
            &scope(&config),
        )
        .map_err(|_| denied())?;
    prepare_source(state, env, &request).await
}
