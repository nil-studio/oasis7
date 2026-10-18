use super::super::chunking::{ChunkCoord, chunk_coord_of};
use super::super::types::{FragmentRefillPreview, LocationId, WorldTime};
use super::super::world_model::ChunkState;
use super::WorldKernel;
use super::fragment_replenish::{compute_replenish_target, count_chunk_fragments};

impl WorldKernel {
    /// Describe the next possible replenishment opportunity without generating
    /// fragments or changing the world. The actual replenish tick still applies
    /// bounds, spacing, and deterministic candidate validation.
    pub fn quote_fragment_refill_preview(
        &self,
        chunk_coord: ChunkCoord,
    ) -> Result<FragmentRefillPreview, String> {
        if !matches!(
            self.model.chunks.get(&chunk_coord),
            Some(ChunkState::Generated | ChunkState::Exhausted)
        ) {
            return Err(format!(
                "chunk is not available for replenishment: {chunk_coord:?}"
            ));
        }

        let config = &self.config.asteroid_fragment;
        let max_fragments = config.max_fragments_per_chunk.max(0) as usize;
        let current_fragments = count_chunk_fragments(&self.model, &self.config.space, chunk_coord);
        let missing_fragments = max_fragments.saturating_sub(current_fragments);
        let replenishment_enabled = self.chunk_runtime.asteroid_fragment_enabled
            && config.replenish_interval_ticks > 0
            && config.replenish_percent_ppm > 0
            && max_fragments > 0;
        let next_replenish_tick = if replenishment_enabled {
            Some(
                next_replenishment_tick(self.time, config.replenish_interval_ticks as u64)
                    .ok_or_else(|| "next replenishment tick is unavailable".to_string())?,
            )
        } else {
            None
        };
        let ticks_until_replenish = next_replenish_tick.map(|tick| tick.saturating_sub(self.time));
        // A public kernel snapshot describes a completed tick. Its refill
        // attempt has already run, including when no candidate was accepted.
        let replenishment_due = false;
        let estimated_replenished_frag_count = if replenishment_enabled {
            compute_replenish_target(
                max_fragments,
                config.replenish_percent_ppm,
                missing_fragments,
            ) as i64
        } else {
            0
        };
        let remaining_by_element_g = self
            .model
            .chunk_resource_budgets
            .get(&chunk_coord)
            .map(|budget| budget.remaining_by_element_g.clone())
            .ok_or_else(|| format!("chunk resource budget is unavailable: {chunk_coord:?}"))?;
        let (target_frag_id, current_frag_remaining_summary) =
            select_target_fragment(&self.model, &self.config.space, chunk_coord);
        let resource_type_count = remaining_by_element_g.len();
        let remaining_total_g = remaining_by_element_g
            .values()
            .fold(0_i64, |total, amount| {
                total.saturating_add((*amount).max(0))
            });
        let chunk_remaining_summary =
            format!("{remaining_total_g}g_remaining_across_{resource_type_count}_resource_types");
        let wait_cost_ticks = ticks_until_replenish.unwrap_or(0);
        let wait_cost_summary = if replenishment_enabled {
            format!("wait_{wait_cost_ticks}_ticks_for_next_replenishment_opportunity")
        } else {
            "replenishment_disabled_no_wait_option".to_string()
        };
        let recommended_resource_action = if replenishment_enabled && missing_fragments > 0 {
            "wait_current_chunk"
        } else {
            "move_or_switch_material_route"
        };
        let next_industrial_goal_relevance = if remaining_total_g > 0 {
            "current_chunk_materials_can_support_collection_before_first_industrial_goal"
        } else if replenishment_enabled && missing_fragments > 0 {
            "next_replenishment_may_restore_collection_options_for_first_industrial_goal"
        } else {
            "switch_route_to_restore_material_progress_toward_first_industrial_goal"
        };

        Ok(FragmentRefillPreview {
            chunk_coord,
            target_frag_id,
            current_frag_remaining_summary,
            chunk_remaining_summary,
            remaining_by_element_g,
            replenishment_enabled,
            replenishment_due,
            next_replenish_tick,
            ticks_until_replenish,
            wait_cost_ticks,
            estimated_replenished_frag_count,
            estimated_replenished_resource_hint:
                "fragment_count_is_an_estimate_material_mix_is_not_guaranteed".to_string(),
            next_industrial_goal_relevance: next_industrial_goal_relevance.to_string(),
            wait_cost_summary,
            recommended_resource_action: recommended_resource_action.to_string(),
        })
    }
}

fn next_replenishment_tick(time: WorldTime, interval_ticks: WorldTime) -> Option<WorldTime> {
    let remainder = time % interval_ticks;
    time.checked_add(interval_ticks - remainder)
}

fn select_target_fragment(
    model: &super::super::world_model::WorldModel,
    space: &super::super::world_model::SpaceConfig,
    chunk_coord: ChunkCoord,
) -> (Option<LocationId>, String) {
    let target = model
        .locations
        .values()
        .filter(|location| location.id.starts_with("frag-"))
        .filter(|location| chunk_coord_of(location.pos, space) == Some(chunk_coord))
        .filter_map(|location| {
            let remaining_g = location
                .fragment_budget
                .as_ref()?
                .remaining_by_element_g
                .values()
                .fold(0_i64, |total, amount| {
                    total.saturating_add((*amount).max(0))
                });
            Some((remaining_g, location.id.clone()))
        })
        .min();
    match target {
        Some((remaining_g, id)) => (
            Some(id.clone()),
            format!("{id}_has_{remaining_g}g_remaining"),
        ),
        None => (None, "no_fragment_budget_available_in_chunk".to_string()),
    }
}
