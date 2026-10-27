//! Fixed paid reconciliation: lookup and registered readonly observations only.
#[path = "identity_owner_setup_reconcile_private.rs"]
mod private;
#[path = "identity_owner_setup_reconcile_transport.rs"]
mod transport;
use crate::native_identity::provision_transport::NativePrivateRoute;
use crate::{
    identity_native_runtime::{clock, js_denied, raw_text, read_fence},
    owner_setup_reconcile_rpc::*,
};
use oasis7_client_api::world_service::{self as ws, SignedServiceResponse};
use oasis7_custody_core::{
    DecimalU64, approval::bytes_digest, provisioning::*, recovery::RecoveryFence,
};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use worker::Env;
type JResult<T> = std::result::Result<T, JsValue>;
#[wasm_bindgen(module = "/platform/owner_setup_reconcile_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=ownerSetupReconcileIdentityCall)]
    async fn owner_call(env: &Env, raw: &str) -> JResult<JsValue>;
    #[wasm_bindgen(catch,js_name=ownerSetupReconcileSign)]
    async fn owner_sign(env: &Env, raw: &str) -> JResult<JsValue>;
}
async fn rpc<T: DeserializeOwned>(
    env: &Env,
    selector: &ReconcileSelector,
    action: ReconcileAction,
) -> JResult<T> {
    let raw = zeroize::Zeroizing::new(
        serde_json::to_string(&ReconcileRuntimeRpc {
            selector: selector.clone(),
            action,
        })
        .map_err(|_| js_denied())?,
    );
    if raw.len() > 128 * 1024 {
        return Err(js_denied());
    }
    serde_json::from_str(&raw_text(owner_call(env, &raw).await?, 128 * 1024)?)
        .map_err(|_| js_denied())
}
struct Observation {
    view: OwnerSetupReconcilePermitV1,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    fn fingerprint(&self) -> JResult<String> {
        let v = &self.view;
        let raw = zeroize::Zeroizing::new(
            oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.owner-setup-reconcile-private-scope.v1",
                &v.reconcile_id,
                &v.attempt_id,
                &v.setup_id,
                &v.provision_id,
                v.stage,
                &v.participant,
                &v.observation,
                &v.original_lookup_bytes,
                &v.final_goal,
                &v.source_revision,
                &v.authority_revision,
                v.target_security_epoch,
                v.service_epoch,
                v.expires_at,
            ))
            .map_err(|_| js_denied())?,
        );
        Ok(bytes_digest(&raw))
    }
}
fn check(env: &Env, selector: &ReconcileSelector, o: &Observation) -> JResult<()> {
    selector.request.validate().map_err(|_| js_denied())?;
    let now = clock()?;
    let v = &o.view;
    let scope = &v.participant.identity_scope;
    let value = |key| env.var(key).map(|v| v.to_string()).map_err(|_| js_denied());
    let epoch: DecimalU64 =
        serde_json::from_value(value("CUSTODY_SERVICE_EPOCH")?.into()).map_err(|_| js_denied())?;
    if value("CUSTODY_OWNER_SETUP_ENABLED")? != "true"
        || v.reconcile_id != selector.request.reconcile_id
        || v.attempt_id != selector.request.attempt_id
        || v.provision_id != selector.provision_id
        || scope.provision_id != selector.provision_id
        || scope.context.identity.account_id != selector.custody_account_id
        || scope.context.identity.environment != value("CUSTODY_ROLE_KEY_ENVIRONMENT")?
        || scope.context.identity.role != oasis7_custody_core::http::Role::Identity
        || v.participant.session.scope.role_scope != *scope
        || v.participant.session.private_sid_generation.0 == 0
        || v.target_security_epoch.0 == 0
        || v.service_epoch != epoch
        || now < v.observed_at.0
        || now - v.observed_at.0 > 5000
        || now >= v.expires_at.0
        || now < o.fence_started
        || now - o.fence_started > 5000
    {
        return Err(js_denied());
    }
    o.fence
        .check_epoch(v.service_epoch.0)
        .map_err(|_| js_denied())?;
    crate::native_identity::provision_transport::NativeProvisionTransport::from_env(env)
        .map_err(|_| js_denied())?
        .validate_binding(&v.participant.native_binding)
        .map_err(|_| js_denied())?;
    Ok(())
}
async fn returned(
    env: &Env,
    selector: &ReconcileSelector,
    view: OwnerSetupReconcilePermitV1,
) -> JResult<Observation> {
    let fence_started = clock()?;
    let fence =
        serde_json::from_str(&raw_text(read_fence(env).await?, 4096)?).map_err(|_| js_denied())?;
    let o = Observation {
        view,
        fence,
        fence_started,
    };
    check(env, selector, &o)?;
    Ok(o)
}
async fn observe(
    env: &Env,
    selector: &ReconcileSelector,
    purpose: OwnerSetupReadPurposeV1,
) -> JResult<Observation> {
    let view: OwnerSetupReconcilePermitV1 = rpc(
        env,
        selector,
        ReconcileAction::GetPermit {
            purpose,
            access: None,
        },
    )
    .await?;
    if view.read_purpose != purpose || view.kind != OwnerSetupReconcilePermitKindV1::Session {
        return Err(js_denied());
    }
    returned(env, selector, view).await
}
async fn refresh_access(
    env: &Env,
    selector: &ReconcileSelector,
    view: &Observation,
    secrets: &private::SessionSecrets,
) -> JResult<OwnerSetupAccessEvidenceV1> {
    let nonces = &view.view.nonces;
    let policy_session =
        transport::session(env, selector, view, secrets, &nonces.policy_session).await?;
    let policy = transport::original(
        env,
        selector,
        view,
        secrets,
        NativePrivateRoute::ParticipantPolicyOriginal,
        &serde_json::json!({"observation_nonce":nonces.policy}),
    )
    .await?;
    check(env, selector, view)?;
    let authorize_session =
        transport::session(env, selector, view, secrets, &nonces.authorize_session).await?;
    #[derive(serde::Deserialize, serde::Serialize)]
    #[serde(deny_unknown_fields)]
    struct AdmissionReply {
        api_version: u8,
        ok: bool,
        phase: String,
        state: String,
        new_registration_enabled: bool,
        admission: ws::SignedReadRequest<oasis7_agent_api::game::GameParticipantAdmissionV1>,
        next_action: String,
    }
    let reply: AdmissionReply = transport::original(
        env,
        selector,
        view,
        secrets,
        NativePrivateRoute::ParticipantAuthorize,
        &view.view.participant_access.owner,
    )
    .await?;
    check(env, selector, view)?;
    // The native authorizer has a separate response shape, never a World receipt.
    if reply.api_version != 2
        || !reply.ok
        || reply.phase != "participant_authorized"
        || reply.state != "pending"
        || reply.next_action != "query_original_participant_request"
    {
        return Err(js_denied());
    }
    // Closed registration still permits existing-binding access; the signed
    // policy/admission and independent current OwnerControl decide this lane.
    let _ = reply.new_registration_enabled;
    let admission = reply.admission;
    Ok(OwnerSetupAccessEvidenceV1 {
        policy,
        policy_session,
        admission,
        authorize_session,
    })
}

async fn world(
    env: &Env,
    selector: &ReconcileSelector,
    view: &Observation,
    secrets: &private::SessionSecrets,
    request: &ws::participant_discovery::ParticipantOwnerViewRequestV1,
) -> JResult<OwnerSetupWorldEvidenceV1> {
    let response = transport::original(
        env,
        selector,
        view,
        secrets,
        NativePrivateRoute::ParticipantOwnerView,
        request,
    )
    .await?;
    check(env, selector, view)?;
    Ok(OwnerSetupWorldEvidenceV1 {
        request: request.clone(),
        response,
    })
}
async fn execute(
    state: JsValue,
    env: &Env,
    selector: &ReconcileSelector,
) -> JResult<serde_json::Value> {
    let first = observe(env, selector, OwnerSetupReadPurposeV1::Session).await?;
    let (second, secrets) = private::read(state, env, selector, &first).await?;
    let access = refresh_access(env, selector, &second, &secrets).await?;
    let raw = zeroize::Zeroizing::new(
        serde_json::to_string(&ReconcileSignRpc {
            selector: selector.clone(),
            access,
        })
        .map_err(|_| js_denied())?,
    );
    let published: ReconcilePublished =
        serde_json::from_str(&raw_text(owner_sign(env, &raw).await?, 128 * 1024)?)
            .map_err(|_| js_denied())?;
    if published.permit.kind != OwnerSetupReconcilePermitKindV1::Preflight
        || published.permit.participant != second.view.participant
        || published.permit.original_lookup_bytes != second.view.original_lookup_bytes
        || published.candidate.observation.read.request != published.permit.observation
        || published.candidate.goal.as_ref().map(|v| &v.request)
            != published.permit.final_goal.as_ref()
    {
        return Err(js_denied());
    }
    let preflight = returned(env, selector, published.permit).await?;
    let session = transport::session(
        env,
        selector,
        &preflight,
        &secrets,
        &preflight.view.preflight_session_nonce,
    )
    .await?;
    let before = world(
        env,
        selector,
        &preflight,
        &secrets,
        &published.candidate.observation,
    )
    .await?;
    let effect: OwnerSetupReconcilePermitV1 = rpc(
        env,
        selector,
        ReconcileAction::Admit {
            evidence: Box::new(OwnerSetupAdmissionEvidenceV1 {
                world: before,
                session,
            }),
        },
    )
    .await?;
    let effect = returned(env, selector, effect).await?;
    if effect.view.kind != OwnerSetupReconcilePermitKindV1::LookupReady
        || effect.view.original_lookup_bytes != preflight.view.original_lookup_bytes
    {
        return Err(js_denied());
    }
    let (canonical, lookup_session) = if let Some(original) = &effect.view.original_lookup_bytes {
        let lookup: OwnerSetupReconcilePermitV1 =
            rpc(env, selector, ReconcileAction::ClaimLookup).await?;
        let lookup = returned(env, selector, lookup).await?;
        if lookup.view.kind != OwnerSetupReconcilePermitKindV1::Lookup
            || lookup.view.original_lookup_bytes.as_deref() != Some(original.as_str())
        {
            return Err(js_denied());
        }
        let session = transport::session(
            env,
            selector,
            &lookup,
            &secrets,
            &lookup.view.lookup_session_nonce,
        )
        .await?;
        let response: SignedServiceResponse<ws::IntentResponse<serde_json::Value>> =
            transport::lookup(
                env,
                selector,
                &lookup,
                &secrets,
                &published.candidate.observation,
                original,
            )
            .await?;
        let fact = OwnerSetupCanonicalFactV1 {
            source: OwnerSetupCanonicalSourceV1::Lookup,
            response: response.clone(),
        };
        let _: OwnerSetupCanonicalFactV1 = rpc(
            env,
            selector,
            ReconcileAction::ArchiveFact {
                fact: Box::new(fact),
            },
        )
        .await?;
        // The original signed lookup fact survives even when this live check fails.
        check(env, selector, &lookup)?;
        (
            Some(OwnerSetupResultV1::Canonical {
                response,
                from_lookup: true,
            }),
            Some(session),
        )
    } else {
        (None, None)
    };
    if canonical.is_none()
        && (effect.view.stage != OwnerSetupStageV1::FinalObservation
            || published.candidate.goal.is_none())
    {
        return Err(js_denied());
    }
    let post: OwnerSetupReconcilePermitV1 =
        rpc(env, selector, ReconcileAction::ClaimResult).await?;
    let post = returned(env, selector, post).await?;
    if post.view.kind != OwnerSetupReconcilePermitKindV1::Poststate
        || post.view.original_lookup_bytes != effect.view.original_lookup_bytes
    {
        return Err(js_denied());
    }
    // This is a new callback-only current observation. Its independent Root
    // nonce cannot revive the lookup send grant or permit another lookup.
    let post_session = transport::session(
        env,
        selector,
        &post,
        &secrets,
        &post.view.poststate_session_nonce,
    )
    .await?;
    let final_goal = if let Some(goal) = &published.candidate.goal {
        let response = transport::original(
            env,
            selector,
            &post,
            &secrets,
            NativePrivateRoute::OwnerControl,
            &ws::entry_control::EntryOwnerControlRequestV1 {
                access: published.candidate.observation.clone(),
                operation: ws::entry_control::EntryOwnerOperationV1::ReadGoal(goal.clone()),
            },
        )
        .await?;
        check(env, selector, &post)?;
        Some(response)
    } else {
        None
    };
    let after = world(
        env,
        selector,
        &post,
        &secrets,
        &published.candidate.observation,
    )
    .await?;
    let result = match canonical {
        Some(result) => result,
        None => OwnerSetupResultV1::FinalObservation {
            owner: Box::new(after.response.clone()),
            goal: Box::new(final_goal.ok_or_else(js_denied)?),
        },
    };
    rpc(
        env,
        selector,
        ReconcileAction::RecordResult {
            completion: Box::new(ReconcileCompletion {
                result,
                session: post_session,
                lookup_session,
                world: after,
            }),
        },
    )
    .await
}
pub(super) async fn response(
    state: JsValue,
    env: &Env,
    raw: &str,
) -> worker::Result<worker::Response> {
    let selector: ReconcileSelector = serde_json::from_str(raw).map_err(|_| super::denied())?;
    let result = execute(state, env, &selector)
        .await
        .map_err(|_| super::denied())?;
    worker::Response::from_json(&result)
}
pub(crate) async fn run(env: &Env, raw: &str) -> worker::Result<String> {
    if raw.len() > 4096 {
        return Err(super::denied());
    }
    let selector: ReconcileSelector = serde_json::from_str(raw).map_err(|_| super::denied())?;
    selector.request.validate().map_err(|_| super::denied())?;
    let name =
        super::manifest_name_parts(env, &selector.custody_account_id, &selector.provision_id)?;
    let value: serde_json::Value =
        super::actor(env, &name, "/internal/owner-setup-reconcile", &selector)
            .await
            .map_err(|_| super::denied())?;
    Ok(serde_json::to_string(&value)?)
}
