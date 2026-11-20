use super::{
    ResourceKind, ResourceStock, StockError, checked_resource_add_v1, checked_resource_remove_v1,
};
use std::collections::BTreeMap;

#[test]
fn scalar_add_preserves_results_and_error_order() {
    for kind in [ResourceKind::Electricity, ResourceKind::Data] {
        let cases = [
            (0, 0, Ok(0)),
            (4, 3, Ok(7)),
            (i64::MAX - 1, 1, Ok(i64::MAX)),
            (i64::MAX, 0, Ok(i64::MAX)),
            (
                i64::MAX,
                1,
                Err(StockError::Overflow {
                    kind,
                    current: i64::MAX,
                    delta: 1,
                }),
            ),
            (i64::MIN, -1, Err(StockError::NegativeAmount { amount: -1 })),
            (
                0,
                i64::MIN,
                Err(StockError::NegativeAmount { amount: i64::MIN }),
            ),
            (-4, 3, Err(StockError::NegativeAmount { amount: -1 })),
            (-4, 4, Ok(0)),
            (-4, 5, Ok(1)),
            (
                i64::MIN,
                i64::MAX,
                Err(StockError::NegativeAmount { amount: -1 }),
            ),
        ];
        for (current, amount, expected) in cases {
            assert_eq!(
                checked_resource_add_v1(kind, current, amount),
                expected,
                "{kind:?}: {current} + {amount}"
            );
        }
    }
}

#[test]
fn scalar_remove_preserves_results_and_error_order() {
    for kind in [ResourceKind::Electricity, ResourceKind::Data] {
        let cases = [
            (0, 0, Ok(0)),
            (7, 3, Ok(4)),
            (i64::MAX, 0, Ok(i64::MAX)),
            (i64::MAX, i64::MAX, Ok(0)),
            (i64::MIN, -1, Err(StockError::NegativeAmount { amount: -1 })),
            (
                0,
                i64::MIN,
                Err(StockError::NegativeAmount { amount: i64::MIN }),
            ),
            (
                0,
                1,
                Err(StockError::Insufficient {
                    kind,
                    requested: 1,
                    available: 0,
                }),
            ),
            (
                -1,
                0,
                Err(StockError::Insufficient {
                    kind,
                    requested: 0,
                    available: -1,
                }),
            ),
            (
                i64::MIN,
                i64::MAX,
                Err(StockError::Insufficient {
                    kind,
                    requested: i64::MAX,
                    available: i64::MIN,
                }),
            ),
        ];
        for (current, amount, expected) in cases {
            assert_eq!(
                checked_resource_remove_v1(kind, current, amount),
                expected,
                "{kind:?}: {current} - {amount}"
            );
        }
    }
}

type StockUpdate = fn(&mut ResourceStock, ResourceKind, i64) -> Result<(), StockError>;

#[test]
fn failed_updates_leave_the_entire_stock_unchanged() {
    let kind = ResourceKind::Electricity;
    let add: StockUpdate = ResourceStock::add;
    let remove: StockUpdate = ResourceStock::remove;
    let cases = [
        (add, i64::MIN, -1, StockError::NegativeAmount { amount: -1 }),
        (
            remove,
            i64::MIN,
            -1,
            StockError::NegativeAmount { amount: -1 },
        ),
        (
            add,
            i64::MAX,
            1,
            StockError::Overflow {
                kind,
                current: i64::MAX,
                delta: 1,
            },
        ),
        (add, -4, 3, StockError::NegativeAmount { amount: -1 }),
        (
            remove,
            2,
            3,
            StockError::Insufficient {
                kind,
                requested: 3,
                available: 2,
            },
        ),
        (
            remove,
            -1,
            0,
            StockError::Insufficient {
                kind,
                requested: 0,
                available: -1,
            },
        ),
    ];
    for (update, current, amount, expected) in cases {
        let mut stock = ResourceStock {
            amounts: BTreeMap::from([(kind, current), (ResourceKind::Data, 17)]),
        };
        let before = stock.clone();
        assert_eq!(update(&mut stock, kind, amount), Err(expected));
        assert_eq!(stock, before);
    }
}

#[test]
fn successful_updates_preserve_sparse_stock_and_legacy_negative_balances() {
    let kind = ResourceKind::Electricity;
    let updates: [StockUpdate; 2] = [ResourceStock::add, ResourceStock::remove];
    for update in updates {
        let mut stock = ResourceStock::new();
        assert_eq!(update(&mut stock, kind, 0), Ok(()));
        assert!(stock.amounts.is_empty());
        stock.amounts.insert(kind, 0);
        assert_eq!(update(&mut stock, kind, 0), Ok(()));
        assert!(stock.amounts.is_empty());
    }

    let mut stock: ResourceStock =
        serde_json::from_str(r#"{"amounts":{"electricity":-4,"data":17}}"#).unwrap();
    assert_eq!(stock.add(kind, 4), Ok(()));
    assert!(!stock.amounts.contains_key(&kind));
    assert_eq!(stock.get(ResourceKind::Data), 17);

    stock.amounts.insert(kind, -4);
    assert_eq!(stock.add(kind, 5), Ok(()));
    assert_eq!(stock.get(kind), 1);
    assert_eq!(stock.remove(kind, 1), Ok(()));
    assert_eq!(stock.amounts, BTreeMap::from([(ResourceKind::Data, 17)]));
}
