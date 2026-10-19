use super::*;
use crate::geometry::GeoPos;
use crate::models::AgentState;
use crate::runtime::{AgentCell, AgentClaimState, IndustryStage, MainTokenSupplyState};
use serde_json::Value;

const SNAPSHOT_PLAYER_ID: &str = "claim-choice-player";

#[derive(Clone, Copy)]
struct CandidateFixture<'a> {
    id: &'a str,
    position: (i64, i64, i64),
    body_kind: &'a str,
    frame_kind: &'a str,
    modules: &'a [&'a str],
}

fn bind_agent_for_snapshot(server: &mut ViewerRuntimeLiveServer, agent_id: &str) {
    server
        .llm_sidecar
        .bind_agent_player(
            agent_id,
            SNAPSHOT_PLAYER_ID,
            Some("claim-choice-public-key"),
            false,
        )
        .expect("bind claim-choice player to agent");
}

fn claim_state(target_agent_id: &str, claim_owner_id: &str) -> AgentClaimState {
    AgentClaimState {
        target_agent_id: target_agent_id.to_string(),
        claim_owner_id: claim_owner_id.to_string(),
        reputation_tier: 0,
        slot_index: 1,
        activation_fee_amount: 100,
        activation_fee_burn_amount: 50,
        activation_fee_treasury_amount: 50,
        claim_bond_amount: 200,
        locked_bond_amount: 200,
        upfront_restricted_spent_amount: 0,
        upfront_liquid_spent_amount: 300,
        claim_bond_locked_restricted_amount: 0,
        claim_bond_locked_liquid_amount: 200,
        claim_bond_restricted_source_treasury_bucket_id: None,
        upkeep_per_epoch: 25,
        release_cooldown_epochs: 2,
        grace_epochs: 2,
        idle_warning_epochs: 7,
        forced_idle_reclaim_epochs: 10,
        forced_reclaim_penalty_bps: 2_000,
        claimed_at_epoch: 0,
        upkeep_paid_through_epoch: 0,
        delinquent_since_epoch: None,
        grace_deadline_epoch: None,
        release_requested_at_epoch: None,
        release_ready_at_epoch: None,
        idle_warning_emitted_at_epoch: None,
    }
}

fn server_with_candidates(
    candidates: &[CandidateFixture<'_>],
    balance: u64,
    industry_stage: IndustryStage,
) -> (ViewerRuntimeLiveServer, String) {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let primary_agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("primary agent");

    let mut state = server.world.state().clone();
    state.industry_progress.stage = industry_stage;
    for candidate in candidates {
        let mut agent = AgentState::new(
            candidate.id,
            GeoPos::new(
                candidate.position.0,
                candidate.position.1,
                candidate.position.2,
            ),
        );
        agent.body.kind = candidate.body_kind.to_string();
        agent.body_state.frame_kind = candidate.frame_kind.to_string();
        for slot in &mut agent.body_state.slots {
            slot.installed_module = None;
        }
        let populated_slot_count = candidate.modules.len().min(agent.body_state.slots.len());
        agent.body_state.slots[0..populated_slot_count]
            .iter_mut()
            .zip(candidate.modules.iter())
            .for_each(|(slot, module_id)| slot.installed_module = Some((*module_id).to_string()));
        let now = state.time;
        state
            .agents
            .insert(candidate.id.to_string(), AgentCell::new(agent, now));
    }
    server.world = crate::runtime::World::new_with_state(state);
    bind_agent_for_snapshot(&mut server, primary_agent_id.as_str());
    server
        .world
        .set_governance_execution_policy(crate::runtime::GovernanceExecutionPolicy {
            epoch_length_ticks: 1,
            ..crate::runtime::GovernanceExecutionPolicy::default()
        })
        .expect("set governance policy");
    server.world.set_main_token_supply(MainTokenSupplyState {
        total_supply: balance,
        circulating_supply: balance,
        ..MainTokenSupplyState::default()
    });
    server
        .world
        .set_main_token_account_balance(primary_agent_id.as_str(), balance, 0)
        .expect("seed main token balance");

    (server, primary_agent_id)
}

fn claim_choice_json(server: &mut ViewerRuntimeLiveServer) -> Value {
    let _env_guard = runtime_provider_env_lock().lock().expect("env lock");
    let snapshot = server.compat_snapshot(Some(SNAPSHOT_PLAYER_ID));
    serde_json::to_value(
        snapshot
            .player_gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.agent_claim.as_ref())
            .and_then(|claim| claim.next_claim_quote.as_ref())
            .and_then(|quote| quote.slot_1_claim_choice_quote.as_ref())
            .expect("slot-1 claim choice quote"),
    )
    .expect("serialize claim choice quote")
}

fn find_field<'a>(value: &'a Value, field: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map
            .get(field)
            .or_else(|| map.values().find_map(|child| find_field(child, field))),
        Value::Array(values) => values.iter().find_map(|child| find_field(child, field)),
        _ => None,
    }
}

fn assert_non_empty_text_field(value: &Value, field: &str) {
    let found = find_field(value, field)
        .unwrap_or_else(|| panic!("claim choice is missing canonical field {field}: {value}"));
    assert!(
        found.as_str().is_some_and(|text| !text.trim().is_empty()),
        "claim choice field {field} must be a non-empty human-readable string: {found}"
    );
}

fn assert_non_null_field(value: &Value, field: &str) {
    let found = find_field(value, field)
        .unwrap_or_else(|| panic!("claim choice is missing canonical field {field}: {value}"));
    assert!(!found.is_null(), "claim choice field {field} must be known");
}

#[test]
fn slot_1_claim_choice_filters_orders_and_deduplicates_without_mutating_state() {
    let modules = ["m1.mobility.basic", "m1.power.storage", "m1.mobility.basic"];
    let unknown_body = CandidateFixture {
        id: "candidate-unknown-body",
        position: (99, 99, 99),
        body_kind: "",
        frame_kind: "",
        modules: &modules,
    };
    let candidate_z = CandidateFixture {
        id: "candidate-z",
        position: (30, 10, 2),
        body_kind: "industrial_worker",
        frame_kind: "light_frame",
        modules: &modules,
    };
    let candidate_a = CandidateFixture {
        id: "candidate-a",
        position: (10, 10, 2),
        body_kind: "industrial_worker",
        frame_kind: "light_frame",
        modules: &modules,
    };
    let (mut server, primary_agent_id) = server_with_candidates(
        &[unknown_body, candidate_z, candidate_a],
        400,
        IndustryStage::Bootstrap,
    );
    let mut state = server.world.state().clone();
    state.agent_claims.insert(
        "candidate-claimed".to_string(),
        claim_state("candidate-claimed", "another-claim-owner"),
    );
    let claimed_agent = AgentState::new("candidate-claimed", GeoPos::new(0, 0, 0));
    let now = state.time;
    state.agents.insert(
        "candidate-claimed".to_string(),
        AgentCell::new(claimed_agent, now),
    );
    server.world = crate::runtime::World::new_with_state(state);
    bind_agent_for_snapshot(&mut server, primary_agent_id.as_str());

    let before = server.world.state().clone();
    let choice = claim_choice_json(&mut server);
    assert_eq!(server.world.state(), &before, "snapshot must be read-only");

    let candidate_ids = choice["candidates"]
        .as_array()
        .expect("candidate array")
        .iter()
        .map(|candidate| candidate["agent_id"].as_str().expect("candidate id"))
        .collect::<Vec<_>>();
    assert_eq!(candidate_ids, ["candidate-a", "candidate-z"]);
    assert_eq!(
        choice["candidates"][0]["installed_module_ids"],
        serde_json::json!(["m1.mobility.basic", "m1.power.storage"])
    );
    assert_eq!(
        choice["candidates"][1]["installed_module_ids"],
        choice["candidates"][0]["installed_module_ids"]
    );
}

#[test]
fn slot_1_claim_choice_complete_single_candidate_is_claim_now_route_fit() {
    let modules = [
        "m1.power.radiation_harvest",
        "m1.power.storage",
        "m1.sensor.basic",
        "m1.mobility.basic",
        "m1.memory.core",
        "m1.storage.cargo",
    ];
    let (mut server, _) = server_with_candidates(
        &[CandidateFixture {
            id: "candidate-complete",
            position: (10, 20, 30),
            body_kind: "industrial_worker",
            frame_kind: "standard_frame",
            modules: &modules,
        }],
        400,
        IndustryStage::Bootstrap,
    );
    let choice = claim_choice_json(&mut server);
    assert_eq!(choice["claim_choice_class"], "claim_now_route_fit");
    assert_eq!(choice["recommended_claim_action"], "claim_now_route_fit");
    assert!(choice.get("fallback_reason").is_none_or(Value::is_null));
    assert_non_null_field(&choice, "candidate_starting_location");
    assert_non_empty_text_field(&choice, "candidate_specialty_summary");
    assert_non_empty_text_field(&choice, "first_industrial_goal_help");
    assert_non_empty_text_field(&choice, "candidate_risk_summary");
    assert_non_empty_text_field(&choice, "candidate_recommendation_reason");
}

#[test]
fn slot_1_claim_choice_complete_candidate_with_known_incomplete_alternative_needs_unique_rationale()
{
    let complete_modules = [
        "m1.power.radiation_harvest",
        "m1.power.storage",
        "m1.sensor.basic",
        "m1.mobility.basic",
        "m1.memory.core",
        "m1.storage.cargo",
    ];
    let known_incomplete_modules = [
        "m1.power.radiation_harvest",
        "m1.power.storage",
        "m1.sensor.basic",
        "m1.mobility.basic",
    ];
    let (mut server, _) = server_with_candidates(
        &[
            CandidateFixture {
                id: "candidate-complete",
                position: (10, 20, 30),
                body_kind: "industrial_worker",
                frame_kind: "standard_frame",
                modules: &complete_modules,
            },
            CandidateFixture {
                id: "candidate-known-incomplete",
                position: (30, 20, 10),
                body_kind: "industrial_worker",
                frame_kind: "standard_frame",
                modules: &known_incomplete_modules,
            },
        ],
        400,
        IndustryStage::Bootstrap,
    );

    let choice = claim_choice_json(&mut server);
    assert_ne!(
        choice["claim_choice_class"], "claim_now_route_fit",
        "a complete candidate is not an immediate route fit when a known-incomplete alternative lacks unique rationale: {choice}"
    );
    assert_ne!(
        choice["recommended_claim_action"], "claim_now_route_fit",
        "the recommendation must not enable a blind immediate claim: {choice}"
    );
    for rationale_field in [
        "candidate_starting_location",
        "candidate_specialty_summary",
        "first_industrial_goal_help",
        "candidate_risk_summary",
        "candidate_recommendation_reason",
    ] {
        assert!(
            choice.get(rationale_field).is_none_or(Value::is_null),
            "mixed complete/incomplete candidates must not publish a unique rationale field {rationale_field}: {choice}"
        );
    }
}

#[test]
fn slot_1_claim_choice_compares_multiple_complete_candidates_without_ranking() {
    let modules_a = [
        "m1.power.radiation_harvest",
        "m1.power.storage",
        "m1.sensor.basic",
        "m1.mobility.basic",
        "m1.memory.core",
        "m1.storage.cargo",
    ];
    let modules_b = [
        "m1.power.radiation_harvest",
        "m1.power.storage",
        "m1.sensor.basic",
        "m1.mobility.basic",
        "m1.memory.core",
        "m1.storage.cargo",
    ];
    let (mut server, _) = server_with_candidates(
        &[
            CandidateFixture {
                id: "candidate-b",
                position: (20, 20, 20),
                body_kind: "industrial_worker",
                frame_kind: "standard_frame",
                modules: &modules_b,
            },
            CandidateFixture {
                id: "candidate-a",
                position: (10, 10, 10),
                body_kind: "industrial_worker",
                frame_kind: "standard_frame",
                modules: &modules_a,
            },
        ],
        400,
        IndustryStage::Bootstrap,
    );
    let choice = claim_choice_json(&mut server);
    assert_eq!(choice["claim_choice_class"], "compare_candidates_first");
    assert_eq!(
        choice["recommended_claim_action"],
        "compare_candidates_first"
    );
    assert!(choice.get("fallback_reason").is_none_or(Value::is_null));
    for candidate in choice["candidates"].as_array().expect("candidate array") {
        for ranking_field in ["rank", "ranking", "score", "preferred", "is_recommended"] {
            assert!(
                candidate.get(ranking_field).is_none(),
                "candidate comparison must not publish Viewer ranking field {ranking_field}"
            );
        }
    }
}

#[test]
fn slot_1_claim_choice_unknown_goal_or_capability_fails_closed() {
    let unknown_modules = ["m1.unknown.capability"];
    let (mut server, _) = server_with_candidates(
        &[CandidateFixture {
            id: "candidate-unknown",
            position: (10, 20, 30),
            body_kind: "industrial_worker",
            frame_kind: "standard_frame",
            modules: &unknown_modules,
        }],
        400,
        IndustryStage::ScaleOut,
    );
    let choice = claim_choice_json(&mut server);
    assert_eq!(choice["status"], "candidate_rationale_missing");
    assert_eq!(choice["claim_choice_class"], "wait_or_fund_first");
    assert_eq!(choice["recommended_claim_action"], "wait_or_fund_first");
    assert_eq!(choice["fallback_reason"], "candidate_rationale_missing");
}
