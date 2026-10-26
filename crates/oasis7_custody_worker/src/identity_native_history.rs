//! Fixed public-fact history capability. No current grant, secret, crypto or native I/O.
use super::{actor, denied, manifest_name};
use crate::{
    native_private_store::manifest::{
        NativeHistoryLookup, NativeHistorySnapshot, NativeManifestStore, NativePhase,
    },
    native_runtime_rpc::NativeRuntimeCall,
};
use oasis7_custody_core::{
    native_history::{NativeHistoryEntryV1, NativeSessionStorageAttestationV1},
    provisioning::{NativeStageKindV1, NativeStageResultV1},
};
use wasm_bindgen::JsValue;
use worker::{Env, Response, Result};
pub(super) fn entry(snapshot: NativeHistorySnapshot) -> Result<Option<NativeHistoryEntryV1>> {
    // Explicit login-required is a local tightening fact, not a signed Root fact.
    if matches!(snapshot.result, NativeStageResultV1::ExplicitLoginRequired) {
        return Ok(None);
    }
    let session_attestation = match (&snapshot.result, snapshot.private_sid_generation) {
        (NativeStageResultV1::SessionStatus(status), Some(generation)) => {
            Some(NativeSessionStorageAttestationV1 {
                status_digest: status.status.digest().map_err(|_| denied())?,
                session_ref: status.status.session_ref.clone(),
                private_sid_generation: generation,
            })
        }
        (NativeStageResultV1::SessionStatus(_), None) => return Err(denied()),
        (_, None) => None,
        _ => return Err(denied()),
    };
    let value = NativeHistoryEntryV1 {
        api_version: 1,
        environment: snapshot
            .scope
            .role_scope
            .context
            .identity
            .environment
            .clone(),
        custody_account_id: snapshot
            .scope
            .role_scope
            .context
            .identity
            .account_id
            .clone(),
        provision_id: snapshot.scope.role_scope.provision_id.clone(),
        scope: snapshot.scope,
        phase: snapshot.phase,
        native_operation_id: snapshot.native_operation_id,
        original_permit_id: snapshot.original_permit_id,
        result_permit_id: snapshot.result_permit_id,
        stable_intent_digest: snapshot.stable_intent_digest,
        result: snapshot.result,
        session_attestation,
    };
    value.validate().map_err(|_| denied())?;
    Ok(Some(value))
}
fn read_snapshot(
    state: JsValue,
    env: &Env,
    call: &NativeRuntimeCall,
) -> Result<Option<NativeHistorySnapshot>> {
    if !call.validate() {
        return Err(denied());
    }
    let environment = env.var("CUSTODY_ROLE_KEY_ENVIRONMENT")?.to_string();
    let (phase, native_operation_id) = match call {
        NativeRuntimeCall::Provision { request } => match request.stage {
            NativeStageKindV1::Registration => (NativePhase::Register, None),
            NativeStageKindV1::Binding => (
                NativePhase::Binding,
                Some(request.binding_operation_id.as_str()),
            ),
            NativeStageKindV1::Login => (NativePhase::InitialLogin, None),
        },
        NativeRuntimeCall::ManagedLogin { .. } => (NativePhase::ManagedLogin, None),
    };
    let snapshot = NativeManifestStore::new(state)
        .read_completed_history(&NativeHistoryLookup {
            environment: &environment,
            account_id: call.account_id(),
            provision_id: call.provision_id(),
            result_permit_id: call.operation_id(),
            phase,
            native_operation_id,
        })
        .map_err(|_| denied())?;
    Ok(snapshot)
}
pub(super) fn read_local(
    state: JsValue,
    env: &Env,
    call: &NativeRuntimeCall,
) -> Result<Option<NativeHistoryEntryV1>> {
    read_snapshot(state, env, call)?
        .map(entry)
        .transpose()
        .map(Option::flatten)
}
fn missing_local(
    state: JsValue,
    env: &Env,
    call: &NativeRuntimeCall,
) -> Result<Option<oasis7_custody_core::native_history::NativeMissingSessionV1>> {
    use oasis7_custody_core::native_history::NativeMissingSessionV1;
    let Some(s) = read_snapshot(state, env, call)? else {
        return Ok(None);
    };
    if !matches!(s.result, NativeStageResultV1::ExplicitLoginRequired) {
        return Ok(None);
    }
    if s.private_sid_generation.is_some() {
        return Err(denied());
    }
    let fact = NativeMissingSessionV1 {
        api_version: 1,
        scope: s.scope,
        phase: s.phase,
        native_operation_id: s.native_operation_id,
        original_permit_id: s.original_permit_id,
        result_permit_id: s.result_permit_id,
        stable_intent_digest: s.stable_intent_digest,
    };
    fact.validate().map_err(|_| denied())?;
    Ok(Some(fact))
}
pub(super) fn missing_response(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let call: NativeRuntimeCall = serde_json::from_str(raw).map_err(|_| denied())?;
    Response::from_json(&missing_local(state, env, &call)?)
}
pub(crate) async fn read_missing_native_session_raw(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > 32 * 1024 {
        return Err(denied());
    }
    let call: NativeRuntimeCall = serde_json::from_str(raw).map_err(|_| denied())?;
    if !call.validate() {
        return Err(denied());
    }
    let result: Option<oasis7_custody_core::native_history::NativeMissingSessionV1> = actor(
        env,
        &manifest_name(env, &call)?,
        "/internal/missing-session",
        &call,
    )
    .await
    .map_err(|_| denied())?;
    serde_json::to_string(&result).map_err(|_| denied())
}
pub(super) fn response(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let call: NativeRuntimeCall = serde_json::from_str(raw).map_err(|_| denied())?;
    Response::from_json(&read_local(state, env, &call)?)
}
pub(crate) async fn read_native_history_raw(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > 32 * 1024 {
        return Err(denied());
    }
    let call: NativeRuntimeCall = serde_json::from_str(raw).map_err(|_| denied())?;
    if !call.validate() {
        return Err(denied());
    }
    let result: Option<NativeHistoryEntryV1> =
        actor(env, &manifest_name(env, &call)?, "/internal/history", &call)
            .await
            .map_err(|_| denied())?;
    serde_json::to_string(&result).map_err(|_| denied())
}
