//! Compare actual typed current settlement with the old materialized kernel.
use super::*;
use oasis7_agent_api::world_action::ResourceKind;
#[test]
fn current_global_rule_cost_matches_world_and_insufficient_cost_writes_nothing() {
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    world.state.resources.remove(&ResourceKind::Data);
    world.state.resources.insert(ResourceKind::Electricity, 100);
    let cost = crate::ResourceDelta {
        entries: [(ResourceKind::Electricity, -20)].into(),
    };
    world.apply_resource_delta(&cost).unwrap();
    let expected = world.resource_balance(ResourceKind::Electricity);
    // GEN starts at the original balance; the reference World remains only
    // until both old-kernel expected values have been calculated.
    world.state.resources.insert(ResourceKind::Electricity, 100);
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [93; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut read_io = genesis::io();
    let mut keys = genesis::keys();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut cas = fixture.cas.clone();
    let mut write_io = genesis::io();
    let mut write_keys = genesis::keys();
    let mut write = genesis::member(&mut cas, &memory, &mut write_io, &mut write_keys, || false);
    let settle = crate::world::incremental_rule_resources::settle_current_resources;
    assert!(matches!(
        &*settle(&cost, &mut overlay, &mut write, &live).unwrap(),
        crate::world::incremental_rule_resources::RuleResourceOutcome::Applied
    ));
    assert_eq!(
        overlay.resource(ResourceKind::Electricity).unwrap(),
        Some(expected)
    );
    // A second settlement must consume 80 from the current overlay, not 100
    // from the qualified parent. A missing Data balance then rejects the entire
    // multi-resource debit, including its otherwise affordable Electricity.
    assert!(matches!(
        &*settle(&cost, &mut overlay, &mut write, &live).unwrap(),
        crate::world::incremental_rule_resources::RuleResourceOutcome::Applied
    ));
    assert_eq!(
        overlay.resource(ResourceKind::Electricity).unwrap(),
        Some(60)
    );
    let impossible = crate::ResourceDelta {
        entries: [(ResourceKind::Electricity, -10), (ResourceKind::Data, -1)].into(),
    };
    let shortfall = settle(&impossible, &mut overlay, &mut write, &live).unwrap();
    assert!(
        matches!(&*shortfall,crate::world::incremental_rule_resources::RuleResourceOutcome::Insufficient(v) if v==&vec![(ResourceKind::Data,1)])
    );
    assert_eq!(
        overlay.resource(ResourceKind::Electricity).unwrap(),
        Some(60)
    );
    assert_eq!(overlay.resource(ResourceKind::Data).unwrap(), None);
    let max_credit = crate::ResourceDelta {
        entries: [(ResourceKind::Data, i64::MAX)].into(),
    };
    assert!(matches!(
        &*settle(&max_credit, &mut overlay, &mut write, &live).unwrap(),
        crate::world::incremental_rule_resources::RuleResourceOutcome::Applied
    ));
    let overflow = crate::ResourceDelta {
        entries: [(ResourceKind::Electricity, 1), (ResourceKind::Data, 1)].into(),
    };
    let outcome = settle(&overflow, &mut overlay, &mut write, &live).unwrap();
    assert!(
        matches!(&*outcome,crate::world::incremental_rule_resources::RuleResourceOutcome::Overflow(crate::WorldError::ResourceBalanceInvalid{reason}) if reason.contains("resource balance overflow"))
    );
    assert_eq!(
        overlay.resource(ResourceKind::Electricity).unwrap(),
        Some(60)
    );
    assert_eq!(
        overlay.resource(ResourceKind::Data).unwrap(),
        Some(i64::MAX)
    );
    let guard = overlay.transaction.guard();
    drop(overlay);
    tx.seal(&guard, &live, fixture.roots.height + 1).unwrap();
}
