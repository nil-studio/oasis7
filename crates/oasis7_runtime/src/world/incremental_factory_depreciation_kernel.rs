//! Shared original saturating depreciation arithmetic. No storage authority.
const FACTORY_DURABILITY_PPM_BASE: i64 = 1_000_000;
const FACTORY_DEPRECIATION_PPM_PER_MAINTENANCE_UNIT: i64 = 1_000;
pub(in crate::world) const FACTORY_DEPRECIATION_REASON: &str = "depreciation_tick";
const FACTORY_LOAD_FACTOR_BASE_BPS: i64 = 10_000;
const FACTORY_LOAD_FACTOR_MAX_BPS: i64 = 20_000;
pub(in crate::world) fn depreciation_v1(
    now: u64,
    built_at: u64,
    durability: i64,
    maintenance: i64,
    recipe_slots: u16,
    active_jobs: i64,
) -> Option<(i64, i64)> {
    const BASE: i64 = FACTORY_DURABILITY_PPM_BASE;
    const LOAD_BASE: i64 = FACTORY_LOAD_FACTOR_BASE_BPS;
    if built_at >= now {
        return None;
    }
    let current = durability.clamp(0, BASE);
    if current <= 0 {
        return None;
    }
    let maintenance = maintenance.max(0);
    if maintenance <= 0 {
        return None;
    }
    let base_decay = maintenance
        .saturating_mul(FACTORY_DEPRECIATION_PPM_PER_MAINTENANCE_UNIT)
        .clamp(0, BASE);
    if base_decay <= 0 {
        return None;
    }
    let slots = i64::from(recipe_slots.max(1));
    let utilization = ((active_jobs as i128)
        .saturating_mul(LOAD_BASE as i128)
        .saturating_div(slots as i128))
    .clamp(0, LOAD_BASE as i128) as i64;
    let factor = LOAD_BASE
        .saturating_add(utilization)
        .clamp(LOAD_BASE, FACTORY_LOAD_FACTOR_MAX_BPS);
    let decay = ((base_decay as i128)
        .saturating_mul(factor as i128)
        .saturating_div(LOAD_BASE as i128))
    .clamp(0, BASE as i128) as i64;
    if decay <= 0 {
        return None;
    }
    let next = current.saturating_sub(decay).clamp(0, BASE);
    (next != current).then_some((current, next))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_clamps_skips_saturation_and_zero_slots() {
        assert_eq!(depreciation_v1(9, 9, 1_000_000, 1, 1, 0), None);
        assert_eq!(depreciation_v1(9, 10, 1_000_000, 1, 1, 0), None);
        assert_eq!(depreciation_v1(9, 0, 0, 1, 1, 1), None);
        assert_eq!(depreciation_v1(9, 0, i64::MIN, 1, 1, 1), None);
        assert_eq!(depreciation_v1(9, 0, 1_000_000, 0, 1, 1), None);
        assert_eq!(depreciation_v1(9, 0, 1_000_000, i64::MIN, 1, 1), None);
        assert_eq!(
            depreciation_v1(9, 0, i64::MAX, 1, 2, 0),
            Some((1_000_000, 999_000))
        );
        assert_eq!(
            depreciation_v1(9, 0, 1_000_000, 1, 2, 1),
            Some((1_000_000, 998_500))
        );
        assert_eq!(
            depreciation_v1(9, 0, 1_000_000, 1, 0, 1),
            Some((1_000_000, 998_000))
        );
        assert_eq!(
            depreciation_v1(9, 0, 1_000_000, 1, 2, i64::MAX),
            Some((1_000_000, 998_000))
        );
        assert_eq!(
            depreciation_v1(9, 0, 1_000_000, i64::MAX, 2, i64::MAX),
            Some((1_000_000, 0))
        );
        assert_eq!(depreciation_v1(9, 0, 1, 1, u16::MAX, 0), Some((1, 0)));
    }
}
