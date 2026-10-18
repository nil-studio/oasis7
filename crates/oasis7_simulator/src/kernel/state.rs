use super::super::KernelEvent;
use super::super::types::{ActionEnvelope, ActionId, WorldEventId, WorldTime};
use super::super::world_model::{WorldConfig, WorldModel};
use super::{ChunkRuntimeConfig, WorldKernel};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Complete serializable kernel state, excluding process-local capabilities and caches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelState {
    pub time: WorldTime,
    pub config: WorldConfig,
    pub next_event_id: WorldEventId,
    pub next_action_id: ActionId,
    pub pending_actions: VecDeque<ActionEnvelope>,
    pub journal: Vec<KernelEvent>,
    pub model: WorldModel,
    #[serde(default)]
    pub chunk_runtime: ChunkRuntimeConfig,
    #[serde(default)]
    pub intel_ttl_ticks: WorldTime,
}

impl WorldKernel {
    pub fn state(&self) -> KernelState {
        KernelState {
            time: self.time,
            config: self.config.clone(),
            next_event_id: self.next_event_id,
            next_action_id: self.next_action_id,
            pending_actions: self.pending_actions.clone(),
            journal: self.journal.clone(),
            model: self.model.clone(),
            chunk_runtime: self.chunk_runtime.clone(),
            intel_ttl_ticks: self.intel_ttl_ticks,
        }
    }
    pub fn into_state(self) -> KernelState {
        KernelState {
            time: self.time,
            config: self.config,
            next_event_id: self.next_event_id,
            next_action_id: self.next_action_id,
            pending_actions: self.pending_actions,
            journal: self.journal,
            model: self.model,
            chunk_runtime: self.chunk_runtime,
            intel_ttl_ticks: self.intel_ttl_ticks,
        }
    }
    /// Restore the direct process-state codec, preserving its historical lack of normalization.
    /// Validated archive restore should use `from_kernel_snapshot` instead.
    pub fn from_state(state: KernelState) -> Self {
        Self {
            time: state.time,
            config: state.config,
            next_event_id: state.next_event_id,
            next_action_id: state.next_action_id,
            pending_actions: state.pending_actions,
            journal: state.journal,
            model: state.model,
            chunk_runtime: state.chunk_runtime,
            intel_ttl_ticks: state.intel_ttl_ticks,
            intel_cache: Default::default(),
            last_intent_batch_report: None,
            rule_hooks: Default::default(),
            source_compiler: Default::default(),
        }
    }
}
