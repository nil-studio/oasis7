//! Complete reducer contracts against the production state library.
#![cfg(not(target_arch = "wasm32"))]

pub use oasis7_world_state::*;

mod tests {
    fn pos(x: i64, y: i64) -> oasis7_agent_api::world_action::GeoPos {
        oasis7_agent_api::world_action::GeoPos {
            x_cm: x,
            y_cm: y,
            z_cm: 0,
        }
    }

    mod apply_domain_event_guards {
        include!("../../oasis7_runtime/src/tests/apply_domain_event_guards.rs");
    }
}
