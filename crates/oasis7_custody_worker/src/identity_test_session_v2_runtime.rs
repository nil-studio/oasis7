//! Once-only TestSession preflight. This actor retains all SID/CSRF plaintext.
#[path = "identity_test_session_v2_observation.rs"]
mod observation;
#[path = "identity_test_session_v2_private.rs"]
mod private;
#[path = "identity_test_session_v2_transport.rs"]
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
    action: TestSessionRuntimeActionV2,
) -> JResult<T> {
    let raw = zeroize::Zeroizing::new(
        serde_json::to_string(&TestSessionRuntimeRpcV2 {
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
        || env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        || selector.api_version != 2
        || selector.session_id.is_empty()
    {
        return Err(js_denied());
    }
    // No private manifest or KEK is read until the original source has completed.
    let d: TestSessionPreflightDescriptorV2 =
        rpc(env, selector, TestSessionRuntimeActionV2::DescribePreflight).await?;
    if d.session_id != selector.session_id {
        return Err(js_denied());
    }
    if d.cleanup_transport_policy == Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2)
        && env
            .var("CUSTODY_CLEANUP_MACHINE_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
    {
        return Err(js_denied());
    }
    let request: ApprovalVerifyRequestV1 = rpc(
        env,
        selector,
        TestSessionRuntimeActionV2::ClaimSource {
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
        TestSessionRuntimeActionV2::RecordSource {
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
    let world3 = d.cleanup_transport_policy
        == Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2);
    let preflight_world = if world3 {
        // This is a replacement of the original FreshAccess claim, not a probe.
        if env
            .var("CUSTODY_CLEANUP_MACHINE_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Err(js_denied());
        }
        let proof: oasis7_custody_core::native_cleanup_v2::SignedAccountPreflightAccessV2 = rpc(
            env,
            selector,
            TestSessionRuntimeActionV2::ClaimPreflightWorldAccess,
        )
        .await?;
        proof
            .certificate
            .validate_shape()
            .map_err(|_| js_denied())?;
        if *proof.certificate.descriptor != d {
            return Err(js_denied());
        }
        Some(proof)
    } else {
        let claimed: TestSessionPreflightDescriptorV2 = rpc(
            env,
            selector,
            TestSessionRuntimeActionV2::ClaimStep {
                step: SessionPreflightStepV1::FreshAccessPair,
            },
        )
        .await?;
        if claimed != d {
            return Err(js_denied());
        }
        None
    };
    let (access, receipts) =
        transport::access(env, selector, &second, &secrets, preflight_world.as_ref()).await?;
    let raw = zeroize::Zeroizing::new(
        serde_json::to_string(&TestSessionSignRpcV2 {
            request: selector.clone(),
            access,
            receipts: receipts.map(Box::new),
        })
        .map_err(|_| js_denied())?,
    );
    let published: TestSessionPublishedObservationV2 =
        serde_json::from_str(&raw_text(owner_sign(env, &raw).await?, 128 * 1024)?)
            .map_err(|_| js_denied())?;
    let current = observation::published(env, selector, &published, &second).await?;
    let claimed: TestSessionPreflightDescriptorV2 = rpc(
        env,
        selector,
        TestSessionRuntimeActionV2::ClaimStep {
            step: SessionPreflightStepV1::Lifetime,
        },
    )
    .await?;
    if claimed != d {
        return Err(js_denied());
    }
    let lifetime = transport::lifetime(env, selector, &current, &secrets).await?;
    if env
        .var("CUSTODY_CLEANUP_MACHINE_V2_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        == Some("true")
    {
        let original: oasis7_custody_core::native_cleanup_v2::NativeReadAndRegisterRequestV2 = rpc(
            env,
            selector,
            TestSessionRuntimeActionV2::RecordLifetimeAndClaimMachineRead {
                original: Box::new(lifetime),
            },
        )
        .await?;
        if original.owner_read != published.original
            || original.pending.session_id != selector.session_id
        {
            return Err(js_denied());
        }
        let reply =
            transport::read_and_register(env, selector, &current, &secrets, &original).await?;
        return rpc(
            env,
            selector,
            TestSessionRuntimeActionV2::ActivateRegisteredMachine {
                reply: Box::new(reply),
                private_sid_generation: d.session.private_sid_generation,
            },
        )
        .await;
    }
    let _: serde_json::Value = rpc(
        env,
        selector,
        TestSessionRuntimeActionV2::RecordLifetime {
            original: Box::new(lifetime),
        },
    )
    .await?;
    let claimed: TestSessionPreflightDescriptorV2 = rpc(
        env,
        selector,
        TestSessionRuntimeActionV2::ClaimStep {
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
        TestSessionRuntimeActionV2::Activate {
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
    if selector.api_version != 2
        || selector.session_id.is_empty()
        || env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
    {
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
