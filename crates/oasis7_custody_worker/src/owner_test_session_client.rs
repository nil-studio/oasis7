//! Fixed Identity-only client lane. Every stage retains its exact original HTTP PoP.
use super::*;
use crate::{
    provision_approval_store::native::client::{ClientLiveCurrent, client_source_operation_id},
    test_session_client_rpc::*,
};
use oasis7_custody_core::{
    native_binding::SignedManagedNativeSessionLifetimeV1, provisioning::SessionClientOperation,
};
#[path = "owner_test_session_client_dispatch.rs"]
mod client_dispatch;
pub(crate) use client_dispatch::dispatch;

fn json<T: serde::Serialize>(value: T) -> Result<String> {
    Ok(serde_json::to_string(&value)?)
}
async fn observed(
    state: &JsValue,
    env: &Env,
    store: &OwnerProvisionApprovalStore,
    rpc: &TestSessionClientAdmissionRpc,
    source: &str,
    inspection: bool,
) -> Result<authority::Observations> {
    let saved = store.snapshot().map_err(|_| denied())?;
    // The fixed authority path derives source principal from Account originals.
    let result =
        session_authority::runtime_observe(env, &saved, &rpc.http.selector, source, inspection)
            .await?;
    session_authority::current(state, env, &result, !inspection).map_err(|_| denied())?;
    Ok(result)
}
#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/test_session_rpc.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch,js_name=readTestSessionAttestation)]
    async fn attest(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
async fn live(
    state: &JsValue,
    env: &Env,
    store: &OwnerProvisionApprovalStore,
    rpc: &TestSessionClientAdmissionRpc,
    source: &str,
    lifetime: SignedManagedNativeSessionLifetimeV1,
) -> Result<ClientLiveCurrent> {
    let id = &rpc.http.selector.session_id;
    let ticket = store
        .reserve_test_session_source_read(id, source)
        .map_err(|_| denied())?;
    if ticket != 4 {
        return Err(denied());
    }
    let started = clock()?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let session = saved.state.test_sessions().get(id).ok_or_else(denied)?;
    let raw = serde_json::to_string(&TestSessionAttestationRequest {
        session: session.session.clone(),
        lifetime: lifetime.clone(),
    })?;
    let proof: TestSessionPrivateAttestation = serde_json::from_str(&super::super::super::text(
        attest(env, &raw).await.map_err(|_| denied())?,
    )?)
    .map_err(|_| denied())?;
    if proof.session_ref != session.session.session_ref
        || proof.private_sid_generation != session.session.private_sid_generation
        || proof.lifetime_digest != lifetime.lifetime.digest().map_err(|_| denied())?
    {
        return Err(denied());
    }
    let current = observed(state, env, store, rpc, source, false).await?;
    Ok(ClientLiveCurrent {
        native: session_authority::current(state, env, &current, true).map_err(|_| denied())?,
        lifetime,
        private_sid_generation: proof.private_sid_generation,
        source_operation_id: source.into(),
        source_ticket: ticket,
        observed_at: started,
    })
}

fn observation(
    store: &OwnerProvisionApprovalStore,
    id: &str,
    descriptor: TestSessionLiveDescriptorV1,
    ticket: u8,
    native: &NativeStageCurrent,
    started: u64,
) -> Result<TestSessionClientObserved> {
    use oasis7_custody_core::{approval::bytes_digest, time_units::UnixMillis};
    let saved = store.snapshot().map_err(|_| denied())?;
    let source = saved
        .state
        .test_sessions()
        .get(id)
        .and_then(|s| s.sources.get(&descriptor.source_operation_id))
        .ok_or_else(denied)?;
    let signed = source.verification.as_ref().ok_or_else(denied)?;
    let expires = started
        .checked_add(5000)
        .ok_or_else(denied)?
        .min(descriptor.deadline.get())
        .min(source.expires_at.0)
        .min(native.provision.approval.handoff_until.0);
    if clock()? >= expires {
        return Err(denied());
    }
    let revision = bytes_digest(&serde_json::to_vec(&(
        "oasis7.custody.client-source-authority.v1",
        &descriptor.authority,
        &descriptor.source_operation_id,
        signed,
    ))?);
    Ok(TestSessionClientObserved {
        service_epoch: native.provision.approval.authority.service_epoch,
        target_security_epoch: native.provision.approval.authority.security_version,
        descriptor,
        source_ticket: ticket,
        authority_revision: revision,
        observed_at: UnixMillis::new(started),
        expires_at: UnixMillis::new(expires),
    })
}
