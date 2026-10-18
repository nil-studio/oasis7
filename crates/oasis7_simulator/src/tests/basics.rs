use super::*;

#[test]
fn resource_stock_add_remove() {
    let mut stock = ResourceStock::new();
    stock.add(ResourceKind::Electricity, 10).unwrap();
    stock.add(ResourceKind::Electricity, 5).unwrap();
    assert_eq!(stock.get(ResourceKind::Electricity), 15);

    stock.remove(ResourceKind::Electricity, 6).unwrap();
    assert_eq!(stock.get(ResourceKind::Electricity), 9);

    let err = stock.remove(ResourceKind::Electricity, 20).unwrap_err();
    assert!(matches!(err, StockError::Insufficient { .. }));
}

#[test]
fn resource_stock_add_rejects_overflow() {
    let mut stock = ResourceStock::new();
    stock
        .set(ResourceKind::Electricity, i64::MAX - 1)
        .expect("seed");
    let err = stock
        .add(ResourceKind::Electricity, 10)
        .expect_err("overflow should be rejected");

    assert!(matches!(
        err,
        StockError::Overflow {
            kind: ResourceKind::Electricity,
            current: _,
            delta: 10
        }
    ));
    assert_eq!(stock.get(ResourceKind::Electricity), i64::MAX - 1);
}

#[test]
fn agent_and_location_defaults() {
    let position = pos(0, 0);
    let location = Location::new("loc-1", "base", position);
    let agent = Agent::new("agent-1", "loc-1", position);

    assert_eq!(location.id, "loc-1");
    assert_eq!(agent.location_id, "loc-1");
    assert_eq!(agent.body.height_cm, DEFAULT_AGENT_HEIGHT_CM);
}

#[test]
fn world_model_starts_empty() {
    let model = WorldModel::default();
    assert!(model.agents.is_empty());
    assert!(model.locations.is_empty());
    assert!(model.assets.is_empty());
    assert!(model.power_plants.is_empty());
}
