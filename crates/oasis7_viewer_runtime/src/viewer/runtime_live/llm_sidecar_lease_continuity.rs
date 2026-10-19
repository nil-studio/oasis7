use super::RuntimeLlmSidecar;
use crate::runtime::{CognitionLeaseStatusV1, World as RuntimeWorld};

impl RuntimeLlmSidecar {
    /// Report whether a sidecar lease is still the exact reserved Runtime
    /// record for an in-flight provider turn. A lease carried only by the
    /// sidecar is insufficient: after restart or binding rotation, normal
    /// recovery must continue to fail closed.
    pub(in crate::viewer::runtime_live) fn provider_cognition_lease_in_flight(
        &self,
        world: &RuntimeWorld,
    ) -> bool {
        !self
            .provider_cognition_lease_agents_in_flight(world)
            .is_empty()
    }

    pub(in crate::viewer::runtime_live) fn provider_cognition_lease_agents_in_flight(
        &self,
        world: &RuntimeWorld,
    ) -> Vec<String> {
        let Ok(economy) = world.cognition_economy() else {
            return Vec::new();
        };
        self.provider_cognition_leases
            .iter()
            .filter_map(|(agent_id, lease)| {
                (lease.status == CognitionLeaseStatusV1::Reserved
                    && economy.leases.get(lease.lease_id.as_str()) == Some(lease))
                .then_some(agent_id.clone())
            })
            .collect()
    }
}
