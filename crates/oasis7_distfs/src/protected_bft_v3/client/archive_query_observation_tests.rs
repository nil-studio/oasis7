use super::*;
fn h(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn minimum() -> BftSuccessorWindowMinimumV1 {
    BftSuccessorWindowMinimumV1 {
        source: BftSuccessorWindowSourceV1 {
            scope: oasis7_proto::distributed_bft::v2::BftScopeV2 {
                world_id: "world".into(),
                chain_id: "chain".into(),
                genesis_hash: h(1),
            },
            binding_digest: h(2),
            handoff_position: 2,
            handoff_record_id: h(3),
            handoff_snapshot_root: h(4),
            target_digest: h(5),
            activation_digest: h(6),
            material_identity: h(7),
        },
        sequence: 1,
        height: 34,
        header_id: h(8),
        checkpoint_digest: h(9),
    }
}
#[test]
fn archive_observation_minimum_requires_both_coordinates_to_advance() {
    let lower = minimum();
    lower.validate().unwrap();
    assert!(archive_minimum_at_least(&lower, &lower));
    let mut higher = lower.clone();
    higher.height += 1;
    assert!(
        !archive_minimum_at_least(&higher, &lower),
        "height advancement cannot reuse a segment sequence"
    );
    higher = lower.clone();
    higher.sequence += 1;
    assert!(
        !archive_minimum_at_least(&higher, &lower),
        "sequence advancement cannot reuse a height"
    );
    higher.height += 1;
    assert!(archive_minimum_at_least(&higher, &lower));
}
#[test]
fn archive_observation_same_revision_requires_exact_digest_and_minimum() {
    let lower = minimum();
    let mut changed = lower.clone();
    changed.sequence += 1;
    changed.height += 1;
    // This shared predicate guards cached checkpoint -> chunk, observed chunk ->
    // chunk, and observed chunk -> newly authenticated control checkpoint paths.
    assert!(
        !archive_state_at_least(7, &h(10), &changed, 7, &h(10), &lower),
        "same revision/digest cannot carry a different minimum"
    );
    assert!(!archive_state_at_least(
        7,
        &h(11),
        &lower,
        7,
        &h(10),
        &lower
    ));
    assert!(archive_state_at_least(7, &h(10), &lower, 7, &h(10), &lower));
    assert!(archive_state_at_least(
        8,
        &h(11),
        &changed,
        7,
        &h(10),
        &lower
    ));
    assert!(archive_state_at_least(8, &h(11), &lower, 7, &h(10), &lower));
    assert!(!archive_state_at_least(
        6,
        &h(10),
        &lower,
        7,
        &h(10),
        &lower
    ));
}
