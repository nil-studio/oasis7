use super::*;

fn provider_at(fixture: &Fixture, address: SocketAddr) -> RemoteBftSignerProviderV1 {
    let trust = BftSignerTrustV1 {
        schema_version: 1,
        provider_public_key: hex::encode(fixture.provider_key.verifying_key().to_bytes()),
        binding_digest: fixture.binding.digest().expect("binding digest"),
    };
    RemoteBftSignerProviderV1::new(
        address,
        Duration::from_secs(2),
        fixture.binding.clone(),
        fixture.requester_key.clone(),
        trust,
    )
    .expect("construct remote signer client")
}

pub(super) fn block(scope: &BftScopeV1, parent_commit_ref: &str) -> BftBlockV1 {
    block_at_height(scope, 1, parent_commit_ref)
}

fn block_at_height(scope: &BftScopeV1, height: u64, parent_commit_ref: &str) -> BftBlockV1 {
    BftBlockV1 {
        scope: scope.clone(),
        height,
        parent_commit_ref: parent_commit_ref.into(),
        authority_epoch: 1,
        manifest_hash: "22".repeat(32),
        actions_cbor: vec![0x80],
        action_root: "33".repeat(32),
        event_root: "44".repeat(32),
        execution_block_hash: "55".repeat(32),
        execution_state_root: "66".repeat(32),
        state_root: "77".repeat(32),
    }
}

fn commit_one_height(
    fixture: &Fixture,
    lease: &mut dyn BftSignerLeaseV1,
    checkpoint: BftProtectedCheckpointV1,
    started_at: i64,
) -> BftProtectedCheckpointV1 {
    let set = fixture.binding.validate().expect("single validator set");
    let mut schedule = BftHeightProposerScheduleV1::genesis(&set, fixture.binding.authority_epoch)
        .expect("genesis schedule");
    for certificate in &checkpoint.state.history {
        schedule = verify_commit_certificate_token_v1(&set, &schedule, certificate)
            .expect("replay committed prefix")
            .next_height_schedule()
            .clone();
    }
    let height = checkpoint.state.journal.height;
    let mut machine = BftConsensusMachineV1::new(
        set.clone(),
        fixture.binding.local_validator_id.clone(),
        height,
        Some(schedule.starting_priority().clone()),
        started_at,
        fixture.binding.timeouts,
    )
    .expect("BFT machine");
    let proposal_intent = machine
        .proposal_intent(
            block_at_height(&fixture.binding.scope, height, schedule.parent_commit_ref()),
            &fixture.binding.local_validator_id,
        )
        .expect("height-one proposal intent");
    let mut candidate = checkpoint.state.candidate();
    candidate.journal.pending_proposal_intent = Some(proposal_intent.clone());
    let reserved = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            candidate,
        )
        .expect("reserve proposal");
    let (proposal, completed_proposal) = lease
        .sign_reserved_proposal(reserved.state.revision, &proposal_intent)
        .expect("sign proposal");
    let actions = machine
        .receive_proposal(proposal, true, last_event_time(&completed_proposal))
        .expect("admit proposal");
    let prevote_intent = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV1::SignVote(intent) if intent.phase == BftVotePhaseV1::Prevote => {
                Some(intent)
            }
            _ => None,
        })
        .expect("proposal requests prevote");
    let mut candidate = completed_proposal.state.candidate();
    candidate.journal.pending_vote_intent = Some(prevote_intent.clone());
    let reserved = lease
        .advance(
            completed_proposal.state.revision,
            &completed_proposal.state.state_digest,
            candidate,
        )
        .expect("reserve prevote");
    let (prevote, completed_prevote) = lease
        .sign_reserved_vote(reserved.state.revision, &prevote_intent)
        .expect("sign prevote");
    let actions = machine
        .record_local_vote(prevote, last_event_time(&completed_prevote))
        .expect("record prevote");
    let precommit_intent = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV1::SignVote(intent) if intent.phase == BftVotePhaseV1::Precommit => {
                Some(intent)
            }
            _ => None,
        })
        .expect("single-validator prevote requests precommit");
    let mut candidate = completed_prevote.state.candidate();
    candidate.journal.pending_vote_intent = Some(precommit_intent.clone());
    let reserved = lease
        .advance(
            completed_prevote.state.revision,
            &completed_prevote.state.state_digest,
            candidate,
        )
        .expect("reserve precommit");
    let (precommit, completed_precommit) = lease
        .sign_reserved_vote(reserved.state.revision, &precommit_intent)
        .expect("sign precommit");
    let actions = machine
        .record_local_vote(precommit, last_event_time(&completed_precommit))
        .expect("record precommit");
    let certificate = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV1::Commit(certificate) => Some(certificate),
            _ => None,
        })
        .expect("single-validator precommit forms a QC");
    let mut candidate = completed_precommit.state.candidate();
    candidate.journal.pending_commit = Some(certificate.clone());
    let pending_commit = lease
        .advance(
            completed_precommit.state.revision,
            &completed_precommit.state.state_digest,
            candidate,
        )
        .expect("persist pending QC");
    let verified = verify_commit_certificate_token_v1(&set, &schedule, &certificate)
        .expect("verify height-one QC");
    let mut history = checkpoint.state.history.clone();
    history.push(certificate);
    let next_height = BftSignerCandidateV1 {
        history,
        journal: BftSigningJournalV1::for_schedule(
            verified.next_height_schedule(),
            fixture.binding.local_validator_id.clone(),
            fixture.binding.local_signing_public_key.clone(),
        ),
    };
    lease
        .advance(
            pending_commit.state.revision,
            &pending_commit.state.state_digest,
            next_height,
        )
        .expect("commit QC")
}

#[test]
fn repeated_protected_time_reads_do_not_reload_or_replay_nonempty_history() {
    let fixture = Fixture::new();
    let (mut lease, checkpoint, started_at) = acquire_started(&fixture);
    let committed = commit_one_height(&fixture, lease.as_mut(), checkpoint, started_at);
    assert_eq!(committed.state.history.len(), 1);

    let generation_before_release = fixture
        .backend
        .0
        .lock()
        .expect("test ledger")
        .lease_generation;
    drop(lease);
    for _ in 0..100 {
        if fixture
            .backend
            .0
            .lock()
            .expect("test ledger")
            .lease_generation
            > generation_before_release
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        fixture
            .backend
            .0
            .lock()
            .expect("released test ledger")
            .lease_generation
            > generation_before_release,
        "client close durably releases its provider lease"
    );

    let replays_before_reacquire = fixture
        .server
        .history_certificate_replays
        .load(Ordering::Relaxed);
    let provider = fixture.provider();
    let mut lease = provider
        .acquire(&fixture.binding)
        .expect("reacquire signer");
    let checkpoint = lease.checkpoint().expect("checkpoint with committed QC");
    assert_eq!(checkpoint.state.history.len(), 1);
    let replays_after_reacquire = fixture
        .server
        .history_certificate_replays
        .load(Ordering::Relaxed);
    assert_eq!(
        replays_after_reacquire - replays_before_reacquire,
        3,
        "prepare, pre-CAS acquire, and acquired snapshot each fully revalidate the history"
    );
    let started = lease
        .start_height_timer(checkpoint.state.revision)
        .expect("start the next height using the cached schedule");
    assert_eq!(started.state.history.len(), 1);
    assert_eq!(
        fixture
            .server
            .history_certificate_replays
            .load(Ordering::Relaxed),
        replays_after_reacquire,
        "a legal journal transition must reuse the verified history prefix"
    );

    let reads_before = fixture
        .backend
        .0
        .lock()
        .expect("backend lock")
        .read_for_lease_calls;
    let mut previous_time = 0;
    for _ in 0..8 {
        let now = lease.protected_time_ms().expect("read protected clock");
        assert!(now > previous_time, "provider clock must advance");
        previous_time = now;
    }
    let reads_after = fixture
        .backend
        .0
        .lock()
        .expect("backend lock")
        .read_for_lease_calls;
    assert_eq!(
        reads_after, reads_before,
        "protected-time polling must use the lease-bound clock fast path"
    );
    assert_eq!(
        fixture
            .server
            .history_certificate_replays
            .load(Ordering::Relaxed),
        replays_after_reacquire,
        "clock polling must not replay any QC history"
    );
    let after = lease.checkpoint().expect("checkpoint after clock reads");
    assert_eq!(after.state.revision, started.state.revision);
    assert_eq!(after.state.state_digest, started.state.state_digest);

    let second_height_started_at = after
        .state
        .journal
        .height_started_at_ms
        .expect("height two timer remains durable");
    let committed_second =
        commit_one_height(&fixture, lease.as_mut(), after, second_height_started_at);
    assert_eq!(committed_second.state.history.len(), 2);
    assert_eq!(
        fixture
            .server
            .history_certificate_replays
            .load(Ordering::Relaxed),
        replays_after_reacquire,
        "a valid second-height QC must verify only its new tail while preserving the cached prefix schedule"
    );
    let replayed_schedule =
        super::super::validation::validate_state(&fixture.binding, &committed_second.state)
            .expect("independently replay complete two-height history");
    assert_eq!(replayed_schedule.height(), 3);
    assert_eq!(
        replayed_schedule.parent_commit_ref(),
        committed_second.state.history[1].proposal.header.block_id
    );

    let started_third = lease
        .start_height_timer(committed_second.state.revision)
        .expect("start height three from cached schedule");
    let set = fixture.binding.validate().expect("validator set");
    let machine = BftConsensusMachineV1::new(
        set,
        fixture.binding.local_validator_id.clone(),
        replayed_schedule.height(),
        Some(replayed_schedule.starting_priority().clone()),
        started_third
            .state
            .journal
            .height_started_at_ms
            .expect("third height timer"),
        fixture.binding.timeouts,
    )
    .expect("third-height deterministic machine");
    let intent = machine
        .proposal_intent(
            block_at_height(
                &fixture.binding.scope,
                replayed_schedule.height(),
                replayed_schedule.parent_commit_ref(),
            ),
            &fixture.binding.local_validator_id,
        )
        .expect("third-height proposal intent");
    let mut candidate = started_third.state.candidate();
    candidate.journal.pending_proposal_intent = Some(intent);
    lease
        .advance(
            started_third.state.revision,
            &started_third.state.state_digest,
            candidate,
        )
        .expect("cached schedule matches full two-height replay");
    assert_eq!(
        fixture
            .server
            .history_certificate_replays
            .load(Ordering::Relaxed),
        replays_after_reacquire,
        "incremental tail and next-height transition must not reverify the old QC prefix"
    );
}

#[test]
fn same_revision_digest_with_changed_journal_poisons_the_cached_session() {
    let fixture = Fixture::new();
    let (mut lease, _, _) = acquire_started(&fixture);
    let original = lease.checkpoint().expect("original checkpoint").state;
    {
        let mut ledger = fixture.backend.0.lock().expect("test ledger");
        ledger.mutate_next_read_snapshot_without_digest = true;
        assert_eq!(ledger.state.revision, original.revision);
        assert_eq!(ledger.state.state_digest, original.state_digest);
    }

    assert!(
        lease.checkpoint().is_err(),
        "the service must not trust the backend digest as a cache key"
    );
    let persisted = fixture.backend.0.lock().expect("persisted test ledger");
    assert_eq!(
        persisted.state, original,
        "the backend's canonical state is unchanged"
    );
    drop(persisted);
    assert!(
        lease.protected_time_ms().is_err(),
        "an unexplained same-revision history change must poison later session work"
    );
}

#[test]
fn reacquire_rejects_a_valid_but_previously_observed_state_rollback() {
    let fixture = Fixture::new();
    let (mut lease, checkpoint, started_at) = acquire_started(&fixture);
    let committed = commit_one_height(&fixture, lease.as_mut(), checkpoint, started_at);
    assert_eq!(committed.state.history.len(), 1);
    let generation_before_release = fixture
        .backend
        .0
        .lock()
        .expect("test ledger")
        .lease_generation;
    drop(lease);

    for _ in 0..100 {
        let ledger = fixture.backend.0.lock().expect("test ledger");
        if ledger.lease_generation > generation_before_release
            && ledger.lease_token_digest.is_empty()
        {
            break;
        }
        drop(ledger);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    let mut ledger = fixture.backend.0.lock().expect("test ledger");
    let released_generation = ledger.lease_generation;
    let released_floor = ledger.protected_time_floor_ms;
    assert!(ledger.lease_token_digest.is_empty());
    ledger.state = BftSignerStateV1::genesis(&fixture.binding).expect("valid old genesis");
    assert_eq!(ledger.lease_generation, released_generation);
    assert_eq!(ledger.protected_time_floor_ms, released_floor);
    drop(ledger);

    let acquire_result = fixture.provider().acquire(&fixture.binding);
    assert!(
        acquire_result.is_err(),
        "a fully valid older state must not replace the state already observed by this service"
    );
    let ledger = fixture.backend.0.lock().expect("test ledger");
    assert_eq!(
        ledger.state.history.len(),
        0,
        "test backend retained the injected rollback"
    );
    assert_eq!(ledger.lease_generation, released_generation);
    assert!(ledger.lease_token_digest.is_empty());
    drop(ledger);

    assert!(
        fixture.provider().acquire(&fixture.binding).is_err(),
        "the unexplained backend rollback must poison the service instead of allowing a later acquire"
    );
}

#[test]
fn reacquire_cannot_erase_a_previously_reserved_proposal_intent() {
    let fixture = Fixture::new();
    let (mut lease, checkpoint, started_at) = acquire_started(&fixture);
    let set = fixture.binding.validate().expect("single validator set");
    let schedule = BftHeightProposerScheduleV1::genesis(&set, fixture.binding.authority_epoch)
        .expect("genesis schedule");
    let machine = BftConsensusMachineV1::new(
        set,
        fixture.binding.local_validator_id.clone(),
        1,
        Some(schedule.starting_priority().clone()),
        started_at,
        fixture.binding.timeouts,
    )
    .expect("BFT machine");
    let intent = machine
        .proposal_intent(
            block(&fixture.binding.scope, schedule.parent_commit_ref()),
            &fixture.binding.local_validator_id,
        )
        .expect("proposal intent");
    let mut candidate = checkpoint.state.candidate();
    candidate.journal.pending_proposal_intent = Some(intent);
    let reserved = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            candidate,
        )
        .expect("persist proposal reservation");
    assert!(reserved.state.journal.pending_proposal_intent().is_some());
    let generation_before_release = fixture
        .backend
        .0
        .lock()
        .expect("test ledger")
        .lease_generation;
    drop(lease);
    for _ in 0..100 {
        let ledger = fixture.backend.0.lock().expect("test ledger");
        if ledger.lease_generation > generation_before_release
            && ledger.lease_token_digest.is_empty()
        {
            break;
        }
        drop(ledger);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    let mut cleared = reserved.state.candidate();
    cleared.journal.pending_proposal_intent = None;
    let forged = BftSignerStateV1::from_candidate(
        &fixture.binding,
        reserved.state.revision + 1,
        cleared.history,
        cleared.journal,
    )
    .expect("state is independently valid despite erasing the old reservation");
    forged
        .validate(&fixture.binding)
        .expect("forged state passes standalone replay");
    {
        let mut ledger = fixture.backend.0.lock().expect("test ledger");
        assert!(ledger.lease_token_digest.is_empty());
        ledger.state = forged;
    }

    assert!(
        fixture.provider().acquire(&fixture.binding).is_err(),
        "the provider must require the exact cached reservation to remain pending until its matching signed event"
    );
}

#[test]
fn reacquire_rejects_a_valid_qc_not_consumed_from_the_observed_pending_slot() {
    let fixture = Fixture::new();
    let second_service = fixture.reopened_service();
    let second_server = TcpServiceServer::start(second_service);
    let provider = provider_at(&fixture, second_server.address);
    let mut lease = provider
        .acquire(&fixture.binding)
        .expect("second service acquires signer");
    let genesis = lease.checkpoint().expect("second service sees genesis");
    let started = lease
        .start_height_timer(genesis.state.revision)
        .expect("second service starts height");
    let started_at = started
        .state
        .journal
        .height_started_at_ms
        .expect("timer origin");
    let committed = commit_one_height(&fixture, lease.as_mut(), started, started_at);
    let certificate = committed.state.history[0].clone();
    let successor_journal = committed.state.journal.clone();
    let generation_before_release = fixture
        .backend
        .0
        .lock()
        .expect("test ledger")
        .lease_generation;
    drop(lease);
    for _ in 0..100 {
        let ledger = fixture.backend.0.lock().expect("test ledger");
        if ledger.lease_generation > generation_before_release
            && ledger.lease_token_digest.is_empty()
        {
            break;
        }
        drop(ledger);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    let forged = BftSignerStateV1::from_candidate(
        &fixture.binding,
        genesis.state.revision + 1,
        vec![certificate],
        successor_journal,
    )
    .expect("standalone valid history and successor journal");
    forged
        .validate(&fixture.binding)
        .expect("candidate independently replays from genesis");
    {
        let mut ledger = fixture.backend.0.lock().expect("test ledger");
        assert!(ledger.lease_token_digest.is_empty());
        ledger.state = forged;
    }

    assert!(
        fixture.provider().acquire(&fixture.binding).is_err(),
        "a QC cannot advance cached history unless it consumes the exact previously observed pending QC"
    );
}

#[test]
fn wire_decoder_rejects_noncanonical_and_oversized_container_declarations() {
    assert!(super::super::wire::decode_bounded::<u8>(&[0x18, 0x01]).is_err());
    assert!(super::super::wire::decode_bounded::<Vec<()>>(&[0x99, 0x10, 0x01]).is_err());
    let legitimate_large_array = vec![(); 5_000];
    let encoded = super::super::wire::encode_bounded(&legitimate_large_array)
        .expect("large but bounded container encodes");
    let decoded: Vec<()> = super::super::wire::decode_bounded(&encoded)
        .expect("valid protocol-sized arrays are not rejected by arbitrary item cap");
    assert_eq!(decoded, legitimate_large_array);
}
