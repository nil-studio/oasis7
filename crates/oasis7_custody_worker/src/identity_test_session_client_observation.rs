//! Identity independently fences each paid Account observation before private use.
use crate::{
    identity_native_runtime::{clock, js_denied, raw_text, read_fence},
    owner_test_session_rpc::TestSessionSelectorV1,
    test_session_client_rpc::TestSessionClientObserved,
};
use oasis7_custody_core::{
    DecimalU64, approval::bytes_digest, http::Role, provisioning::TestSessionLiveDescriptorV1,
    recovery::RecoveryFence,
};
use wasm_bindgen::JsValue;
use worker::Env;
type Result<T> = std::result::Result<T, JsValue>;

pub(super) struct Observation {
    pub descriptor: TestSessionLiveDescriptorV1,
    authority_revision: String,
    pub(super) service_epoch: DecimalU64,
    target_security_epoch: DecimalU64,
    observed_at: u64,
    expires_at: u64,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    pub async fn new(
        env: &Env,
        selector: &TestSessionSelectorV1,
        paid: TestSessionClientObserved,
        expected_ticket: u8,
    ) -> Result<Self> {
        if paid.source_ticket != expected_ticket || !matches!(expected_ticket, 1..=3) {
            return Err(js_denied());
        }
        let fence_started = clock()?;
        let fence = serde_json::from_str(&raw_text(read_fence(env).await?, 4096)?)
            .map_err(|_| js_denied())?;
        let observation = Self {
            descriptor: paid.descriptor,
            authority_revision: paid.authority_revision,
            service_epoch: paid.service_epoch,
            target_security_epoch: paid.target_security_epoch,
            observed_at: paid.observed_at.get(),
            expires_at: paid.expires_at.get(),
            fence,
            fence_started,
        };
        observation.check(env, selector)?;
        Ok(observation)
    }

    pub fn fingerprint(&self) -> Result<String> {
        Ok(bytes_digest(
            &oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.client-private-observation.v1",
                &self.descriptor,
                &self.authority_revision,
                self.service_epoch,
                self.target_security_epoch,
            ))
            .map_err(|_| js_denied())?,
        ))
    }

    pub fn check(&self, env: &Env, selector: &TestSessionSelectorV1) -> Result<()> {
        let now = clock()?;
        let d = &self.descriptor;
        let scope = &d.session.scope.role_scope;
        let value = |name| {
            env.var(name)
                .map(|v| v.to_string())
                .map_err(|_| js_denied())
        };
        let epoch: DecimalU64 = serde_json::from_value(value("CUSTODY_SERVICE_EPOCH")?.into())
            .map_err(|_| js_denied())?;
        if value("CUSTODY_TEST_SESSION_ENABLED")? != "true"
            || selector.api_version != 1
            || d.api_version != 1
            || d.session_id != selector.session_id
            || scope.provision_id != selector.provision_id
            || scope.context.identity.account_id != selector.custody_account_id
            || scope.context.identity.environment != value("CUSTODY_ROLE_KEY_ENVIRONMENT")?
            || scope.context.identity.role != Role::Identity
            || d.session.private_sid_generation.0 == 0
            || self.service_epoch != epoch
            || self.target_security_epoch.0 == 0
            || self.authority_revision.is_empty()
            || now < self.observed_at
            || now - self.observed_at > 5000
            || self.expires_at <= self.observed_at
            || self.expires_at - self.observed_at > 5000
            || now >= self.expires_at
            || now >= d.deadline.get()
            || now < self.fence_started
            || now - self.fence_started > 5000
        {
            return Err(js_denied());
        }
        self.fence.check_epoch(epoch.0).map_err(|_| js_denied())?;
        crate::native_identity::provision_transport::NativeProvisionTransport::from_env(env)
            .map_err(|_| js_denied())?
            .validate_binding(&d.native_binding)
            .map_err(|_| js_denied())?;
        Ok(())
    }
}
