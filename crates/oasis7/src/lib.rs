extern crate self as oasis7;

pub mod geometry {
    pub use oasis7_agent_api::world_action::GeoPos;
    pub use oasis7_agent_api::world_geometry::{
        DEFAULT_CLOUD_DEPTH_CM, DEFAULT_CLOUD_DEPTH_KM, DEFAULT_CLOUD_HEIGHT_CM,
        DEFAULT_CLOUD_HEIGHT_KM, DEFAULT_CLOUD_WIDTH_CM, DEFAULT_CLOUD_WIDTH_KM, SPACE_UNIT_CM,
        space_distance_cm, space_distance_m,
    };
}
pub mod industry_types {
    pub use oasis7_agent_api::world_industry::StarterIndustrialSettlementSummaryV1;
    pub use oasis7_domain_types::MaterialLedgerId;
}
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime_host;
pub mod simulator;
#[cfg(not(target_arch = "wasm32"))]
pub mod world_service;

pub use geometry::GeoPos;
pub use oasis7_runtime::agent_state::AgentState;

#[cfg(test)]
mod snapshot_facade_tests {
    use crate::simulator::{
        PlayerGameplayExecutionState, PlayerGameplayStageStatus, WorldKernel, WorldSnapshot,
    };

    #[test]
    fn snapshot_player_gameplay_execution_state_backfills_from_legacy_fields() {
        let mut value = serde_json::to_value(WorldKernel::new().snapshot()).unwrap();
        value["player_gameplay"] = serde_json::json!({
            "stage_id": "post_onboarding",
            "stage_status": "blocked",
            "goal_id": "post_onboarding.recover_capability",
            "goal_kind": "recover_capability",
            "goal_title": "Recover sustainable capability",
            "objective": "Restore the first blocked capability chain.",
            "progress_detail": "The primary line is blocked.",
            "next_step_hint": "Replenish upstream materials and advance again."
        });
        let restored = WorldSnapshot::from_json(&serde_json::to_string(&value).unwrap()).unwrap();
        let gameplay = restored.player_gameplay.unwrap();
        assert_eq!(gameplay.stage_status, PlayerGameplayStageStatus::Blocked);
        assert_eq!(
            gameplay.execution_state,
            PlayerGameplayExecutionState::Blocked
        );
    }
}
