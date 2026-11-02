//! Fixed service -> Actor Prepare operations. No arbitrary signer/URL/Current.
#[cfg(feature = "owner")]
use oasis7_custody_core::provisioning::{
    ExecutionPrepareDispatchV2, ExecutionPreparePhysicalAttemptV2,
};
use oasis7_custody_core::provisioning::{
    ExecutionPrepareRkvHopV2, ExecutionPrepareSignedOriginalV2, ExecutionPrepareSignerOperationV2,
    ExecutionSigningBundleV2, SignedAccountExecutionPreparePermitV2,
    execution_prepare_physical_port_id_v2,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use worker::*;
const MAX_BYTES: usize = 32768;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    proof: SignedAccountExecutionPreparePermitV2,
    bundle: ExecutionSigningBundleV2,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    api_version: u8,
    operation: ExecutionPrepareSignerOperationV2,
    proof_digest: String,
    port_ids: [String; 2],
    #[serde(deserialize_with = "required_original")]
    original: Option<Box<ExecutionPrepareSignedOriginalV2>>,
}
fn required_original<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<Box<ExecutionPrepareSignedOriginalV2>>, D::Error> {
    Option::<Box<ExecutionPrepareSignedOriginalV2>>::deserialize(d)
}
fn denied() -> JsValue {
    JsValue::from_str("execution Prepare RPC rejected")
}
#[cfg(feature = "executor")]
fn clock() -> std::result::Result<u64, JsValue> {
    let n = js_sys::Date::now();
    if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n > 9_007_199_254_740_991.0 {
        return Err(denied());
    }
    Ok(n as u64)
}
#[cfg(feature = "executor")]
fn decode(raw: &str) -> std::result::Result<Wire, JsValue> {
    if raw.len() > MAX_BYTES {
        return Err(denied());
    }
    serde_json::from_str(raw).map_err(|_| denied())
}
fn ports(
    proof: &SignedAccountExecutionPreparePermitV2,
) -> std::result::Result<[String; 2], JsValue> {
    Ok([
        execution_prepare_physical_port_id_v2(
            proof,
            ExecutionPrepareRkvHopV2::AccountToExecutorService,
        )
        .map_err(|_| denied())?,
        execution_prepare_physical_port_id_v2(
            proof,
            ExecutionPrepareRkvHopV2::ExecutorServiceToActor,
        )
        .map_err(|_| denied())?,
    ])
}
fn proof_digest(
    proof: &SignedAccountExecutionPreparePermitV2,
) -> std::result::Result<String, JsValue> {
    Ok(oasis7_custody_core::approval::bytes_digest(
        &oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.execution-signer.permit.v2",
            proof,
        ))
        .map_err(|_| denied())?,
    ))
}
fn check_reply(wire: &Wire, raw: &str) -> std::result::Result<Reply, JsValue> {
    if raw.len() > MAX_BYTES {
        return Err(denied());
    }
    let r: Reply = serde_json::from_str(raw).map_err(|_| denied())?;
    if r.api_version != 2
        || r.operation != wire.proof.permit.operation
        || r.proof_digest != proof_digest(&wire.proof)?
        || r.port_ids != ports(&wire.proof)?
    {
        return Err(denied());
    }
    match (r.operation, &r.original) {
        (ExecutionPrepareSignerOperationV2::InitializeSigner, None) => (),
        (ExecutionPrepareSignerOperationV2::SignPrepare, Some(original)) => wire
            .bundle
            .verify_prepare_original(original)
            .map_err(|_| denied())?,
        _ => return Err(denied()),
    }
    Ok(r)
}
#[cfg(feature = "executor")]
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readBoundedText)]
    async fn bounded(body: &JsValue, limit: u32) -> std::result::Result<JsValue, JsValue>;
}
#[cfg(feature = "executor")]
async fn response_text(response: Response) -> std::result::Result<String, JsValue> {
    if response.status_code() != 200 {
        return Err(denied());
    }
    let value: web_sys::Response = response.into();
    bounded(value.as_ref(), MAX_BYTES as u32)
        .await?
        .as_string()
        .filter(|s| s.len() <= MAX_BYTES)
        .ok_or_else(denied)
}
#[cfg(feature = "executor")]
fn verify(
    env: &Env,
    wire: &Wire,
    at: u64,
) -> std::result::Result<oasis7_custody_core::provisioning::VerifiedExecutionPreparePermitV2, JsValue>
{
    crate::role_key_store::ExecutionIssuerConfigurationV2::load(env)?.verify(
        wire.proof.clone(),
        wire.bundle.clone(),
        oasis7_custody_core::provisioning::ExecutionPrepareRecipientV2 {
            scope: &wire.proof.permit.recipient_scope,
            key: &wire.proof.permit.recipient_key,
            operation: wire.proof.permit.operation,
        },
        at,
    )
}
#[cfg(feature = "executor")]
async fn gateway_at(env: &Env, raw: &str, at: u64) -> std::result::Result<String, JsValue> {
    let wire = decode(raw)?;
    let verified = verify(env, &wire, at)?;
    // Fixed configured Account pin verification happens before namespace access.
    let name = verified
        .proof()
        .permit
        .recipient_scope
        .task_digest()
        .map_err(|_| denied())?;
    let stub = env
        .durable_object("ROLE_KEYS")
        .map_err(|_| denied())?
        .id_from_name(&name)
        .map_err(|_| denied())?
        .get_stub()
        .map_err(|_| denied())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_body(Some(raw.into()))
        .with_redirect(RequestRedirect::Manual);
    let mut request = Request::new_with_init(
        &format!(
            "https://role-key.internal{}",
            wire.proof.permit.recipient_route
        ),
        &init,
    )
    .map_err(|_| denied())?;
    request
        .headers_mut()
        .map_err(|_| denied())?
        .set("Content-Type", "application/json")
        .map_err(|_| denied())?;
    let raw = response_text(
        stub.fetch_with_request(request)
            .await
            .map_err(|_| denied())?,
    )
    .await?;
    check_reply(&wire, &raw)?;
    Ok(raw)
}
#[cfg(feature = "executor")]
pub(crate) async fn gateway(env: &Env, raw: &str) -> std::result::Result<String, JsValue> {
    gateway_at(env, raw, clock()?).await
}
#[cfg(all(feature = "executor", feature = "execution-signer-harness"))]
pub(crate) async fn historical_gateway(
    env: &Env,
    raw: &str,
) -> std::result::Result<String, JsValue> {
    gateway_at(env, raw, 14_008).await
}
#[cfg(feature = "executor")]
async fn receive_at(
    state: JsValue,
    env: &Env,
    raw: &str,
    path: &str,
    at: u64,
    historical: bool,
) -> std::result::Result<String, JsValue> {
    let wire = decode(raw)?;
    if path != wire.proof.permit.recipient_route {
        return Err(denied());
    }
    let verified = verify(env, &wire, at)?;
    let store = crate::role_key_store::RoleKeyStore::new(state);
    let original = match wire.proof.permit.operation {
        ExecutionPrepareSignerOperationV2::InitializeSigner => {
            #[cfg(feature = "execution-signer-harness")]
            if historical {
                store.initialize_execution_historical_harness_v2(verified, at)?;
            } else {
                store.initialize_execution_signer_v2(verified)?;
            }
            #[cfg(not(feature = "execution-signer-harness"))]
            {
                let _ = historical;
                store.initialize_execution_signer_v2(verified)?;
            }
            None
        }
        ExecutionPrepareSignerOperationV2::SignPrepare => {
            #[cfg(feature = "execution-signer-harness")]
            let output = if historical {
                let scope = wire.proof.permit.recipient_scope.clone();
                let current = || {
                    crate::role_key_store::execution_prepare_sealed_current_v2(
                        env,
                        &wire.proof,
                        &wire.bundle,
                        at,
                    )
                };
                store
                    .execution_prepare_historical_harness_v2(
                        crate::role_key_store::ExecutionPrepareKeyV2 {
                            scope: &scope,
                            wrapping_version: oasis7_custody_core::DecimalU64(1),
                            kek: &[0x93; 32],
                        },
                        verified,
                        current,
                        || Ok(at),
                    )
                    .await?
            } else {
                store
                    .sign_execution_prepare_sealed_v2(env, verified)
                    .await?
            };
            #[cfg(not(feature = "execution-signer-harness"))]
            let output = store
                .sign_execution_prepare_sealed_v2(env, verified)
                .await?;
            Some(Box::new(output))
        }
    };
    let reply = Reply {
        api_version: 2,
        operation: wire.proof.permit.operation,
        proof_digest: proof_digest(&wire.proof)?,
        port_ids: ports(&wire.proof)?,
        original,
    };
    let encoded = serde_json::to_string(&reply).map_err(|_| denied())?;
    check_reply(&wire, &encoded)?;
    Ok(encoded)
}
#[cfg(feature = "executor")]
pub(crate) async fn receive(
    state: JsValue,
    env: &Env,
    raw: &str,
    path: &str,
) -> std::result::Result<String, JsValue> {
    receive_at(state, env, raw, path, clock()?, false).await
}
#[cfg(all(feature = "executor", feature = "execution-signer-harness"))]
pub(crate) async fn historical_receive(
    state: JsValue,
    env: &Env,
    raw: &str,
    path: &str,
) -> std::result::Result<String, JsValue> {
    receive_at(state, env, raw, path, 14_009, true).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen(module = "/platform/execution_prepare_rpc_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=executionPrepareV2)]
    async fn send(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
#[cfg(feature = "owner")]
async fn dispatch_one(
    env: &Env,
    attempt: ExecutionPreparePhysicalAttemptV2,
) -> std::result::Result<Option<Box<ExecutionPrepareSignedOriginalV2>>, JsValue> {
    let (paid, proof, bundle) = attempt.into_parts();
    if paid != ports(&proof)? {
        return Err(denied());
    }
    let wire = Wire { proof, bundle };
    let raw = serde_json::to_string(&wire).map_err(|_| denied())?;
    if raw.len() > MAX_BYTES {
        return Err(denied());
    }
    // No retry, lookup or implicit initialization. The two operation leaves
    // were committed together before this service RPC and its Actor forwarding.
    let raw = send(env, &raw)
        .await?
        .as_string()
        .filter(|s| s.len() <= MAX_BYTES)
        .ok_or_else(denied)?;
    Ok(check_reply(&wire, &raw)?.original)
}
#[cfg(feature = "owner")]
pub(crate) async fn dispatch<F, Fut>(
    env: &Env,
    paid: ExecutionPrepareDispatchV2,
    before_send: F,
) -> std::result::Result<ExecutionPrepareSignedOriginalV2, JsValue>
where
    F: Fn() -> Fut,
    Fut: core::future::Future<Output = std::result::Result<(), JsValue>>,
{
    let [initialize, sign] = paid.into_attempts();
    before_send().await?;
    if dispatch_one(env, initialize).await?.is_some() {
        return Err(denied());
    }
    before_send().await?;
    dispatch_one(env, sign)
        .await?
        .map(|v| *v)
        .ok_or_else(denied)
}

#[cfg(feature = "executor")]
#[wasm_bindgen]
pub async fn execution_prepare_role_v2(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    gateway(&env, &raw).await
}
#[cfg(all(feature = "executor", feature = "execution-signer-harness"))]
#[wasm_bindgen]
pub async fn execution_prepare_role_historical_fixture_v2(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    historical_gateway(&env, &raw).await
}
