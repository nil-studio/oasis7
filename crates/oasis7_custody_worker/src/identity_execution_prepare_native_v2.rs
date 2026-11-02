//! Fixed Identity-only Native10 consumer. Private response/slots never cross
//! the Account boundary; only the independently signed projection/ref escapes.
use super::*;
use crate::{
    execution_context_stage_store_v2 as stage,
    native_identity::provision_transport::NativeProvisionTransport,
    owner_test_session_rpc::TestSessionSelectorV1,
};
use oasis7_custody_core::{
    DecimalU64, context_body::ContextBodySlotKindV1, context_body_v2::*,
    native_cleanup_v2::AccountMachinePurposeV2, native_execution_prepare_v2::*,
};
use worker::{Env, Response};
const PATH: &str = "/internal/execution-prepare-native-v2";
fn check(env: &Env, q: &NativeExecutionPrepareRequestV2) -> JResult<()> {
    let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
        env,
        AccountMachinePurposeV2::NativeExecutionPrepareV2,
    )?;
    q.wire_bytes().map_err(|_| js_denied())?;
    q.proof
        .verify_issuer(&pin, &floor, clock()?)
        .map_err(|_| js_denied())?;
    Ok(())
}
pub(super) async fn response(state: JsValue, env: &Env, raw: &str) -> worker::Result<Response> {
    let frame: crate::execution_dispatch_rpc_v2::ExecutionDispatchFrameV2 =
        serde_json::from_str(raw).map_err(|_| denied())?;
    frame
        .check(
            env,
            ExecutionPreparePhysicalEdgeV2::IdentityServiceToNativeActor,
            clock()?,
        )
        .map_err(|_| denied())?;
    let q = frame.request.as_ref();
    check(env, q).map_err(|_| denied())?;
    let transport = NativeProvisionTransport::from_env(env).map_err(|_| denied())?;
    if transport.native_origin() != q.proof.certificate.issuer.native_audience_origin {
        return Err(denied());
    }
    transport
        .validate_binding(&q.proof.certificate.native_binding)
        .map_err(|_| denied())?;
    let role = &q.proof.certificate.session.scope.role_scope;
    let name = manifest_name_parts(env, &role.context.identity.account_id, &role.provision_id)?;
    stage::require_manifest_recipient(&state, env, &name).map_err(|_| denied())?;
    let token = stage::claim_native_prepare_gated(&state, env, frame).map_err(|_| denied())?;
    let q = token.request();
    let c = &q.proof.certificate;
    let private = token.private_current_proof(&state).map_err(|_| denied())?;
    let current_private = || -> JResult<()> {
        check(env, q)?;
        let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
            env,
            AccountMachinePurposeV2::ExecutionPrivateCurrentV2,
        )?;
        private
            .verify_issuer(&pin, &floor, clock()?)
            .map_err(|_| js_denied())?;
        super::test_session_attestation::metadata(
            state.clone(),
            env,
            &crate::owner_test_session_rpc::TestSessionAttestationRequest {
                session: private.certificate.session.clone(),
                lifetime: private.certificate.lifetime.clone(),
            },
        )
        .map_err(|_| js_denied())?;
        Ok(())
    };
    current_private().map_err(|_| denied())?;
    let role = &c.session.scope.role_scope;
    let selector = TestSessionSelectorV1 {
        api_version: 2,
        custody_account_id: role.context.identity.account_id.clone(),
        provision_id: role.provision_id.clone(),
        session_id: c.session_id.clone(),
    };
    let secrets = super::private_session_read_v2::read_with_async(
        state.clone(),
        env,
        &selector,
        &c.session,
        &current_private,
        |edge, scope| {
            let token = &token;
            let state = &state;
            async move {
                let body = serde_json::to_vec(&CipherRead { scope }).map_err(|_| js_denied())?;
                token
                    .claim_before_io(
                        state,
                        env,
                        edge,
                        oasis7_custody_core::approval::bytes_digest(&body),
                    )
                    .await?;
                Ok(())
            }
        },
    )
    .await
    .map_err(|_| denied())?;
    current_private().map_err(|_| denied())?;
    let native_authorization = token
        .claim_before_io(
            &state,
            env,
            ExecutionPreparePhysicalEdgeV2::IdentityActorToNative,
            oasis7_custody_core::approval::bytes_digest(&q.wire_bytes().map_err(|_| denied())?),
        )
        .await
        .map_err(|_| denied())?;
    current_private().map_err(|_| denied())?;
    let csrf = std::str::from_utf8(secrets.csrf.as_bytes()).map_err(|_| denied())?;
    let raw = String::from_utf8(q.wire_bytes().map_err(|_| denied())?).map_err(|_| denied())?;
    let reply = transport
        .request_gated_execution_prepare::<NativeExecutionPrepareGatedReplyV2>(
            &raw,
            &secrets.cookie,
            csrf,
            &native_authorization,
        )
        .await
        .map_err(|_| denied())?;
    if reply.http_status != 200 || reply.cookie.is_some() {
        return Err(denied());
    }
    let gated_reply = reply.body;
    gated_reply.wire_bytes().map_err(|_| denied())?;
    let reply = gated_reply.reply;
    reply.wire_bytes().map_err(|_| denied())?;
    let pin = transport.fact_pin();
    let result_digest = prepared_response_digest_v2(&reply.world_response).map_err(|_| denied())?;
    reply
        .physical_receipt
        .verify_native(q, &pin.key_id, &pin.public_key, &result_digest)
        .map_err(|_| denied())?;
    let projection = &reply.world_response.payload.projection;
    let projection_digest =
        oasis7_client_api::world_service::task_context_projection::public_projection_digest_v2(
            projection,
        )
        .map_err(|_| denied())?;
    reply
        .physical_receipt
        .verify_native_public(q, &pin.key_id, &pin.public_key, &projection_digest)
        .map_err(|_| denied())?;
    current_private().map_err(|_| denied())?;
    let scope = ContextBodyScopeV2 {
        api_version: 2,
        format: ContextBodyFormatV2::FullSignedResponseV2,
        identity_scope: role.clone(),
        world: c.world.clone(),
        session_id: c.session_id.clone(),
        context_operation_id: c.stage_operation_id.clone(),
        prepare_operation_id: q
            .original
            .request
            .original
            .request
            .client_operation_id
            .clone(),
        session_intent_digest: c.session_intent_digest.clone(),
        native_binding_digest: c.native_binding.digest().map_err(|_| denied())?,
        slot: ContextBodySlotKindV1::Prepared,
        version: DecimalU64(1),
        write_deadline: c.execution_deadline,
        context: projection.payload.context.clone(),
        public_projection_digest: projection_digest,
        body: projection.payload.body.clone(),
    };
    let crypto = crate::context_body_crypto::ContextBodyCrypto::load(env).map_err(|_| denied())?;
    // This performs the actual complete SDK/private-body/nested signatures
    // and exact 64KiB full-signed boundary, before any sealing or PUT.
    let sealed = crypto
        .prepare_fullsigned_slot_v2(scope, &c.world, &q.original.request, &reply.world_response)
        .await
        .map_err(|_| denied())?;
    current_private().map_err(|_| denied())?;
    token
        .native_completed(&state, env, &gated_reply.world_claims, clock()?)
        .map_err(|_| denied())?;
    token
        .claim_before_io(
            &state,
            env,
            ExecutionPreparePhysicalEdgeV2::IdentityActorToContextBody,
            oasis7_custody_core::approval::bytes_digest(
                &sealed
                    .original_record()
                    .wire_bytes()
                    .map_err(|_| denied())?,
            ),
        )
        .await
        .map_err(|_| denied())?;
    current_private().map_err(|_| denied())?;
    token
        .persist_sealed_put(
            &state,
            sealed.original_record(),
            sealed.reference(),
            &reply.physical_receipt,
        )
        .map_err(|_| denied())?;
    current_private().map_err(|_| denied())?;
    // Exactly one immutable PUT follows its own committed attempt. Unknown
    // reply cannot re-enter this handler, reseal, or acquire another PUT token.
    let storage = put(env, sealed.original_record())
        .await
        .map_err(|_| denied())?;
    let expected = sealed
        .original_record()
        .storage_fact()
        .map_err(|_| denied())?;
    if storage != expected {
        return Err(denied());
    }
    let public = ExecutionPreparedPublicResultV2 {
        api_version: 2,
        proof_digest: q.proof.digest().map_err(|_| denied())?,
        reference: sealed.reference().clone(),
        native_receipt: reply.physical_receipt,
        storage,
    };
    token.finish_put(&state, &public).map_err(|_| denied())?;
    Response::from_json(&public)
}
async fn put(env: &Env, record: &ContextBodyRecordV2) -> JResult<ContextBodyStorageFactV2> {
    let raw = record.wire_bytes().map_err(|_| js_denied())?;
    let stub = env
        .durable_object("CONTEXT_BODIES")
        .map_err(|_| js_denied())?
        .id_from_name(&record.scope.derived_actor_name().map_err(|_| js_denied())?)
        .map_err(|_| js_denied())?
        .get_stub()
        .map_err(|_| js_denied())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_redirect(RequestRedirect::Manual)
        .with_body(Some(
            String::from_utf8(raw).map_err(|_| js_denied())?.into(),
        ));
    let mut request = Request::new_with_init(
        "https://context-body.internal/internal/fullsigned-v2/put",
        &init,
    )
    .map_err(|_| js_denied())?;
    request
        .headers_mut()
        .map_err(|_| js_denied())?
        .set("Content-Type", "application/json")
        .map_err(|_| js_denied())?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| js_denied())?;
    if response.status_code() != 200 {
        return Err(js_denied());
    }
    let response: web_sys::Response = response.into();
    let raw = raw_text(bounded_body(response.as_ref(), MAX as u32).await?, MAX)?;
    serde_json::from_str(&raw).map_err(|_| js_denied())
}
pub(crate) async fn run(env: &Env, raw: &str) -> worker::Result<String> {
    // Old Native10 requests deliberately do not deserialize as this explicit
    // per-leaf mode. No receipt-less request can enter the new receiver.
    let frame: crate::execution_dispatch_rpc_v2::ExecutionDispatchFrameV2 =
        serde_json::from_str(raw).map_err(|_| denied())?;
    frame
        .check(
            env,
            ExecutionPreparePhysicalEdgeV2::AccountToIdentityService,
            clock()?,
        )
        .map_err(|_| denied())?;
    let q = frame.request.as_ref();
    check(env, q).map_err(|_| denied())?;
    let role = &q.proof.certificate.session.scope.role_scope;
    let name = manifest_name_parts(env, &role.context.identity.account_id, &role.provision_id)?;
    let body = oasis7_custody_core::approval::bytes_digest(&q.wire_bytes().map_err(|_| denied())?);
    let authorization = crate::identity_execution_dispatch_client_v2::claim(
        env,
        q,
        &frame.authorization,
        ExecutionPreparePhysicalEdgeV2::IdentityServiceToNativeActor,
        body,
    )
    .await
    .map_err(|_| denied())?;
    let next = crate::execution_dispatch_rpc_v2::ExecutionDispatchFrameV2 {
        api_version: 2,
        format: frame.format,
        request: Box::new(q.clone()),
        authorization,
    };
    let public = funded_actor(env, &name, &next)
        .await
        .map_err(|_| denied())?;
    if public.api_version != 2 || public.proof_digest != q.proof.digest().map_err(|_| denied())? {
        return Err(denied());
    }
    serde_json::to_string(&public).map_err(|_| denied())
}

async fn funded_actor(
    env: &Env,
    name: &str,
    frame: &crate::execution_dispatch_rpc_v2::ExecutionDispatchFrameV2,
) -> JResult<ExecutionPreparedPublicResultV2> {
    let raw =
        String::from_utf8(frame.wire_bytes().map_err(|_| js_denied())?).map_err(|_| js_denied())?;
    let stub = env
        .durable_object("NATIVE_SESSIONS")
        .map_err(|_| js_denied())?
        .id_from_name(name)
        .map_err(|_| js_denied())?
        .get_stub()
        .map_err(|_| js_denied())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_redirect(RequestRedirect::Manual)
        .with_body(Some(raw.into()));
    let mut request =
        Request::new_with_init(&format!("https://identity-native.internal{PATH}"), &init)
            .map_err(|_| js_denied())?;
    request
        .headers_mut()
        .map_err(|_| js_denied())?
        .set("Content-Type", "application/json")
        .map_err(|_| js_denied())?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| js_denied())?;
    if response.status_code() != 200 {
        return Err(js_denied());
    }
    let response: web_sys::Response = response.into();
    let raw = raw_text(bounded_body(response.as_ref(), 131072).await?, 131072)?;
    serde_json::from_str(&raw).map_err(|_| js_denied())
}
