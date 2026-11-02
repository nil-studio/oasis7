//! One already-owned Account invocation, one prepaid direct Owner Actor request.
//! The real Term6 producer must supply its handler-local Current; no new public
//! route, history reconstruction, self-fetch or ordinary signer callback exists.
use super::*;
use oasis7_agent_api::game::SignedGameTaskStopV2;
use oasis7_custody_core::term_guarded_signer_v2::TERM_GUARDED_STOP_SIGNER_PATH_V2;
use oasis7_custody_core::{provisioning::SessionPurposeV1, time_units::UnixMillis};
use worker::{Env, Method, Request, RequestInit, RequestRedirect};
impl OwnerProvisionApprovalStore {
    pub(crate) async fn sign_term_guarded_stop_owner_v2<F>(
        &self,
        env: &Env,
        id: &str,
        attempt: &str,
        current: &F,
    ) -> Result<SignedGameTaskStopV2>
    where
        F: Fn() -> Result<super::client::ClientLiveCurrent>,
    {
        if !env
            .var("CUSTODY_TERM_GUARDED_SIGNER_V2_ENABLED")
            .ok()
            .is_some_and(|v| v.to_string() == "true")
        {
            return Err(rejected());
        }
        let first = self
            .native_verified(
                &|| Ok(current()?.native),
                "term_guarded_owner_vault_claim",
                id,
                |state, _, _, time| {
                    let owned = current()?;
                    let mut jobs = state
                        .test_session_v2_native_restore_checks(id, &owned.native.scope.as_core())
                        .map_err(|_| rejected())?;
                    jobs.push(owned.with_core(|c| {
                        state
                            .test_session_v2_live_scope(
                                id,
                                SessionPurposeV1::TerminationPlan,
                                c,
                                UnixMillis::new(time),
                            )
                            .map_err(|_| rejected())
                    })?);
                    Ok(jobs)
                },
                |state, _, _, time, cache| {
                    current()?.with_core(|c| {
                        state
                            .claim_test_session_guarded_stop_vault_v2(
                                id,
                                attempt,
                                c,
                                UnixMillis::new(time),
                                cache,
                            )
                            .map_err(|_| rejected())
                    })
                },
            )
            .await?;
        // Only after actual SQL commit may this opaque handle reach the issuer.
        let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)
            .map_err(|_| rejected())?;
        let permit = issuer
            .sign_term_guarded_stop(first, now()?)
            .map_err(|_| rejected())?;
        let expected = permit.payload.original.clone();
        let name = permit
            .payload
            .owner_scope
            .task_digest()
            .map_err(|_| rejected())?;
        let raw = serde_json::to_string(&permit).map_err(|_| rejected())?;
        if raw.len() > 32768 || now()? >= permit.payload.expires_at.0 {
            return Err(rejected());
        }
        let stub = env
            .durable_object("ROLE_KEYS")
            .map_err(|_| rejected())?
            .id_from_name(&name)
            .map_err(|_| rejected())?
            .get_stub()
            .map_err(|_| rejected())?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_body(Some(raw.into()))
            .with_redirect(RequestRedirect::Manual);
        let mut request = Request::new_with_init(
            &format!("https://role-key.internal{TERM_GUARDED_STOP_SIGNER_PATH_V2}"),
            &init,
        )
        .map_err(|_| rejected())?;
        request
            .headers_mut()
            .map_err(|_| rejected())?
            .set("Content-Type", "application/json")
            .map_err(|_| rejected())?;
        // Exactly one application fetch; error/unknown never recreates first.
        let response = stub
            .fetch_with_request(request)
            .await
            .map_err(|_| rejected())?;
        if response.status_code() != 200 {
            return Err(rejected());
        }
        let response: web_sys::Response = response.into();
        let text = bounded_body(response.as_ref(), 32768)
            .await
            .map_err(|_| rejected())?
            .as_string()
            .ok_or_else(rejected)?;
        let signed: SignedGameTaskStopV2 = serde_json::from_str(&text).map_err(|_| rejected())?;
        let original: serde_json::Value = serde_json::from_str(&text).map_err(|_| rejected())?;
        if serde_json::to_value(&signed).map_err(|_| rejected())? != original
            || signed.request != expected
        {
            return Err(rejected());
        }
        self.local_reservation("term_guarded_owner_original", id, |state, _| {
            state
                .record_test_session_guarded_stop_signature_v2(id, signed.clone())
                .map_err(|_| rejected())
        })?;
        // Strong historical original only. No World dispatch/Current follows here.
        Ok(signed)
    }
}

#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch,js_name=readBoundedText)]
    async fn bounded_body(response: &JsValue, limit: u32) -> std::result::Result<JsValue, JsValue>;
}
