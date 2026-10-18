use super::*;

impl ViewerRuntimeLiveServer {
    /// Keep an async provider turn's captured Runtime base stable until its
    /// lease is committed or terminalized.
    pub(super) fn should_advance_compatibility_tick(&self, iteration_logical_time: u64) -> bool {
        self.world.state().time == iteration_logical_time
            && !self
                .llm_sidecar
                .provider_cognition_lease_in_flight(&self.world)
    }
}
