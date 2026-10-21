use std::collections::BTreeSet;
pub const MATERIAL_TRANSFER_LOSS_PER_KM_BPS: i64 = 5;
pub const MATERIAL_TRANSFER_SPEED_KM_PER_TICK: i64 = 100;
pub const MATERIAL_TRANSFER_MAX_INFLIGHT: usize = 2;
fn parse_industry_stage(raw: &str) -> Option<crate::IndustryStage> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "bootstrap" => Some(crate::IndustryStage::Bootstrap),
        "scale_out" | "scaleout" | "scale-out" => Some(crate::IndustryStage::ScaleOut),
        "governance" => Some(crate::IndustryStage::Governance),
        _ => None,
    }
}
use crate::*;
pub fn material_transit_has_capacity(state: &crate::WorldState) -> bool {
    state.pending_material_transits.len() < MATERIAL_TRANSFER_MAX_INFLIGHT
}
pub fn material_transit_loss_bps_for_state(state: &crate::WorldState, kind: &str) -> i64 {
    let base = MATERIAL_TRANSFER_LOSS_PER_KM_BPS.max(0);
    let factor = state
        .material_profiles
        .get(kind)
        .map(|profile| match profile.transport_loss_class {
            MaterialTransportLossClass::Low => 1_i64,
            MaterialTransportLossClass::Medium => 2_i64,
            MaterialTransportLossClass::High => 4_i64,
        })
        .unwrap_or(1);
    base.saturating_mul(factor)
}
pub fn material_transit_ticks(distance_km: i64) -> u64 {
    if distance_km == 0 {
        0
    } else {
        ((distance_km + MATERIAL_TRANSFER_SPEED_KM_PER_TICK - 1)
            / MATERIAL_TRANSFER_SPEED_KM_PER_TICK)
            .max(1) as u64
    }
}
pub fn recipe_stage_gate_allowed(current_stage: crate::IndustryStage, stage_gate: &str) -> bool {
    let normalized = stage_gate.trim();
    if normalized.is_empty() {
        return true;
    }
    let Some(required_stage) = parse_industry_stage(normalized) else {
        return true;
    };
    current_stage >= required_stage
}
pub fn recipe_preferred_tags_compatible(
    preferred_tags: &[String],
    factory_tags: &[String],
) -> bool {
    if preferred_tags.is_empty() {
        return true;
    }
    let normalized_factory: BTreeSet<String> = factory_tags
        .iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    preferred_tags.iter().any(|tag| {
        let normalized = tag.trim().to_ascii_lowercase();
        !normalized.is_empty() && normalized_factory.contains(normalized.as_str())
    })
}
