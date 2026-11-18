//! Genuine four-Alliance FIFO and independent Accepted+result World oracle.
use super::*;
#[path = "incremental_normal_action_diplomacy_case.rs"]
mod case;

#[test]
fn queued_alliance_lifecycle_preserves_outer_risk_war_rejections_and_exact_world_originals() {
    case::run();
}
