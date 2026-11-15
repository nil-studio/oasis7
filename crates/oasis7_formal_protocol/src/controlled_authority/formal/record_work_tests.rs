use super::*;
use crate::controlled_authority::formal::test_fixtures;

#[derive(Debug)]
enum Stop {
    Invalid(FormalError),
    Budget,
}
impl From<FormalError> for Stop {
    fn from(value: FormalError) -> Self {
        Self::Invalid(value)
    }
}
fn sample() -> CanonicalRecordV1 {
    test_fixtures::initial().3.record().clone()
}
fn iws() -> CanonicalRecordV1 {
    let mut r = sample();
    r.schema_version = 7;
    r.artifact_roots = [
        (ArtifactRole::Input, r.payload_digest.clone()),
        (ArtifactRole::IncrementalGenesisConfig, r.world.genesis_digest.clone()),
        (ArtifactRole::IncrementalRootSet, r.after_state_root.clone()),
        (ArtifactRole::ExecutionManifest, r.runtime_manifest_digest.clone()),
        (ArtifactRole::IncrementalCommitBody, "01".repeat(32)),
        (ArtifactRole::IncrementalHistoryPrefix, "02".repeat(32)),
        (ArtifactRole::Wasm, "03".repeat(32)),
        (ArtifactRole::Rules, "04".repeat(32)),
        (ArtifactRole::NativeExecutionEnvironment, "05".repeat(32)),
    ].into_iter().collect();
    r
}
#[test]
fn metered_record_preserves_valid_legacy_and_iws_wire() {
    for r in [sample(), iws()] {
        let before = r.record_id().unwrap();
        let mut calls = Vec::new();
        r.validate_with_work(&mut |n| { calls.push(n); Ok::<(), Stop>(()) }).unwrap();
        assert!(calls.iter().any(|n| *n > 0));
        assert!(calls.contains(&0));
        assert_eq!(r.record_id().unwrap(), before);
    }
}
#[test]
fn metered_record_every_work_boundary_can_stop_without_mutation() {
    for r in [sample(), iws()] {
        let original = r.clone();
        let mut count = 0;
        r.validate_with_work(&mut |_| { count += 1; Ok::<(), Stop>(()) }).unwrap();
        for stop_at in 1..=count {
            let mut calls = 0;
            let result = r.validate_with_work(&mut |_| {
                calls += 1;
                if calls == stop_at { Err(Stop::Budget) } else { Ok(()) }
            });
            assert!(matches!(result, Err(Stop::Budget)));
            assert_eq!(calls, stop_at);
            assert_eq!(r, original);
        }
    }
}
#[test]
fn metered_record_keeps_original_first_validation_error() {
    let mut cases = vec![sample(), iws(), sample(), sample(), iws()];
    cases[0].world.world_id = " ".into();
    cases[0].position = 0;
    cases[1].artifact_roots.insert(ArtifactRole::Input, "bad".into());
    cases[2].parent_record_id = "bad".into();
    cases[3].kind = FormalRecordKindV1::Execution;
    cases[3].frozen_reservation_id = Some("06".repeat(32));
    cases[3].handoff = Some(ProfileHandoffV1 {
        next_authority_digest: "bad".into(),
        next_profile: "".into(),
        next_profile_version: 0,
        valid_from_position: 0,
    });
    cases[4].artifact_roots.remove(&ArtifactRole::Rules);
    // Fixed messages from the pre-extraction validator, not the new wrapper.
    let errors = [
        "formal world identity bounds",
        "incremental record original input root",
        "formal digest/key requires canonical lowercase hex32",
        "formal digest/key requires canonical lowercase hex32",
        "formal record version/position/closure roots",
    ];
    for (r, reason) in cases.into_iter().zip(errors) {
        let expected = FormalError::Invalid(reason.into());
        assert_eq!(r.validate().unwrap_err(), expected);
        let result = r.validate_with_work(&mut |_| Ok::<(), Stop>(()));
        match result {
            Err(Stop::Invalid(error)) => assert_eq!(error, expected),
            other => panic!("unexpected validation result: {other:?}"),
        }
    }
}

#[test]
fn metered_handoff_preserves_boundary_and_work_failure() {
    let h = ProfileHandoffV1 {
        next_authority_digest: "07".repeat(32),
        next_profile: "controlled_single_authority".into(),
        next_profile_version: 1,
        valid_from_position: 3,
    };
    h.validate_with_work(2, &mut |_| Ok::<(), Stop>(())).unwrap();
    assert_eq!(h.validate(3), Err(FormalError::Invalid("formal handoff target/boundary".into())));
    let mut calls = 0;
    let result = h.validate_with_work(2, &mut |_| {
        calls += 1;
        if calls == 2 { Err(Stop::Budget) } else { Ok(()) }
    });
    assert!(matches!(result, Err(Stop::Budget)));
    assert_eq!(calls, 2);
}
