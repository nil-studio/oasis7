//! Simulator state derivation of shared resource contracts.
use super::{
    ChunkGenerationCause, ChunkRuntimeConfig, ChunkState, WorldConfig, WorldEvent, WorldEventKind,
    WorldModel, chunk_coord_of, chunk_seed,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_agent_api::world_geometry::ChunkCoord;
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

pub fn from_simulator_state(
    context: ChainResourceDerivationContext<'_>,
    model: &WorldModel,
    config: &WorldConfig,
    chunk_runtime: &ChunkRuntimeConfig,
    journal: &[WorldEvent],
) -> ChainResourceManifest {
    let world_seed = chunk_runtime.world_seed;
    let world_config_hash = hash_json(config).unwrap_or_default();
    let generation_algorithm_hash = hash_json(&(config, chunk_runtime)).unwrap_or_default();
    let mut generated_chunks = BTreeMap::new();
    for (coord, state) in &model.chunks {
        if !matches!(state, ChunkState::Generated | ChunkState::Exhausted) {
            continue;
        }
        let default_chunk_budget = Default::default();
        let chunk_budget = model
            .chunk_resource_budgets
            .get(coord)
            .unwrap_or(&default_chunk_budget);
        let mut fragment_refs = Vec::new();
        let mut block_count = 0_u32;
        for location in model.locations.values() {
            if chunk_coord_of(location.pos, &config.space) != Some(*coord) {
                continue;
            }
            let Some(fragment_profile) = location.fragment_profile.as_ref() else {
                continue;
            };
            block_count = block_count.saturating_add(fragment_profile.blocks.blocks.len() as u32);
            fragment_refs.push(ChainFragmentResourceRef {
                fragment_id: location.id.clone(),
                location_id: location.id.clone(),
                profile_hash: hash_json(fragment_profile).unwrap_or_default(),
                budget_total_hash: location
                    .fragment_budget
                    .as_ref()
                    .map(|budget| hash_json(&budget.total_by_element_g).unwrap_or_default())
                    .unwrap_or_default(),
                budget_remaining_hash: location
                    .fragment_budget
                    .as_ref()
                    .map(|budget| hash_json(&budget.remaining_by_element_g).unwrap_or_default())
                    .unwrap_or_default(),
            });
        }
        let commit_ref = latest_chunk_commit_ref(
            journal,
            *coord,
            context.manifest_height,
            context.commit_block_hash,
        );
        let chunk_seed_base = if chunk_runtime.asteroid_fragment_enabled {
            chunk_runtime.asteroid_fragment_seed()
        } else {
            world_seed
        };
        let mut entry = ChainChunkResourceManifestEntry {
            schema_version: CHAIN_RESOURCE_MANIFEST_SCHEMA_V1.to_string(),
            world_id: context.world_id.to_string(),
            chain_id: context.chain_id.to_string(),
            world_seed,
            chunk_generation_schema_version: CHUNK_GENERATION_SCHEMA_V1.to_string(),
            coord: *coord,
            seed: chunk_seed(chunk_seed_base, *coord),
            chunk_status: if matches!(state, ChunkState::Exhausted) {
                ChainChunkResourceStatus::Exhausted
            } else {
                ChainChunkResourceStatus::Committed
            },
            fragment_count: fragment_refs.len() as u32,
            block_count,
            fragment_refs,
            chunk_budget_total_hash: hash_json(&chunk_budget.total_by_element_g)
                .unwrap_or_default(),
            chunk_budget_remaining_hash: hash_json(&chunk_budget.remaining_by_element_g)
                .unwrap_or_default(),
            total_by_element_g: chunk_budget.total_by_element_g.clone(),
            remaining_by_element_g: chunk_budget.remaining_by_element_g.clone(),
            commit_ref,
            manifest_hash: String::new(),
        };
        entry.manifest_hash = entry.canonical_hash();
        generated_chunks.insert(chunk_key(*coord), entry);
    }
    let mut manifest = ChainResourceManifest {
        schema_version: CHAIN_RESOURCE_MANIFEST_SCHEMA_V1.to_string(),
        world_id: context.world_id.to_string(),
        chain_id: context.chain_id.to_string(),
        genesis_ref: context.genesis_ref.map(ToOwned::to_owned),
        world_seed,
        chunk_generation_schema_version: CHUNK_GENERATION_SCHEMA_V1.to_string(),
        world_config_hash,
        generation_algorithm_id: "simulator_chunk_fragment_v1".to_string(),
        generation_algorithm_hash,
        created_at_height: context.created_at_height,
        created_at_block_hash: context.commit_block_hash.map(ToOwned::to_owned),
        manifest_height: context.manifest_height,
        manifest_hash: String::new(),
        resource_balances: aggregate_agent_resource_balances(model),
        material_ledgers: BTreeMap::new(),
        generated_chunks,
    };
    manifest.manifest_hash = manifest.canonical_hash();
    manifest
}

pub fn latest_from_simulator_journal(
    context: ChainResourceDerivationContext<'_>,
    manifest: &ChainResourceManifest,
    journal: &[WorldEvent],
) -> ChainResourceDelta {
    for event in journal.iter().rev() {
        if let Some(delta) = delta_from_simulator_event(context, manifest, event) {
            return delta;
        }
    }
    let mut delta = ChainResourceDelta::empty_for_manifest(
        context.world_id,
        manifest.manifest_hash.clone(),
        context.manifest_height,
        context.tick,
    );
    delta.chain_id = context.chain_id.to_string();
    delta.commit_block_hash = context.commit_block_hash.map(ToOwned::to_owned);
    delta
}

fn latest_chunk_commit_ref(
    journal: &[WorldEvent],
    coord: ChunkCoord,
    height: u64,
    block_hash: Option<&str>,
) -> ChainResourceCommitRef {
    let event_id = journal.iter().rev().find_map(|event| match &event.kind {
        WorldEventKind::ChunkGenerated {
            coord: event_coord, ..
        } if *event_coord == coord => Some(event.id),
        _ => None,
    });
    ChainResourceCommitRef {
        height,
        block_hash: block_hash.map(ToOwned::to_owned),
        event_id,
        action_id: None,
    }
}

fn aggregate_agent_resource_balances(model: &WorldModel) -> BTreeMap<ResourceKind, i64> {
    let mut balances: BTreeMap<ResourceKind, i64> = BTreeMap::new();
    for agent in model.agents.values() {
        for (kind, amount) in &agent.resources.amounts {
            let entry = balances.entry(*kind).or_insert(0);
            *entry = (*entry).saturating_add(*amount);
        }
    }
    balances
}

pub(crate) fn delta_from_simulator_event(
    context: ChainResourceDerivationContext<'_>,
    manifest: &ChainResourceManifest,
    event: &WorldEvent,
) -> Option<ChainResourceDelta> {
    let mut entries = Vec::new();
    let source = match &event.kind {
        WorldEventKind::ChunkGenerated {
            coord,
            chunk_budget,
            cause,
            ..
        } => {
            for (element, total) in &chunk_budget.total_by_element_g {
                let remaining = chunk_budget.get_remaining(*element);
                entries.push(ChainResourceDeltaEntry::ChunkResource {
                    coord: *coord,
                    element: *element,
                    total_delta_g: *total,
                    remaining_delta_g: remaining,
                    resulting_remaining_g: remaining,
                    remaining_after_hash: hash_json(&chunk_budget.remaining_by_element_g)
                        .unwrap_or_default(),
                    chunk_remaining_after_hash: hash_json(&chunk_budget.remaining_by_element_g)
                        .unwrap_or_default(),
                });
            }
            match cause {
                ChunkGenerationCause::Init => ChainResourceDeltaSource::Genesis,
                ChunkGenerationCause::Observe | ChunkGenerationCause::Action => {
                    ChainResourceDeltaSource::ChunkGenerated
                }
            }
        }
        WorldEventKind::FragmentsReplenished { .. } | WorldEventKind::CompoundMined { .. } => {
            return None;
        }
        _ => return None,
    };
    if entries.is_empty() {
        return None;
    }
    Some(ChainResourceDelta {
        schema_version: CHAIN_RESOURCE_DELTA_SCHEMA_V1.to_string(),
        world_id: context.world_id.to_string(),
        chain_id: context.chain_id.to_string(),
        delta_id: format!(
            "resource-delta-{}-{}-{}",
            context.manifest_height, event.id, event.time
        ),
        action_id: None,
        event_id: Some(event.id),
        ordering_key: ChainResourceOrderingKey {
            height: context.manifest_height,
            event_sequence: event.id,
            action_sequence: 0,
        },
        base_manifest_hash: chunk_generation_base_manifest_hash(manifest, event),
        resulting_manifest_hash: manifest.manifest_hash.clone(),
        block_height: context.manifest_height,
        commit_block_hash: context.commit_block_hash.map(ToOwned::to_owned),
        tick: event.time,
        source,
        replay_status: ChainResourceReplayStatus::Committed,
        entries,
    })
}

fn chunk_generation_base_manifest_hash(
    manifest: &ChainResourceManifest,
    event: &WorldEvent,
) -> String {
    let WorldEventKind::ChunkGenerated { coord, cause, .. } = &event.kind else {
        return manifest.manifest_hash.clone();
    };
    if matches!(cause, ChunkGenerationCause::Init) {
        return ChainResourceManifest::empty_at_height(
            manifest.world_id.clone(),
            manifest.world_seed,
            manifest.created_at_height,
        )
        .canonical_hash();
    }
    let mut base = manifest.clone();
    base.generated_chunks.remove(&chunk_key(*coord));
    base.manifest_height = manifest.manifest_height.saturating_sub(1);
    base.refresh_hashes();
    base.manifest_hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn chain_resource_manifest_hash_is_stable_across_roundtrip() {
        let mut manifest = ChainResourceManifest::empty_at_height("world-a", 42, 7);
        manifest.chain_id = "chain-a".to_string();
        manifest.world_config_hash = "world-config-hash".to_string();
        manifest.generation_algorithm_hash = "algorithm-hash".to_string();
        manifest.manifest_hash = manifest.canonical_hash();

        let encoded = serde_json::to_string(&manifest).expect("encode manifest");
        let decoded: ChainResourceManifest =
            serde_json::from_str(encoded.as_str()).expect("decode manifest");

        assert_eq!(decoded.schema_version, CHAIN_RESOURCE_MANIFEST_SCHEMA_V1);
        assert_eq!(
            decoded.chunk_generation_schema_version,
            CHUNK_GENERATION_SCHEMA_V1
        );
        assert_eq!(decoded.manifest_hash, manifest.manifest_hash);
        assert!(decoded.is_schema_current());
    }

    #[test]
    fn chain_resource_delta_requires_world_resource_schema_and_commit_hashes() {
        let delta = ChainResourceDelta::empty_for_manifest("world-a", "manifest-hash", 9, 11);

        assert_eq!(delta.schema_version, CHAIN_RESOURCE_DELTA_SCHEMA_V1);
        assert_eq!(delta.chain_id, "unbound");
        assert_eq!(delta.ordering_key.height, 9);
        assert!(delta.is_schema_current());
    }

    #[test]
    fn chunk_generation_delta_base_hash_uses_pre_event_manifest() {
        let coord = ChunkCoord { x: 1, y: 2, z: 3 };
        let mut budget = crate::ChunkResourceBudget::default();
        budget
            .total_by_element_g
            .insert(FragmentElementKind::Iron, 100);
        budget
            .remaining_by_element_g
            .insert(FragmentElementKind::Iron, 80);

        let mut manifest = ChainResourceManifest::empty_at_height("world-a", 42, 3);
        manifest.chain_id = "chain-a".to_string();
        manifest.manifest_height = 3;
        manifest.generated_chunks.insert(
            chunk_key(coord),
            ChainChunkResourceManifestEntry {
                schema_version: CHAIN_RESOURCE_MANIFEST_SCHEMA_V1.to_string(),
                world_id: "world-a".to_string(),
                chain_id: "chain-a".to_string(),
                world_seed: 42,
                chunk_generation_schema_version: CHUNK_GENERATION_SCHEMA_V1.to_string(),
                coord,
                seed: 99,
                chunk_status: ChainChunkResourceStatus::Committed,
                fragment_count: 0,
                block_count: 0,
                fragment_refs: Vec::new(),
                chunk_budget_total_hash: hash_json(&budget.total_by_element_g).unwrap(),
                chunk_budget_remaining_hash: hash_json(&budget.remaining_by_element_g).unwrap(),
                total_by_element_g: budget.total_by_element_g.clone(),
                remaining_by_element_g: budget.remaining_by_element_g.clone(),
                commit_ref: ChainResourceCommitRef {
                    height: 3,
                    block_hash: Some("block-3".to_string()),
                    event_id: Some(7),
                    action_id: None,
                },
                manifest_hash: String::new(),
            },
        );
        manifest.refresh_hashes();

        let mut expected_base = manifest.clone();
        expected_base.generated_chunks.remove(&chunk_key(coord));
        expected_base.manifest_height = 2;
        expected_base.refresh_hashes();

        let event = WorldEvent {
            id: 7,
            time: 9,
            kind: WorldEventKind::ChunkGenerated {
                coord,
                seed: 99,
                fragment_count: 0,
                block_count: 0,
                chunk_budget: budget,
                cause: ChunkGenerationCause::Action,
            },
        };
        let delta = delta_from_simulator_event(
            ChainResourceDerivationContext {
                world_id: "world-a",
                chain_id: "chain-a",
                genesis_ref: None,
                created_at_height: 0,
                manifest_height: 3,
                commit_block_hash: Some("block-3"),
                tick: 9,
            },
            &manifest,
            &event,
        )
        .expect("chunk delta");

        assert_eq!(delta.base_manifest_hash, expected_base.manifest_hash);
        assert_ne!(delta.base_manifest_hash, manifest.manifest_hash);
        assert_eq!(delta.resulting_manifest_hash, manifest.manifest_hash);
    }
}
