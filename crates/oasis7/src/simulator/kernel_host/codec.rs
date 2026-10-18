//! Historical direct WorldKernel codec. Host extensions never enter native kernel state.
use super::*;
use oasis7_simulator::KernelState;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::VecDeque;

#[derive(Serialize, Deserialize)]
struct LegacyKernelState {
    time: WorldTime,
    config: WorldConfig,
    next_event_id: WorldEventId,
    next_action_id: ActionId,
    pending_actions: VecDeque<ActionEnvelope>,
    journal: Vec<WorldEvent>,
    model: WorldModel,
    #[serde(default)]
    chunk_runtime: ChunkRuntimeConfig,
    #[serde(default)]
    intel_ttl_ticks: WorldTime,
}
impl Serialize for WorldKernel {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let state = self.kernel.kernel_snapshot();
        LegacyKernelState {
            time: state.time,
            config: state.config,
            next_event_id: state.next_event_id,
            next_action_id: state.next_action_id,
            pending_actions: state.pending_actions.into(),
            journal: self.journal_snapshot().events,
            model: state.model,
            chunk_runtime: state.chunk_runtime,
            intel_ttl_ticks: state.intel_ttl_ticks,
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for WorldKernel {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let state = LegacyKernelState::deserialize(deserializer)?;
        let (native_events, runtime_events) = split_events(state.journal);
        let kernel = NativeKernel::from_state(KernelState {
            time: state.time,
            config: state.config,
            next_event_id: state.next_event_id,
            next_action_id: state.next_action_id,
            pending_actions: state.pending_actions,
            journal: native_events,
            model: state.model,
            chunk_runtime: state.chunk_runtime,
            intel_ttl_ticks: state.intel_ttl_ticks,
        });
        let mut host = WorldKernel::from_native(kernel);
        host.runtime_events = runtime_events;
        Ok(host)
    }
}
