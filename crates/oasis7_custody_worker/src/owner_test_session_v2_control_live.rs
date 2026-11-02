use super::*;
#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/test_session_rpc.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch,js_name=readTestSessionAttestation)]
    async fn attest(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
pub(super) async fn current(
    state: &JsValue,
    env: &Env,
    store: &OwnerProvisionApprovalStore,
    rpc: &TestSessionClientAdmissionRpc,
    purpose: SessionPurposeV1,
    source: &str,
    lifetime: SignedManagedNativeSessionLifetimeV1,
) -> Result<ClientLiveCurrent> {
    let id = &rpc.http.selector.session_id;
    let ticket = store
        .reserve_test_session_v2_source_read(id, source)
        .map_err(|_| denied())?;
    if ticket != 4 {
        return Err(denied());
    }
    let started = clock()?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let descriptor = saved
        .state
        .test_session_v2_credential_live_descriptor(id, purpose, source)
        .map_err(|_| denied())?;
    let raw = serde_json::to_string(&TestSessionAttestationRequest {
        session: descriptor.session.clone(),
        lifetime: lifetime.clone(),
    })?;
    let proof: TestSessionPrivateAttestation = serde_json::from_str(
        &super::super::super::super::super::text(attest(env, &raw).await.map_err(|_| denied())?)?,
    )
    .map_err(|_| denied())?;
    if proof.session_ref != descriptor.session.session_ref
        || proof.private_sid_generation != descriptor.session.private_sid_generation
        || proof.lifetime_digest != lifetime.lifetime.digest().map_err(|_| denied())?
    {
        return Err(denied());
    }
    let current = observed(state, env, store, rpc, source, false).await?;
    Ok(ClientLiveCurrent {
        native: session_authority::current_v2(state, env, &current, true).map_err(|_| denied())?,
        lifetime,
        private_sid_generation: proof.private_sid_generation,
        source_operation_id: source.into(),
        source_ticket: ticket,
        observed_at: started,
    })
}
pub(super) fn observation(
    store: &OwnerProvisionApprovalStore,
    id: &str,
    descriptor: TestSessionCredentialLiveDescriptorV2,
    ticket: u8,
    native: &NativeStageCurrent,
    started: u64,
) -> Result<TestSessionCredentialObservedV2> {
    let saved = store.snapshot().map_err(|_| denied())?;
    let source = saved
        .state
        .test_session_v2_source(id, &descriptor.source_operation_id)
        .ok_or_else(denied)?;
    let signed = source.source.verification.as_ref().ok_or_else(denied)?;
    let expires = started
        .checked_add(5000)
        .ok_or_else(denied)?
        .min(descriptor.deadline.get())
        .min(source.source.expires_at.0)
        .min(native.provision.approval.handoff_until.0);
    if clock()? >= expires {
        return Err(denied());
    }
    let revision = oasis7_custody_core::approval::bytes_digest(&serde_json::to_vec(&(
        "oasis7.custody.client-source-authority.v2",
        &descriptor.authority,
        &descriptor.approved_input_digest,
        &descriptor.source_operation_id,
        signed,
    ))?);
    Ok(TestSessionCredentialObservedV2 {
        service_epoch: native.provision.approval.authority.service_epoch,
        target_security_epoch: native.provision.approval.authority.security_version,
        descriptor,
        source_ticket: ticket,
        authority_revision: revision,
        observed_at: UnixMillis::new(started),
        expires_at: UnixMillis::new(expires),
    })
}
