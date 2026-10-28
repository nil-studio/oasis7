//! Once-only TestSession preflight. This actor retains all SID/CSRF plaintext.
#[path = "identity_test_session_observation.rs"]
mod observation;
#[path = "identity_test_session_private.rs"]
mod private;
#[path = "identity_test_session_transport.rs"]
mod transport;
use crate::{
    identity_native_runtime::{js_denied, raw_text},
    owner_test_session_rpc::*,
};
use oasis7_custody_core::{
    approval::{ApprovalVerifyRequestV1, SignedApprovalVerificationV1},
    provisioning::*,
};
use observation::Observation;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use worker::Env;
type Selector = TestSessionSelectorV1;
type JResult<T> = Result<T, JsValue>;
#[wasm_bindgen(module = "/platform/test_session_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = ownerTestSessionCall)]
    async fn owner_call(env: &Env, raw: &str) -> JResult<JsValue>;
    #[wasm_bindgen(catch, js_name = ownerTestSessionSign)]
    async fn owner_sign(env: &Env, raw: &str) -> JResult<JsValue>;
}
async fn rpc<T: DeserializeOwned>(
    env: &Env,
    selector: &Selector,
    action: TestSessionRuntimeAction,
) -> JResult<T> {
    let raw = zeroize::Zeroizing::new(
        serde_json::to_string(&TestSessionRuntimeRpc {
            request: selector.clone(),
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
fn check(env: &Env, selector: &Selector, o: &Observation) -> JResult<()> {
    o.check(env, selector)
}

pub(super) async fn response(
    state: JsValue,
    env: &Env,
    raw: &str,
) -> worker::Result<worker::Response> {
    let selector: Selector = serde_json::from_str(raw).map_err(|_| super::denied())?;
    let result = execute(state, env, &selector)
        .await
        .map_err(|_| super::denied())?;
    worker::Response::from_json(&result)
}
async fn execute(state: JsValue, env: &Env, selector: &Selector) -> JResult<serde_json::Value> {
    if env
        .var("CUSTODY_TEST_SESSION_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
        || selector.api_version != 1
        || selector.session_id.is_empty()
    {
        return Err(js_denied());
    }
    // No private manifest or KEK is read until the original source has completed.
    let d: TestSessionPreflightDescriptorV1 =
        rpc(env, selector, TestSessionRuntimeAction::DescribePreflight).await?;
    if d.session_id != selector.session_id {
        return Err(js_denied());
    }
    let request: ApprovalVerifyRequestV1 = rpc(
        env,
        selector,
        TestSessionRuntimeAction::ClaimSource {
            operation_id: d.source_operation_id.clone(),
        },
    )
    .await?;
    let original: SignedApprovalVerificationV1 =
        crate::native_identity::NativeIdentityConfig::from_env(env)
            .map_err(|_| js_denied())?
            .ok_or_else(js_denied)?
            .verify_approval(&request)
            .await
            .map_err(|_| js_denied())?;
    let _: () = rpc(
        env,
        selector,
        TestSessionRuntimeAction::RecordSource {
            operation_id: d.source_operation_id.clone(),
            original: Box::new(original),
        },
    )
    .await?;
    let first = observation::observe(env, selector, 1).await?;
    if first.descriptor != d {
        return Err(js_denied());
    }
    let (second, secrets) = private::read(state, env, selector, &first).await?;
    let claimed: TestSessionPreflightDescriptorV1 = rpc(
        env,
        selector,
        TestSessionRuntimeAction::ClaimStep {
            step: SessionPreflightStepV1::FreshAccessPair,
        },
    )
    .await?;
    if claimed != d {
        return Err(js_denied());
    }
    let access = transport::access(env, selector, &second, &secrets).await?;
    let raw = zeroize::Zeroizing::new(
        serde_json::to_string(&TestSessionSignRpc {
            request: selector.clone(),
            access,
        })
        .map_err(|_| js_denied())?,
    );
    let published: TestSessionPublishedObservation =
        serde_json::from_str(&raw_text(owner_sign(env, &raw).await?, 128 * 1024)?)
            .map_err(|_| js_denied())?;
    let current = observation::published(env, selector, &published, &second).await?;
    let claimed: TestSessionPreflightDescriptorV1 = rpc(
        env,
        selector,
        TestSessionRuntimeAction::ClaimStep {
            step: SessionPreflightStepV1::Lifetime,
        },
    )
    .await?;
    if claimed != d {
        return Err(js_denied());
    }
    let lifetime = transport::lifetime(env, selector, &current, &secrets).await?;
    let _: serde_json::Value = rpc(
        env,
        selector,
        TestSessionRuntimeAction::RecordLifetime {
            original: Box::new(lifetime),
        },
    )
    .await?;
    let claimed: TestSessionPreflightDescriptorV1 = rpc(
        env,
        selector,
        TestSessionRuntimeAction::ClaimStep {
            step: SessionPreflightStepV1::OwnerRead,
        },
    )
    .await?;
    if claimed != d {
        return Err(js_denied());
    }
    let response = transport::world(env, selector, &current, &secrets, &published.original).await?;
    check(env, selector, &current)?;
    rpc(
        env,
        selector,
        TestSessionRuntimeAction::Activate {
            world: Box::new(OwnerSetupWorldEvidenceV1 {
                request: published.original,
                response,
            }),
            private_sid_generation: d.session.private_sid_generation,
        },
    )
    .await
}

pub(crate) async fn run(env: &Env, raw: &str) -> worker::Result<String> {
    if raw.len() > 4096 {
        return Err(super::denied());
    }
    let selector: Selector = serde_json::from_str(raw).map_err(|_| super::denied())?;
    if selector.api_version != 1 || selector.session_id.is_empty() {
        return Err(super::denied());
    }
    let name =
        super::manifest_name_parts(env, &selector.custody_account_id, &selector.provision_id)?;
    let result: serde_json::Value =
        super::actor(env, &name, "/internal/test-session-preflight", &selector)
            .await
            .map_err(|_| super::denied())?;
    Ok(serde_json::to_string(&result)?)
}
