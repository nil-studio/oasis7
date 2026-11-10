use super::super::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1;
use super::*;

fn read_work() -> IndustrialEligibilityReadWorkV1 {
    IndustrialEligibilityReadWorkV1 {
        cells_remaining: 32,
        comparison_bytes_remaining: 4096,
        max_text_bytes: 128,
    }
}

#[test]
fn owner_order_selection_keeps_runtime_and_state_delivery_rules_distinct() {
    let rows = [
        // Runtime identity accepts a delivered timestamp even in another phase.
        (
            "a",
            "manufactured",
            AgentIndustrialPhaseV1::Producing,
            Some(1),
        ),
        // A Delivered phase without the timestamp is not selected by either path.
        ("b", "manufactured", AgentIndustrialPhaseV1::Delivered, None),
        (
            "c",
            "manufactured",
            AgentIndustrialPhaseV1::Delivered,
            Some(3),
        ),
    ];
    let mut work = read_work();
    let selected = select_orders(
        rows.iter()
            .map(|(id, target, phase, time)| (*id, *target, *phase, *time)),
        "manufactured",
        &mut work,
        &mut || false,
    )
    .unwrap();

    assert!(selected.manufactured);
    assert_eq!(selected.runtime_identity, Some("a"));
    assert_eq!(selected.state_owner, Some("c"));
}

#[test]
fn owner_order_selection_is_bounded_and_checks_cancellation() {
    let rows = [("order", "agent", AgentIndustrialPhaseV1::Delivered, Some(1))];
    let mut work = read_work();
    work.comparison_bytes_remaining = 1;
    assert!(matches!(
        select_orders(rows.iter().copied(), "agent", &mut work, &mut || false),
        Err(ValueTreeError::ResourceLimited(_))
    ));

    let mut work = read_work();
    assert!(matches!(
        select_orders(rows.iter().copied(), "agent", &mut work, &mut || true),
        Err(ValueTreeError::Cancelled)
    ));
}

#[test]
fn current_world_binding_digest_matches_the_original_json_tuple() {
    let world_id = "world/\"one";
    let chain_id = "chain\\two";
    let genesis = "genesis\nref";
    let expected =
        crate::util::hash_json(&("oasis7-world-rule-world-v1", world_id, chain_id, genesis))
            .unwrap();
    let expected = hex::decode(expected).unwrap();
    let mut work = 1 << 20;
    let actual =
        world_rule_binding_digest(world_id, chain_id, genesis, &mut work, || false).unwrap();

    assert_eq!(actual.as_slice(), expected);
    assert!(work < 1 << 20);
}

#[test]
fn current_world_binding_digest_fails_closed_on_work_or_cancellation() {
    let mut work = 1;
    assert!(matches!(
        world_rule_binding_digest("world", "chain", "genesis", &mut work, || false),
        Err(ValueTreeError::ResourceLimited(_))
    ));

    let mut work = 1 << 20;
    assert!(matches!(
        world_rule_binding_digest("world", "chain", "genesis", &mut work, || true),
        Err(ValueTreeError::Cancelled)
    ));
}
