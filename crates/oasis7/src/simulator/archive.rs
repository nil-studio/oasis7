//! Lossless adapters for the historical host-owned combined archives.
use super::persist::{PlayerGameplaySnapshot, WorldSnapshot};
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::{Snapshot as RuntimeSnapshot, WorldEvent as RuntimeWorldEvent};
use oasis7_agent_api::world_resource::{ChainResourceDelta, ChainResourceManifest};
use oasis7_simulator::WorldEventKind;
use oasis7_simulator::{KernelEvent, KernelSnapshot};
use oasis7_simulator::{WorldEventId, WorldTime};
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use serde_json::Value as RuntimeSnapshot;
#[cfg(target_arch = "wasm32")]
use serde_json::Value as RuntimeWorldEvent;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldEvent {
    pub id: WorldEventId,
    pub time: WorldTime,
    pub kind: WorldEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_event: Option<RuntimeWorldEvent>,
}

/// Host-owned fields that must accompany a kernel snapshot on archive rewrite.
/// This is an in-process adapter, not a replacement wire format.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldSnapshotExtensions {
    pub runtime_snapshot: Option<RuntimeSnapshot>,
    pub player_gameplay: Option<PlayerGameplaySnapshot>,
    pub chain_resource_manifest: ChainResourceManifest,
    pub latest_chain_resource_delta: ChainResourceDelta,
}

impl WorldSnapshot {
    pub fn into_kernel_parts(self) -> (KernelSnapshot, WorldSnapshotExtensions) {
        let kernel = KernelSnapshot {
            version: self.version,
            chunk_generation_schema_version: self.chunk_generation_schema_version,
            time: self.time,
            config: self.config,
            model: self.model,
            chunk_runtime: self.chunk_runtime,
            intel_ttl_ticks: self.intel_ttl_ticks,
            next_event_id: self.next_event_id,
            next_action_id: self.next_action_id,
            pending_actions: self.pending_actions,
            journal_len: self.journal_len,
        };
        (
            kernel,
            WorldSnapshotExtensions {
                runtime_snapshot: self.runtime_snapshot,
                player_gameplay: self.player_gameplay,
                chain_resource_manifest: self.chain_resource_manifest,
                latest_chain_resource_delta: self.latest_chain_resource_delta,
            },
        )
    }

    pub fn from_kernel_parts(kernel: KernelSnapshot, extensions: WorldSnapshotExtensions) -> Self {
        Self {
            version: kernel.version,
            chunk_generation_schema_version: kernel.chunk_generation_schema_version,
            time: kernel.time,
            config: kernel.config,
            model: kernel.model,
            runtime_snapshot: extensions.runtime_snapshot,
            player_gameplay: extensions.player_gameplay,
            chain_resource_manifest: extensions.chain_resource_manifest,
            latest_chain_resource_delta: extensions.latest_chain_resource_delta,
            chunk_runtime: kernel.chunk_runtime,
            intel_ttl_ticks: kernel.intel_ttl_ticks,
            next_event_id: kernel.next_event_id,
            next_action_id: kernel.next_action_id,
            pending_actions: kernel.pending_actions,
            journal_len: kernel.journal_len,
        }
    }
}

impl WorldEvent {
    pub fn into_kernel_parts(self) -> (KernelEvent, Option<RuntimeWorldEvent>) {
        (
            KernelEvent {
                id: self.id,
                time: self.time,
                kind: self.kind,
            },
            self.runtime_event,
        )
    }

    pub fn from_kernel_parts(
        kernel: KernelEvent,
        runtime_event: Option<RuntimeWorldEvent>,
    ) -> Self {
        Self {
            id: kernel.id,
            time: kernel.time,
            kind: kernel.kind,
            runtime_event,
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
