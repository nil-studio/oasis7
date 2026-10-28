//! Real signed issuer-history gate tests. Scope data is not a qualified cut.
use super::*;
use crate::formal_live_service::full_sync_read_gate::BftCommittedReadScopeV1;
fn scope(f: &IssuerFixture) -> BftCommittedReadScopeV1 {
    BftCommittedReadScopeV1 {
        world: WorldIdentity {
            world_id: f.trust.world.world_id.clone(),
            genesis_digest: f.trust.world.genesis_digest.clone(),
        },
        chain_id: f.trust.world.chain_id.clone(),
        authority_epoch: f.verified_v3.body().authority_epoch,
    }
}
fn grant(base: u64, role: FormalNodeReadRoleV1) -> FormalNodeReadGrantV1 {
    let mut body = signed_grant(&key(READER_KEY), base, 1).body;
    body.role = role;
    FormalNodeReadGrantV1::sign(body, &hex::encode([9; 32])).unwrap()
}
#[test]
fn committed_gate_rejects_forged_grant_before_remote_history_work() {
    let (_, trust, _, _, _) = full_sync_fixture();
    let base = now_ms();
    let f = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(SignedEvidenceSource::new(&f, f.lease_not_before));
    let gate = test_gate(&f, source.clone(), PROVIDER_INCARNATION);
    let mut forged = grant(base, FormalNodeReadRoleV1::StateSync);
    forged.signature_hex = hex::encode([0; 64]);
    let calls = AtomicU64::new(0);
    assert!(
        gate.authorize_bft_committed_with_response(
            &forged,
            &scope(&f),
            &public(READER_KEY),
            |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(vec![7])
            }
        )
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(source.fetch_count(), 0);
    assert!(
        gate.authorize_bft_committed_with_response(
            &grant(base, FormalNodeReadRoleV1::StateSync),
            &scope(&f),
            &public(READER_KEY),
            |_| Ok(vec![7])
        )
        .is_ok()
    );
    assert_eq!(source.fetch_count(), 1);
}
#[test]
fn committed_gate_preserves_both_legacy_roles_and_current_revoke() {
    for (role, legacy, other) in [
        (
            FullSyncReadRoleV2::StateSync,
            FormalNodeReadRoleV1::StateSync,
            FormalNodeReadRoleV1::FullReadOnly,
        ),
        (
            FullSyncReadRoleV2::FullReadOnly,
            FormalNodeReadRoleV1::FullReadOnly,
            FormalNodeReadRoleV1::StateSync,
        ),
    ] {
        let (_, trust, _, _, _) = full_sync_fixture();
        let base = now_ms();
        let f = issue_lease_fixture_role(trust.initial_trust.clone(), base, role);
        let source = Arc::new(SignedEvidenceSource::new(&f, f.lease_not_before));
        let gate = test_gate(&f, source.clone(), PROVIDER_INCARNATION);
        let who = scope(&f);
        let calls = AtomicU64::new(0);
        let original = grant(base, legacy);
        let output = gate
            .authorize_bft_committed_with_response(
                &original,
                &who,
                &public(READER_KEY),
                |deadline| {
                    assert!(deadline > Instant::now());
                    assert!(
                        deadline
                            <= Instant::now()
                                + Duration::from_millis(
                                    f.verified_v3.body().max_authorization_staleness_ms
                                )
                    );
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(vec![7, 8])
                },
            )
            .unwrap();
        assert_eq!(output, vec![7, 8]);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // A separately valid root-signed grant for the other role cannot borrow
        // this role's committed provider lease, even when a target exists.
        assert!(
            gate.authorize_bft_committed_with_response(
                &grant(base, other),
                &who,
                &public(READER_KEY),
                |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        source.revoke_target(&f);
        assert!(
            gate.authorize_bft_committed_with_response(
                &original,
                &who,
                &public(READER_KEY),
                |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
#[test]
fn committed_gate_rejects_foreign_scope_epoch_and_subject_before_callback() {
    let (_, trust, _, _, _) = full_sync_fixture();
    let base = now_ms();
    let f = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(SignedEvidenceSource::new(&f, f.lease_not_before));
    let gate = test_gate(&f, source, PROVIDER_INCARNATION);
    let original = grant(base, FormalNodeReadRoleV1::StateSync);
    let calls = AtomicU64::new(0);
    for change in 0..5 {
        let mut who = scope(&f);
        let mut subject = public(READER_KEY);
        match change {
            0 => who.world.world_id = "wrong-world".into(),
            1 => who.world.genesis_digest = issuer_hash(99),
            2 => who.chain_id = "wrong-chain".into(),
            3 => who.authority_epoch += 1,
            _ => subject = public(99),
        }
        assert!(
            gate.authorize_bft_committed_with_response(&original, &who, &subject, |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .is_err()
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
fn original_full_sync_cut_epoch_cannot_borrow_current_grant_epoch() {
    let (_, trust, anchor, _, _) = full_sync_fixture();
    let base = now_ms();
    let f = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(SignedEvidenceSource::new(&f, f.lease_not_before));
    let gate = test_gate(&f, source, PROVIDER_INCARNATION);
    let original = grant(base, FormalNodeReadRoleV1::StateSync);
    let mut requested = cut(&anchor);
    let calls = AtomicU64::new(0);
    gate.authorize_with_response(&original, &requested, &public(READER_KEY), || {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
    requested.authority_epoch += 1;
    assert!(
        gate.authorize_with_response(&original, &requested, &public(READER_KEY), || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn pending_candidate_lease_cannot_authorize_committed_history() {
    let (_, trust, _, _, _) = full_sync_fixture();
    let base = now_ms();
    let f = issue_lease_fixture_role(
        trust.initial_trust.clone(),
        base,
        FullSyncReadRoleV2::BftPendingCandidateV1,
    );
    let source = Arc::new(SignedEvidenceSource::new(&f, f.lease_not_before));
    let gate = test_gate(&f, source, PROVIDER_INCARNATION);
    let calls = AtomicU64::new(0);
    assert!(
        gate.authorize_bft_committed_with_response(
            &grant(base, FormalNodeReadRoleV1::StateSync),
            &scope(&f),
            &public(READER_KEY),
            |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
