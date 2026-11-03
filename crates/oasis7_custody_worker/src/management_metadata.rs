//! One bounded metadata query: provider, realm, Native, Owner RPC, directory DO.
use crate::{
    directory_rpc::{ParentsQueryRpc, request_text},
    management_common::{denied, digest},
    management_metadata_policy::{BINDING, Config},
    native_identity::NativeIdentityConfig,
};
use oasis7_custody_core::management_metadata::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use worker::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AbuseRequest {
    pub bucket: String,
    pub provider: bool,
}
#[wasm_bindgen(module = "/platform/directory_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=queryOwnerParents)]
    async fn query_owner(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
pub(crate) async fn handle(request: Request, env: Env) -> Result<Response> {
    let config = Config::enabled(&env)?;
    if request.method() != Method::Post
        || request.url()?.as_str() != format!("{}{PARENTS_QUERY_PATH}", config.origin)
        || request.headers().get("Content-Type")?.as_deref() != Some("application/json")
    {
        return Response::error("not found", 404);
    }
    // Validate closed bounded syntax before any service call; this cannot authorize.
    let query: ManagementParentsQueryV1 =
        serde_json::from_str(&request_text(&request).await?).map_err(|_| denied())?;
    query.validate().map_err(|_| denied())?;
    precharge(&env, &request, &config).await?;
    let native = NativeIdentityConfig::from_env(&env)?.ok_or_else(denied)?;
    let session = native.management_session(&request).await?;
    let raw = serde_json::to_string(&ParentsQueryRpc {
        manager_session: session,
        request: query.clone(),
    })?;
    let raw = query_owner(&env, &raw)
        .await
        .map_err(|_| denied())?
        .as_string()
        .ok_or_else(denied)?;
    if raw.len() > MAX_PAGE_BYTES {
        return Err(denied());
    }
    let page: ManagementParentsPageV1 = serde_json::from_str(&raw).map_err(|_| denied())?;
    page.validate(&query).map_err(|_| denied())?;
    let mut response = Response::ok(raw)?;
    response
        .headers_mut()
        .set("Content-Type", "application/json")?;
    response.headers_mut().set("Cache-Control", "no-store")?;
    Ok(response)
}
async fn precharge(env: &Env, request: &Request, c: &Config) -> Result<()> {
    if request.cf().is_none() {
        return Err(denied());
    }
    let ip: std::net::IpAddr = request
        .headers()
        .get("CF-Connecting-IP")?
        .ok_or_else(denied)?
        .parse()
        .map_err(|_| denied())?;
    for (provider, bucket) in [
        (
            true,
            digest(
                "oasis7.custody.management-metadata-provider.v1",
                &(&c.realm, &c.origin, ip.to_string()),
            )?,
        ),
        (
            false,
            digest(
                "oasis7.custody.management-metadata-realm.v1",
                &(&c.realm, &c.origin),
            )?,
        ),
    ] {
        let stub = env
            .durable_object(BINDING)?
            .id_from_name(&c.actor_name(&bucket)?)?
            .get_stub()?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post).with_body(Some(
            serde_json::to_string(&AbuseRequest { bucket, provider })?.into(),
        ));
        let response = stub
            .fetch_with_request(Request::new_with_init(
                "https://enrollment.internal/internal/management-metadata-abuse",
                &init,
            )?)
            .await?;
        if response.status_code() != 200 {
            return Err(denied());
        }
        // The fixed actor returns an empty body; no unbounded response decoding.
    }
    Ok(())
}
