use super::pos;
use crate::{
    Action, GovernanceProposalStatus, MaterialDefaultPriority, MaterialLedgerId, MaterialProfileV1,
    MaterialTransportLossClass, World,
};
use oasis7_wasm_abi::MaterialStack;

#[test]
fn market_quote_decision_preview_is_conditional_and_read_only() {
    let mut world = World::new();
    let local = MaterialLedgerId::site("preview-site");
    world.submit_action(Action::RegisterAgent {
        agent_id: "preview-operator".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register preview operator");
    authorize_policy_update(
        &mut world,
        "preview-operator",
        "proposal.preview.market-tax",
    );
    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "preview-operator".to_string(),
        electricity_tax_bps: 900,
        data_tax_bps: 700,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 16,
        blocked_agents: Vec::new(),
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("set preview tax policy");
    world
        .set_ledger_material_balance(local.clone(), "iron_ingot", 2)
        .expect("seed local inventory");
    world
        .set_material_balance("iron_ingot", 3)
        .expect("seed world inventory");
    world
        .upsert_material_profile(MaterialProfileV1 {
            kind: "iron_ingot".to_string(),
            tier: 2,
            category: "intermediate".to_string(),
            stack_limit: 500,
            transport_loss_class: MaterialTransportLossClass::High,
            decay_bps_per_tick: 0,
            default_priority: MaterialDefaultPriority::Standard,
        })
        .expect("insert material profile");
    let before = world.snapshot();
    let preview =
        world.market_quote_decision_preview(&local, &[MaterialStack::new("iron_ingot", 6)]);

    assert!(preview.conditional);
    assert_eq!(preview.market_quotes.len(), 1);
    assert_eq!(preview.market_quotes[0].local_deficit_amount, 4);
    assert_eq!(preview.market_quotes[0].world_available_amount, 3);
    assert_eq!(preview.market_quotes[0].transit_loss_bps, 20);
    assert_eq!(preview.market_quotes[0].governance_tax_bps, 1_600);
    assert_eq!(preview.market_quotes[0].effective_cost_index_ppm, 1_828_600);
    assert_eq!(preview.local_vs_world_delta[0].world_cover_amount, 3);
    assert_eq!(preview.total_unsatisfied_shortfall, 3);
    assert_eq!(preview.market_pressure, "unsatisfied_shortfall");
    assert_eq!(preview.recommendation, "reduce_or_source_materials");
    assert_eq!(preview.next_reduction_action, "reduce_requested_amount");
    assert_eq!(world.snapshot(), before);
}

fn authorize_policy_update(world: &mut World, operator_agent_id: &str, proposal_key: &str) {
    world.submit_action(Action::OpenGovernanceProposal {
        proposer_agent_id: operator_agent_id.to_string(),
        proposal_key: proposal_key.to_string(),
        title: format!("title.{proposal_key}"),
        description: "authorize gameplay policy update".to_string(),
        options: vec!["approve".to_string(), "reject".to_string()],
        voting_window_ticks: 1,
        quorum_weight: 3,
        pass_threshold_bps: 5_000,
    });
    world.step().expect("open governance proposal");
    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: operator_agent_id.to_string(),
        proposal_key: proposal_key.to_string(),
        option: "approve".to_string(),
        weight: 3,
    });
    world.step().expect("cast governance vote");
    for _ in 0..2 {
        if world
            .state()
            .governance_proposals
            .get(proposal_key)
            .is_some_and(|proposal| proposal.status == GovernanceProposalStatus::Open)
        {
            world.step().expect("advance governance proposal");
        }
    }
    assert_eq!(
        world.state().governance_proposals[proposal_key].status,
        GovernanceProposalStatus::Passed
    );
}
