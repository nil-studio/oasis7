use super::pos;
use crate::{
    Action, AgentLocationAuthorityV1, DomainEvent, FactoryConstructionPowerMode,
    FactoryConstructionPowerProfileV1, FactoryProfileV1, FactorySiteAuthorityV1,
    GovernanceProposalStatus, IndustryStage, LocationAnchorV1, MaterialDefaultPriority,
    MaterialLedgerId, MaterialProfileV1, MaterialTransitPriority, MaterialTransportLossClass,
    ProductProfileV1, ProposalDecision, RecipeProfileV1, RejectReason, World, WorldEventBody,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{FactoryModuleSpec, MaterialStack, RecipeExecutionPlan};

#[path = "economy_priority_governance_tests.rs"]
mod governance_tests;

fn factory_spec(factory_id: &str, build_time_ticks: u32, recipe_slots: u16) -> FactoryModuleSpec {
    FactoryModuleSpec {
        factory_id: factory_id.to_string(),
        display_name: "Test Factory".to_string(),
        tier: 1,
        tags: vec!["assembly".to_string()],
        build_cost: vec![
            MaterialStack::new("steel_plate", 10),
            MaterialStack::new("circuit_board", 2),
        ],
        build_time_ticks,
        base_power_draw: 5,
        recipe_slots,
        throughput_bps: 10_000,
        maintenance_per_tick: 1,
    }
}

fn prepare_factory_build(
    world: &mut World,
    builder_agent_id: &str,
    site_id: &str,
    spec: &FactoryModuleSpec,
) {
    let location_id = format!("location-{site_id}");
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: location_id.clone(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("install location anchor");
    world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: builder_agent_id.to_string(),
            location_id: location_id.clone(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("install agent location authority");
    world
        .set_factory_site_authority(FactorySiteAuthorityV1 {
            site_id: site_id.to_string(),
            location_id,
            owner_agent_id: builder_agent_id.to_string(),
            authorized_agent_ids: Vec::new(),
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        })
        .expect("install factory site authority");
    const CONSTRUCTION_POWER: i64 = 10;
    world
        .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
            factory_id: spec.factory_id.clone(),
            factory_kind: "test".to_string(),
            source_module_id: None,
            electricity_amount: CONSTRUCTION_POWER,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        })
        .expect("install construction power profile");
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: spec.factory_id.clone(),
            tier: spec.tier,
            recipe_slots: spec.recipe_slots,
            tags: spec.tags.clone(),
        })
        .expect("install factory capability profile");
    let builder_ledger = MaterialLedgerId::agent(builder_agent_id);
    for stack in &spec.build_cost {
        world
            .set_ledger_material_balance(builder_ledger.clone(), stack.kind.as_str(), stack.amount)
            .expect("seed builder construction material");
    }
    world
        .set_agent_resource_balance(
            builder_agent_id,
            ResourceKind::Electricity,
            CONSTRUCTION_POWER,
        )
        .expect("seed builder construction power");
}

fn prepare_factory_build_id(
    world: &mut World,
    builder_agent_id: &str,
    site_id: &str,
    factory_id: &str,
) {
    prepare_factory_build(
        world,
        builder_agent_id,
        site_id,
        &factory_spec(factory_id, 1, 1),
    );
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
        let Some(proposal) = world.state().governance_proposals.get(proposal_key) else {
            break;
        };
        if proposal.status != GovernanceProposalStatus::Open {
            break;
        }
        world.step().expect("advance governance proposal");
    }

    let proposal = world
        .state()
        .governance_proposals
        .get(proposal_key)
        .expect("proposal finalized");
    assert_eq!(proposal.status, GovernanceProposalStatus::Passed);
}

fn approved_manifest_proposal(world: &mut World, author: &str) -> u64 {
    let mut manifest = world.manifest().clone();
    manifest.version = manifest.version.saturating_add(1);
    let proposal_id = world
        .propose_manifest_update(manifest, author.to_string())
        .expect("propose manifest update");
    world
        .shadow_proposal(proposal_id)
        .expect("shadow manifest proposal");
    world
        .approve_proposal(proposal_id, author.to_string(), ProposalDecision::Approve)
        .expect("approve manifest proposal");
    proposal_id
}

fn latest_action_rejected_message(world: &World) -> String {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
                Some(format!("{reason:?}"))
            }
            _ => None,
        })
        .expect("action rejected")
}

fn latest_factory_production_block(world: &World) -> (String, String) {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::FactoryProductionBlocked {
                blocker_kind,
                blocker_detail,
                ..
            }) => Some((blocker_kind.clone(), blocker_detail.clone())),
            _ => None,
        })
        .expect("factory production blocked")
}

fn seed_builder_electricity(world: &mut World, amount: i64) {
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, amount)
        .expect("seed builder electricity");
}

include!("economy_priority_logistics_priority_tests.rs");
include!("economy_priority_logistics_recipe_tests.rs");
include!("economy_priority_logistics_recipe_path_authority_tests.rs");
include!("economy_priority_logistics_network_tests.rs");
include!("economy_priority_logistics_network_integrity_tests.rs");
