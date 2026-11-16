//! Pure signed intent correlation shared by execution and native clients.
use crate::world_service::*;
use crate::world_service::{PaidServiceProtocolRefusalV1, ServiceAuthorityWorkErrorV1};
use crate::world_service_wire::*;
use serde::Serialize;

/// Browser proof authority stays in its adapter; native task clients have no
/// browser payloads. A callback must verify the legacy request before returning
/// its subject, domain, nonce scope and request identity.
pub type LegacyIdentity = (String, &'static str, &'static str, String);

pub fn derive_correlation_with_legacy<
    C: Serialize,
    P: Serialize,
    L: Serialize,
    CP: Serialize,
    ST: Serialize,
    R: Serialize,
    D: Serialize,
    X: Serialize,
>(
    world: WorldIdentity,
    payload: &WorldServicePayloadProfileV1<C, P, L, CP, ST, R, D, X>,
    legacy: impl FnOnce(
        &WorldServicePayloadProfileV1<C, P, L, CP, ST, R, D, X>,
    ) -> Result<LegacyIdentity, String>,
) -> Result<RequestCorrelation, String> {
    derive_correlation_with_legacy_work_v1(
        world,
        payload,
        LegacyBrowserCallbackV1 { verify: legacy },
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .map(work::PaidRequestCorrelationDataV1::into_legacy)
    .map_err(crate::world_service::legacy_service_work_error_v1)
}

pub(crate) fn derive_correlation_with_legacy_work_v1<
    C: Serialize,
    P: Serialize,
    L: Serialize,
    CP: Serialize,
    ST: Serialize,
    R: Serialize,
    D: Serialize,
    X: Serialize,
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
    V: FixedBrowserVerifierV1<WorldServicePayloadProfileV1<C, P, L, CP, ST, R, D, X>, A>,
>(
    world: WorldIdentity,
    payload: &WorldServicePayloadProfileV1<C, P, L, CP, ST, R, D, X>,
    legacy: V,
    source: &A,
) -> Result<PaidRequestCorrelationDataV1<A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>> {
    let mut phase = work::Phase::new(source)?;
    phase.trace(&world)?;
    phase.hold(
        std::mem::size_of::<RequestCorrelation>()
            + world.world_id.len()
            + world.genesis_digest.len(),
    )?;
    if let WorldServicePayloadV1::GameStarterClaim(signed) = payload {
        use oasis7_agent_api::game::GAME_STARTER_CLAIM_DOMAIN;
        phase.validation(&signed.request, || signed.request.validate())?;
        crate::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
            GAME_STARTER_CLAIM_DOMAIN,
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
            source,
        )?;
        phase.trace(&(
            &signed.request.world_id,
            &signed.request.genesis_digest,
            &world.world_id,
            &world.genesis_digest,
        ))?;
        if signed.request.world_id != world.world_id
            || signed.request.genesis_digest != world.genesis_digest
        {
            return phase.fail("starter world identity mismatch");
        }
        let correlation = RequestCorrelation {
            key: RequestKey {
                world,
                verified_subject: phase.copy(&signed.subject_public_key)?,
                operation_domain: phase.copy(GAME_STARTER_CLAIM_DOMAIN)?,
                nonce_scope: phase.prefixed("agent:", &signed.request.agent_id)?,
                request_id_or_nonce: phase.copy(&signed.request.client_operation_id)?,
            },
            payload_digest: phase.digest("intent", payload)?,
        };
        phase.validation(&correlation, || correlation.validate())?;
        return Ok(phase.finish(correlation));
    }
    if let WorldServicePayloadV1::GameParticipantRegister { owner, admission } = payload {
        use oasis7_agent_api::game::*;
        phase.validation(&owner.request, || owner.request.validate())?;
        phase.trace(&owner.subject_public_key)?;
        if owner.subject_public_key != owner.request.owner_public_key {
            return phase.fail("participant owner signature mismatch");
        }
        crate::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
            GAME_PARTICIPANT_REGISTER_DOMAIN,
            &owner.request,
            &owner.subject_public_key,
            &owner.signature_hex,
            source,
        )?;
        crate::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
            GAME_PARTICIPANT_ADMISSION_DOMAIN,
            &admission.request,
            &admission.subject_public_key,
            &admission.signature_hex,
            source,
        )?;
        phase.trace(&(
            &owner.request.world_id,
            &owner.request.genesis_digest,
            &world.world_id,
            &world.genesis_digest,
        ))?;
        if owner.request.world_id != world.world_id
            || owner.request.genesis_digest != world.genesis_digest
        {
            return phase.fail("participant world identity mismatch");
        }
        let correlation = RequestCorrelation {
            key: RequestKey {
                world,
                verified_subject: phase.copy(&owner.subject_public_key)?,
                operation_domain: phase.copy(GAME_PARTICIPANT_REGISTER_DOMAIN)?,
                nonce_scope: phase.digest(
                    "participant-account",
                    &(
                        &owner.request.identity_namespace.service_id,
                        &owner.request.identity_namespace.environment_id,
                        &owner.request.account_id,
                    ),
                )?,
                request_id_or_nonce: phase.copy(&owner.request.client_operation_id)?,
            },
            payload_digest: phase.digest(
                oasis7_agent_api::game::GAME_PARTICIPANT_REGISTER_DOMAIN,
                &owner.request,
            )?,
        };
        phase.validation(&correlation, || correlation.validate())?;
        return Ok(phase.finish(correlation));
    }
    if let WorldServicePayloadV1::SafeAlternative(signed) = payload {
        phase.validation(&signed.request, || signed.request.validate())?;
        crate::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
            signed.request.signing_domain(),
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
            source,
        )?;
        phase.trace(&(
            &signed.request.world_id,
            &signed.request.genesis_digest,
            &world.world_id,
            &world.genesis_digest,
        ))?;
        if signed.request.world_id != world.world_id
            || signed.request.genesis_digest != world.genesis_digest
        {
            return phase.fail("safe alternative world identity mismatch");
        }
        let correlation = RequestCorrelation {
            key: RequestKey {
                world,
                verified_subject: phase.copy(&signed.subject_public_key)?,
                operation_domain: phase.copy(signed.request.signing_domain())?,
                nonce_scope: phase.prefixed("agent:", &signed.request.agent_id)?,
                request_id_or_nonce: phase.copy(&signed.request.client_operation_id)?,
            },
            payload_digest: phase.digest("intent", payload)?,
        };
        phase.validation(&correlation, || correlation.validate())?;
        return Ok(phase.finish(correlation));
    }
    if let WorldServicePayloadV1::GameExecutorControl(signed) = payload {
        phase.validation(&signed.request, || signed.request.validate())?;
        if let oasis7_agent_api::game::GameExecutorOperationV1::Bind {
            executor_public_key,
            ..
        } = &signed.request.operation
        {
            crate::legacy_task_results::strong_key_with_work_v1(executor_public_key, source)?;
        }
        phase.trace(&signed.subject_public_key)?;
        if signed.subject_public_key != signed.request.fence.owner_public_key {
            return phase.fail("executor control requires owner signature");
        }
        crate::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
            oasis7_agent_api::game::GAME_EXECUTOR_CONTROL_DOMAIN,
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
            source,
        )?;
        let f = &signed.request.fence;
        phase.trace(&(
            &f.world_id,
            &f.genesis_digest,
            &world.world_id,
            &world.genesis_digest,
        ))?;
        if f.world_id != world.world_id || f.genesis_digest != world.genesis_digest {
            return phase.fail("executor world identity mismatch");
        }
        let correlation = RequestCorrelation {
            key: RequestKey {
                world,
                verified_subject: phase.copy(&signed.subject_public_key)?,
                operation_domain: phase
                    .copy(oasis7_agent_api::game::GAME_EXECUTOR_CONTROL_DOMAIN)?,
                nonce_scope: phase.prefixed("task:", &f.task_id)?,
                request_id_or_nonce: phase.copy(&signed.request.client_operation_id)?,
            },
            payload_digest: phase.digest("intent", payload)?,
        };
        phase.validation(&correlation, || correlation.validate())?;
        return Ok(phase.finish(correlation));
    }
    if let WorldServicePayloadV1::GameGoalPublish(signed) = payload {
        phase.validation(&signed.request, || signed.request.validate())?;
        phase.trace(&signed.subject_public_key)?;
        if signed.subject_public_key != signed.request.owner_public_key {
            return phase.fail("goal publish signer differs from owner");
        }
        crate::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
            oasis7_agent_api::game::GAME_GOAL_PUBLISH_DOMAIN,
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
            source,
        )?;
        phase.trace(&(
            &signed.request.world_id,
            &signed.request.genesis_digest,
            &world.world_id,
            &world.genesis_digest,
        ))?;
        if signed.request.world_id != world.world_id
            || signed.request.genesis_digest != world.genesis_digest
        {
            return phase.fail("goal publish world identity mismatch");
        }
        let correlation = RequestCorrelation {
            key: RequestKey {
                world,
                verified_subject: phase.copy(&signed.subject_public_key)?,
                operation_domain: phase.copy(oasis7_agent_api::game::GAME_GOAL_PUBLISH_DOMAIN)?,
                nonce_scope: phase.prefixed("task:", &signed.request.task_id)?,
                request_id_or_nonce: phase.copy(&signed.request.client_operation_id)?,
            },
            payload_digest: phase.digest("intent", payload)?,
        };
        phase.validation(&correlation, || correlation.validate())?;
        return Ok(phase.finish(correlation));
    }
    if let WorldServicePayloadV1::GameTaskCreate(signed) = payload {
        verify_read_request_with_work_v1(
            oasis7_agent_api::game::GAME_TASK_CREATE_DOMAIN,
            signed,
            source,
        )?;
        phase.validation(&signed.request, || signed.request.validate())?;
        phase.trace(&(
            &signed.request.world_id,
            &signed.request.genesis_digest,
            &world.world_id,
            &world.genesis_digest,
        ))?;
        if signed.request.world_id != world.world_id
            || signed.request.genesis_digest != world.genesis_digest
        {
            return phase.fail("Game task world identity mismatch");
        }
        let correlation = RequestCorrelation {
            key: RequestKey {
                world,
                verified_subject: phase.copy(&signed.subject_public_key)?,
                operation_domain: phase.copy(oasis7_agent_api::game::GAME_TASK_CREATE_DOMAIN)?,
                nonce_scope: phase.prefixed("agent:", &signed.request.agent_id)?,
                request_id_or_nonce: phase.copy(&signed.request.client_operation_id)?,
            },
            payload_digest: phase.digest("intent", payload)?,
        };
        phase.validation(&correlation, || correlation.validate())?;
        return Ok(phase.finish(correlation));
    }
    let (subject, domain, scope, id) = match payload {
        WorldServicePayloadV1::GameStarterClaim(_)
        | WorldServicePayloadV1::GameParticipantRegister { .. }
        | WorldServicePayloadV1::SafeAlternative(_)
        | WorldServicePayloadV1::GameTaskCreate(_)
        | WorldServicePayloadV1::GameGoalPublish(_)
        | WorldServicePayloadV1::GameExecutorControl(_) => {
            unreachable!("handled above")
        }
        WorldServicePayloadV1::GameplayJson(_) | WorldServicePayloadV1::AgentChat(_) => {
            let paid = legacy.verify(payload, source)?;
            let identity = paid.identity();
            (
                phase.copy(&identity.0)?,
                identity.1,
                identity.2,
                phase.copy(&identity.3)?,
            )
        }
        WorldServicePayloadV1::Cognition(signed) => {
            verify_read_request_with_work_v1("cognition", signed, source)?;
            if let Some(fence) = &signed.request.task_fence {
                if signed.request.causal_proposal.is_some() {
                    return phase.fail("task cognition does not publish private reasoning");
                }
                phase.validation(fence, || fence.validate())?;
                crate::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
                    "cognition",
                    &signed.request,
                    &signed.subject_public_key,
                    &signed.signature_hex,
                    source,
                )?;
            }
            let request = &signed.request.request;
            phase.validation(request, || request.validate())?;
            phase.hold(4096)?;
            if let Err(error) = signed
                .request
                .response_artifact
                .validate_for_request_with_work_v1(request, source)
                .map_err(ServiceAuthorityWorkErrorV1::Work)?
            {
                return Err(phase.owned_error(error.to_string())?);
            }
            (
                phase.copy(&signed.subject_public_key)?,
                "cognition",
                "agent_request",
                phase.copy(&request.request_digest)?,
            )
        }
        WorldServicePayloadV1::FeedbackAck(signed) => {
            verify_read_request_with_work_v1("feedback_ack", signed, source)?;
            phase.validation(&signed.request, || signed.request.validate())?;
            (
                phase.copy(&signed.subject_public_key)?,
                "feedback_ack",
                "agent_feedback_ack",
                phase.copy(&signed.request.feedback_id)?,
            )
        }
        WorldServicePayloadV1::Delegation(signed) => {
            verify_read_request_with_work_v1("delegation", signed, source)?;
            (
                phase.copy(&signed.subject_public_key)?,
                "delegation",
                "owner_delegation",
                phase.decimal(signed.request.nonce)?,
            )
        }
        WorldServicePayloadV1::Scheduler(signed) => {
            verify_read_request_with_work_v1("scheduler", signed, source)?;
            if signed.request.task_prefix_binding_revision > 1
                || (signed.request.task_prefix_binding_revision != 0
                    && !matches!(
                        &signed.request.operation,
                        SchedulerOperationV1::TaskProviderPrefix { .. }
                    ))
            {
                return phase.fail("unsupported scheduler task prefix binding revision");
            }
            if let SchedulerOperationV1::TaskProviderPrefix { context }
            | SchedulerOperationV1::TaskProviderFailure { context, .. } =
                &signed.request.operation
            {
                phase.validation(context, || context.validate())?;
                if matches!(
                    &signed.request.operation,
                    SchedulerOperationV1::TaskProviderPrefix { .. }
                ) {
                    phase.validation(context, || {
                        context.validate_capture_deadline(
                            signed.request.captured_base_binding.base_tick,
                        )
                    })?;
                }

                if let SchedulerOperationV1::TaskProviderFailure { reason, .. } =
                    &signed.request.operation
                {
                    phase.trace(reason)?;
                }
                if let SchedulerOperationV1::TaskProviderFailure { reason, .. } =
                    &signed.request.operation
                    && !matches!(
                        reason.as_str(),
                        "provider_failed"
                            | "transport_exhausted"
                            | "context_expired"
                            | "executor_aborted"
                    )
                {
                    return phase.fail("task failure requires bounded public code");
                }
                crate::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
                    "scheduler",
                    &signed.request,
                    &signed.subject_public_key,
                    &signed.signature_hex,
                    source,
                )?;
            }
            phase.trace(&signed.request.request_id)?;
            if signed.request.request_id.trim().is_empty() {
                return phase.fail("missing scheduler request identity");
            }
            (
                phase.copy(&signed.subject_public_key)?,
                "scheduler",
                "agent_scheduler",
                phase.copy(&signed.request.request_id)?,
            )
        }
    };
    let correlation = RequestCorrelation {
        key: RequestKey {
            world,
            verified_subject: subject,
            operation_domain: phase.copy(domain)?,
            nonce_scope: phase.copy(scope)?,
            request_id_or_nonce: id,
        },
        payload_digest: phase.digest("intent", payload)?,
    };
    phase.validation(&correlation, || correlation.validate())?;
    Ok(phase.finish(correlation))
}

fn strong_key(key: &str) -> Result<(), String> {
    crate::legacy_task_results::strong_key_with_work_v1(
        key,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .map_err(crate::world_service::legacy_service_work_error_v1)
}

/// Native task profile: unsupported legacy browser variants are never accepted.
pub fn derive_correlation(
    world: WorldIdentity,
    payload: &WorldServicePayloadV1,
) -> Result<RequestCorrelation, String> {
    derive_correlation_with_legacy(world, payload, |_| {
        Err("unsupported native browser payload".into())
    })
}

/// Renewable admission proofs do not change the original business identity.
pub fn participant_business_digest(
    request: &oasis7_agent_api::game::GameParticipantRegisterV1,
) -> Result<String, String> {
    request_digest(
        oasis7_agent_api::game::GAME_PARTICIPANT_REGISTER_DOMAIN,
        request,
    )
}

#[cfg(test)]
mod participant_tests {
    use super::*;
    use oasis7_agent_api::game::*;
    #[test]
    fn participant_grant_refresh_preserves_business_correlation_and_signature_tampering_fails() {
        let key = hex::encode([72; 32]);
        let public = sign_read_request("fixture", (), &key)
            .unwrap()
            .subject_public_key;
        let r = GameParticipantRegisterV1 {
            api_rev: 1,
            client_operation_id: "original".into(),
            identity_namespace: GameIdentityNamespaceV1 {
                service_id: "identity".into(),
                environment_id: "private".into(),
                instance_epoch: DecimalU64(1),
            },
            account_id: "account".into(),
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            owner_public_key: public,
            owner_key_version: DecimalU64(1),
        };
        let owner = sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN, r.clone(), &key).unwrap();
        let a = GameParticipantAdmissionV1 {
            api_rev: 1,
            business_digest: participant_business_digest(&r).unwrap(),
            identity_namespace: r.identity_namespace.clone(),
            account_id: r.account_id.clone(),
            world_id: r.world_id.clone(),
            genesis_digest: r.genesis_digest.clone(),
            owner_public_key: r.owner_public_key.clone(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            policy_generation: DecimalU64(1),
            valid_from_tick: DecimalU64(0),
            valid_until_tick: DecimalU64(16),
        };
        let admission = sign_read_request(
            GAME_PARTICIPANT_ADMISSION_DOMAIN,
            a.clone(),
            &hex::encode([71; 32]),
        )
        .unwrap();
        let original: WorldServicePayloadV1 = WorldServicePayloadV1::GameParticipantRegister {
            owner: owner.clone(),
            admission,
        };
        let mut fresh = a;
        fresh.valid_from_tick = DecimalU64(10);
        fresh.valid_until_tick = DecimalU64(26);
        let fresh: WorldServicePayloadV1 = WorldServicePayloadV1::GameParticipantRegister {
            owner,
            admission: sign_read_request(
                GAME_PARTICIPANT_ADMISSION_DOMAIN,
                fresh,
                &hex::encode([71; 32]),
            )
            .unwrap(),
        };
        let world = WorldIdentity {
            world_id: r.world_id,
            genesis_digest: r.genesis_digest,
        };
        assert_eq!(
            derive_correlation(world.clone(), &original).unwrap(),
            derive_correlation(world.clone(), &fresh).unwrap()
        );
        let mut changed = original;
        if let WorldServicePayloadV1::GameParticipantRegister { owner, .. } = &mut changed {
            owner.request.account_id = "other".into();
        }
        assert!(derive_correlation(world, &changed).is_err());
    }
}

#[path = "world_service_correlation_trace_work.rs"]
pub(crate) mod trace_work;
#[path = "world_service_correlation_work.rs"]
pub(crate) mod work;
pub use work::PaidRequestCorrelationDataV1;

// The public historical callback can only select Legacy/no-meter execution.
// The paid concrete path has one fixed verifier and accepts no verdict callback.
mod browser_sealed {
    pub trait Sealed {}
}
pub(crate) trait FixedBrowserVerifierV1<P, A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>:
    browser_sealed::Sealed
{
    fn verify(
        self,
        payload: &P,
        source: &A,
    ) -> Result<work::PaidLegacyIdentityDataV1<A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>>;
}
struct LegacyBrowserCallbackV1<F> {
    verify: F,
}
impl<F> browser_sealed::Sealed for LegacyBrowserCallbackV1<F> {}
impl<P, F: FnOnce(&P) -> Result<LegacyIdentity, String>>
    FixedBrowserVerifierV1<P, oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1>
    for LegacyBrowserCallbackV1<F>
{
    fn verify(
        self,
        payload: &P,
        _: &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    ) -> Result<work::PaidLegacyIdentityDataV1<()>, ServiceAuthorityWorkErrorV1<()>> {
        (self.verify)(payload)
            .map(work::PaidLegacyIdentityDataV1::legacy)
            .map_err(|message| {
                ServiceAuthorityWorkErrorV1::Protocol(PaidServiceProtocolRefusalV1 {
                    message,
                    fee: None,
                })
            })
    }
}
pub(crate) struct ConcreteBrowserVerifierV1;
impl browser_sealed::Sealed for ConcreteBrowserVerifierV1 {}
impl<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>
    FixedBrowserVerifierV1<crate::legacy_world_service::WorldServicePayloadV1, A>
    for ConcreteBrowserVerifierV1
{
    fn verify(
        self,
        payload: &crate::legacy_world_service::WorldServicePayloadV1,
        source: &A,
    ) -> Result<work::PaidLegacyIdentityDataV1<A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>>
    {
        crate::legacy_world_service::correlation_work::derive_browser(payload, source)
    }
}
