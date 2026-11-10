//! Legacy material semantics and error priority for bounded scalar consumers.
use oasis7_world_state::{MaterialLedgerId, MaterialStack, state::support::*};
use std::collections::BTreeMap;

#[test]
fn material_preflight_checks_input_errors_before_sorted_raw_balances() {
    let ledger = MaterialLedgerId::World;
    let balances = BTreeMap::from([(
        ledger.clone(),
        BTreeMap::from([("a".into(), i64::MAX), ("z".into(), i64::MAX)]),
    )]);
    let cases = [
        (
            vec![MaterialStack::new("a", 1), MaterialStack::new("z", -1)],
            "negative material amount not allowed: -1",
        ),
        (
            vec![
                MaterialStack::new("z", i64::MAX),
                MaterialStack::new("z", 1),
                MaterialStack::new("a", -1),
            ],
            "material balance addition overflow: kind=z amount=1",
        ),
        (
            vec![MaterialStack::new("z", 1), MaterialStack::new("a", 1)],
            "material balance overflow: kind=a current=9223372036854775807 amount=1",
        ),
    ];
    for (stacks, expected) in cases {
        let before = balances.clone();
        assert_eq!(
            preflight_material_balance_additions(&balances, &ledger, &stacks),
            Err(expected.into())
        );
        assert_eq!(balances, before);
    }
}

#[test]
fn signed_material_balances_keep_negative_additions_and_removal_priority() {
    let ledger = MaterialLedgerId::World;
    let mut balances = BTreeMap::from([(ledger.clone(), BTreeMap::from([("ore".into(), -5)]))]);
    add_material_balance_for_ledger(&mut balances, &ledger, "ore", -2).unwrap();
    assert_eq!(balances[&ledger]["ore"], -7);
    assert_eq!(checked_material_balance_add_v1("ore", -5, -2), Ok(-7));
    assert_eq!(
        remove_material_balance_for_ledger(&mut balances, &ledger, "ore", -1),
        Err("negative material amount not allowed: -1".into())
    );
    assert_eq!(
        remove_material_balance_for_ledger(&mut balances, &ledger, "ore", 0),
        Err("insufficient material ore: requested=0 available=-7".into())
    );
    add_material_balance_for_ledger(&mut balances, &ledger, "ore", 7).unwrap();
    assert!(!balances[&ledger].contains_key("ore"));
    add_material_balance_for_ledger(&mut balances, &ledger, "ore", 3).unwrap();
    remove_material_balance_for_ledger(&mut balances, &ledger, "ore", 3).unwrap();
    assert!(!balances[&ledger].contains_key("ore"));
}
