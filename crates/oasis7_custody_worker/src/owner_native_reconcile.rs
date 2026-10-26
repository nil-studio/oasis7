//! Read fixed Identity facts without reopening a completed login send.
use super::*;
use crate::native_runtime_rpc::{
    NativeRuntimeAction, NativeRuntimeCall, NativeRuntimeRpc, NativeSessionAttestation,
};
use oasis7_custody_core::{
    native_history::NativeHistoryEntryV1, provisioning::SessionStatusRecoveryRequestV1,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/platform/provision_drive_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readNativeHistory)]
    async fn read_history(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = recoverNativeStatus)]
    async fn recover_status(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}

/// Deployment-owned public material only. Archival is available without the
/// business signing seed, KEK, enabled flag, or a current Owner service pin.
fn verification(
    env: &Env,
) -> Result<crate::provision_approval_store::native::history::NativeHistoryVerificationContext> {
    use crate::provision_approval_store::native::history::NativeHistoryVerificationContext;
    let value = |key| env.var(key).map(|v| v.to_string()).map_err(|_| denied());
    let http_origin = value("CUSTODY_NATIVE_ORIGIN")?;
    let url = worker::Url::parse(&http_origin).map_err(|_| denied())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.origin().ascii_serialization() != http_origin
    {
        return Err(denied());
    }
    let native_root_key_id = value("CUSTODY_NATIVE_APPROVAL_KEY_ID")?;
    let native_root_public_key = value("CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY")?;
    let bytes: [u8; 32] = hex::decode(&native_root_public_key)
        .map_err(|_| denied())?
        .try_into()
        .map_err(|_| denied())?;
    if hex::encode(bytes) != native_root_public_key
        || native_root_key_id.is_empty()
        || native_root_key_id.len() > 256
    {
        return Err(denied());
    }
    Ok(NativeHistoryVerificationContext {
        http_origin,
        native_root_key_id,
        native_root_public_key,
    })
}

pub(super) async fn reconcile(
    state: &JsValue,
    store: &OwnerProvisionApprovalStore,
    env: &Env,
    call: &NativeRuntimeCall,
    management_operation: &str,
) -> Result<()> {
    let raw = serde_json::to_string(call)?;
    let entry: Option<NativeHistoryEntryV1> = serde_json::from_str(&super::super::text(
        read_history(env, &raw).await.map_err(|_| denied())?,
    )?)
    .map_err(|_| denied())?;
    let Some(entry) = entry else {
        return Ok(());
    };
    let entry = store
        .archive_native_history(entry, &verification(env)?)
        .await
        .map_err(|_| denied())?;
    // Re-submit the immutable fact through the original admission checks, never
    // GetPermit. Fresh source/Root/deadline/freeze checks remain mandatory. A
    // failure cannot imply success or renew the original permission.
    let original_fact = NativeRuntimeRpc {
        call: call.clone(),
        action: NativeRuntimeAction::RecordResult {
            result: entry.result.clone(),
            session: entry
                .session_attestation
                .as_ref()
                .map(|v| NativeSessionAttestation {
                    status_digest: v.status_digest.clone(),
                    session_ref: v.session_ref.clone(),
                    private_sid_generation: v.private_sid_generation,
                }),
        },
    };
    if super::dispatch::dispatch(state, env, original_fact)
        .await
        .is_ok()
    {
        return Ok(());
    }
    if !matches!(entry.result, NativeStageResultV1::SessionStatus(_)) {
        return Ok(());
    }
    let request = SessionStatusRecoveryRequestV1 {
        api_version: 1,
        custody_account_id: call.account_id().into(),
        provision_id: call.provision_id().into(),
        operation_id: crate::directory_config::digest(
            "oasis7.custody.management-status-recovery.v1",
            &(
                management_operation,
                call.operation_id(),
                entry.digest().map_err(|_| denied())?,
            ),
        )?,
        history_digest: entry.digest().map_err(|_| denied())?,
    };
    // Insufficient/expired funding remains archived pending. No fallback login.
    // Only Account's separately charged recovery can admit a new fresh status.
    let _ = recover_status(env, &serde_json::to_string(&request)?).await;
    Ok(())
}
