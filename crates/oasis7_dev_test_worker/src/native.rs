//! Worker-native fixed HTTPS transport. No SDK proxy, redirects or automatic write retry.
use crate::{
    cache::Cache,
    config::{Account, Material, Resource},
};
use oasis7_client_api::world_service::{self as ws, entry_executor::*, executor_codec as codec, *};
use oasis7_client_api::world_service_wire::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/platform/native.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = nativeHttp)]
    async fn native_http(
        origin: &str,
        path: &str,
        method: &str,
        cookie: &str,
        csrf: &str,
        json: &str,
        deadline: f64,
        limit: usize,
        audience: &str,
    ) -> Result<JsValue, JsValue>;
}
#[wasm_bindgen(module = "/platform/store.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = randomHex)]
    fn random_hex() -> Result<String, JsValue>;
}
pub(crate) fn nonce() -> Result<String, &'static str> {
    random_hex().map_err(|_| "entropy_unavailable")
}
pub(crate) struct Client<'a> {
    pub resource: &'a Resource,
    pub account: &'a Account,
    pub material: &'a Material,
    pub cache: Cache<'a>,
    pub deadline: f64,
    /// Revalidate policy, authentication, session and deadline before every hop.
    pub guard: &'a dyn Fn() -> Result<(), &'static str>,
}
impl Client<'_> {
    async fn http(
        &self,
        path: &str,
        method: &str,
        csrf: &str,
        body: &str,
    ) -> Result<String, &'static str> {
        (self.guard)()?;
        if js_sys::Date::now() >= self.deadline {
            return Err("native_deadline_unknown");
        }
        let value = native_http(
            &self.resource.endpoint,
            path,
            method,
            &self.material.identity_cookie,
            csrf,
            body,
            self.deadline,
            2 * 1024 * 1024,
            self.resource
                .identity_audience
                .as_deref()
                .unwrap_or(&self.resource.endpoint),
        )
        .await
        .map_err(|_| "native_outcome_unknown")?;
        (self.guard)()?;
        value.as_string().ok_or("native_response_unknown")
    }
    pub async fn current_account(&self) -> Result<(), &'static str> {
        let raw = self.http("/session", "GET", "", "").await?;
        let value: Value =
            oasis7_agent_api::game::parse_bounded_json(raw.as_bytes(), 2 * 1024 * 1024)
                .map_err(|_| "native_response_unknown")?;
        if value["account"]["account_id"].as_str() != Some(&self.account.account_id) {
            return Err("native_identity_changed");
        }
        Ok(())
    }
    pub async fn call(&self, operation: EntryExecutorOperationV1) -> Result<Value, &'static str> {
        (self.guard)()?;
        let original = codec::gateway(
            self.resource.fence.clone(),
            operation,
            nonce()?,
            &self.material.executor_seed,
        )
        .map_err(|_| "native_resource_denied")?;
        let digest = ws::request_digest("executor-gateway-transport/v1", &original)
            .map_err(|_| "native_original_unknown")?;
        let body = serde_json::to_string(&original).map_err(|_| "native_original_unknown")?;
        if body.len() > 65536 {
            return Err("native_request_bound");
        }
        self.cache.put("transport", &digest, &original).await?;
        self.current_account().await?;
        let caps = self.http("/capabilities", "GET", "", "").await?;
        let caps: Value =
            oasis7_agent_api::game::parse_bounded_json(caps.as_bytes(), 2 * 1024 * 1024)
                .map_err(|_| "native_response_unknown")?;
        let csrf = caps["csrf_token"]
            .as_str()
            .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("native_authentication_unavailable")?;
        // Absolute guard is checked again after BOTH authentication hops.
        let raw = self
            .http("/entry-executor/control", "POST", csrf, &body)
            .await?;
        let reply: Value =
            oasis7_agent_api::game::parse_bounded_json(raw.as_bytes(), 2 * 1024 * 1024)
                .map_err(|_| "native_response_unknown")?;
        if reply.get("upstream_read_error").is_some() {
            return Err("native_read_unavailable");
        }
        reply
            .get("world_response")
            .filter(|v| !v.is_null())
            .cloned()
            .ok_or("native_response_unknown")
    }
    pub async fn signed<Q: Serialize, R: Serialize + DeserializeOwned>(
        &self,
        path: &str,
        request: &SignedReadRequest<Q>,
        operation: EntryExecutorOperationV1,
    ) -> Result<R, &'static str> {
        let value = self.call(operation).await?;
        let bytes = serde_json::to_vec(&value).map_err(|_| "native_response_unknown")?;
        codec::verify_world_reply(
            path,
            request,
            &bytes,
            2 * 1024 * 1024,
            &self.resource.service_public_key,
        )
        .map(|v| v.payload)
        .map_err(|_| "native_signed_response_invalid")
    }
    pub async fn view(&self) -> Result<ws::task_projection::ValidatedTaskProjection, &'static str> {
        let request = ReadWorldViewRequest {
            observation_nonce: Some(nonce()?),
            formal_anchor: None,
            contract_version: 1,
            world: WorldIdentity {
                world_id: self.resource.fence.world_id.clone(),
                genesis_digest: self.resource.fence.genesis_digest.clone(),
            },
            scope_id: format!("agent:{}", self.resource.fence.agent_id),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        };
        let signed = ws::sign_read_request(
            VIEW_JSON_PATH,
            request.clone(),
            &self.material.executor_seed,
        )
        .map_err(|_| "native_resource_denied")?;
        let response: ReadWorldViewResponse<Value> = self
            .signed(
                VIEW_JSON_PATH,
                &signed,
                EntryExecutorOperationV1::ReadView(signed.clone()),
            )
            .await?;
        if response.execution_evidence_scope.is_some() {
            return Err("native_evidence_profile_denied");
        }
        let projection = ws::task_projection::validate_task_projection(&response, &request)
            .map_err(|_| "native_projection_invalid")?;
        if projection.task_fence.as_ref() != Some(&self.resource.fence) {
            return Err("native_task_fence_changed");
        }
        Ok(projection)
    }
    pub async fn intent(
        &self,
        original: &SubmitIntentRequest<WorldServicePayloadV1>,
        submit: bool,
    ) -> Result<IntentResponse<Value>, &'static str> {
        let lookup = codec::lookup(original).map_err(|_| "native_original_invalid")?;
        let operation = if submit {
            EntryExecutorOperationV1::Submit(original.clone())
        } else {
            EntryExecutorOperationV1::Lookup(lookup.clone())
        };
        let value = self.call(operation).await?;
        let bytes = serde_json::to_vec(&value).map_err(|_| "native_response_unknown")?;
        if submit {
            verified_intent(
                SUBMIT_PATH,
                original,
                &bytes,
                &original.correlation,
                &self.resource.service_public_key,
            )
        } else {
            verified_intent(
                LOOKUP_PATH,
                &lookup,
                &bytes,
                &original.correlation,
                &self.resource.service_public_key,
            )
        }
    }
}

/// Both submit and authenticated lookup return signed service envelopes.
/// Correlation alone is not response authority.
fn verified_intent<Q: Serialize>(
    path: &str,
    request: &Q,
    bytes: &[u8],
    expected: &RequestCorrelation,
    key: &str,
) -> Result<IntentResponse<Value>, &'static str> {
    let response: IntentResponse<Value> =
        codec::verify_world_reply(path, request, bytes, 2 * 1024 * 1024, key)
            .map_err(|_| "native_signed_response_invalid")?
            .payload;
    if response.execution_evidence_scope.is_some() || response.validate(expected).is_err() {
        return Err("native_correlation_invalid");
    }
    Ok(response)
}
#[cfg(test)]
mod intent_tests {
    use super::*;
    #[test]
    fn intent_envelopes_bind_domain_request_signature_and_original_correlation() {
        let seed = "11".repeat(32);
        let key = ws::sign_read_request("fixture", (), &seed)
            .unwrap()
            .subject_public_key;
        let request = serde_json::json!({"original":"immutable"});
        let correlation = RequestCorrelation {
            key: RequestKey {
                world: WorldIdentity {
                    world_id: "world".into(),
                    genesis_digest: "genesis".into(),
                },
                verified_subject: "executor".into(),
                operation_domain: "cognition".into(),
                nonce_scope: "task".into(),
                request_id_or_nonce: "original".into(),
            },
            payload_digest: "digest".into(),
        };
        let payload = IntentResponse::<Value> {
            terminal_recovery_authority: None,
            formal_profile: None,
            execution_evidence_scope: None,
            contract_version: 1,
            correlation: correlation.clone(),
            outcome: IntentOutcome::Unknown,
        };
        for path in [SUBMIT_PATH, LOOKUP_PATH] {
            let signed = ws::sign_service_response(
                path,
                ws::request_digest(path, &request).unwrap(),
                payload.clone(),
                &seed,
            )
            .unwrap();
            let bytes = serde_json::to_vec(&signed).unwrap();
            assert!(matches!(
                verified_intent(path, &request, &bytes, &correlation, &key)
                    .unwrap()
                    .outcome,
                IntentOutcome::Unknown
            ));
            assert!(
                verified_intent(
                    path,
                    &request,
                    &serde_json::to_vec(&payload).unwrap(),
                    &correlation,
                    &key
                )
                .is_err()
            );
            assert!(
                verified_intent(
                    path,
                    &serde_json::json!({"original":"changed"}),
                    &bytes,
                    &correlation,
                    &key
                )
                .is_err()
            );
            assert!(
                verified_intent("/wrong-domain", &request, &bytes, &correlation, &key).is_err()
            );
            let mut tampered = signed.clone();
            tampered.payload.outcome = IntentOutcome::Pending;
            assert!(
                verified_intent(
                    path,
                    &request,
                    &serde_json::to_vec(&tampered).unwrap(),
                    &correlation,
                    &key
                )
                .is_err()
            );
            let mut other = correlation.clone();
            other.payload_digest = "other".into();
            assert_eq!(
                verified_intent(path, &request, &bytes, &other, &key).unwrap_err(),
                "native_correlation_invalid"
            );
        }
    }
}
