//! Shared original rolling arithmetic for cached and root-backed allocators.
use super::World;

impl World {
    // Child pub(super) retains the former private World-module scope.
    pub(super) fn allocate_rolling_sequence_id(next_id: &mut u64, era: &mut u64) -> u64 {
        let (allocated, next_id_after, era_after) =
            Self::preview_rolling_sequence_id(*next_id, *era);
        *next_id = next_id_after;
        *era = era_after;
        allocated
    }

    pub(super) fn preview_rolling_sequence_id(next_id: u64, era: u64) -> (u64, u64, u64) {
        let allocated = next_id.max(1);
        if allocated == u64::MAX {
            (allocated, 1, era.saturating_add(1))
        } else {
            (allocated, allocated + 1, era)
        }
    }
}
