use crate::geometry::space_distance_cm;

use super::super::ChunkState;
use super::super::chunking::ChunkCoord;
use super::super::init::{AsteroidFragmentInitConfig, WorldInitConfig, generate_chunk_fragments};
use super::WorldKernel;
use super::types::{
    ChunkGenerationCause, Observation, ObservedAgent, ObservedLocation,
    ObservedModuleArtifactRecord, ObservedModuleLifecycleState, ObservedModuleMarketState,
    ObservedPowerMarketState, ObservedSocialState, RejectReason, WorldEventKind,
};

impl WorldKernel {
    pub fn observe(&mut self, agent_id: &str) -> Result<Observation, RejectReason> {
        if self.intel_ttl_ticks > 0
            && let Some(cached) = self.intel_cache.get(agent_id)
            && cached.expires_at_tick > self.time
        {
            return Ok(cached.observation.clone());
        }

        let Some(agent) = self.model.agents.get(agent_id) else {
            return Err(RejectReason::AgentNotFound {
                agent_id: agent_id.to_string(),
            });
        };
        let agent_pos = agent.pos;
        self.ensure_chunk_generated_at(agent_pos, ChunkGenerationCause::Observe)?;

        let Some(agent) = self.model.agents.get(agent_id) else {
            return Err(RejectReason::AgentNotFound {
                agent_id: agent_id.to_string(),
            });
        };
        let visibility_range_cm = self.config.visibility_range_cm;
        let mut visible_agents = Vec::new();
        for (other_id, other) in &self.model.agents {
            if other_id == agent_id {
                continue;
            }
            let distance_cm = space_distance_cm(agent.pos, other.pos);
            if distance_cm <= visibility_range_cm {
                visible_agents.push(ObservedAgent {
                    agent_id: other_id.clone(),
                    location_id: other.location_id.clone(),
                    pos: other.pos,
                    distance_cm,
                });
            }
        }

        let mut visible_locations = Vec::new();
        for (location_id, location) in &self.model.locations {
            let distance_cm = space_distance_cm(agent.pos, location.pos);
            if distance_cm <= visibility_range_cm {
                visible_locations.push(ObservedLocation {
                    location_id: location_id.clone(),
                    name: location.name.clone(),
                    pos: location.pos,
                    profile: location.profile.clone(),
                    distance_cm,
                });
            }
        }

        let module_artifacts = self
            .model
            .module_artifacts
            .values()
            .map(|artifact| ObservedModuleArtifactRecord {
                wasm_hash: artifact.wasm_hash.clone(),
                publisher_agent_id: artifact.publisher_agent_id.clone(),
                module_id_hint: artifact.module_id_hint.clone(),
                bytes_len: artifact.wasm_bytes.len() as u64,
                deployed_at_tick: artifact.deployed_at_tick,
            })
            .collect::<Vec<_>>();

        let installed_modules = self
            .model
            .installed_modules
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let mut module_listings = self
            .model
            .module_artifact_listings
            .values()
            .cloned()
            .collect::<Vec<_>>();
        module_listings.sort_by_key(|listing| listing.order_id);

        let mut module_bids = self
            .model
            .module_artifact_bids
            .values()
            .flat_map(|bids| bids.iter().cloned())
            .collect::<Vec<_>>();
        module_bids.sort_by_key(|bid| bid.order_id);

        let mut power_open_orders = self.model.power_order_book.open_orders.clone();
        power_open_orders.sort_by_key(|order| order.order_id);

        let social_facts = self
            .model
            .social_facts
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let social_edges = self
            .model
            .social_edges
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let observation = Observation {
            time: self.time,
            agent_id: agent_id.to_string(),
            pos: agent.pos,
            self_resources: agent.resources.clone(),
            visibility_range_cm,
            visible_agents,
            visible_locations,
            module_lifecycle: ObservedModuleLifecycleState {
                artifacts: module_artifacts,
                installed_modules,
            },
            module_market: ObservedModuleMarketState {
                listings: module_listings,
                bids: module_bids,
            },
            power_market: ObservedPowerMarketState {
                next_order_id: self.model.power_order_book.next_order_id,
                open_orders: power_open_orders,
            },
            social_state: ObservedSocialState {
                facts: social_facts,
                edges: social_edges,
            },
        };
        if self.intel_ttl_ticks > 0 {
            self.intel_cache.insert(
                agent_id.to_string(),
                super::IntelCacheEntry {
                    observation: observation.clone(),
                    expires_at_tick: self.time.saturating_add(self.intel_ttl_ticks),
                },
            );
        }
        Ok(observation)
    }

    pub(super) fn ensure_chunk_generated_at(
        &mut self,
        pos: crate::geometry::GeoPos,
        cause: ChunkGenerationCause,
    ) -> Result<(), RejectReason> {
        let Some(coord) = super::super::chunking::chunk_coord_of(pos, &self.config.space) else {
            return Ok(());
        };
        if self
            .model
            .chunks
            .get(&coord)
            .is_some_and(|state| matches!(state, ChunkState::Generated | ChunkState::Exhausted))
        {
            return Ok(());
        }

        if !self.chunk_runtime.asteroid_fragment_enabled {
            self.model.chunks.insert(coord, ChunkState::Generated);
            self.model.chunk_resource_budgets.entry(coord).or_default();
            return Ok(());
        }

        let init = WorldInitConfig {
            seed: self.chunk_runtime.world_seed,
            asteroid_fragment: AsteroidFragmentInitConfig {
                enabled: self.chunk_runtime.asteroid_fragment_enabled,
                seed_offset: self.chunk_runtime.asteroid_fragment_seed_offset,
                min_fragment_spacing_cm: self.chunk_runtime.min_fragment_spacing_cm,
                bootstrap_chunks: Vec::new(),
            },
            ..WorldInitConfig::default()
        };

        let summary = generate_chunk_fragments(
            &mut self.model,
            &self.config,
            &init,
            coord,
            Some(self.chunk_runtime.asteroid_fragment_seed()),
        )
        .map_err(|_| reject_chunk_generation(coord))?;

        self.record_event(WorldEventKind::ChunkGenerated {
            coord: summary.coord,
            seed: summary.seed,
            fragment_count: summary.fragment_count,
            block_count: summary.block_count,
            chunk_budget: summary.chunk_budget,
            cause,
        });

        Ok(())
    }
}

fn reject_chunk_generation(coord: ChunkCoord) -> RejectReason {
    RejectReason::ChunkGenerationFailed {
        x: coord.x,
        y: coord.y,
        z: coord.z,
    }
}
