//! Fixed private Enrollment fact reader shared by Owner and Identity.
//! This module exposes neither the Enrollment class nor public ingress/mutation.
use crate::client_enrollment_authority::{BINDING, Config, denied, verify};
use oasis7_custody_core::client_enrollment::ProvedEnrollmentFactV1;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, RequestInit};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FactView {
    pub actual_actor_id: String,
    pub realm: String,
    pub public_origin: String,
    pub fact: ProvedEnrollmentFactV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadFact {
    pub enrollment_id: String,
}
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readBoundedText)]
    async fn bounded(body: &JsValue, limit: u32) -> Result<JsValue, JsValue>;
}
pub(crate) async fn read_proved(env: &Env, id: &str) -> Result<FactView, JsValue> {
    let c = Config::load(env)?;
    let expected = c.actor_id(env, id)?;
    let stub = env
        .durable_object(BINDING)
        .map_err(|_| denied())?
        .id_from_name(&c.actor_name(id)?)
        .map_err(|_| denied())?
        .get_stub()
        .map_err(|_| denied())?;
    let raw = serde_json::to_string(&ReadFact {
        enrollment_id: id.into(),
    })
    .map_err(|_| denied())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let request = Request::new_with_init("https://enrollment.internal/internal/read-proved", &init)
        .map_err(|_| denied())?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| denied())?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    let raw: web_sys::Response = response.into();
    let body = bounded(raw.as_ref(), 1024 * 1024)
        .await?
        .as_string()
        .ok_or_else(denied)?;
    let v: FactView = serde_json::from_str(&body).map_err(|_| denied())?;
    physical(&v, &c, &expected, id)?;
    // A fixed namespace is necessary but not a substitute for the exact original proof.
    verify(v.fact.checks().map_err(|_| denied())?).await?;
    let current = Config::load(env)?;
    physical(&v, &current, &current.actor_id(env, id)?, id)?;
    Ok(v)
}
fn physical(v: &FactView, c: &Config, expected: &str, id: &str) -> Result<(), JsValue> {
    if v.actual_actor_id != expected
        || v.realm != c.realm
        || v.public_origin != c.origin
        || v.fact.registration.enrollment_id != id
        || v.fact.registration.physical_actor_id != expected
        || v.fact.registration.realm != c.realm
        || v.fact.registration.public_origin != c.origin
    {
        return Err(denied());
    }
    Ok(())
}
