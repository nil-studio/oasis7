//! Execution/publication tests use synthetic parents; host tests cover qualification.
use super::*;
use crate::{GovernanceThresholdSignerPolicy, WorldEventBody};

fn world() -> World {
    let mut world = World::new();
    let key = super::super::governance_authorization_test_support::fixture_key();
    world.set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
        genesis_controller_account_id: "controller".into(),
        controller_signer_policies: BTreeMap::from([("controller".into(), GovernanceThresholdSignerPolicy {
            threshold: 1, allowed_public_keys: [hex::encode(key.verifying_key().to_bytes())].into(),
        })]), ..Default::default()
    }).unwrap();
    world
}
fn command() -> Action {
    Action::SubmitGovernanceValidatorAdmission {
        controller_account_id: "controller".into(), candidate_id: "candidate".into(), node_id: "node".into(),
        finality_signer_public_key: hex::encode(ed25519_dalek::SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes()),
        stake: 25, operator_owner: "owner".into(), public_manifest_hash: "a".repeat(64),
    }
}

#[test]
fn new_unsigned_actions_and_raw_events_cannot_publish_governance() {
    let mut world = world();
    world.submit_action(command());
    let before = world.snapshot();
    let journal = world.journal().clone();
    assert!(world.step().is_err());
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.journal(), &journal);
    let event = GovernanceEvent::ValidatorAdmissionApproved {
        authorization: None, controller_account_id: "controller".into(), candidate_id: "candidate".into(), approved_at_epoch: 0,
    };
    assert!(world.append_event(WorldEventBody::Governance(event), None).is_err());
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.journal(), &journal);
}

#[test]
fn signed_event_tampering_is_rejected_before_nonce_or_registry_publication() {
    let mut world = world();
    world.submit_governance_fixture_action(command());
    let envelope = world.pending_actions.front().unwrap();
    let WorldEventBody::Governance(mut event) = world.action_to_event(envelope).unwrap() else { panic!("governance event"); };
    let GovernanceEvent::ValidatorAdmissionSubmitted { stake, .. } = &mut event else { unreachable!() };
    *stake += 1;
    let before = world.snapshot();
    let journal = world.journal().clone();
    let error = world.append_event(WorldEventBody::Governance(event), None).unwrap_err();
    assert!(format!("{error:?}").contains("signed action/effect mismatch"));
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.journal(), &journal);
    assert!(world.state.governance_authorization_nonces.is_empty());
}

#[test]
fn post_prepare_failure_does_not_consume_a_valid_governance_nonce() {
    let mut world = world();
    world.submit_governance_fixture_action(command());
    let event = world.action_to_event(world.pending_actions.front().unwrap()).unwrap();
    let before = world.snapshot();
    let journal = world.journal().clone();
    world.fail_next_append_after_publication_prepare_for_test();
    assert!(world.append_event(event.clone(), None).is_err());
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.journal(), &journal);
    world.append_event(event, None).unwrap();
    assert_eq!(world.state.governance_authorization_nonces.get("controller"), Some(&1));
    assert!(world.state.governance_validator_admissions.contains_key("candidate"));
}

#[test]
fn signed_replay_requires_its_original_qualified_parent_and_blocks_unsigned_suffix() {
    let mut original = world();
    original.submit_governance_fixture_action(command());
    let WorldEventBody::Governance(event) = original.action_to_event(original.pending_actions.front().unwrap()).unwrap() else { unreachable!() };
    let mut missing_parent = original.clone();
    missing_parent.formal_history_reader = None;
    missing_parent.governance_replaying = true;
    assert!(missing_parent.apply_governance_event(&event).is_err());
    assert!(missing_parent.state.governance_authorization_nonces.is_empty());
    let mut replay = original;
    replay.governance_replaying = true;
    replay.apply_governance_event(&event).unwrap();
    assert_eq!(replay.state.governance_authorization_nonces.get("controller"), Some(&1));
    let unsigned = GovernanceEvent::ValidatorAdmissionApproved {
        authorization: None, controller_account_id: "controller".into(), candidate_id: "candidate".into(), approved_at_epoch: 0,
    };
    assert!(replay.apply_governance_event(&unsigned).is_err());
}
