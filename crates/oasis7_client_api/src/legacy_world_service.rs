//! Complete concrete legacy contract. Values confer no Runtime write authority.
pub type SchedulerOperationV1 = crate::world_service_wire::SchedulerOperationV1<
    oasis7_agent_api::ContinuousAgentRequestContextV1,
    oasis7_agent_api::legacy_economy::CognitionLeaseRequestV1,
    oasis7_agent_api::legacy_scheduler::CognitionContinuationProposalV1,
    oasis7_agent_api::legacy_scheduler::ContinuationStatusV1,
    oasis7_agent_api::legacy_scheduler::CognitionContinuationResumeRequestV1,
    oasis7_agent_api::legacy_scheduler::CognitionWakeDispositionV1,
    oasis7_agent_api::legacy_scheduler::CognitionContextDigestsV1,
>;
pub type SchedulerIntentV1 = crate::world_service_wire::SchedulerIntentV1<SchedulerOperationV1>;
pub type WorldServicePayloadV1 = crate::world_service_wire::WorldServicePayloadV1<
    oasis7_proto::viewer::AgentChatRequest,
    SchedulerIntentV1,
>;
pub type AuthenticatedLookup = crate::world_service_wire::AuthenticatedLookup<
    oasis7_proto::viewer::AgentChatRequest,
    SchedulerIntentV1,
>;
pub type CanonicalIntentResultV1 = crate::world_service_wire::CanonicalIntentResultV1<
    oasis7_proto::viewer::AgentChatRequest,
    SchedulerIntentV1,
>;

use crate::world_service::*;
/// Derives identity from verified signed material; server additionally checks
/// canonical ownership/delegation and expected world before accepting it.
pub fn derive_correlation(
    world: WorldIdentity,
    payload: &WorldServicePayloadV1,
) -> Result<RequestCorrelation, String> {
    derive_correlation_with_work_v1(
        world,
        payload,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .map(crate::world_service_correlation::work::PaidRequestCorrelationDataV1::into_legacy)
    .map_err(crate::world_service::legacy_service_work_error_v1)
}

/// Validate the signed request identity and its pure receipt association.
pub fn validate_canonical_result(key: &str, value: &serde_json::Value) -> Result<(), String> {
    let result: CanonicalIntentResultV1 =
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    validate_canonical_result_fields(key, &result)
}

/// Validate an already decoded canonical result using the original identity
/// and receipt rules. Decoding and source authentication remain the caller's
/// responsibility; this function does not confer accepted-result authority.
pub fn validate_canonical_result_fields(
    key: &str,
    result: &CanonicalIntentResultV1,
) -> Result<(), String> {
    validate_canonical_result_fields_with_work_v1(
        key,
        result,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .map_err(crate::world_service::legacy_service_work_error_v1)
}

#[path = "legacy_world_service_correlation_work.rs"]
pub(crate) mod correlation_work;
pub use crate::world_service_correlation::PaidRequestCorrelationDataV1;
pub fn derive_correlation_with_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
    world: WorldIdentity,
    payload: &WorldServicePayloadV1,
    source: &A,
) -> Result<
    PaidRequestCorrelationDataV1<A::Charge>,
    crate::world_service::ServiceAuthorityWorkErrorV1<A::Charge>,
> {
    crate::world_service_correlation::derive_correlation_with_legacy_work_v1(
        world,
        payload,
        crate::world_service_correlation::ConcreteBrowserVerifierV1,
        source,
    )
}

/// The original identity and receipt association, over already-decoded fields.
/// All graph/source authentication remains with the caller's actual owner.
pub fn validate_canonical_result_fields_with_work_v1<
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
>(
    key: &str,
    result: &CanonicalIntentResultV1,
    source: &A,
) -> Result<(), crate::world_service::ServiceAuthorityWorkErrorV1<A::Charge>> {
    use crate::world_service::ServiceAuthorityWorkErrorV1 as Error;
    use crate::world_service_correlation::work::Phase;
    let mut phase = Phase::new(source)?;
    phase.validation(&result.request.correlation, || result.request.validate())?;
    let world = &result.request.correlation.key.world;
    let world = WorldIdentity {
        world_id: phase.copy(&world.world_id)?,
        genesis_digest: phase.copy(&world.genesis_digest)?,
    };
    let derived = derive_correlation_with_work_v1(world, &result.request.signed_payload, source)?;
    phase.trace(derived.correlation())?;
    phase.trace(&result.request.correlation)?;
    if derived.correlation() != &result.request.correlation
        || phase.digest("request-key", &result.request.correlation.key)? != key
        || result.committed_height == 0
    {
        return phase.fail("invalid canonical intent result identity");
    }
    crate::legacy_task_results::validate_result_with_work_v1(result, source)?;
    source.charge_work(0, 0).map_err(Error::Work)?;
    Ok(())
}

#[cfg(test)]
#[path = "legacy_world_service_work_tests.rs"]
mod work_tests;
