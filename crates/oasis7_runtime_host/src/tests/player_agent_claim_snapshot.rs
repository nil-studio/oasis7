use super::*;

#[test]
fn player_agent_claim_snapshot_legacy_json_defaults_first_chat_unlock_preview_and_omits_none() {
    let legacy: PlayerAgentClaimSnapshot = serde_json::from_value(serde_json::json!({
        "claimer_agent_id": "agent-0",
        "current_epoch": 4,
        "reputation_tier": 1,
        "claim_cap": 2,
        "owned_claim_count": 0,
        "liquid_main_token_balance": 0
    }))
    .expect("legacy player claim snapshot");
    assert!(legacy.first_chat_unlock_preview.is_none());
    let serialized = serde_json::to_value(legacy).expect("serialize player claim snapshot");
    assert!(serialized.get("first_chat_unlock_preview").is_none());
}

#[test]
fn player_agent_claim_snapshot_roundtrips_slot_1_candidate_choice_package() {
    let snapshot: PlayerAgentClaimSnapshot = serde_json::from_value(serde_json::json!({
        "claimer_agent_id": "agent-0",
        "current_epoch": 4,
        "reputation_tier": 0,
        "claim_cap": 1,
        "owned_claim_count": 0,
        "liquid_main_token_balance": 325,
        "next_claim_quote": {
            "slot_index": 1,
            "reputation_tier": 0,
            "claim_cap": 1,
            "owned_claim_count": 0,
            "activation_fee_amount": 100,
            "claim_bond_amount": 200,
            "upkeep_per_epoch": 25,
            "total_upfront_amount": 325,
            "release_cooldown_epochs": 2,
            "grace_epochs": 2,
            "idle_warning_epochs": 7,
            "forced_idle_reclaim_epochs": 10,
            "forced_reclaim_penalty_bps": 2000,
            "slot_1_claim_choice_quote": {
                "status": "candidate_facts_only",
                "candidates": [{
                    "agent_id": "agent-candidate",
                    "location_x_cm": 120,
                    "location_y_cm": -40,
                    "location_z_cm": 5,
                    "body_kind": "industrial_worker",
                    "frame_kind": "light_frame",
                    "installed_module_ids": ["drill", "scanner"]
                }],
                "fallback_reason": "candidate_rationale_missing",
                "claim_choice_class": "wait_or_fund_first",
                "recommended_claim_action": "wait_or_fund_first"
            }
        }
    }))
    .expect("deserialize claim snapshot with candidate choice package");

    let serialized = serde_json::to_value(snapshot).expect("serialize claim snapshot");
    assert_eq!(
        serialized["next_claim_quote"]["slot_1_claim_choice_quote"]["candidates"][0]["agent_id"],
        "agent-candidate"
    );
    assert_eq!(
        serialized["next_claim_quote"]["slot_1_claim_choice_quote"]["candidates"][0]["installed_module_ids"],
        serde_json::json!(["drill", "scanner"])
    );
    assert_eq!(
        serialized["next_claim_quote"]["slot_1_claim_choice_quote"]["fallback_reason"],
        "candidate_rationale_missing"
    );
}
