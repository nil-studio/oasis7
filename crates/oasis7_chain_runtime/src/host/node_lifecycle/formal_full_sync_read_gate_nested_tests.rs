//! Actual signed-history gate reentrancy and post-callback authority binding.
use super::*;

fn bounded(case: impl FnOnce() + Send + 'static) {
    let (done, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        case();
        let _ = done.send(());
    });
    assert!(
        result.recv_timeout(Duration::from_secs(5)).is_ok(),
        "actual issuer gate callback must complete without retaining its own installed lock"
    );
    worker.join().unwrap();
}

#[test]
fn actual_issuer_gate_nested_callback_reuses_stable_installed_history() {
    bounded(|| {
        let (_, trust, anchor, _, _) = full_sync_fixture();
        let base = now_ms();
        let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
        let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
        let gate = test_gate(&fixture, source.clone(), PROVIDER_INCARNATION);
        let grant = signed_grant(&key(READER_KEY), base, 1);
        let calls = AtomicU64::new(0);
        let result = gate
            .authorize_with_response(&grant, &cut(&anchor), &public(READER_KEY), || {
                calls.fetch_add(1, Ordering::SeqCst);
                gate.authorize_with_response(&grant, &cut(&anchor), &public(READER_KEY), || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(vec![1, 2, 3])
                })
            })
            .unwrap();
        assert_eq!(result, vec![1, 2, 3]);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            source.fetch_count(),
            2,
            "nested part must perform its own real witness fetch"
        );
    });
}

#[test]
fn actual_issuer_gate_callback_rejects_concurrently_installed_revocation() {
    bounded(|| {
        let (_, trust, anchor, _, _) = full_sync_fixture();
        let base = now_ms();
        let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
        let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
        let gate = test_gate(&fixture, source.clone(), PROVIDER_INCARNATION);
        let grant = signed_grant(&key(READER_KEY), base, 1);
        let result =
            gate.authorize_with_response(&grant, &cut(&anchor), &public(READER_KEY), || {
                source.revoke_target(&fixture);
                let concurrent = gate.clone();
                let concurrent_grant = grant.clone();
                let concurrent_cut = cut(&anchor);
                let rejected = std::thread::spawn(move || {
                    concurrent.authorize_with_response::<Vec<u8>>(
                        &concurrent_grant,
                        &concurrent_cut,
                        &public(READER_KEY),
                        || panic!("revoked target cannot construct material"),
                    )
                })
                .join()
                .unwrap();
                assert!(rejected.is_err());
                Ok(vec![4, 5, 6])
            });
        assert!(
            matches!(result, Err(FormalReadError::HistoryUnavailable(_))),
            "old outer response must not use the newly installed revoked history"
        );
        assert_eq!(source.fetch_count(), 2);
    });
}

#[test]
fn actual_issuer_gate_nested_callback_preserves_original_error() {
    bounded(|| {
        let (_, trust, anchor, _, _) = full_sync_fixture();
        let base = now_ms();
        let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
        let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
        let gate = test_gate(&fixture, source.clone(), PROVIDER_INCARNATION);
        let grant = signed_grant(&key(READER_KEY), base, 1);
        let expected = FormalReadError::Integrity("original material construction failed".into());
        let result = gate.authorize_with_response::<Vec<u8>>(
            &grant,
            &cut(&anchor),
            &public(READER_KEY),
            || {
                gate.authorize_with_response(
                    &grant,
                    &cut(&anchor),
                    &public(READER_KEY),
                    || Ok(()),
                )?;
                Err(expected.clone())
            },
        );
        assert_eq!(result.unwrap_err(), expected);
        assert_eq!(source.fetch_count(), 2);
    });
}

#[test]
fn actual_issuer_gate_callback_cannot_release_after_original_deadline() {
    // No production timeout changes: cross the exact deadline supplied by the gate.
    let (_, trust, anchor, _, _) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), base);
    let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
    let gate = test_gate(&fixture, source.clone(), PROVIDER_INCARNATION);
    let grant = signed_grant(&key(READER_KEY), base, 1);
    let original_cut = cut(&anchor);
    let requested = crate::formal_live_service::full_sync_read_gate::BftCommittedReadScopeV1 {
        world: original_cut.world,
        chain_id: original_cut.chain_id,
        authority_epoch: original_cut.authority_epoch,
    };
    let material = AtomicU64::new(0);
    let result = gate.authorize_bft_committed_with_response(
        &grant,
        &requested,
        &public(READER_KEY),
        |deadline| {
            std::thread::sleep(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
            material.fetch_add(1, Ordering::SeqCst);
            Ok(vec![1, 2, 3])
        },
    );
    let error = result.unwrap_err();
    // The original lease is rechecked before check_time_bounds; its exact
    // protected interval has expired after the callback crosses that deadline.
    assert_eq!(
        error,
        FormalReadError::GrantRejected(
            "no exact committed full-sync V3 lease covers this provider, incarnation, target and protected time".into()
        )
    );
    assert_eq!(material.load(Ordering::SeqCst), 1);
    assert_eq!(source.fetch_count(), 1);
}
