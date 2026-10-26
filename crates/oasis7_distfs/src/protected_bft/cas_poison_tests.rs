use super::*;
use oasis7_consensus::tendermint_v1::{
    BftConsensusMachineV1, BftHeightProposerScheduleV1, BftProposalIntentV1,
};

fn reserve_proposal(
    fixture: &Fixture,
    lease: &mut dyn BftSignerLeaseV1,
    checkpoint: &BftProtectedCheckpointV1,
    started_at: i64,
) -> (BftProtectedCheckpointV1, BftProposalIntentV1) {
    let set = fixture.binding.validate().expect("fixed validator set");
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
            super::protected_time_cache_tests::block(
                &fixture.binding.scope,
                schedule.parent_commit_ref(),
            ),
            &fixture.binding.local_validator_id,
        )
        .expect("proposal intent");
    let mut candidate = checkpoint.state.candidate();
    candidate.journal.pending_proposal_intent = Some(intent.clone());
    let reserved = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            candidate,
        )
        .expect("reserve exact proposal before signing");
    (reserved, intent)
}

#[test]
fn invalid_successful_journal_cas_poisons_service_but_preserves_signed_decision() {
    let fixture = Fixture::new();
    let provider = fixture.provider();
    let mut lease = provider.acquire(&fixture.binding).expect("lease");
    let genesis = lease.checkpoint().expect("genesis checkpoint");
    let checkpoint = lease
        .start_height_timer(genesis.state.revision)
        .expect("start height timer");
    let started_at = checkpoint
        .state
        .journal
        .height_started_at_ms
        .expect("persisted timer origin");
    let (reserved, intent) = reserve_proposal(&fixture, lease.as_mut(), &checkpoint, started_at);

    fixture
        .backend
        .0
        .lock()
        .expect("test backend lock")
        .corrupt_next_cas_snapshot = true;
    assert!(
        lease
            .sign_reserved_proposal(reserved.state.revision, &intent)
            .is_err(),
        "an invalid successful CAS response must fail closed"
    );

    let committed = {
        let ledger = fixture.backend.0.lock().expect("test backend lock");
        let state = ledger.state.clone();
        assert_eq!(state.revision, reserved.state.revision + 1);
        assert!(state.journal.pending_proposal_intent().is_none());
        assert!(matches!(
            state.journal.events().last(),
            Some(BftSigningJournalEventV1::Proposal {
                proposal,
                local: true,
                ..
            }) if proposal.block == intent.block
                && proposal.header.verify_signature().is_ok()
        ));
        state
            .validate(&fixture.binding)
            .expect("committed decision is valid");
        state
    };

    assert!(
        lease
            .sign_reserved_proposal(reserved.state.revision, &intent)
            .is_err(),
        "the same lease must not continue signing after an ambiguous CAS reply"
    );
    let ledger = fixture.backend.0.lock().expect("test backend lock");
    assert_eq!(
        ledger.state, committed,
        "the original signature remains durable"
    );
    assert_eq!(ledger.lease_generation, 1);
    assert!(!ledger.lease_token_digest.is_empty());
}

#[test]
fn journal_cas_rejects_clock_floor_regression_below_previous_floor() {
    let fixture = Fixture::new();
    let provider = fixture.provider();
    let mut lease = provider.acquire(&fixture.binding).expect("lease");
    let genesis = lease.checkpoint().expect("genesis checkpoint");
    let checkpoint = lease
        .start_height_timer(genesis.state.revision)
        .expect("start height timer");
    let started_at = checkpoint
        .state
        .journal
        .height_started_at_ms
        .expect("persisted timer origin");

    {
        let mut ledger = fixture.backend.0.lock().expect("test backend lock");
        ledger.now_ms = 149;
        assert!(ledger.protected_time_floor_ms < 150);
    }
    assert_eq!(lease.protected_time_ms().expect("advance clock floor"), 150);
    assert_eq!(
        checkpoint.state.journal.height_started_at_ms,
        Some(started_at)
    );
    assert!(
        started_at < 120,
        "the returned 120 floor remains above event time"
    );

    let (schedule, machine) = {
        let set = fixture.binding.validate().expect("fixed validator set");
        let schedule = BftHeightProposerScheduleV1::genesis(&set, fixture.binding.authority_epoch)
            .expect("genesis schedule");
        let machine = BftConsensusMachineV1::new(
            set.clone(),
            fixture.binding.local_validator_id.clone(),
            1,
            Some(schedule.starting_priority().clone()),
            started_at,
            fixture.binding.timeouts,
        )
        .expect("BFT machine");
        (schedule, machine)
    };
    let intent = machine
        .proposal_intent(
            super::protected_time_cache_tests::block(
                &fixture.binding.scope,
                schedule.parent_commit_ref(),
            ),
            &fixture.binding.local_validator_id,
        )
        .expect("proposal intent");
    let mut candidate = checkpoint.state.candidate();
    candidate.journal.pending_proposal_intent = Some(intent.clone());

    fixture
        .backend
        .0
        .lock()
        .expect("test backend lock")
        .next_cas_snapshot_floor_override_ms = Some(120);
    assert!(
        lease
            .advance(
                checkpoint.state.revision,
                &checkpoint.state.state_digest,
                candidate,
            )
            .is_err(),
        "a successful CAS response cannot roll its protected clock floor back"
    );

    let committed = {
        let ledger = fixture.backend.0.lock().expect("test backend lock");
        assert_eq!(ledger.protected_time_floor_ms, 152);
        assert_eq!(ledger.state.revision, checkpoint.state.revision + 1);
        assert_eq!(
            ledger.state.journal.pending_proposal_intent(),
            Some(&intent)
        );
        assert!(ledger.state.journal.events().is_empty());
        ledger.state.clone()
    };

    assert!(
        lease
            .sign_reserved_proposal(committed.revision, &intent)
            .is_err(),
        "a valid follow-up signing request must be blocked by the poisoned service"
    );
    let ledger = fixture.backend.0.lock().expect("test backend lock");
    assert_eq!(
        ledger.state, committed,
        "the reserved decision remains unchanged"
    );
    assert_eq!(ledger.lease_generation, 1);
    assert!(!ledger.lease_token_digest.is_empty());
}
