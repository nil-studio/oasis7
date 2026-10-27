//! Only the fixed public pairing ports and private namespace reader reach these actors.
use crate::client_enrollment_authority::*;
pub(crate) use crate::client_enrollment_reader::{FactView, ReadFact, read_proved};
use crate::client_enrollment_store::{
    self as store, AbuseKind, AbuseRequest, AbuseScope, HeaderPair, ProveInput,
};
use oasis7_custody_core::{
    client_enrollment::*,
    http_pop::{CanonicalHttpTarget, HttpPopRoute},
};
use wasm_bindgen::prelude::*;
use worker::*;
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readBoundedText)]
    async fn bounded(body: &JsValue, limit: u32) -> std::result::Result<JsValue, JsValue>;
}
async fn text(r: &Request, max: u32) -> std::result::Result<String, JsValue> {
    bounded(r.inner().as_ref(), max)
        .await?
        .as_string()
        .ok_or_else(denied)
}
#[durable_object]
pub struct ClientEnrollmentAuthority {
    state: JsValue,
    env: Env,
    actor_id: String,
    realm: String,
}
impl DurableObject for ClientEnrollmentAuthority {
    fn new(state: State, env: Env) -> Self {
        let actor_id = state.id().to_string();
        let realm = env
            .var("CUSTODY_ENROLLMENT_REALM")
            .map(|v| v.to_string())
            .unwrap_or_default();
        Self {
            state: state._inner().into(),
            env,
            actor_id,
            realm,
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        if request.method() != Method::Post
            || url.scheme() != "https"
            || url.host_str() != Some("enrollment.internal")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Response::error("not found", 404);
        }
        let raw = match text(&request, 128 * 1024).await {
            Ok(v) => v,
            Err(_) => return Response::error("invalid enrollment request", 400),
        };
        let value = match url.path() {
            "/internal/create" => {
                store::create(&self.state, &self.env, &self.actor_id, &self.realm, &raw).and_then(
                    |s| {
                        let r = s.registration();
                        serde_json::to_string(&ClientEnrollmentCreateResponseV1 {
                            api_version: 1,
                            enrollment_id: r.enrollment_id.clone(),
                            key_id: r.key_id.clone(),
                            server_nonce: r.server_nonce.clone(),
                            challenge_expires_at: r.challenge_expires_at,
                            expires_at: r.expires_at,
                            descriptor_digest: r.descriptor_digest.clone(),
                        })
                        .map_err(|_| denied())
                    },
                )
            }
            "/internal/prove" => match serde_json::from_str::<ProveInput>(&raw) {
                Ok(p) => store::prove(&self.state, &self.env, &self.actor_id, &self.realm, p)
                    .await
                    .and_then(|f| {
                        serde_json::to_string(&ClientEnrollmentProofResponseV1 {
                            api_version: 1,
                            enrollment_id: f.registration.enrollment_id,
                            status: ClientEnrollmentProofStatusV1::Proved,
                        })
                        .map_err(|_| denied())
                    }),
                Err(_) => Err(denied()),
            },
            "/internal/read-proved" => match serde_json::from_str::<ReadFact>(&raw) {
                Ok(p) => store::fact(
                    &self.state,
                    &self.env,
                    &self.actor_id,
                    &self.realm,
                    &p.enrollment_id,
                )
                .await
                .and_then(|fact| {
                    serde_json::to_string(&FactView {
                        actual_actor_id: self.actor_id.clone(),
                        realm: self.realm.clone(),
                        public_origin: fact.registration.public_origin.clone(),
                        fact,
                    })
                    .map_err(|_| denied())
                }),
                Err(_) => Err(denied()),
            },
            "/internal/abuse" => match serde_json::from_str::<AbuseRequest>(&raw) {
                Ok(r) => {
                    store::charge_abuse(&self.state, &self.env, &self.actor_id, &self.realm, &r)
                        .map(|_| "{}".into())
                }
                Err(_) => Err(denied()),
            },
            _ => return Response::error("not found", 404),
        };
        match value {
            Ok(raw) => {
                let mut response = Response::ok(raw)?;
                response
                    .headers_mut()
                    .set("Content-Type", "application/json")?;
                Ok(response)
            }
            Err(_) => Response::error("client enrollment unavailable", 409),
        }
    }
}
async fn call(
    env: &Env,
    name: &str,
    path: &str,
    raw: &str,
) -> std::result::Result<String, JsValue> {
    let stub = env
        .durable_object(BINDING)
        .map_err(|_| denied())?
        .id_from_name(name)
        .map_err(|_| denied())?
        .get_stub()
        .map_err(|_| denied())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let r = Request::new_with_init(&format!("https://enrollment.internal{path}"), &init)
        .map_err(|_| denied())?;
    let response = stub.fetch_with_request(r).await.map_err(|_| denied())?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    let raw: web_sys::Response = response.into();
    bounded(raw.as_ref(), 1024 * 1024)
        .await?
        .as_string()
        .ok_or_else(denied)
}
async fn abuse(
    env: &Env,
    request: &Request,
    c: &Config,
    kind: AbuseKind,
) -> std::result::Result<(), JsValue> {
    // `cf` is provider metadata, not an ordinary HTTP header. Local tests must use
    // a dedicated fixture that installs this trusted ingress context explicitly.
    if request.cf().is_none() {
        return Err(denied());
    }
    let ip = request
        .headers()
        .get("CF-Connecting-IP")
        .map_err(|_| denied())?
        .ok_or_else(denied)?;
    let ip: std::net::IpAddr = ip.parse().map_err(|_| denied())?;
    let (realm_domain, provider_domain) = match kind {
        AbuseKind::ClientHttp => (
            "oasis7.custody.client-http-abuse-global.v1",
            "oasis7.custody.client-http-abuse-provider.v1",
        ),
        AbuseKind::Create | AbuseKind::Prove => (
            "oasis7.custody.enrollment-abuse-global.v1",
            "oasis7.custody.enrollment-abuse-provider.v1",
        ),
    };
    for (scope, key) in [
        (
            AbuseScope::Provider,
            hash(provider_domain, &(&c.realm, &c.origin, ip.to_string()))?,
        ),
        (
            AbuseScope::Realm,
            hash(realm_domain, &(&c.realm, &c.origin))?,
        ),
    ] {
        call(
            env,
            &c.bucket_name(&key)?,
            "/internal/abuse",
            &serde_json::to_string(&AbuseRequest {
                bucket: key,
                scope,
                kind,
            })
            .map_err(|_| denied())?,
        )
        .await?;
    }
    Ok(())
}
pub(crate) async fn create(env: &Env, request: Request) -> Result<Response> {
    let c =
        Config::enabled(env).map_err(|_| Error::RustError("client enrollment disabled".into()))?;
    if request.method() != Method::Post
        || request.url()?.as_str() != format!("{}/v1/client-enrollments", c.origin)
        || request.headers().get("Content-Type")?.as_deref() != Some("application/json")
    {
        return Response::error("invalid enrollment target", 400);
    }
    abuse(env, &request, &c, AbuseKind::Create)
        .await
        .map_err(|_| Error::RustError("enrollment ingress unavailable".into()))?;
    let body = text(&request, 32768)
        .await
        .map_err(|_| Error::RustError("enrollment request bound".into()))?;
    let r: ClientEnrollmentCreateRequestV1 =
        serde_json::from_str(&body).map_err(|_| Error::RustError("invalid enrollment".into()))?;
    r.validate()
        .map_err(|_| Error::RustError("invalid enrollment".into()))?;
    let id = c
        .enrollment_id(&r)
        .map_err(|_| Error::RustError("invalid enrollment".into()))?;
    let raw = call(
        env,
        &c.actor_name(&id)
            .map_err(|_| Error::RustError("invalid enrollment".into()))?,
        "/internal/create",
        &body,
    )
    .await
    .map_err(|_| Error::RustError("enrollment unavailable".into()))?;
    json_response(raw)
}
pub(crate) async fn prove(env: &Env, id: &str, request: Request) -> Result<Response> {
    let c =
        Config::enabled(env).map_err(|_| Error::RustError("client enrollment disabled".into()))?;
    let target = CanonicalHttpTarget::new(&c.origin, HttpPopRoute::EnrollmentProve(id.into()))
        .map_err(|_| Error::RustError("invalid enrollment target".into()))?;
    if request.method() != Method::Post || request.url()?.as_str() != target.uri() {
        return Response::error("invalid enrollment target", 400);
    }
    abuse(env, &request, &c, AbuseKind::Prove)
        .await
        .map_err(|_| Error::RustError("enrollment ingress unavailable".into()))?;
    let body = text(&request, 32768)
        .await
        .map_err(|_| Error::RustError("enrollment request bound".into()))?;
    let headers = request
        .headers()
        .entries()
        .map(|(name, value)| HeaderPair { name, value })
        .collect();
    let raw = serde_json::to_string(&ProveInput {
        enrollment_id: id.into(),
        body,
        headers,
    })?;
    let reply = call(
        env,
        &c.actor_name(id)
            .map_err(|_| Error::RustError("invalid enrollment".into()))?,
        "/internal/prove",
        &raw,
    )
    .await
    .map_err(|_| Error::RustError("enrollment proof rejected".into()))?;
    json_response(reply)
}

fn json_response(raw: String) -> Result<Response> {
    let mut response = Response::ok(raw)?;
    response
        .headers_mut()
        .set("Content-Type", "application/json")?;
    Ok(response)
}

/// Independent provider-level ingress limiting; this cannot consume session authority.
pub(crate) async fn limit_client_http(
    env: &Env,
    request: &Request,
) -> std::result::Result<(), JsValue> {
    let config = Config::load(env)?;
    let url = request.url().map_err(|_| denied())?;
    if url.origin().ascii_serialization() != config.origin {
        return Err(denied());
    }
    abuse(env, request, &config, AbuseKind::ClientHttp).await
}
