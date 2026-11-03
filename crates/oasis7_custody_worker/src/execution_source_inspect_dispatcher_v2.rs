//! Dedicated Identity-owned first receiver. No public Actions handler is mounted.
use super::{NativeIdentityConfig, checked, clock, rejected};
use oasis7_custody_core::{
    DecimalU64, execution_pending_source_inspect_v2::*, execution_pending_source_result_v2::*,
};
use wasm_bindgen::{JsCast, prelude::*};
use worker::*;
#[path = "execution_source_inspect_config_v2.rs"]
mod config;
#[path = "execution_source_inspect_store_v2.rs"]
mod store;
fn js_rejected() -> JsValue {
    JsValue::from_str("source inspection unavailable")
}
fn strings(values: Vec<String>) -> JsValue {
    let a = js_sys::Array::new();
    for s in values {
        a.push(&JsValue::from_str(&s));
    }
    a.into()
}
#[wasm_bindgen(module = "/platform/execution_source_inspect_sql_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=claimPendingSourceInspect)]
    fn claim_sql(
        state: &JsValue,
        capacity: f64,
        transition: &js_sys::Function,
    ) -> core::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=authorizePendingSourceInspectNative)]
    fn authorize_sql(
        state: &JsValue,
        capacity: f64,
        transition: &js_sys::Function,
    ) -> core::result::Result<JsValue, JsValue>;
}
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readBoundedText)]
    async fn bounded_body(request: &JsValue, limit: u32) -> core::result::Result<JsValue, JsValue>;
}
#[wasm_bindgen(module = "/platform/execution_source_inspect_transport_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=claimPendingSourceNativeLeaf)]
    async fn account_leaf(
        env: &JsValue,
        body: &str,
        until: f64,
    ) -> core::result::Result<JsValue, JsValue>;
}
#[durable_object]
pub struct ExecutionSourceInspectDispatcher {
    state: JsValue,
    object_id: String,
    env: Env,
}
impl DurableObject for ExecutionSourceInspectDispatcher {
    fn new(state: State, env: Env) -> Self {
        Self {
            object_id: state.id().to_string(),
            state: state._inner().into(),
            env,
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        if request.method() != Method::Post
            || url.scheme() != "https"
            || url.host_str() != Some("source-inspect.internal")
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != SOURCE_INSPECT_DISPATCH_PATH_V2
        {
            return Response::error("not found", 404);
        }
        if request.headers().get("Content-Type")?.as_deref() != Some("application/json") {
            return Response::error("source inspection unavailable", 409);
        }
        let raw = bounded_body(
            request.inner().as_ref(),
            SOURCE_INSPECT_MAX_WIRE_BYTES_V2 as u32,
        )
        .await
        .map_err(|_| rejected())?
        .as_string()
        .ok_or_else(rejected)?;
        match self.run(raw.as_bytes()).await {
            Ok(raw) => {
                let mut response = Response::from_bytes(raw)?;
                response
                    .headers_mut()
                    .set("Content-Type", "application/json")?;
                Ok(response)
            }
            Err(_) => Response::error("source inspection unavailable", 409),
        }
    }
}
impl ExecutionSourceInspectDispatcher {
    async fn run(&self, raw: &[u8]) -> Result<Vec<u8>> {
        let native = NativeIdentityConfig::from_env(&self.env)?.ok_or_else(rejected)?;
        let server = config::Server::load(&self.env, &native)?;
        let request = PendingSourceInspectDispatchRequestV2::decode(raw).map_err(|_| rejected())?;
        let name = source_inspect_receiver_name_v2(&server.issuer()).map_err(|_| rejected())?;
        let actual = self
            .env
            .durable_object("EXECUTION_SOURCE_INSPECT_DISPATCHERS")?
            .id_from_name(&name)?
            .to_string();
        checked(actual == self.object_id && request.proof.binding.dispatcher_object_id == actual)?;
        let at = clock()?;
        request
            .proof
            .verify(&server.dispatch_pin, &server.floor, &self.object_id, at)
            .map_err(|_| rejected())?;
        request
            .verify_native_approval(&native.key_id, &native.public_key)
            .map_err(|_| rejected())?;
        let captured_server = server.clone();
        let captured_object = self.object_id.clone();
        let captured_request = request.clone();
        let transition =
            Closure::<dyn FnMut(String) -> core::result::Result<JsValue, JsValue>>::new(
                move |raw: String| {
                    store::claim_transition(
                        &captured_server,
                        &captured_object,
                        &captured_request,
                        at,
                        &raw,
                    )
                    .map(strings)
                    .map_err(|_| js_rejected())
                },
            );
        let committed = claim_sql(
            &self.state,
            f64::from(server.capacity),
            transition.as_ref().unchecked_ref(),
        )
        .map_err(|_| rejected())?
        .as_string()
        .ok_or_else(rejected)?;
        drop(transition);
        // The first token is constructed only after the real SQLite commit returns.
        let first = store::first_after_commit(
            server.clone(),
            self.object_id.clone(),
            request,
            at,
            &committed,
        )?;
        let until = first.request().proof.binding.expires_at.0;
        checked(clock()? < until)?;
        let gate_body = serde_json::to_string(&first.request().proof).map_err(|_| rejected())?;
        let raw = account_leaf(self.env.as_ref(), &gate_body, until as f64)
            .await
            .map_err(|_| rejected())?
            .as_string()
            .ok_or_else(rejected)?;
        let leaf =
            SignedPendingSourceNativeLeafV2::decode(raw.as_bytes()).map_err(|_| rejected())?;
        let authorize_at = clock()?;
        checked(authorize_at < until && leaf.claim.dispatch == first.request().proof)?;
        leaf.verify(
            &server.leaf_pin,
            &server.floor,
            &native.origin,
            authorize_at,
        )
        .map_err(|_| rejected())?;
        // Re-load the actual server configuration and refuse any local drift.
        checked(config::Server::load(&self.env, &native)? == server)?;
        let compare = store::native_comparison(&first, leaf.clone(), authorize_at);
        let transition =
            Closure::<dyn FnMut(String) -> core::result::Result<JsValue, JsValue>>::new(
                move |raw: String| compare(&raw).map(strings).map_err(|_| js_rejected()),
            );
        let committed = authorize_sql(
            &self.state,
            f64::from(server.capacity),
            transition.as_ref().unchecked_ref(),
        )
        .map_err(|_| rejected())?
        .as_string()
        .ok_or_else(rejected)?;
        drop(transition);
        let authorized = store::native_after_commit(first, leaf, authorize_at, &committed)?;
        send_native_once(&native, authorized).await
    }
}
async fn send_native_once(
    native: &NativeIdentityConfig,
    authorized: store::Authorized,
) -> Result<Vec<u8>> {
    let q = authorized.request();
    let now = clock()?;
    let until = q.leaf.claim.expires_at.0;
    checked(now < until)?;
    let timeout = (until - now).min(5000);
    let raw = q.wire_bytes().map_err(|_| rejected())?;
    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    headers.set(
        "Authorization",
        &format!("Bearer {}", native.adapter_secret),
    )?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_redirect(RequestRedirect::Manual)
        .with_body(Some(JsValue::from_str(
            std::str::from_utf8(&raw).map_err(|_| rejected())?,
        )));
    let request = Request::new_with_init(
        &format!("{}{}", native.origin, SOURCE_NATIVE_PATH_V2),
        &init,
    )?;
    // One actual fetch, no retry and no ordinary verify fallback.
    let response = crate::native_http_bounded::send_status(request, false, timeout).await?;
    let raw =
        crate::native_http_bounded::read_body_bytes(response, SOURCE_INSPECT_MAX_WIRE_BYTES_V2)
            .await?;
    let result = SignedPendingSourceInspectResultV2::decode(&raw).map_err(|_| rejected())?;
    result
        .verify_for(q, &native.key_id, &native.public_key)
        .map_err(|_| rejected())?;
    checked(clock()? < until && result.result.completed_at.0 <= clock()?)?;
    Ok(raw.to_vec())
}
