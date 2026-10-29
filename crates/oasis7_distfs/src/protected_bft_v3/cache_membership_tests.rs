//! Pending-install invariants must survive exact-prefix caching.
use super::*;
use crate::protected_bft_v3::state::{
    validate_candidate_transition, validate_state, validate_state_against_cache,
};

#[test]
fn legal_pending_membership_cannot_drift_into_another_revision() {
    let binding = fixture::binding();
    let state = fixture::pending_membership_state(&binding);
    let previous = validate_state(&binding, Some(&f::public(33)), &state)
        .expect("full oracle accepts signed confirmed-transition pending install");
    let pending = state
        .membership_install
        .as_ref()
        .unwrap()
        .pending
        .as_ref()
        .unwrap();
    assert_eq!(state.revision, pending.body.expected_revision + 1);
    assert_eq!(pending.body.transition_source_height, 6);
    assert_eq!(pending.body.confirmation_height, 7);
    assert_eq!(previous.schedule.height(), 8);
    assert_eq!(
        previous
            .schedule
            .active_set()
            .validator("validator-a")
            .unwrap()
            .voting_power,
        2
    );
    let unchanged =
        validate_state_against_cache(&binding, &previous, &state, &f::public(33)).unwrap();
    assert_eq!(unchanged.state, state);

    // This journal reservation is legal in isolation. Retaining an existing
    // pending install while advancing revision, however, invalidates its ticket.
    let machine = BftConsensusMachineV3::new(
        previous.schedule.clone(),
        "validator-a",
        1000,
        binding.timeouts,
    )
    .unwrap();
    let intent = machine
        .proposal_intent(fixture::value(&previous.schedule), "validator-a")
        .unwrap();
    let mut candidate = state.candidate();
    candidate.journal.as_mut().unwrap().pending_proposal_intent = Some(intent);
    let journal = candidate.journal.as_ref().unwrap();
    oasis7_consensus::tendermint_v3::verify_bft_signing_journal_transition_v3(
        state.journal.as_ref().unwrap(),
        journal,
        previous.schedule.active_set(),
        &previous.schedule,
        binding.timeouts,
    )
    .expect("journal-only oracle accepts the single reservation");
    let full = BftSignerStateV3::from_candidate(&binding, state.revision + 1, candidate.clone());
    assert!(
        full.is_err(),
        "full oracle must reject stale pending-install revision"
    );
    let cached = validate_candidate_transition(&binding, &previous, &candidate, &f::public(33));
    assert!(
        cached.is_err(),
        "cached oracle must retain the pending-install revision fence"
    );
}
