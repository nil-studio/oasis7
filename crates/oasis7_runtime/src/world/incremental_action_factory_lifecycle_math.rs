//! Pure bounded factory arithmetic; callers own all source/authorization checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::world) struct MaintenanceRepairV1 {
    pub consumed_parts: i64,
    pub durability_ppm: i64,
}
pub(in crate::world) fn maintenance_repair_v1(
    current: i64,
    requested: i64,
) -> Option<MaintenanceRepairV1> {
    if requested <= 0 {
        return None;
    }
    let current = current.clamp(0, 1_000_000);
    let repair = requested.saturating_mul(25_000).max(0);
    let durability = current.saturating_add(repair).clamp(0, 1_000_000);
    let recovered = durability.saturating_sub(current);
    if recovered <= 0 {
        return None;
    }
    let consumed = recovered
        .saturating_add(24_999)
        .saturating_div(25_000)
        .clamp(1, requested);
    Some(MaintenanceRepairV1 {
        consumed_parts: consumed,
        durability_ppm: durability,
    })
}
pub(in crate::world) fn recycle_stack_amount_v1(cost: i64, durability: i64) -> Option<i64> {
    if cost <= 0 {
        return None;
    }
    let amount = ((cost as i128)
        .saturating_mul(700_000)
        .saturating_mul(durability.clamp(0, 1_000_000) as i128)
        / 1_000_000
        / 1_000_000) as i64;
    (amount > 0).then_some(amount)
}
