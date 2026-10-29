//! Regression oracles for exact-state reuse; no elapsed-time acceptance threshold.
use super::*;
use crate::protected_bft_v3::state::{
    validate_candidate_transition, validate_state, validate_state_against_cache,
};

fn proposal_candidate(
    binding: &BftSignerBindingV3,
    previous: &super::super::super::state::VerifiedBftSignerStateV3,
) -> BftSignerCandidateV3 {
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
    let mut candidate = previous.state.candidate();
    candidate.journal.as_mut().unwrap().pending_proposal_intent = Some(intent);
    candidate
}

#[test]
fn exact_cache_and_single_transition_do_not_revalidate_old_history() {
    let binding = fixture::binding();
    let state = fixture::signed_history(&binding, 5);
    let previous = validate_state(&binding, Some(&f::public(33)), &state).unwrap();
    let before = measurement_counts::history_qcs();
    let same = validate_state_against_cache(&binding, &previous, &state, &f::public(33)).unwrap();
    assert_eq!(same.state, previous.state);
    assert_eq!(measurement_counts::history_qcs(), before);

    let candidate = proposal_candidate(&binding, &previous);
    let full =
        BftSignerStateV3::from_candidate(&binding, state.revision + 1, candidate.clone()).unwrap();
    let expected = validate_state(&binding, Some(&f::public(33)), &full).unwrap();
    let before = measurement_counts::history_qcs();
    let next =
        validate_candidate_transition(&binding, &previous, &candidate, &f::public(33)).unwrap();
    assert_eq!(next.state, expected.state);
    assert_eq!(next.schedule, expected.schedule);
    assert_eq!(measurement_counts::history_qcs(), before);
    let checked = validate_state_against_cache(&binding, &previous, &full, &f::public(33)).unwrap();
    assert_eq!(checked.state, expected.state);
    assert_eq!(measurement_counts::history_qcs(), before);
}

#[test]
fn cached_state_rejects_same_revision_substitution_regression_and_jump() {
    let binding = fixture::binding();
    let state = fixture::signed_history(&binding, 1);
    let previous = validate_state(&binding, Some(&f::public(33)), &state).unwrap();
    let mut changed = state.clone();
    changed.schema_version = 2; // Retain the old digest deliberately.
    assert!(validate_state_against_cache(&binding, &previous, &changed, &f::public(33)).is_err());
    for revision in [state.revision - 1, state.revision + 2] {
        let valid_but_discontinuous =
            BftSignerStateV3::from_candidate(&binding, revision, state.candidate()).unwrap();
        assert!(
            validate_state_against_cache(
                &binding,
                &previous,
                &valid_but_discontinuous,
                &f::public(33)
            )
            .is_err()
        );
    }
    let mut other_binding = binding.clone();
    other_binding.operator_instance_id.push_str("-other");
    assert!(
        validate_state_against_cache(&other_binding, &previous, &state, &f::public(33)).is_err()
    );
}

#[test]
fn cached_transition_rejects_rewritten_prefix_archive_and_driver_receipt() {
    let binding = fixture::binding();
    let state = fixture::signed_history(&binding, 1);
    let previous = validate_state(&binding, Some(&f::public(33)), &state).unwrap();
    let base = proposal_candidate(&binding, &previous);
    let mut forked = base.clone();
    forked.history[0].header.execution_commitment_hash = "a".repeat(64);
    assert!(validate_candidate_transition(&binding, &previous, &forked, &f::public(33)).is_err());
    let mut archive = base.clone();
    archive.completed_journals[0]
        .as_mut()
        .unwrap()
        .events
        .clear();
    assert!(validate_candidate_transition(&binding, &previous, &archive, &f::public(33)).is_err());
    let mut receipt = base.clone();
    receipt.applied_installs[0].driver_install_digest = "a".repeat(64);
    assert!(validate_candidate_transition(&binding, &previous, &receipt, &f::public(33)).is_err());
    let mut skipped = base.clone();
    skipped
        .history
        .extend([state.history[0].clone(), state.history[0].clone()]);
    assert!(validate_candidate_transition(&binding, &previous, &skipped, &f::public(33)).is_err());
    let mut false_journal = base;
    false_journal.journal.as_mut().unwrap().height_started_at_ms = Some(900);
    assert!(
        validate_candidate_transition(&binding, &previous, &false_journal, &f::public(33)).is_err()
    );
}

#[test]
fn cached_checkpoint_still_checks_signature_freshness_and_lease() {
    let binding = fixture::binding();
    let state = fixture::signed_history(&binding, 1);
    let previous = validate_state(&binding, Some(&f::public(33)), &state).unwrap();
    let mut session = Session::new(&binding, state, 1);
    let reply = session.rpc("checkpoint", BftSignerCommandV3::Checkpoint, false);
    let checkpoint = reply.body.checkpoint.unwrap();
    let validate = |checkpoint: &BftSignerCheckpointV3, challenge: &str, generation| {
        checkpoint.validate_against_cache(
            &binding,
            &previous,
            challenge,
            &checkpoint.request_digest,
            generation,
            &f::public(33),
        )
    };
    assert!(
        validate(
            &checkpoint,
            &checkpoint.challenge,
            checkpoint.lease_generation
        )
        .is_ok()
    );
    assert!(validate(&checkpoint, "wrong-challenge", checkpoint.lease_generation).is_err());
    assert!(
        validate(
            &checkpoint,
            &checkpoint.challenge,
            checkpoint.lease_generation + 1
        )
        .is_err()
    );
    let mut bad_signature = checkpoint.as_ref().clone();
    bad_signature.provider_signature_hex = "00".repeat(64);
    assert!(
        validate(
            &bad_signature,
            &bad_signature.challenge,
            bad_signature.lease_generation
        )
        .is_err()
    );
    let mut substituted = checkpoint.as_ref().clone();
    substituted.state.schema_version = 2;
    substituted.provider_signature_hex = super::super::super::crypto::sign_preimage(
        &fixture::Key(f::key(33)),
        "oasis7.bft.protected-signer.v3.checkpoint",
        &substituted.signable_body(),
    )
    .unwrap();
    // Even an authenticated provider cannot substitute another state at our revision.
    assert!(
        validate(
            &substituted,
            &substituted.challenge,
            substituted.lease_generation
        )
        .is_err()
    );
}

#[test]
fn cached_qc_append_and_driver_confirmation_match_full_validation() {
    let binding = fixture::binding();
    let base = fixture::signed_history(&binding, 1);
    let two = fixture::signed_history(&binding, 2);
    let certificate = two.history[1].clone();
    let mut before = base.candidate();
    before.journal = two.completed_journals[1].clone();
    let state = BftSignerStateV3::from_candidate(&binding, base.revision, before).unwrap();
    let previous = validate_state(&binding, Some(&f::public(33)), &state).unwrap();
    let schedule = oasis7_formal_protocol::bft::verify_formal_bft_header_certificate_v3(
        &previous.schedule,
        &certificate,
    )
    .unwrap();
    let mut candidate = state.candidate();
    candidate.history.push(certificate.clone());
    candidate.completed_journals.push(state.journal.clone());
    candidate.journal = crate::protected_bft_v3::state::journal_for_schedule(
        &binding,
        schedule.next_height_schedule(),
    )
    .unwrap();
    candidate.pending_driver_apply = Some(crate::protected_bft_v3::BftDriverApplyIntentV3 {
        height: certificate.execution_commitment.height,
        header_id: certificate.header.block_id().unwrap(),
    });
    let full =
        BftSignerStateV3::from_candidate(&binding, state.revision + 1, candidate.clone()).unwrap();
    let expected = validate_state(&binding, Some(&f::public(33)), &full).unwrap();
    let next =
        validate_candidate_transition(&binding, &previous, &candidate, &f::public(33)).unwrap();
    assert_eq!(next.state, expected.state);
    assert_eq!(next.schedule, expected.schedule);
    let mut confirmation = next.state.candidate();
    confirmation.pending_driver_apply = None;
    confirmation
        .applied_installs
        .push(crate::protected_bft_v3::BftDriverAppliedReceiptV3 {
            height: certificate.execution_commitment.height,
            header_id: certificate.header.block_id().unwrap(),
            driver_install_digest: f::h(112),
        });
    let full =
        BftSignerStateV3::from_candidate(&binding, next.state.revision + 1, confirmation.clone())
            .unwrap();
    let confirmed =
        validate_candidate_transition(&binding, &next, &confirmation, &f::public(33)).unwrap();
    assert_eq!(confirmed.state, full);
    assert_eq!(confirmed.schedule, next.schedule);
    confirmation
        .applied_installs
        .last_mut()
        .unwrap()
        .driver_install_digest = "bad".into();
    assert!(validate_candidate_transition(&binding, &next, &confirmation, &f::public(33)).is_err());
}

#[path = "cache_cas_tests.rs"]
mod cas;
