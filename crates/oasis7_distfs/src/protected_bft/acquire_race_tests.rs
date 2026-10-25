use super::*;

#[test]
fn acquire_reprepares_after_old_connection_release_between_prepare_and_cas() {
    let fixture = Fixture::new();
    let provider = fixture.provider();
    let mut old_lease = Some(provider.acquire(&fixture.binding).expect("old lease"));
    let mut release_old_lease = true;
    let mut prepare_attempts = 0;

    let new_lease = provider.acquire_with_prepare_interleave(&fixture.binding, || {
        prepare_attempts += 1;
        if release_old_lease {
            drop(old_lease.take());
            wait_for_generation(&fixture.backend, 2);
            release_old_lease = false;
        }
    });
    let mut new_lease = new_lease.expect(
        "a signed stale-generation conflict after a completed release should trigger fresh prepare",
    );
    assert_eq!(prepare_attempts, 2, "retry only after the release conflict");
    assert_eq!(
        new_lease
            .checkpoint()
            .expect("new lease checkpoint")
            .state
            .revision,
        0
    );
    let ledger = fixture.backend.0.lock().expect("test backend lock");
    assert_eq!(ledger.lease_generation, 3);
    assert!(!ledger.lease_token_digest.is_empty());
}

#[test]
fn acquire_does_not_retry_when_conflict_has_a_new_active_owner() {
    let fixture = Fixture::new();
    let provider = fixture.provider();
    let mut competing_lease = None;
    let mut prepare_attempts = 0;

    let result = provider.acquire_with_prepare_interleave(&fixture.binding, || {
        prepare_attempts += 1;
        competing_lease = Some(
            provider
                .acquire(&fixture.binding)
                .expect("competing active owner"),
        );
    });
    assert!(
        result.is_err(),
        "the client must not fence or retry over a newer active owner"
    );
    assert_eq!(prepare_attempts, 1);
    {
        let ledger = fixture.backend.0.lock().expect("test backend lock");
        assert_eq!(ledger.lease_generation, 1);
        assert!(!ledger.lease_token_digest.is_empty());
    }
    drop(competing_lease);
    wait_for_generation(&fixture.backend, 2);
}

#[test]
fn acquire_retries_at_most_three_inactive_generation_conflicts() {
    let fixture = Fixture::new();
    let provider = fixture.provider();
    let mut prepare_attempts = 0;
    let mut expected_generation = 0u64;

    let result = provider.acquire_with_prepare_interleave(&fixture.binding, || {
        prepare_attempts += 1;
        let competitor = provider
            .acquire(&fixture.binding)
            .expect("temporary competing lease");
        drop(competitor);
        expected_generation = expected_generation
            .checked_add(2)
            .expect("test generation bound");
        wait_for_generation(&fixture.backend, expected_generation);
    });
    assert!(result.is_err(), "continuous churn must be bounded");
    assert_eq!(prepare_attempts, 3);
    let ledger = fixture.backend.0.lock().expect("test backend lock");
    assert_eq!(ledger.lease_generation, 6);
    assert!(ledger.lease_token_digest.is_empty());
}

#[test]
fn acquire_cas_returns_latest_valid_state_after_old_lease_progresses() {
    let fixture = Fixture::new();
    let old_provider = fixture.provider();
    let mut old_lease = old_provider.acquire(&fixture.binding).expect("old lease");
    let target_server = TcpServiceServer::start(fixture.reopened_service());
    let trust = BftSignerTrustV1 {
        schema_version: 1,
        provider_public_key: hex::encode(fixture.provider_key.verifying_key().to_bytes()),
        binding_digest: fixture.binding.digest().expect("binding digest"),
    };
    let target_provider = RemoteBftSignerProviderV1::new(
        target_server.address,
        Duration::from_secs(2),
        fixture.binding.clone(),
        fixture.requester_key.clone(),
        trust,
    )
    .expect("target provider");
    let (entered, resume) = fixture.backend.pause_next_acquire();
    let binding = fixture.binding.clone();
    let acquire_thread = thread::spawn(move || target_provider.acquire(&binding));

    entered
        .recv_timeout(Duration::from_secs(2))
        .expect("target backend pauses after its generation read");
    let before = old_lease.checkpoint().expect("old lease checkpoint");
    let progressed = old_lease
        .start_height_timer(before.state.revision)
        .expect("old lease advances current journal before target CAS");
    resume.send(()).expect("resume target acquire CAS");

    let mut acquired = acquire_thread
        .join()
        .expect("target acquire thread")
        .expect("acquire returns latest valid journal");
    assert_eq!(
        acquired.checkpoint().expect("new owner checkpoint").state,
        progressed.state
    );
    assert!(
        old_lease.checkpoint().is_err(),
        "new generation fences old owner"
    );
    drop(old_lease);
    wait_for_generation(&fixture.backend, 2);
}

#[test]
fn invalid_successful_acquire_snapshot_poisons_service() {
    let fixture = Fixture::new();
    fixture
        .backend
        .0
        .lock()
        .expect("test backend lock")
        .corrupt_next_acquire_snapshot = true;
    let provider = fixture.provider();

    assert!(
        provider.acquire(&fixture.binding).is_err(),
        "an invalid CAS result must not create a usable client lease"
    );
    assert!(
        provider.acquire(&fixture.binding).is_err(),
        "an uncertain successful CAS must poison later service commands"
    );
    let ledger = fixture.backend.0.lock().expect("test backend lock");
    assert_eq!(ledger.lease_generation, 1);
    assert_eq!(ledger.state.revision, 0);
    assert!(!ledger.lease_token_digest.is_empty());
}
