//! Cached computation must remain below the native durable Prepare boundary.
use super::*;
#[test]
fn formal_cached_candidate_matches_exact_frozen_and_cannot_assert_prepared_rank() {
    let policy = ReleaseSecurityPolicy::default();
    let (world, _, anchor, _) = fixtures::actual_world_initial_fixture_with_execution_policy(
        formal_package::execution_policy_material(&policy)
            .unwrap()
            .2,
    );
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("oasis7-saved-candidate-{}", std::process::id()));
    assert!(!root.exists());
    let mut writer = FormalWriter::open(
        root.clone(),
        fixtures::key(4),
        policy,
        world.clone(),
        anchor.clone(),
        endpoint(&anchor, EndpointRole::Primary, false),
        endpoint(&anchor, EndpointRole::Replica, false),
    )
    .unwrap();
    writer.prepare(&continuation(&world)).unwrap();
    let candidate = writer.candidate_for_resume().unwrap();
    let mut remote = writer.pending.clone().unwrap();
    // Native Frozen readback carries the dual certificate; local prepare does not.
    remote.frozen_certificate = candidate.frozen.clone();
    assert!(remote.candidate.is_none());
    let mut local = remote.clone();
    local.candidate = Some(candidate.clone());
    assert_eq!(
        saved_candidate::checked_cached_candidate(&anchor, None, &local, &remote).unwrap(),
        Some(candidate.clone())
    );
    let mut forged = local.clone();
    forged
        .candidate
        .as_mut()
        .unwrap()
        .candidate
        .writer_signature_hex = "00".repeat(64);
    assert!(saved_candidate::checked_cached_candidate(&anchor, None, &forged, &remote).is_err());
    let mut altered = local.clone();
    altered
        .candidate
        .as_mut()
        .unwrap()
        .frozen_material
        .as_mut()
        .unwrap()
        .objects[0]
        .bytes
        .push(0);
    assert!(saved_candidate::checked_cached_candidate(&anchor, None, &altered, &remote).is_err());
    let mut absent = remote.clone();
    absent.frozen_certificate = None;
    assert!(saved_candidate::checked_cached_candidate(&anchor, None, &local, &absent).is_err());
    local.candidate.as_mut().unwrap().branch = FormalCompletionBranchV1::CompletePrepared;
    assert!(
        saved_candidate::checked_cached_candidate(&anchor, None, &local, &remote)
            .unwrap()
            .is_none()
    );
    drop(writer);
    std::fs::remove_dir_all(root).unwrap();
}
