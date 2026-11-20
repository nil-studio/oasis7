//! Cached computation must remain below the native durable Prepare boundary.
use super::*;

#[test]
fn recovery_commit_adapter_uses_original_authority_from_successor_anchor() {
    use oasis7_formal_protocol::controlled_authority::formal::{
        InitialActivationTrustV1, test_fixtures as protocol_fixtures,
    };

    let (certificate, _, initial_proof, _) = protocol_fixtures::initial();
    let trust = InitialActivationTrustV1 {
        issuer_public_key: protocol_fixtures::public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
    };
    let initial =
        VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &initial_proof).unwrap();
    for branch in [
        FormalCompletionBranchV1::ReplayFrozen,
        FormalCompletionBranchV1::CompletePrepared,
    ] {
        let (intent, original, cut, frozen, record) =
            protocol_fixtures::recovery_fixture_for(branch, None);
        let recovery = protocol_fixtures::verified_recovery(&intent, &original);
        let anchor = initial.with_pending_recovery(&recovery).unwrap();
        assert_eq!(anchor.head().head(), cut.head());
        assert_ne!(anchor.authority().definition(), original.definition());
        let proof = protocol_fixtures::proof(
            record.clone(),
            recovery.successor(),
            branch,
            Some(recovery.certificate_digest().into()),
            Some(frozen),
            if branch == FormalCompletionBranchV1::ReplayFrozen {
                10
            } else {
                4
            },
            11,
            12,
        );
        let verified = saved_candidate::verify_qualified_commit(
            &anchor,
            Some(&recovery),
            &proof,
            record.position,
        )
        .expect("active successor anchor must retain the original authority for recovery");
        assert_eq!(verified.record(), &record);
        assert_eq!(
            verified.finalizing_authority_id(),
            anchor.authority().authority_id().unwrap()
        );
        assert_eq!(
            saved_candidate::verify_qualified_commit(&anchor, None, &proof, record.position)
                .unwrap_err(),
            "recovery proof lacks checked context"
        );
        let (other_intent, _, _, _, _) =
            protocol_fixtures::recovery_fixture_with_profile(branch, None, 13, 1);
        let other_recovery = protocol_fixtures::verified_recovery(&other_intent, &original);
        assert!(
            saved_candidate::verify_qualified_commit(
                &anchor,
                Some(&other_recovery),
                &proof,
                record.position,
            )
            .is_err()
        );
        assert!(
            saved_candidate::verify_qualified_commit(
                &anchor,
                Some(&recovery),
                &proof,
                record.position + 1,
            )
            .unwrap_err()
            .contains("candidate position mismatch")
        );
    }
}

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
