//! Actual directory, independent Fence and physically addressed enrollment observations.
use super::*;
use oasis7_custody_core::approval::{CustodySessionStatusV1, SignedApprovalVerificationV1};
pub(super) fn enabled(env: &Env) -> Result<()> {
    if env
        .var("CUSTODY_TEST_SESSION_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(denied());
    }
    Ok(())
}
pub(super) async fn observe_manager(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    input: ApprovalAdmissionInput,
    manager: CustodySessionStatusV1,
) -> Result<authority::Observations> {
    enabled(env)?;
    authority::observe_fresh_manager(env, saved.state.draft(), input, manager).await
}
pub(super) async fn observe(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    input: ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
    inspection_only: bool,
) -> Result<authority::Observations> {
    enabled(env)?;
    authority::observe(env, saved.state.draft(), input, signed, inspection_only).await
}
pub(super) fn current(
    state: &JsValue,
    env: &Env,
    observed: &authority::Observations,
    business: bool,
) -> std::result::Result<NativeStageCurrent, JsValue> {
    enabled(env).map_err(|_| JsValue::from_str("test session unavailable"))?;
    authority::current(state, env, observed, business)
}
pub(super) struct Enrollment {
    fact: crate::client_enrollment_reader::FactView,
    expected_actor: String,
    realm: String,
    origin: String,
}
pub(super) async fn enrollment(env: &Env, id: &str) -> Result<Enrollment> {
    let config = crate::client_enrollment_authority::Config::load(env).map_err(|_| denied())?;
    let expected_actor = config.actor_id(env, id).map_err(|_| denied())?;
    let fact = crate::client_enrollment_reader::read_proved(env, id)
        .await
        .map_err(|_| denied())?;
    if fact.actual_actor_id != expected_actor
        || fact.realm != config.realm
        || fact.public_origin != config.origin
    {
        return Err(denied());
    }
    Ok(Enrollment {
        fact,
        expected_actor,
        realm: config.realm,
        origin: config.origin,
    })
}
pub(super) fn approval_current(
    state: &JsValue,
    env: &Env,
    observed: &authority::Observations,
    enrollment: &Enrollment,
    observed_at: u64,
) -> std::result::Result<TestSessionApprovalOwned, JsValue> {
    let config = crate::client_enrollment_authority::Config::load(env)?;
    if config.realm != enrollment.realm
        || config.origin != enrollment.origin
        || config.actor_id(env, &enrollment.fact.fact.registration.enrollment_id)?
            != enrollment.expected_actor
    {
        return Err(JsValue::from_str("test session unavailable"));
    }
    Ok(TestSessionApprovalOwned {
        native: current(state, env, observed, true)?,
        enrollment: crate::client_enrollment_reader::FactView {
            actual_actor_id: enrollment.fact.actual_actor_id.clone(),
            realm: enrollment.fact.realm.clone(),
            public_origin: enrollment.fact.public_origin.clone(),
            fact: enrollment.fact.fact.clone(),
        },
        expected_actor_id: enrollment.expected_actor.clone(),
        expected_realm: config.realm,
        expected_origin: config.origin,
        deployments: config.deployments,
        observed_at,
    })
}
pub(super) fn input(
    saved: &ProvisionApprovalSnapshot,
    request: &TestSessionSelectorV1,
) -> Result<ApprovalAdmissionInput> {
    let draft = saved.state.draft();
    if request.api_version != 1
        || request.custody_account_id != draft.entry.custody_account_id
        || request.provision_id != draft.entry.provision_id
    {
        return Err(denied());
    }
    Ok(saved
        .state
        .test_sessions()
        .get(&request.session_id)
        .ok_or_else(denied)?
        .input
        .clone())
}
pub(super) async fn runtime_observe(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    request: &TestSessionSelectorV1,
    source: &str,
    inspection_only: bool,
) -> Result<authority::Observations> {
    let input = input(saved, request)?;
    let s = saved
        .state
        .test_sessions()
        .get(&request.session_id)
        .ok_or_else(denied)?;
    let signed = s
        .sources
        .get(source)
        .and_then(|s| s.verification.as_ref())
        .or_else(|| {
            s.book
                .original(&request.session_id)
                .and_then(|o| o.verification())
        })
        .ok_or_else(denied)?;
    observe(env, saved, input, signed, inspection_only).await
}

/// Caller is the fixed Owner/ticket3 branch after its SQL reservation returns.
pub(super) async fn runtime_observe_paid_factory(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    request: &TestSessionSelectorV1,
    source: &str,
) -> Result<authority::Observations> {
    enabled(env)?;
    let input = input(saved, request)?;
    let session = saved
        .state
        .test_sessions()
        .get(&request.session_id)
        .ok_or_else(denied)?;
    let signed = session
        .sources
        .get(source)
        .and_then(|s| s.verification.as_ref())
        .or_else(|| {
            session
                .book
                .original(&request.session_id)
                .and_then(|o| o.verification())
        })
        .ok_or_else(denied)?;
    authority::observe_paid_test_session_factory(env, saved.state.draft(), input, signed).await
}

use crate::directory_config::DirectoryConfig;
pub(super) async fn management_read<R, F>(
    state: &JsValue,
    env: &Env,
    manager: &CustodySessionStatusV1,
    provision_id: &str,
    enrollment_id: &str,
    read: F,
) -> Result<R>
where
    F: FnOnce(
        &oasis7_custody_core::provisioning::AccountProvisionApprovalState,
        &oasis7_custody_core::provisioning::OwnerSetupManagementCurrent<'_>,
        &oasis7_custody_core::provisioning::NativeStageScope<'_>,
        &oasis7_custody_core::provisioning::ProvedEnrollmentObservation<'_>,
        &oasis7_custody_core::client_enrollment::HostDeploymentAllowlist,
        u64,
    ) -> Result<R>,
{
    let enrolled = enrollment(env, enrollment_id).await?;
    let enrollment_config =
        crate::client_enrollment_authority::Config::load(env).map_err(|_| denied())?;
    let scope_owned = public_native_scope(env)?;
    management_only(state, env, manager, provision_id, |saved, current, time| {
        let scope = oasis7_custody_core::provisioning::NativeStageScope {
            http_origin: &scope_owned.http_origin,
            native_root_key_id: &scope_owned.native_root_key_id,
            native_root_public_key: &scope_owned.native_root_public_key,
            owner_binding_pin: &scope_owned.owner_binding_pin,
        };
        let observed = ProvedEnrollmentObservation {
            actual_actor_id: &enrolled.fact.actual_actor_id,
            expected_actor_id: &enrolled.expected_actor,
            actual_realm: &enrolled.fact.realm,
            expected_realm: &enrolled.realm,
            expected_public_origin: &enrolled.origin,
            fact: &enrolled.fact.fact,
        };
        observed.validate_physical().map_err(|_| denied())?;
        // Fixed namespace revalidation after all awaits; no grant or state mutation.
        if enrollment_config.realm != enrolled.realm
            || enrollment_config.origin != enrolled.origin
            || enrollment_config
                .actor_id(env, enrollment_id)
                .map_err(|_| denied())?
                != enrolled.expected_actor
        {
            return Err(denied());
        }
        read(
            saved,
            current,
            &scope,
            &observed,
            &enrollment_config.deployments,
            time,
        )
    })
    .await
}

pub(super) async fn management_only<R, F>(
    state: &JsValue,
    env: &Env,
    manager: &CustodySessionStatusV1,
    provision_id: &str,
    read: F,
) -> Result<R>
where
    F: FnOnce(
        &oasis7_custody_core::provisioning::AccountProvisionApprovalState,
        &OwnerSetupManagementCurrent<'_>,
        u64,
    ) -> Result<R>,
{
    use crate::directory_rpc::{ProvisionGateRpc, ProvisionGateView};
    use oasis7_custody_core::{approval::ApprovalContextV1, provisioning::ProvisionPolicy};
    let store = OwnerProvisionApprovalStore::open(state.clone())
        .map_err(|_| management_diagnostic("open", denied()))?;
    let saved = store
        .snapshot()
        .map_err(|_| management_diagnostic("snapshot", denied()))?;
    let draft = saved.state.draft();
    let config = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
    if config.principal(manager)? != draft.entry.manager || provision_id != draft.entry.provision_id
    {
        return Err(management_diagnostic("principal", denied()));
    }
    let started = clock()?;
    let gate = ProvisionGateRpc {
        manager_session: manager.clone(),
        provision_id: provision_id.into(),
        custody_account_id: draft.entry.custody_account_id.clone(),
    };
    let raw = crate::owner_directory::call_directory(
        env,
        "/internal/provision-gate",
        &serde_json::to_string(&gate)?,
        manager,
    )
    .await
    .map_err(|e| management_diagnostic("directory", e))?;
    let gate: ProvisionGateView =
        serde_json::from_str(&raw).map_err(|_| management_diagnostic("gate_decode", denied()))?;
    let (fence, fence_started) = super::super::super::fence(env)
        .await
        .map_err(|e| management_diagnostic("fence_read", e))?;
    // Everything below is synchronous, including the final Account resnapshot.
    let saved = OwnerProvisionApprovalStore::open(state.clone())
        .map_err(|_| denied())?
        .snapshot()
        .map_err(|_| denied())?;
    let config = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
    let now = clock()?;
    super::super::super::check_gate(&gate, saved.state.draft(), now)
        .map_err(|e| management_diagnostic("gate_current", e))?;
    super::super::super::fresh(DecimalU64(started), now)
        .map_err(|e| management_diagnostic("gate_fresh", e))?;
    super::super::super::fresh(fence_started, now)
        .map_err(|e| management_diagnostic("fence_fresh", e))?;
    if config.principal(manager)? != saved.state.draft().entry.manager
        || provision_id != saved.state.draft().entry.provision_id
    {
        return Err(denied());
    }
    let value = |key| env.var(key).map(|v| v.to_string()).map_err(|_| denied());
    let service_epoch: DecimalU64 =
        serde_json::from_value(value("CUSTODY_SERVICE_EPOCH")?.into()).map_err(|_| denied())?;
    fence
        .check_epoch(service_epoch.0)
        .map_err(|_| management_diagnostic("fence_epoch", denied()))?;
    let authority = ApprovalContextV1 {
        environment: config.environment.clone(),
        custody_account_id: saved.state.draft().entry.custody_account_id.clone(),
        audience: value("CUSTODY_NATIVE_AUDIENCE")?,
        policy_version: config.version,
        security_version: saved.state.provision().security_epoch(),
        service_epoch,
    };
    let current = oasis7_custody_core::provisioning::OwnerSetupManagementCurrent {
        manager,
        authority: &authority,
        policy: ProvisionPolicy {
            routes: &config.routes,
            managers: &config.managers,
            version: config.version.0,
        },
        world_identity: &config.world,
        native_identity_namespace: &config.namespace,
        fence: &fence,
        observed_at: started,
    };
    read(&saved.state, &current, now).map_err(|e| management_diagnostic("callback", e))
}

fn public_native_scope(
    env: &Env,
) -> Result<crate::provision_approval_store::native::NativeStageScopeCurrent> {
    let value = |key| env.var(key).map(|v| v.to_string()).map_err(|_| denied());
    let http_origin = value("CUSTODY_NATIVE_ORIGIN")?;
    let url = worker::Url::parse(&http_origin).map_err(|_| denied())?;
    if url.scheme() != "https" || url.origin().ascii_serialization() != http_origin {
        return Err(denied());
    }
    let native_root_key_id = value("CUSTODY_NATIVE_APPROVAL_KEY_ID")?;
    let native_root_public_key = value("CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY")?;
    let raw: [u8; 32] = hex::decode(&native_root_public_key)
        .map_err(|_| denied())?
        .try_into()
        .map_err(|_| denied())?;
    if hex::encode(raw) != native_root_public_key
        || native_root_key_id.is_empty()
        || native_root_key_id.len() > 256
        || ed25519_dalek::VerifyingKey::from_bytes(&raw)
            .map_err(|_| denied())?
            .is_weak()
    {
        return Err(denied());
    }
    let owner_binding_pin: oasis7_custody_core::native_binding::NativeBindingPinV1 =
        serde_json::from_str(&value("CUSTODY_NATIVE_BINDING_OWNER_PIN")?).map_err(|_| denied())?;
    owner_binding_pin.validate().map_err(|_| denied())?;
    let directory = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
    let epoch: DecimalU64 =
        serde_json::from_value(value("CUSTODY_SERVICE_EPOCH")?.into()).map_err(|_| denied())?;
    if owner_binding_pin.environment != directory.environment
        || owner_binding_pin.service_epoch != epoch
    {
        return Err(denied());
    }
    Ok(
        crate::provision_approval_store::native::NativeStageScopeCurrent {
            http_origin,
            native_root_key_id,
            native_root_public_key,
            owner_binding_pin,
        },
    )
}

fn management_diagnostic(stage: &str, error: worker::Error) -> worker::Error {
    #[cfg(feature = "approval-harness")]
    worker::console_warn!("test_session.management.{}", stage);
    #[cfg(not(feature = "approval-harness"))]
    let _ = stage;
    error
}
