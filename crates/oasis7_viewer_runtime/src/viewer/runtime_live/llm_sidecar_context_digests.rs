use oasis7_simulator::Observation;

pub(super) fn provider_wait_precondition_digest(observation: &Observation) -> String {
    crate::simulator::h_v1("oasis7.cognition.provider-wait-precondition.v1", &{
        let mut stable = observation.clone();
        stable.time = 0;
        stable
    })
    .to_string()
}

pub(super) fn provider_policy_context_digest(
    request: &crate::simulator::ContinuousAgentRequestContextV1,
) -> String {
    let policy_hash = request
        .base_decision_request
        .capability_catalog
        .as_ref()
        .map(|catalog| catalog.policy_hash.as_str())
        .unwrap_or("missing-provider-policy");
    crate::simulator::h_v1("oasis7.cognition.provider-policy.v1", &policy_hash).to_string()
}
