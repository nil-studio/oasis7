//! Identity observations never carry an Owner signing capability.
use super::{JResult, Selector, rpc};
use crate::identity_native_runtime::{clock, js_denied, raw_text, read_fence};
use crate::owner_test_session_rpc::{TestSessionPublishedObservation, TestSessionRuntimeAction};
use oasis7_custody_core::{
    DecimalU64,
    approval::bytes_digest,
    http::Role,
    provisioning::{
        OwnerSetupReadPurposeV1, TestSessionPreflightDescriptorV1, TestSessionPreflightPermitV1,
    },
    recovery::RecoveryFence,
};
use worker::Env;

pub(super) struct Observation {
    pub descriptor: TestSessionPreflightDescriptorV1,
    pub authority_revision: String,
    pub service_epoch: DecimalU64,
    pub target_security_epoch: DecimalU64,
    observed_at: u64,
    expires_at: u64,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    pub fn fingerprint(&self) -> JResult<String> {
        Ok(bytes_digest(
            &oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.test-session-private-observation.v1",
                &self.descriptor,
                &self.authority_revision,
                self.service_epoch,
                self.target_security_epoch,
            ))
            .map_err(|_| js_denied())?,
        ))
    }
    pub fn check(&self, env: &Env, selector: &Selector) -> JResult<()> {
        let now = clock()?;
        let d = &self.descriptor;
        let scope = &d.identity_scope;
        let epoch: DecimalU64 = serde_json::from_value(
            env.var("CUSTODY_SERVICE_EPOCH")
                .map_err(|_| js_denied())?
                .to_string()
                .into(),
        )
        .map_err(|_| js_denied())?;
        let environment = env
            .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
            .map_err(|_| js_denied())?
            .to_string();
        if env
            .var("CUSTODY_TEST_SESSION_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
            || selector.api_version != 1
            || d.api_version != 1
            || d.session_id != selector.session_id
            || scope.provision_id != selector.provision_id
            || scope.context.identity.account_id != selector.custody_account_id
            || scope.context.identity.environment != environment
            || scope.context.identity.role != Role::Identity
            || d.session.scope.role_scope != *scope
            || d.session.private_sid_generation.0 == 0
            || self.service_epoch != epoch
            || self.target_security_epoch.0 == 0
            || self.authority_revision.is_empty()
            || now < self.observed_at
            || now - self.observed_at > 5000
            || self.expires_at <= self.observed_at
            || self.expires_at - self.observed_at > 5000
            || now >= self.expires_at
            || now >= d.expires_at.get()
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

pub(super) async fn observe(env: &Env, selector: &Selector, ticket: u8) -> JResult<Observation> {
    let p: TestSessionPreflightPermitV1 = rpc(
        env,
        selector,
        TestSessionRuntimeAction::GetPermit { access: None },
    )
    .await?;
    if p.role != Role::Identity
        || p.purpose != OwnerSetupReadPurposeV1::Session
        || p.ticket != ticket
        || !matches!(ticket, 1 | 2)
    {
        return Err(js_denied());
    }
    // Construct the independent Fence only after receiving the fixed Account port.
    let fence_started = clock()?;
    let fence =
        serde_json::from_str(&raw_text(read_fence(env).await?, 4096)?).map_err(|_| js_denied())?;
    let o = Observation {
        descriptor: p.descriptor,
        authority_revision: p.authority_revision,
        service_epoch: p.service_epoch,
        target_security_epoch: p.target_security_epoch,
        observed_at: p.observed_at.get(),
        expires_at: p.expires_at.get(),
        fence,
        fence_started,
    };
    o.check(env, selector)?;
    Ok(o)
}

pub(super) async fn published(
    env: &Env,
    selector: &Selector,
    p: &TestSessionPublishedObservation,
    prior: &Observation,
) -> JResult<Observation> {
    let fence_started = clock()?;
    let fence =
        serde_json::from_str(&raw_text(read_fence(env).await?, 4096)?).map_err(|_| js_denied())?;
    let o = Observation {
        descriptor: p.descriptor.clone(),
        authority_revision: p.authority_revision.clone(),
        service_epoch: p.service_epoch,
        target_security_epoch: p.target_security_epoch,
        observed_at: p.observed_at.get(),
        expires_at: p.expires_at.get(),
        fence,
        fence_started,
    };
    o.check(env, selector)?;
    if o.fingerprint()? != prior.fingerprint()?
        || p.original.read.request != o.descriptor.owner_read
        || p.original.read.subject_public_key != o.descriptor.owner_key.public_key
    {
        return Err(js_denied());
    }
    Ok(o)
}
