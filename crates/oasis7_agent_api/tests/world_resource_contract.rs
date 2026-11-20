use oasis7_agent_api::world_action::ResourceKind;
use oasis7_agent_api::world_resource::{
    ResourceStock, StockError, checked_resource_add_v1, checked_resource_remove_v1,
};
use oasis7_domain_types::world_resource as domain;

#[test]
fn legacy_resource_paths_share_the_domain_owner() {
    let kind: domain::ResourceKind = ResourceKind::Electricity;
    let mut stock: domain::ResourceStock = ResourceStock::new();
    assert_eq!(stock.add(kind, 4), Ok(()));

    let added: Result<i64, domain::StockError> = checked_resource_add_v1(kind, stock.get(kind), 3);
    assert_eq!(added, Ok(7));
    let removed: Result<i64, StockError> = domain::checked_resource_remove_v1(kind, 7, 3);
    assert_eq!(removed, Ok(4));
    assert_eq!(checked_resource_remove_v1(kind, 4, 4), Ok(0));
    assert_eq!(stock.remove(kind, 4), Ok(()));
    assert!(stock.amounts.is_empty());
}

#[test]
fn legacy_resource_paths_preserve_arithmetic_errors() {
    let kind = ResourceKind::Data;
    assert_eq!(
        checked_resource_add_v1(kind, i64::MIN, -1),
        Err(StockError::NegativeAmount { amount: -1 })
    );
    assert_eq!(
        checked_resource_remove_v1(kind, i64::MIN, -1),
        Err(StockError::NegativeAmount { amount: -1 })
    );
    assert_eq!(
        checked_resource_add_v1(kind, -4, 3),
        Err(StockError::NegativeAmount { amount: -1 })
    );
    assert_eq!(checked_resource_add_v1(kind, -4, 4), Ok(0));
    assert_eq!(checked_resource_add_v1(kind, -4, 5), Ok(1));
    assert_eq!(
        checked_resource_remove_v1(kind, -1, 0),
        Err(StockError::Insufficient {
            kind,
            requested: 0,
            available: -1,
        })
    );

    let mut stock = ResourceStock::new();
    stock.set(kind, i64::MAX).unwrap();
    let before = stock.clone();
    let expected: domain::StockError = StockError::Overflow {
        kind,
        current: i64::MAX,
        delta: 1,
    };
    assert_eq!(stock.add(kind, 1), Err(expected));
    assert_eq!(stock, before);
}
