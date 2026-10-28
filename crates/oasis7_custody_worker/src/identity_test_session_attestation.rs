//! Fixed private manifest metadata reader; no key load, decryption or native I/O.
use super::{actor, denied, manifest_name_parts};
use crate::{
    native_private_store::manifest::{NativeHistoryLookup, NativeManifestStore},
    owner_test_session_rpc::{TestSessionAttestationRequest, TestSessionPrivateAttestation},
};
use wasm_bindgen::JsValue;
use worker::{Env, Response, Result};

pub(super) fn response(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let request: TestSessionAttestationRequest = serde_json::from_str(raw).map_err(|_| denied())?;
    let session = &request.session;
    let identity = &session.scope.role_scope.context.identity;
    if env.var("CUSTODY_ROLE_KEY_ENVIRONMENT")?.to_string() != identity.environment {
        return Err(denied());
    }
    request
        .lifetime
        .verify_root(
            &env.var("CUSTODY_NATIVE_APPROVAL_KEY_ID")?.to_string(),
            &env.var("CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY")?.to_string(),
        )
        .map_err(|_| denied())?;
    if request.lifetime.lifetime.status.session_ref != session.session_ref
        || request.lifetime.lifetime.status.generation != session.native_binding_generation
    {
        return Err(denied());
    }
    let store = NativeManifestStore::new(state);
    let snapshot = store
        .read_completed_history(&NativeHistoryLookup {
            environment: &identity.environment,
            account_id: &identity.account_id,
            provision_id: &session.scope.role_scope.provision_id,
            result_permit_id: &session.source_permit_id,
            phase: session.phase,
            native_operation_id: Some(&session.native_operation_id),
        })
        .map_err(|_| denied())?
        .ok_or_else(denied)?;
    let history = super::history::entry(snapshot)?.ok_or_else(denied)?;
    match &history.result {
        oasis7_custody_core::provisioning::NativeStageResultV1::SessionStatus(old)
            if old.status.receipt == request.lifetime.lifetime.status.receipt => {}
        _ => return Err(denied()),
    }
    let stored = history.session_attestation.ok_or_else(denied)?;
    if history.scope != session.scope
        || stored.session_ref != session.session_ref
        || stored.private_sid_generation != session.private_sid_generation
        || store
            .current_private_session_generation()
            .map_err(|_| denied())?
            != session.private_sid_generation
    {
        return Err(denied());
    }
    Response::from_json(&TestSessionPrivateAttestation {
        session_ref: stored.session_ref,
        private_sid_generation: stored.private_sid_generation,
        lifetime_digest: request.lifetime.lifetime.digest().map_err(|_| denied())?,
    })
}

pub(crate) async fn read(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let request: TestSessionAttestationRequest = serde_json::from_str(raw).map_err(|_| denied())?;
    let scope = &request.session.scope.role_scope;
    let name = manifest_name_parts(env, &scope.context.identity.account_id, &scope.provision_id)?;
    let result: TestSessionPrivateAttestation =
        actor(env, &name, "/internal/test-session-attestation", &request)
            .await
            .map_err(|_| denied())?;
    serde_json::to_string(&result).map_err(|_| denied())
}
