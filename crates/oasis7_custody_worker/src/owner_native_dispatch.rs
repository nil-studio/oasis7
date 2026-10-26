//! Fixed Account native RPC coordinator. Reads spend their local ticket before remote I/O.
use super::authority;
use super::*;
use crate::native_runtime_rpc::{NativeRuntimeAction, NativeRuntimeCall, NativeRuntimeRpc};
use crate::provision_approval_store::native::NativeStageScopeCurrent;
use oasis7_custody_core::provisioning::PersistedNativeSessionAttestation;
use wasm_bindgen::JsValue;

fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
fn scope(config: &NativeConfig) -> NativeStageScopeCurrent {
    NativeStageScopeCurrent {
        http_origin: config.http_origin.clone(),
        native_root_key_id: config.native_root_key_id.clone(),
        native_root_public_key: config.native_root_public_key.clone(),
        owner_binding_pin: config.owner_pin.clone(),
    }
}
async fn observation(
    store: &OwnerProvisionApprovalStore,
    env: &Env,
    call: &NativeRuntimeCall,
) -> Result<authority::Observations> {
    let saved = store.snapshot().map_err(|_| denied())?;
    let (input, signed) = authority::original_source(&saved, call)?;
    authority::observe(env, saved.state.draft(), input, &signed, false).await
}
async fn prepare_stage(
    state: &JsValue,
    env: &Env,
    store: &OwnerProvisionApprovalStore,
    call: &NativeRuntimeCall,
) -> Result<()> {
    let NativeRuntimeCall::Provision { request } = call else {
        return Err(denied());
    };
    let saved = store.snapshot().map_err(|_| denied())?;
    let original = store.native_stage_original(request).map_err(|_| denied())?;
    if original.claimed_at.is_some() {
        return Ok(());
    }
    let inspection = saved
        .state
        .origin_inspections()
        .get(&original.inspection_id)
        .ok_or_else(denied)?;
    if inspection.verification.is_none() {
        if inspection.claimed_at.is_some() {
            return Err(denied());
        }
        let (input, signed) = authority::original_source(&saved, call)?;
        // Historical reference preflight only permits claiming this charged nonce.
        let before =
            authority::observe(env, saved.state.draft(), input.clone(), &signed, true).await?;
        let request = store
            .claim_native_origin_inspection(&original.request.operation_id, || {
                authority::current(state, env, &before, false)
            })
            .map_err(|_| denied())?;
        let raw = serde_json::to_string(&request)?;
        let signed: oasis7_custody_core::approval::SignedApprovalVerificationV1 =
            serde_json::from_str(&super::super::text(
                super::super::verify_native(env, &raw)
                    .await
                    .map_err(|_| denied())?,
            )?)
            .map_err(|_| denied())?;
        // Fresh native signature + normal directory gate are mandatory before business claim.
        let after = authority::observe(env, saved.state.draft(), input, &signed, false).await?;
        store
            .complete_native_origin_inspection(&original.request.operation_id, signed, || {
                authority::current(state, env, &after, true)
            })
            .await
            .map_err(|_| denied())?;
        store
            .claim_native_stage(&original.request.operation_id, || {
                authority::current(state, env, &after, true)
            })
            .await
            .map_err(|_| denied())?;
    } else {
        let after = observation(store, env, call).await?;
        store
            .claim_native_stage(&original.request.operation_id, || {
                authority::current(state, env, &after, true)
            })
            .await
            .map_err(|_| denied())?;
    }
    Ok(())
}
pub(super) async fn dispatch(state: &JsValue, env: &Env, rpc: NativeRuntimeRpc) -> Result<String> {
    if !rpc.call.validate() {
        return Err(denied());
    }
    let config = NativeConfig::load(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let draft = saved.state.draft();
    if rpc.call.account_id() != draft.entry.custody_account_id
        || rpc.call.provision_id() != draft.entry.provision_id
    {
        return Err(denied());
    }
    if let NativeRuntimeCall::Provision { request } = &rpc.call {
        if saved
            .state
            .native_stages()
            .contains_key(&request.operation_id)
        {
            store.native_stage_original(request).map_err(|_| denied())?;
        } else if matches!(&rpc.action, NativeRuntimeAction::GetPermit) {
            let inspection_id = crate::directory_config::digest(
                "oasis7.custody.native-source-inspection.v1",
                &(&request.operation_id, &request.custody_account_id),
            )?;
            store
                .reserve_native_stage(request.clone(), &inspection_id, &scope(&config))
                .map_err(|_| denied())?;
        } else {
            return Err(denied());
        }
    }
    let id = rpc.call.operation_id();
    match rpc.action {
        NativeRuntimeAction::GetPermit => match &rpc.call {
            NativeRuntimeCall::Provision { .. } => {
                let ticket = store.reserve_native_stage_read(id).map_err(|_| denied())?;
                prepare_stage(state, env, &store, &rpc.call).await?;
                let current = observation(&store, env, &rpc.call).await?;
                encode(
                    &store
                        .authorize_native_stage_read(id, ticket, || {
                            authority::current(state, env, &current, true)
                        })
                        .await
                        .map_err(|_| denied())?,
                )
            }
            NativeRuntimeCall::ManagedLogin { .. } => {
                let ticket = store.reserve_managed_login_read(id).map_err(|_| denied())?;
                let current = observation(&store, env, &rpc.call).await?;
                if store
                    .snapshot()
                    .map_err(|_| denied())?
                    .state
                    .managed_login_task(id)
                    .ok_or_else(denied)?
                    .claimed_at
                    .is_none()
                {
                    store
                        .claim_managed_login(id, || authority::current(state, env, &current, true))
                        .await
                        .map_err(|_| denied())?;
                }
                encode(
                    &store
                        .authorize_managed_login_read(id, ticket, || {
                            authority::current(state, env, &current, true)
                        })
                        .await
                        .map_err(|_| denied())?,
                )
            }
        },
        NativeRuntimeAction::OwnerStatement { challenge } => {
            let NativeRuntimeCall::Provision { request } = &rpc.call else {
                return Err(denied());
            };
            if let Ok(original) = store.native_binding_statement(request) {
                return encode(&original);
            }
            let ticket = store.reserve_native_stage_read(id).map_err(|_| denied())?;
            let current = observation(&store, env, &rpc.call).await?;
            encode(
                &super::issue_binding_statement(state, env, request, challenge, ticket, || {
                    authority::current(state, env, &current, true)
                })
                .await?,
            )
        }
        NativeRuntimeAction::BindingStatement {
            binding_operation_id,
        } => {
            let NativeRuntimeCall::Provision { request } = &rpc.call else {
                return Err(denied());
            };
            if binding_operation_id != request.binding_operation_id {
                return Err(denied());
            }
            encode(
                &store
                    .native_binding_statement(request)
                    .map_err(|_| denied())?,
            )
        }
        NativeRuntimeAction::BindingReceipt => {
            let saved = store.snapshot().map_err(|_| denied())?;
            let receipt = saved
                .state
                .native_stages()
                .values()
                .find_map(|v| match &v.result {
                    Some(NativeStageResultV1::BindingReceipt(v)) => Some(v),
                    _ => None,
                })
                .ok_or_else(denied)?;
            encode(receipt)
        }
        NativeRuntimeAction::RecordResult { result, session } => {
            // No caller-provided Current. Session evidence is only from the fixed Identity named RPC.
            let session = session.as_ref().map(|v| PersistedNativeSessionAttestation {
                status_digest: &v.status_digest,
                session_ref: &v.session_ref,
                private_sid_generation: v.private_sid_generation.0,
            });
            let original = match &rpc.call {
                NativeRuntimeCall::Provision { .. } => {
                    store.recover_native_result(id, &result, session.as_ref())
                }
                NativeRuntimeCall::ManagedLogin { .. } => {
                    store.recover_managed_login_result(id, &result, session.as_ref())
                }
            }
            .map_err(|_| denied())?;
            if let Some(original) = original {
                return encode(&original);
            }
            let current = observation(&store, env, &rpc.call).await?;
            match &rpc.call {
                NativeRuntimeCall::Provision { .. } => encode(
                    &store
                        .complete_native_stage(id, result, session.as_ref(), || {
                            authority::current(state, env, &current, true)
                        })
                        .await
                        .map_err(|_| denied())?,
                ),
                NativeRuntimeCall::ManagedLogin { .. } => encode(
                    &store
                        .complete_managed_login(id, result, session.as_ref(), || {
                            authority::current(state, env, &current, true)
                        })
                        .await
                        .map_err(|_| denied())?,
                ),
            }
        }
    }
}
