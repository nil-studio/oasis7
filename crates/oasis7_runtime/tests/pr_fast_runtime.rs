//! Complete runtime contracts without the full library test harness.
#![cfg(not(target_arch = "wasm32"))]

pub use oasis7_runtime::*;

mod tests {
    fn pos(x: i64, y: i64) -> oasis7_agent_api::world_action::GeoPos {
        oasis7_agent_api::world_action::GeoPos {
            x_cm: x,
            y_cm: y,
            z_cm: 0,
        }
    }

    mod basic {
        include!("../src/tests/basic.rs");
    }
}
