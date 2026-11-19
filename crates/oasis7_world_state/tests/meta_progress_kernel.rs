use oasis7_world_state::{
    MetaProgressState, world::governance_meta_publication::apply_meta_progress_delta_v1,
};
use std::collections::BTreeMap;

#[test]
fn meta_delta_preserves_outer_independent_identity_and_sticky_tiers() {
    let mut progress = MetaProgressState {
        agent_id: "embedded-other".into(),
        track_points: BTreeMap::from([("science".into(), i64::MAX - 1)]),
        total_points: -5,
        achievements: vec!["z".into(), "a".into(), "a".into()],
        unlocked_tiers: BTreeMap::from([(
            "science".into(),
            vec!["bronze".into(), "bronze".into()],
        )]),
        last_granted_at: 0,
    };
    let mut visits = 0;
    apply_meta_progress_delta_v1(&mut progress, "science", 10, Some("a"), 7, |_| {
        visits += 1;
        Ok::<(), std::convert::Infallible>(())
    })
    .unwrap();
    assert!(visits > 0);
    assert_eq!(progress.agent_id, "embedded-other");
    assert_eq!(progress.track_points["science"], i64::MAX);
    assert_eq!(progress.total_points, 5);
    assert_eq!(progress.last_granted_at, 7);
    assert_eq!(
        progress.unlocked_tiers["science"],
        ["bronze", "gold", "silver"]
    );
    assert_eq!(
        progress.achievements,
        [
            "a",
            "tier.science.bronze",
            "tier.science.gold",
            "tier.science.silver",
            "z"
        ]
    );
    let earned = progress.achievements.clone();
    apply_meta_progress_delta_v1(&mut progress, "science", -i64::MAX, None, 8, |_| {
        Ok::<(), std::convert::Infallible>(())
    })
    .unwrap();
    assert_eq!(progress.track_points["science"], 0);
    assert_eq!(progress.total_points, 5 - i64::MAX);
    assert_eq!(progress.achievements, earned);
    assert_eq!(
        progress.unlocked_tiers["science"],
        ["bronze", "gold", "silver"]
    );
    apply_meta_progress_delta_v1(&mut progress, "science", i64::MIN, None, 9, |_| {
        Ok::<(), std::convert::Infallible>(())
    })
    .unwrap();
    assert_eq!(progress.track_points["science"], i64::MIN);
    assert_eq!(progress.total_points, i64::MIN);
    assert_eq!(progress.last_granted_at, 9);
}

#[test]
fn exhausted_kernel_visit_fails_before_mutating_the_owned_record() {
    let mut progress = MetaProgressState {
        agent_id: "inner".into(),
        track_points: BTreeMap::new(),
        total_points: 0,
        achievements: Vec::new(),
        unlocked_tiers: BTreeMap::new(),
        last_granted_at: 4,
    };
    let before = progress.clone();
    let result =
        apply_meta_progress_delta_v1(&mut progress, "science", 100, Some("earned"), 8, |_| {
            Err("work exhausted")
        });
    assert_eq!(result, Err("work exhausted"));
    assert_eq!(progress, before);
}
