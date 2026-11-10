//! Child of module_execution_tests: actual GEN/CAS fixture, independent World oracle.
use super::*;
#[path = "incremental_industrial_eligibility_owner_integration_tests.rs"]
mod owner_integration;
use crate::world::{
    incremental_factory_directory_orders::*,
    incremental_industrial_eligibility::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1,
    incremental_industrial_eligibility::*,
};
use oasis7_world_state::incremental_world_rule_composite_family::bind_current_agent_orders_v1;
use oasis7_world_state::incremental_world_rule_receipts::qualify_world_rule_agent_orders_v1;
fn typed() -> oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1 {
    oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1 {
        json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 100000,
            max_depth: 64,
            max_payload_bytes: 1 << 20,
        },
        max_retained_trees: 4096,
        max_tree_heap: 1 << 20,
    }
}
pub(super) fn producing_world() -> World {
    let mut world = World::new();
    // The GEN fixture requires an unexecuted world with no mailbox/history.
    // Install the actual AgentCell as in initialized_world instead of stepping.
    world.state.time = 1;
    world.state.agents.insert(
        "owner".into(),
        crate::AgentCell::new(
            oasis7_world_state::agent_state::AgentState::new(
                "owner",
                oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
            ),
            1,
        ),
    );
    world.state.starter_oc_claims.insert(
        "owner".into(),
        oasis7_world_state::StarterOcClaimState {
            agent_id: "owner".into(),
            player_id: "player".into(),
            public_key: Some("owner-key".into()),
            ..Default::default()
        },
    );
    world.state.factories.insert(
        "assembler".into(),
        crate::FactoryState {
            factory_id: "assembler".into(),
            site_id: "site".into(),
            builder_agent_id: "owner".into(),
            spec: crate::builtin_factory_spec("factory.assembler.mk1", "assembler").unwrap(),
            input_ledger: oasis7_world_state::MaterialLedgerId::World,
            output_ledger: oasis7_world_state::MaterialLedgerId::World,
            durability_ppm: 1_000_000,
            production: crate::FactoryProductionState::default(),
            site_authority_revision: Some(1),
            site_location_id: Some("location".into()),
            location_anchor_revision: Some(1),
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 0,
        },
    );
    world.state.factory_site_authorities.insert(
        "site".into(),
        crate::FactorySiteAuthorityV1 {
            site_id: "site".into(),
            location_id: "location".into(),
            owner_agent_id: "owner".into(),
            active: true,
            chunk_ready: true,
            authority_revision: 1,
            ..Default::default()
        },
    );
    world.state.location_anchors.insert(
        "location".into(),
        crate::LocationAnchorV1 {
            location_id: "location".into(),
            active: true,
            authority_revision: 1,
            ..Default::default()
        },
    );
    let mut order = super::factory_order_directory::directory_order(
        "order",
        oasis7_world_state::world_rules::AgentIndustrialPhaseV1::Producing,
    );
    order.quote.dependency_digest = "real-dependency".into();
    order.quote.delivery_location_id = "location".into();
    order.quote.owner_source_id = "starter_claim:owner:0".into();
    order.quote.quote_digest.clear();
    order.quote.quote_digest = crate::util::hash_json(&order.quote).unwrap();
    order.validate().unwrap();
    let mut rules = oasis7_world_state::WorldRulesStateV1::default();
    rules.agent_orders.insert("order".into(), order);
    world.state.world_rules = Some(rules);
    world
}
fn current_equals_independent_world(world: World, expected_blocked: bool) {
    let order = &world.state.world_rules.as_ref().unwrap().agent_orders["order"];
    let oracle = world.industrial_order_blocker(order, 2);
    assert_eq!(oracle.is_some(), expected_blocked);
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut ri = genesis::io();
    let mut rk = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut ri,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut scan = genesis::scan();
    let mut order_typed = typed();
    let proof =
        qualify_world_rule_agent_orders_v1(&fixture.roots, &mut read, &mut scan, &mut order_typed)
            .unwrap();
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [91; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 4096,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut oi = genesis::io();
    let mut ok = genesis::keys();
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut oi,
        },
        key_work: &mut ok,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let guard = ctx.transaction.guard();
    let source = bind_current_agent_orders_v1(
        &proof,
        &fixture.roots,
        &reader,
        &mut ctx,
        &mut scan,
        &mut order_typed,
        &guard,
        &live,
    )
    .unwrap();
    assert!(
        matches!(
            ctx.transaction.validate_guard(&guard, &live),
            Err(ExecutionError::StaleGuard)
        ),
        "the genuine Current source read advances the read-set revision"
    );
    let mut directory_work = FactoryOrderDirectoryWorkV1 {
        ids:
            oasis7_world_state::incremental_world_rule_composite_family::AgentOrderDirectoryWorkV1 {
                max_ids: 32,
                max_id_bytes: 256,
                comparison_bytes_remaining: 16 << 20,
            },
        pending_entries_remaining: 10000,
        merge_comparison_bytes_remaining: 16 << 20,
        max_index_text_bytes: 256,
    };
    let guard = ctx.transaction.guard();
    let accepted = build_accepted_factory_order_directory_v1(
        &fixture.runtime,
        &fixture.head,
        &source,
        &reader,
        &mut ctx,
        &mut scan,
        &mut order_typed,
        &mut directory_work,
        &guard,
        &live,
    )
    .unwrap();
    let guard = ctx.transaction.guard();
    let directory = open_current_factory_order_directory_v1(
        &accepted,
        &source,
        &reader,
        &mut ctx,
        &mut scan,
        &mut order_typed,
        &mut directory_work,
        &guard,
        &live,
    )
    .unwrap();
    let native = fixture.current.native();
    let runtime = fixture.current.runtime();
    let sources = IndustrialEligibilitySourcesV1 {
        roots: &fixture.roots,
        orders: &directory,
        agents: fixture.current.raw().agents(),
        native: native.native(),
        factories: native.factory(),
        sites: native.factory_authority(),
        anchors: native.agent_authority(),
        revocation: runtime.revocation(),
        artifacts: runtime.resources(),
    };
    let mut work = IndustrialEligibilityWorkV1 {
        read: IndustrialEligibilityReadWorkV1 {
            cells_remaining: 100000,
            comparison_bytes_remaining: 32 << 20,
            max_text_bytes: 4096,
        },
        typed: typed(),
        scan: genesis::scan(),
        revocation: typed(),
        pending_entries_remaining: 100000,
    };
    let guard = ctx.transaction.guard();
    let result = current_producing_order_blocker_v1(
        &sources, "order", 2, &reader, &mut ctx, &mut read, &mut work, &guard, &live,
    )
    .unwrap();
    assert_eq!(result.blocker, oracle);
    let fresh_guard = ctx.transaction.guard();
    ctx.transaction.validate_guard(&fresh_guard, &live).unwrap();
    assert!(
        matches!(
            current_producing_order_blocker_v1(
                &sources, "order", 2, &reader, &mut ctx, &mut read, &mut work, &guard, &live
            ),
            Err(IndustrialEligibilityErrorV1::Execution(
                ExecutionError::StaleGuard
            ))
        ),
        "internal fresh child queries must not weaken the caller's entry guard"
    );
    let terminal_guard = ctx.transaction.guard();
    assert!(matches!(
        tx.seal(&terminal_guard, &live, fixture.roots.height + 1),
        Err(ExecutionError::Aborted)
    ));
}
#[test]
fn current_producing_order_matches_world_and_compares_entire_factory_spec() {
    current_equals_independent_world(producing_world(), false);
    let mut changed_tags = producing_world();
    changed_tags
        .state
        .factories
        .get_mut("assembler")
        .unwrap()
        .spec
        .tags
        .push("noncanonical".into());
    current_equals_independent_world(changed_tags, true);
    let mut changed_cost = producing_world();
    changed_cost
        .state
        .factories
        .get_mut("assembler")
        .unwrap()
        .spec
        .build_cost[2]
        .amount += 1;
    current_equals_independent_world(changed_cost, true);
}
#[test]
fn current_producing_order_reads_actual_site_and_anchor() {
    let mut site = producing_world();
    site.state
        .factory_site_authorities
        .get_mut("site")
        .unwrap()
        .chunk_ready = false;
    current_equals_independent_world(site, true);
    let mut anchor = producing_world();
    anchor
        .state
        .location_anchors
        .get_mut("location")
        .unwrap()
        .active = false;
    current_equals_independent_world(anchor, true);
}
