use crate::geometry::GeoPos;

use super::world_model::{Location, SpaceConfig};

pub(super) const FRAGMENT_LOCATION_PREFIX: &str = "frag-";

const FRAGMENT_SPAWN_STANDOFF_CM: i64 = 5_000;
const FRAGMENT_SPAWN_MIN_STANDOFF_CM: i64 = 2_000;

pub(super) fn fragment_spawn_pos(
    location: &Location,
    space: &SpaceConfig,
    _spawn_roll: u64,
) -> GeoPos {
    let radius_cm = location.profile.radius_cm.max(1);
    let spawn_at = |standoff_cm: i64| {
        let distance_cm = radius_cm + standoff_cm;
        GeoPos::new(
            location.pos.x_cm,
            location.pos.y_cm,
            location.pos.z_cm + distance_cm,
        )
    };

    let primary = spawn_at(FRAGMENT_SPAWN_STANDOFF_CM);
    if space.contains(primary) {
        return primary;
    }

    let fallback = spawn_at(FRAGMENT_SPAWN_MIN_STANDOFF_CM);
    if space.contains(fallback) {
        return fallback;
    }

    location.pos
}
