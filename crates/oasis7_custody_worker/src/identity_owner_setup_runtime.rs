//! Fixed separately approved Owner setup driver. Secrets stay in this actor.
#[cfg(feature = "native-runtime-harness")]
#[path = "setup_diagnostic.rs"]
mod diagnostic;
#[path = "identity_owner_setup_private.rs"]
mod private;
#[path = "identity_owner_setup_transport.rs"]
mod transport;
use crate::native_identity::provision_transport::NativePrivateRoute;
use crate::{
    identity_native_runtime::{clock, js_denied, raw_text, read_fence},
    owner_setup_rpc::*,
};
use oasis7_client_api::world_service::{self as ws, SignedServiceResponse};
use oasis7_custody_core::provisioning::{
    OwnerSetupAccessEvidenceV1, OwnerSetupAdmissionEvidenceV1, OwnerSetupCanonicalFactV1,
    OwnerSetupCanonicalSourceV1, OwnerSetupOriginalV1, OwnerSetupResultV1,
    OwnerSetupWorldEvidenceV1,
};
use oasis7_custody_core::{
    DecimalU64,
    approval::bytes_digest,
    provisioning::{OwnerSetupPermitKindV1, OwnerSetupPermitViewV1, OwnerSetupReadPurposeV1},
    recovery::RecoveryFence,
};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use worker::Env;
type JResult<T> = std::result::Result<T, JsValue>;

#[wasm_bindgen(module = "/platform/owner_setup_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = ownerSetupIdentityCall)]
    async fn owner_call(env: &Env, raw: &str) -> JResult<JsValue>;
    #[wasm_bindgen(catch, js_name = ownerSetupSign)]
    async fn owner_sign(env: &Env, raw: &str) -> JResult<JsValue>;
    #[wasm_bindgen(catch, js_name = waitOwnerSetupCommit)]
    async fn wait_commit() -> JResult<JsValue>;
}

async fn rpc<T: DeserializeOwned>(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    action: OwnerSetupRuntimeAction,
) -> JResult<T> {
    let raw = zeroize::Zeroizing::new(
        serde_json::to_string(&OwnerSetupRuntimeRpc {
            request: selector.request.clone(),
            stage: selector.stage,
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
    view: OwnerSetupPermitViewV1,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    fn fingerprint(&self) -> JResult<String> {
        let v = &self.view;
        let bytes = zeroize::Zeroizing::new(
            oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.owner-setup-private-scope.v1",
                &v.setup_id,
                v.stage,
                &v.plan,
                &v.participant,
                &v.source_revision,
                &v.authority_revision,
                v.service_epoch,
                v.target_security_epoch,
                v.expires_at,
            ))
            .map_err(|_| js_denied())?,
        );
        Ok(bytes_digest(&bytes))
    }
}
fn check(env: &Env, selector: &OwnerSetupStageRpc, observation: &Observation) -> JResult<()> {
    let now = clock()?;
    let v = &observation.view;
    let scope = &v.participant.identity_scope;
    let configured_epoch: DecimalU64 = serde_json::from_value(
        env.var("CUSTODY_SERVICE_EPOCH")
            .map_err(|_| js_denied())?
            .to_string()
            .into(),
    )
    .map_err(|_| js_denied())?;
    let environment = env
        .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
        .map_err(|_| js_denied())?
        .to_string();
    if env
        .var("CUSTODY_OWNER_SETUP_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
        || v.setup_id != selector.request.setup_id
        || v.stage != selector.stage
        || v.plan.provision_id != selector.request.provision_id
        || scope.provision_id != selector.request.provision_id
        || scope.context.identity.account_id != selector.request.custody_account_id
        || scope.context.identity.environment != environment
        || scope.context.identity.role != oasis7_custody_core::http::Role::Identity
        || v.participant.session.scope.role_scope != *scope
        || v.participant.session.private_sid_generation.0 == 0
        || v.target_security_epoch.0 == 0
        || v.service_epoch != configured_epoch
        || now < v.observed_at.0
        || now - v.observed_at.0 > 5000
        || now >= v.expires_at.0
        || now < observation.fence_started
        || now - observation.fence_started > 5000
    {
        return Err(js_denied());
    }
    v.plan.validate().map_err(|_| js_denied())?;
    selector.request.validate().map_err(|_| js_denied())?;
    observation
        .fence
        .check_epoch(v.service_epoch.0)
        .map_err(|_| js_denied())?;
    crate::native_identity::provision_transport::NativeProvisionTransport::from_env(env)
        .map_err(|_| js_denied())?
        .validate_binding(&v.participant.native_binding)
        .map_err(|_| js_denied())?;
    Ok(())
}
async fn observe(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    purpose: OwnerSetupReadPurposeV1,
) -> JResult<Observation> {
    let view: OwnerSetupPermitViewV1 = rpc(
        env,
        selector,
        OwnerSetupRuntimeAction::GetPermit {
            purpose,
            access: None,
        },
    )
    .await?;
    if view.read_purpose != purpose || view.kind != OwnerSetupPermitKindV1::Session {
        return Err(js_denied());
    }
    let fence_started = clock()?;
    let fence =
        serde_json::from_str(&raw_text(read_fence(env).await?, 4096)?).map_err(|_| js_denied())?;
    let result = Observation {
        view,
        fence,
        fence_started,
    };
    check(env, selector, &result)?;
    Ok(result)
}

async fn returned(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    view: OwnerSetupPermitViewV1,
) -> JResult<Observation> {
    let fence_started = clock()?;
    let fence =
        serde_json::from_str(&raw_text(read_fence(env).await?, 4096)?).map_err(|_| js_denied())?;
    let observation = Observation {
        view,
        fence,
        fence_started,
    };
    check(env, selector, &observation)?;
    Ok(observation)
}

async fn refresh_access(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    view: &Observation,
    secrets: &private::SessionSecrets,
) -> JResult<OwnerSetupAccessEvidenceV1> {
    let nonces = &view.view.access_nonces;
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

async fn canonical(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    view: &Observation,
    secrets: &private::SessionSecrets,
    original: &OwnerSetupOriginalV1,
    #[cfg(feature = "native-runtime-harness")] diagnostic: &diagnostic::Trace,
) -> JResult<(
    OwnerSetupResultV1,
    oasis7_custody_core::provisioning::ParticipantSessionEvidenceV1,
)> {
    let report = |code: &'static str, error| {
        #[cfg(feature = "native-runtime-harness")]
        diagnostic.fail(code);
        #[cfg(not(feature = "native-runtime-harness"))]
        let _ = code;
        error
    };
    let OwnerSetupOriginalV1::Canonical { submit, lookup } = original else {
        return Err(js_denied());
    };
    let session = transport::session(
        env,
        selector,
        view,
        secrets,
        &view.view.session_observation_nonce,
    )
    .await
    .map_err(|e| report("submit.root_status", e))?;
    let response: JResult<SignedServiceResponse<ws::IntentResponse<serde_json::Value>>> =
        transport::original(
            env,
            selector,
            view,
            secrets,
            NativePrivateRoute::OwnerControl,
            &ws::entry_control::EntryOwnerControlRequestV1 {
                access: view.view.observation.clone().ok_or_else(js_denied)?,
                operation: ws::entry_control::EntryOwnerOperationV1::Submit((**submit).clone()),
            },
        )
        .await;
    if let Ok(response) = response {
        let _: OwnerSetupCanonicalFactV1 = rpc(
            env,
            selector,
            OwnerSetupRuntimeAction::ArchiveCanonical {
                fact: Box::new(OwnerSetupCanonicalFactV1 {
                    source: OwnerSetupCanonicalSourceV1::Submit,
                    response: response.clone(),
                }),
            },
        )
        .await
        .map_err(|e| report("archive.submit", e))?;
        check(env, selector, view).map_err(|e| report("submit.post_archive_current", e))?;
        if matches!(
            response.payload.outcome,
            ws::IntentOutcome::Committed { .. } | ws::IntentOutcome::Rejected { .. }
        ) {
            return Ok((
                OwnerSetupResultV1::Canonical {
                    response,
                    from_lookup: false,
                },
                session,
            ));
        }
    }
    check(env, selector, view).map_err(|e| report("lookup.pre_wait_current", e))?;
    wait_commit().await.map_err(|e| report("lookup.wait", e))?;
    let permit = rpc(env, selector, OwnerSetupRuntimeAction::ClaimLookup)
        .await
        .map_err(|e| report("lookup.claim_rpc", e))?;
    let lookup_view = returned(env, selector, permit)
        .await
        .map_err(|e| report("lookup.returned_fence", e))?;
    if lookup_view.view.kind != OwnerSetupPermitKindV1::Lookup
        || lookup_view.view.original.as_ref() != Some(original)
    {
        return Err(report("lookup.exact_original", js_denied()));
    }
    let lookup_session = transport::session(
        env,
        selector,
        &lookup_view,
        secrets,
        lookup_view
            .view
            .lookup_session_observation_nonce
            .as_deref()
            .ok_or_else(js_denied)?,
    )
    .await
    .map_err(|e| report("lookup.root_status", e))?;
    let response: SignedServiceResponse<ws::IntentResponse<serde_json::Value>> =
        transport::original(
            env,
            selector,
            &lookup_view,
            secrets,
            NativePrivateRoute::OwnerControl,
            &ws::entry_control::EntryOwnerControlRequestV1 {
                access: lookup_view.view.observation.clone().ok_or_else(js_denied)?,
                operation: ws::entry_control::EntryOwnerOperationV1::Lookup((**lookup).clone()),
            },
        )
        .await
        .map_err(|e| report("native.lookup", e))?;
    let _: OwnerSetupCanonicalFactV1 = rpc(
        env,
        selector,
        OwnerSetupRuntimeAction::ArchiveCanonical {
            fact: Box::new(OwnerSetupCanonicalFactV1 {
                source: OwnerSetupCanonicalSourceV1::Lookup,
                response: response.clone(),
            }),
        },
    )
    .await
    .map_err(|e| report("archive.lookup", e))?;
    check(env, selector, &lookup_view).map_err(|e| report("lookup.post_archive_current", e))?;
    Ok((
        OwnerSetupResultV1::Canonical {
            response,
            from_lookup: true,
        },
        lookup_session,
    ))
}

pub(super) async fn response(
    state: JsValue,
    env: &Env,
    raw: &str,
) -> worker::Result<worker::Response> {
    let selector: OwnerSetupStageRpc = serde_json::from_str(raw).map_err(|_| super::denied())?;
    let progress = execute(state, env, &selector)
        .await
        .map_err(|_| super::denied())?;
    worker::Response::from_json(&progress)
}

async fn execute(
    state: JsValue,
    env: &Env,
    selector: &OwnerSetupStageRpc,
) -> JResult<serde_json::Value> {
    #[cfg(feature = "native-runtime-harness")]
    let diagnostic = diagnostic::Trace::new("identity", selector.stage, None);
    let report = |code: &'static str, error| {
        #[cfg(feature = "native-runtime-harness")]
        diagnostic.fail(code);
        #[cfg(not(feature = "native-runtime-harness"))]
        let _ = code;
        error
    };
    let outcome = async {
        let first = observe(env, selector, OwnerSetupReadPurposeV1::Session)
            .await
            .map_err(|e| report("session.first", e))?;
        let (second, secrets) = private::read(state, env, selector, &first)
            .await
            .map_err(|e| report("session.private", e))?;
        let access = refresh_access(env, selector, &second, &secrets)
            .await
            .map_err(|e| report("access.pair", e))?;
        let raw = serde_json::to_string(&OwnerSetupSignRpc {
            selector: OwnerSetupStageRpc {
                request: selector.request.clone(),
                stage: selector.stage,
            },
            access,
        })
        .map_err(|_| js_denied())?;
        let published: OwnerSetupPublished = serde_json::from_str(&raw_text(
            owner_sign(env, &raw)
                .await
                .map_err(|e| report("owner.sign", e))?,
            128 * 1024,
        )?)
        .map_err(|_| js_denied())?;
        if published.permit.kind != OwnerSetupPermitKindV1::Preflight
            || published.permit.plan != second.view.plan
            || published.permit.participant != second.view.participant
            || published.permit.original.as_ref() != Some(&published.candidate.original)
            || published.permit.observation.as_ref() != Some(&published.candidate.observation)
        {
            return Err(report("owner.published_exact", js_denied()));
        }
        let preflight = returned(env, selector, published.permit)
            .await
            .map_err(|e| report("preflight.returned_fence", e))?;
        let session = transport::session(
            env,
            selector,
            &preflight,
            &secrets,
            &preflight.view.preflight_session_observation_nonce,
        )
        .await
        .map_err(|e| report("preflight.root_status", e))?;
        let request = published.candidate.observation.clone();
        let world: SignedServiceResponse<ws::participant_discovery::OwnerSetupObservationV1> =
            transport::original(
                env,
                selector,
                &preflight,
                &secrets,
                NativePrivateRoute::ParticipantOwnerView,
                &request,
            )
            .await
            .map_err(|e| report("native.preflight_world", e))?;
        check(env, selector, &preflight).map_err(|e| report("preflight.post_world_current", e))?;
        let effect = rpc(
            env,
            selector,
            OwnerSetupRuntimeAction::Admit {
                evidence: Box::new(OwnerSetupAdmissionEvidenceV1 {
                    world: OwnerSetupWorldEvidenceV1 {
                        request,
                        response: world.clone(),
                    },
                    session,
                }),
            },
        )
        .await
        .map_err(|e| report("effect.admit_rpc", e))?;
        let effect = returned(env, selector, effect)
            .await
            .map_err(|e| report("effect.returned_fence", e))?;
        if effect.view.kind != OwnerSetupPermitKindV1::Effect
            || effect.view.original.as_ref() != Some(&published.candidate.original)
        {
            return Err(js_denied());
        }
        let (result, session) = match &published.candidate.original {
            OwnerSetupOriginalV1::Canonical { .. } => {
                canonical(
                    env,
                    selector,
                    &effect,
                    &secrets,
                    &published.candidate.original,
                    #[cfg(feature = "native-runtime-harness")]
                    &diagnostic,
                )
                .await?
            }
            OwnerSetupOriginalV1::PrepareGoal(goal) => {
                let session = transport::session(
                    env,
                    selector,
                    &effect,
                    &secrets,
                    &effect.view.session_observation_nonce,
                )
                .await?;
                let result = transport::original(
                    env,
                    selector,
                    &effect,
                    &secrets,
                    NativePrivateRoute::OwnerControl,
                    &ws::entry_control::EntryOwnerControlRequestV1 {
                        access: effect.view.observation.clone().ok_or_else(js_denied)?,
                        operation: ws::entry_control::EntryOwnerOperationV1::PrepareGoal(
                            (**goal).clone(),
                        ),
                    },
                )
                .await?;
                check(env, selector, &effect).map_err(|e| report("effect.post_read_current", e))?;
                (OwnerSetupResultV1::PreparedGoal(result), session)
            }
            OwnerSetupOriginalV1::FinalObservation { owner, goal } => {
                let session = transport::session(
                    env,
                    selector,
                    &effect,
                    &secrets,
                    &effect.view.session_observation_nonce,
                )
                .await?;
                if owner.as_ref() != &published.candidate.observation {
                    return Err(js_denied());
                }
                let goal_result = transport::original(
                    env,
                    selector,
                    &effect,
                    &secrets,
                    NativePrivateRoute::OwnerControl,
                    &ws::entry_control::EntryOwnerControlRequestV1 {
                        access: effect.view.observation.clone().ok_or_else(js_denied)?,
                        operation: ws::entry_control::EntryOwnerOperationV1::ReadGoal(
                            (**goal).clone(),
                        ),
                    },
                )
                .await?;
                check(env, selector, &effect).map_err(|e| report("effect.post_read_current", e))?;
                (
                    OwnerSetupResultV1::FinalObservation {
                        owner: Box::new(world),
                        goal: Box::new(goal_result),
                    },
                    session,
                )
            }
        };
        rpc(
            env,
            selector,
            OwnerSetupRuntimeAction::RecordResult {
                result: Box::new(result),
                session: Box::new(session),
            },
        )
        .await
        .map_err(|e| report("result.record_rpc", e))
    }
    .await;
    outcome.map_err(|e| report("stage.execute", e))
}

pub(crate) async fn run(env: &Env, raw: &str) -> worker::Result<String> {
    if raw.len() > 4096 {
        return Err(super::denied());
    }
    let selector: OwnerSetupStageRpc = serde_json::from_str(raw).map_err(|_| super::denied())?;
    selector.request.validate().map_err(|_| super::denied())?;
    let name = super::manifest_name_parts(
        env,
        &selector.request.custody_account_id,
        &selector.request.provision_id,
    )?;
    let progress: serde_json::Value = super::actor(env, &name, "/internal/owner-setup", &selector)
        .await
        .map_err(|_| super::denied())?;
    Ok(serde_json::to_string(&progress)?)
}
