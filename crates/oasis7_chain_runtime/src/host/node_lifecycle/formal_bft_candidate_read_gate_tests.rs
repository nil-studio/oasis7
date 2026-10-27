//! Real issuer history and route-facing gate; this is not a production witness.
use super::*;
use crate::formal_live_service::full_sync_read_gate::BftCandidateReadSubjectV1;

fn subject(f: &IssuerFixture) -> BftCandidateReadSubjectV1 {
    BftCandidateReadSubjectV1 {
        world: oasis7_client_api::world_service::WorldIdentity {
            world_id: f.trust.world.world_id.clone(),
            genesis_digest: f.trust.world.genesis_digest.clone(),
        },
        chain_id: f.trust.world.chain_id.clone(),
        authority_epoch: f.verified_v3.body().authority_epoch,
        node_public_key: f.target.node_public_key.clone(),
        policy_digest: f.verified_v3.policy_digest().into(),
        grant_generation: 1,
    }
}
fn candidate_fixture() -> IssuerFixture {
    let (_, trust, _, _, _) = full_sync_fixture();
    issue_lease_fixture_role(
        trust.initial_trust.clone(),
        now_ms(),
        FullSyncReadRoleV2::BftPendingCandidateV1,
    )
}
#[test]
fn candidate_gate_requires_a_separately_issued_purpose() {
    for role in [
        FullSyncReadRoleV2::StateSync,
        FullSyncReadRoleV2::FullReadOnly,
    ] {
        let (_, trust, _, _, _) = full_sync_fixture();
        let f = issue_lease_fixture_role(trust.initial_trust.clone(), now_ms(), role);
        let source = Arc::new(SignedEvidenceSource::new(&f, f.lease_not_before));
        let gate = test_gate(&f, source, PROVIDER_INCARNATION);
        let calls = AtomicU64::new(0);
        assert!(
            gate.authorize_candidate_with_response(&subject(&f), || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(vec![7])
            })
            .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn candidate_gate_releases_only_with_current_lease_and_stops_after_revoke() {
    let f = candidate_fixture();
    let source = Arc::new(SignedEvidenceSource::new(&f, f.lease_not_before));
    let gate = test_gate(&f, source.clone(), PROVIDER_INCARNATION);
    let who = subject(&f);
    let before = gate
        .authorize_candidate_with_response(&who, || Ok(vec![7, 8]))
        .unwrap();
    assert_eq!(before, vec![7, 8]);
    source.revoke_target(&f);
    let calls = AtomicU64::new(0);
    assert!(
        gate.authorize_candidate_with_response(&who, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![9])
        })
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
fn candidate_gate_checks_policy_generation_world_incarnation_and_clock() {
    let f = candidate_fixture();
    let source = Arc::new(SignedEvidenceSource::new(&f, f.lease_not_before));
    let gate = test_gate(&f, source.clone(), PROVIDER_INCARNATION);
    for change in 0..5 {
        let mut who = subject(&f);
        match change {
            0 => who.policy_digest = issuer_hash(99),
            1 => who.grant_generation += 1,
            2 => who.authority_epoch += 1,
            3 => who.world.genesis_digest = issuer_hash(99),
            _ => who.node_public_key = public(99),
        }
        assert!(
            gate.authorize_candidate_with_response(&who, || Ok(vec![7]))
                .is_err()
        );
    }
    let wrong_incarnation = test_gate(&f, source.clone(), PROVIDER_INCARNATION + 1);
    assert!(
        wrong_incarnation
            .authorize_candidate_with_response(&subject(&f), || Ok(vec![7]))
            .is_err()
    );
    assert!(
        gate.authorize_candidate_with_response(&subject(&f), || Ok(vec![7]))
            .is_ok()
    );
    source.set_time(f.lease_expires);
    assert!(
        gate.authorize_candidate_with_response(&subject(&f), || Ok(vec![7]))
            .is_err()
    );
    source.set_time(f.lease_not_before.saturating_sub(1));
    assert!(
        gate.authorize_candidate_with_response(&subject(&f), || Ok(vec![7]))
            .is_err()
    );
}
#[test]
fn candidate_gate_does_not_release_on_unavailable_or_trimmed_evidence() {
    let f = candidate_fixture();
    let source = Arc::new(SignedEvidenceSource::new(&f, f.lease_not_before));
    let gate = test_gate(&f, source.clone(), PROVIDER_INCARNATION);
    source.set_unavailable(true);
    assert!(
        gate.authorize_candidate_with_response(&subject(&f), || Ok(vec![7]))
            .is_err()
    );
    source.set_unavailable(false);
    source.return_bad_nonce(true);
    assert!(
        gate.authorize_candidate_with_response(&subject(&f), || Ok(vec![7]))
            .is_err()
    );
    source.return_bad_nonce(false);
    source.remove_committed_lease();
    assert!(
        gate.authorize_candidate_with_response(&subject(&f), || Ok(vec![7]))
            .is_err()
    );
}
