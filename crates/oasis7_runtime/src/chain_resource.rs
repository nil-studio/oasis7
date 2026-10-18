//! Authoritative state derivation of shared resource contracts.
use super::WorldState;
use oasis7_agent_api::world_geometry::{DEFAULT_CLOUD_DEPTH_CM, DEFAULT_CLOUD_HEIGHT_CM, DEFAULT_CLOUD_WIDTH_CM};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_agent_api::world_geometry::{ChunkCoord, chunk_coord_of, chunk_seed};
use oasis7_agent_api::world_resource::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
fn hash_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(value)?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn from_runtime_state(
    context: ChainResourceDerivationContext<'_>,
    world_config_hash: impl Into<String>,
    generation_algorithm_hash: impl Into<String>,
    state: &WorldState,
) -> ChainResourceManifest {
    let world_config_hash = world_config_hash.into();
    let generation_algorithm_hash = generation_algorithm_hash.into();
    let world_seed = runtime_world_seed(
        context.world_id,
        context.chain_id,
        world_config_hash.as_str(),
    );
    let mut generated_chunks = BTreeMap::new();
    let agents_by_chunk = runtime_agents_by_chunk(state);
    let starter_coord = ChunkCoord { x: 0, y: 0, z: 0 };
    let starter_budget = runtime_starter_chunk_budget(world_seed, starter_coord);
    generated_chunks.insert(
        chunk_key(starter_coord),
        runtime_chunk_resource_entry(
            context,
            world_seed,
            starter_coord,
            starter_budget,
            vec![runtime_starter_fragment_ref(
                context,
                world_seed,
                starter_coord,
            )],
        ),
    );
    for (chunk_key, agents) in agents_by_chunk {
        let coord = agents
            .first()
            .map(|(_, coord)| *coord)
            .unwrap_or(ChunkCoord { x: 0, y: 0, z: 0 });
        let previous_entry = generated_chunks.remove(&chunk_key);
        let mut fragment_refs = previous_entry
            .as_ref()
            .map(|entry| entry.fragment_refs.clone())
            .unwrap_or_default();
        let chunk_budget =
            previous_entry
                .as_ref()
                .map_or_else(ChunkResourceBudget::default, |entry| ChunkResourceBudget {
                    total_by_element_g: entry.total_by_element_g.clone(),
                    remaining_by_element_g: entry.remaining_by_element_g.clone(),
                });
        for (agent_id, _) in agents {
            fragment_refs.push(ChainFragmentResourceRef {
                fragment_id: format!("runtime-agent:{agent_id}"),
                location_id: format!("runtime-agent:{agent_id}"),
                profile_hash: hash_json(agent_id).unwrap_or_default(),
                budget_total_hash: hash_json(&chunk_budget.total_by_element_g).unwrap_or_default(),
                budget_remaining_hash: hash_json(&chunk_budget.remaining_by_element_g)
                    .unwrap_or_default(),
            });
        }
        generated_chunks.insert(
            chunk_key,
            runtime_chunk_resource_entry(context, world_seed, coord, chunk_budget, fragment_refs),
        );
    }
    let mut manifest = ChainResourceManifest {
        schema_version: CHAIN_RESOURCE_MANIFEST_SCHEMA_V1.to_string(),
        world_id: context.world_id.to_string(),
        chain_id: context.chain_id.to_string(),
        genesis_ref: context.genesis_ref.map(ToOwned::to_owned),
        world_seed,
        chunk_generation_schema_version: CHUNK_GENERATION_SCHEMA_V1.to_string(),
        world_config_hash,
        generation_algorithm_id: "runtime_agent_chunk_v1".to_string(),
        generation_algorithm_hash,
        created_at_height: context.created_at_height,
        created_at_block_hash: context.commit_block_hash.map(ToOwned::to_owned),
        manifest_height: context.manifest_height,
        manifest_hash: String::new(),
        resource_balances: aggregate_runtime_resource_balances(state),
        material_ledgers: state.material_ledgers.clone(),
        generated_chunks,
    };
    manifest.manifest_hash = manifest.canonical_hash();
    manifest
}

pub fn latest_from_runtime_manifest(
    context: ChainResourceDerivationContext<'_>,
    manifest: &ChainResourceManifest,
) -> ChainResourceDelta {
    let mut entries = Vec::new();
    if context.manifest_height == context.created_at_height {
        for chunk in manifest.generated_chunks.values() {
            if chunk.total_by_element_g.is_empty() && chunk.remaining_by_element_g.is_empty() {
                continue;
            }
            for (element, total_delta_g) in &chunk.total_by_element_g {
                let remaining_delta_g = *chunk.remaining_by_element_g.get(element).unwrap_or(&0);
                entries.push(ChainResourceDeltaEntry::ChunkResource {
                    coord: chunk.coord,
                    element: *element,
                    total_delta_g: *total_delta_g,
                    remaining_delta_g,
                    resulting_remaining_g: remaining_delta_g,
                    remaining_after_hash: chunk.chunk_budget_remaining_hash.clone(),
                    chunk_remaining_after_hash: chunk.chunk_budget_remaining_hash.clone(),
                });
            }
        }
    }
    ChainResourceDelta {
        schema_version: CHAIN_RESOURCE_DELTA_SCHEMA_V1.to_string(),
        world_id: context.world_id.to_string(),
        chain_id: context.chain_id.to_string(),
        delta_id: format!(
            "resource-delta-{}-{}",
            context.manifest_height, context.tick
        ),
        action_id: None,
        event_id: None,
        ordering_key: ChainResourceOrderingKey {
            height: context.manifest_height,
            event_sequence: 0,
            action_sequence: 0,
        },
        base_manifest_hash: if entries.is_empty() {
            manifest.manifest_hash.clone()
        } else {
            ChainResourceManifest::empty_at_height(
                manifest.world_id.clone(),
                manifest.world_seed,
                manifest.created_at_height,
            )
            .canonical_hash()
        },
        resulting_manifest_hash: manifest.manifest_hash.clone(),
        block_height: context.manifest_height,
        commit_block_hash: context.commit_block_hash.map(ToOwned::to_owned),
        tick: context.tick,
        source: ChainResourceDeltaSource::Genesis,
        replay_status: ChainResourceReplayStatus::Committed,
        entries,
    }
}

fn aggregate_runtime_resource_balances(state: &WorldState) -> BTreeMap<ResourceKind, i64> {
    let mut balances = state.resources.clone();
    for agent in state.agents.values() {
        for (kind, amount) in &agent.state.resources.amounts {
            let entry = balances.entry(*kind).or_insert(0);
            *entry = (*entry).saturating_add(*amount);
        }
    }
    balances
}

fn runtime_agents_by_chunk(state: &WorldState) -> BTreeMap<String, Vec<(&String, ChunkCoord)>> {
    let mut out: BTreeMap<String, Vec<(&String, ChunkCoord)>> = BTreeMap::new();
    let space = runtime_chunk_space_for_state(state);
    for (agent_id, agent) in &state.agents {
        let coord =
            chunk_coord_of(agent.state.pos, space).unwrap_or(ChunkCoord { x: 0, y: 0, z: 0 });
        out.entry(chunk_key(coord))
            .or_default()
            .push((agent_id, coord));
    }
    out
}

fn runtime_starter_chunk_budget(world_seed: u64, coord: ChunkCoord) -> ChunkResourceBudget {
    let mut budget = ChunkResourceBudget::default();
    for (element, base_g, spread_g) in [
        (FragmentElementKind::Oxygen, 80_000_i64, 25_000_i64),
        (FragmentElementKind::Silicon, 45_000_i64, 18_000_i64),
        (FragmentElementKind::Iron, 30_000_i64, 15_000_i64),
        (FragmentElementKind::Nickel, 8_000_i64, 6_000_i64),
    ] {
        let salt = format!(
            "runtime_starter_chunk_resource_v1:{world_seed}:{}:{}:{}:{element:?}",
            coord.x, coord.y, coord.z
        );
        let hash = sha256_hex(salt.as_bytes());
        let sample = u64::from_str_radix(hash.get(0..8).unwrap_or("0"), 16).unwrap_or(0);
        let amount = base_g.saturating_add((sample % spread_g as u64) as i64);
        budget.total_by_element_g.insert(element, amount);
        budget.remaining_by_element_g.insert(element, amount);
    }
    budget
}

fn runtime_starter_fragment_ref(
    context: ChainResourceDerivationContext<'_>,
    world_seed: u64,
    coord: ChunkCoord,
) -> ChainFragmentResourceRef {
    let chunk_budget = runtime_starter_chunk_budget(world_seed, coord);
    ChainFragmentResourceRef {
        fragment_id: format!(
            "runtime-starter-fragment:{}:{}:{}",
            coord.x, coord.y, coord.z
        ),
        location_id: format!(
            "runtime-starter-location:{}:{}:{}",
            coord.x, coord.y, coord.z
        ),
        profile_hash: hash_json(&(
            "runtime_starter_chunk_fragment_v1",
            context.world_id,
            context.chain_id,
            world_seed,
            coord,
        ))
        .unwrap_or_default(),
        budget_total_hash: hash_json(&chunk_budget.total_by_element_g).unwrap_or_default(),
        budget_remaining_hash: hash_json(&chunk_budget.remaining_by_element_g).unwrap_or_default(),
    }
}

fn runtime_chunk_resource_entry(
    context: ChainResourceDerivationContext<'_>,
    world_seed: u64,
    coord: ChunkCoord,
    chunk_budget: ChunkResourceBudget,
    fragment_refs: Vec<ChainFragmentResourceRef>,
) -> ChainChunkResourceManifestEntry {
    let mut entry = ChainChunkResourceManifestEntry {
        schema_version: CHAIN_RESOURCE_MANIFEST_SCHEMA_V1.to_string(),
        world_id: context.world_id.to_string(),
        chain_id: context.chain_id.to_string(),
        world_seed,
        chunk_generation_schema_version: CHUNK_GENERATION_SCHEMA_V1.to_string(),
        coord,
        seed: chunk_seed(world_seed, coord),
        chunk_status: ChainChunkResourceStatus::Committed,
        fragment_count: fragment_refs.len() as u32,
        block_count: fragment_refs.len() as u32,
        fragment_refs,
        chunk_budget_total_hash: hash_json(&chunk_budget.total_by_element_g).unwrap_or_default(),
        chunk_budget_remaining_hash: hash_json(&chunk_budget.remaining_by_element_g)
            .unwrap_or_default(),
        total_by_element_g: chunk_budget.total_by_element_g,
        remaining_by_element_g: chunk_budget.remaining_by_element_g,
        commit_ref: ChainResourceCommitRef {
            height: context.manifest_height,
            block_hash: context.commit_block_hash.map(ToOwned::to_owned),
            event_id: None,
            action_id: None,
        },
        manifest_hash: String::new(),
    };
    entry.manifest_hash = entry.canonical_hash();
    entry
}

fn runtime_chunk_space_for_state(state: &WorldState) -> (i64, i64, i64) {
    let mut space = (
        DEFAULT_CLOUD_WIDTH_CM,
        DEFAULT_CLOUD_DEPTH_CM,
        DEFAULT_CLOUD_HEIGHT_CM,
    );
    for agent in state.agents.values() {
        space.0 = space
            .0
            .max(agent.state.pos.x_cm.saturating_add(DEFAULT_CLOUD_WIDTH_CM));
        space.1 = space
            .1
            .max(agent.state.pos.y_cm.saturating_add(DEFAULT_CLOUD_WIDTH_CM));
        space.2 = space
            .2
            .max(agent.state.pos.z_cm.saturating_add(DEFAULT_CLOUD_WIDTH_CM));
    }
    space
}

fn runtime_world_seed(world_id: &str, chain_id: &str, world_config_hash: &str) -> u64 {
    let hash = sha256_hex(format!("{world_id}:{chain_id}:{world_config_hash}").as_bytes());
    u64::from_str_radix(hash.get(0..16).unwrap_or("1"), 16)
        .unwrap_or(1)
        .max(1)
}
