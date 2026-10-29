//! Offline synthetic replay only. Fixed component time never enters live crypto.
use oasis7_client_api::world_service::{self as ws, task_context::*};
use oasis7_custody_core::{
    DecimalU64,
    approval::{ApprovedWorldV1, bytes_digest},
    approval_admission::ApprovalAdmissionContext,
    native_binding::*,
    provisioning::*,
    time_units::UnixMillis,
};
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use worker::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    routes: ProvisionRoutes,
    managers: Vec<BootstrapManager>,
    version: u64,
    world_identity: ApprovedWorldV1,
    native_identity_namespace: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Native {
    http_origin: String,
    native_root_key_id: String,
    native_root_public_key: String,
    owner_binding_pin: NativeBindingPinV1,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    operation_id: String,
    source_ticket: u8,
    observed_at: UnixMillis,
    private_sid_generation: DecimalU64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replay {
    name: String,
    session_id: String,
    component_now: UnixMillis,
    account_state: AccountProvisionApprovalState,
    approval_context: ApprovalAdmissionContext,
    policy: Policy,
    native_scope: Native,
    source: Source,
    lifetime: SignedManagedNativeSessionLifetimeV1,
    response: serde_json::Value,
}
impl Replay {
    async fn run(&self) -> std::result::Result<serde_json::Value, ()> {
        let provision = ProvisionApprovalContext {
            approval: &self.approval_context,
            policy: ProvisionPolicy {
                routes: &self.policy.routes,
                managers: &self.policy.managers,
                version: self.policy.version,
            },
            world_identity: &self.policy.world_identity,
            native_identity_namespace: &self.policy.native_identity_namespace,
        };
        let native = NativeStageScope {
            http_origin: &self.native_scope.http_origin,
            native_root_key_id: &self.native_scope.native_root_key_id,
            native_root_public_key: &self.native_scope.native_root_public_key,
            owner_binding_pin: &self.native_scope.owner_binding_pin,
        };
        // These references are reconstructed only in a component artifact from
        // public synthetic fixture facts. They are not deployment Current.
        let current = TestSessionLiveCurrent {
            provision: &provision,
            native_scope: &native,
            lifetime: &self.lifetime,
            private_sid_generation: self.source.private_sid_generation,
            source_ticket: self.source.source_ticket,
            source_operation_id: &self.source.operation_id,
            observed_at: self.source.observed_at,
        };
        let (jobs, body) = match self.name.as_str() {
            "prepared" => {
                let response: ws::SignedServiceResponse<PreparedTaskContextV1> =
                    serde_json::from_value(self.response.clone()).map_err(|_| ())?;
                let checks = self
                    .account_state
                    .test_session_prepared_context_checks(
                        &self.session_id,
                        &response,
                        &current,
                        self.component_now,
                    )
                    .map_err(|_| ())?;
                let jobs = checks.checks().len();
                let mut cache = crate::context_body_signatures::verify(checks)
                    .await
                    .map_err(|_| ())?;
                let body = self
                    .account_state
                    .validate_test_session_prepared_body(
                        &self.session_id,
                        &response,
                        &current,
                        self.component_now,
                        &mut cache,
                    )
                    .map_err(|_| ())?;
                (jobs, body)
            }
            "captured" => {
                let response: ws::SignedServiceResponse<CapturedTaskContextV1> =
                    serde_json::from_value(self.response.clone()).map_err(|_| ())?;
                let checks = self
                    .account_state
                    .test_session_captured_context_checks(
                        &self.session_id,
                        &response,
                        &current,
                        self.component_now,
                    )
                    .map_err(|_| ())?;
                let jobs = checks.checks().len();
                let mut cache = crate::context_body_signatures::verify(checks)
                    .await
                    .map_err(|_| ())?;
                let body = self
                    .account_state
                    .validate_test_session_captured_body(
                        &self.session_id,
                        &response,
                        &current,
                        self.component_now,
                        &mut cache,
                    )
                    .map_err(|_| ())?;
                (jobs, body)
            }
            _ => return Err(()),
        };
        Ok(serde_json::json!({"manifest_jobs": jobs,
            "canonical_bytes": body.canonical_bytes().len(),
            "body_digest": bytes_digest(body.canonical_bytes()),
            "clock_mode": "fixed-component-replay-only"}))
    }
}
#[durable_object]
pub struct ContextBodyChecksHarness;
impl DurableObject for ContextBodyChecksHarness {
    fn new(_state: State, _env: Env) -> Self {
        Self
    }
    async fn fetch(&self, mut request: Request) -> Result<Response> {
        // Request::json uses serde-wasm-bindgen's known-fields projection;
        // strict JSON text decoding must see unknown fields as well.
        let result = match request.text().await {
            Ok(raw) => match serde_json::from_str::<Replay>(&raw) {
                Ok(replay) => replay.run().await,
                Err(_) => Err(()),
            },
            Err(_) => Err(()),
        };
        match result {
            Ok(value) => Response::from_json(&value),
            Err(()) => Response::error("synthetic context signature replay rejected", 409),
        }
    }
}
