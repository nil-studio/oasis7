use super::*;
use crate::{
    Action, FactoryProductionState, FactoryState, MainTokenSupplyState, World, WorldStateProjection,
};
use oasis7_agent_api::world_action::{GeoPos, world_rule_proposal_description};

fn fixture() -> WorldState {
    let mut world = World::new();
    for id in ["owner", "primary", "recipient"] {
        world.submit_action(Action::RegisterAgent {
            agent_id: id.into(),
            pos: GeoPos::new(0, 0, 0),
        });
    }
    world.step().unwrap();
    world.set_agent_reputation_score("owner", 25).unwrap();
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100_000,
        circulating_supply: 100_000,
        ..Default::default()
    });
    world
        .set_main_token_account_balance_with_restricted("owner", 100_000, 0, 0)
        .unwrap();
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "owner".into(),
        target_agent_id: "primary".into(),
    });
    world.step().unwrap();
    let mut state = world.state().clone();
    state.factories.insert(
        "assembly".into(),
        FactoryState {
            factory_id: "assembly".into(),
            site_id: "site".into(),
            builder_agent_id: "owner".into(),
            spec: crate::builtin_factory_spec("factory.assembler.mk1", "assembly").unwrap(),
            input_ledger: MaterialLedgerId::agent("owner"),
            output_ledger: MaterialLedgerId::agent("owner"),
            durability_ppm: 1_000_000,
            production: FactoryProductionState::default(),
            site_authority_revision: None,
            site_location_id: None,
            location_anchor_revision: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 0,
        },
    );
    state.material_ledgers.insert(
        MaterialLedgerId::agent("owner"),
        BTreeMap::from([
            ("alloy_plate".into(), 120),
            ("control_chip".into(), 20),
            ("motor_mk1".into(), 40),
            ("sensor_pack".into(), 20),
        ]),
    );
    state
}
fn event(actor: &str, request: &str, command: WorldRuleCommandV1) -> DomainEvent {
    DomainEvent::WorldRuleCommitted {
        action_id: 1_000_000,
        actor_id: actor.into(),
        request_id: request.into(),
        command,
        governance_epoch: 0,
        world_binding: None,
    }
}
fn apply(state: &mut WorldState, event: &DomainEvent, now: u64) {
    let prepared = PreparedWorldRule::prepare(state, event, now).unwrap();
    let projected = serde_json::to_value(
        WorldStateProjection::borrowed(state).with_world_rule_overlay(&prepared),
    )
    .unwrap();
    state.apply_domain_event(event, now).unwrap();
    state.route_domain_event(event);
    let actual = serde_json::to_value(&*state).unwrap();
    let differences = projected
        .as_object()
        .unwrap()
        .keys()
        .chain(actual.as_object().unwrap().keys())
        .filter(|key| projected.get(*key) != actual.get(*key))
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        differences.is_empty(),
        "projection differs in {differences:?}"
    );
}

fn approve(state: &mut WorldState, actor: &str, proposal: &str, command: &WorldRuleCommandV1) {
    let p = crate::GovernanceProposalState {
        proposal_key: proposal.into(),
        proposer_agent_id: actor.into(),
        title: "World rule".into(),
        description: world_rule_proposal_description(command).unwrap(),
        options: vec!["approve".into()],
        voting_window_ticks: 1,
        quorum_weight: 3,
        pass_threshold_bps: 5_001,
        opened_at: 0,
        closes_at: 1,
        status: crate::GovernanceProposalStatus::Passed,
        finalized_at: Some(1),
        winning_option: Some("approve".into()),
        winning_weight: 3,
        total_weight_at_finalize: 3,
        snapshot_at_tick: 0,
        vote_weight_snapshot: BTreeMap::new(),
    };
    state.governance_proposals.insert(proposal.into(), p);
}

fn organization_fixture() -> WorldState {
    let mut state = fixture();
    state.alliances.insert(
        "org".into(),
        crate::AllianceState {
            alliance_id: "org".into(),
            members: vec!["owner".into(), "primary".into()],
            charter: "initial".into(),
            formed_by_agent_id: "owner".into(),
            formed_at: 0,
        },
    );
    state
}

mod industrial_tests;
mod organization_continuity_tests;
mod organization_tests;
mod supply_tests;

pub(super) fn emergency_authorize_fixture() -> (World, Action) {
    supply_tests::emergency_authorize_fixture()
}
#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
pub(super) fn essential_recipe_fixture() -> (World, Action) {
    supply_tests::essential_recipe_fixture()
}
