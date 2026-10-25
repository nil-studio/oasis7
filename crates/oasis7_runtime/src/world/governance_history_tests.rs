//! Synthetic tokens test projection validation, not cryptographic qualification.
//! Host tests independently exercise cryptographically qualified history.
use super::super::*;
use crate::{Action, GovernanceEvent, GovernanceThresholdSignerPolicy, WorldEventBody};
use oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeReader;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalHeadRefV1, FormalWorldIdentityV1,
};
use std::sync::{Arc, Mutex};

struct EmptyNodes;
impl BusinessIndexNodeReader for EmptyNodes {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}
fn attach(world: &mut World, position: u64) {
    attach_with_nodes(world, position, Arc::new(EmptyNodes));
}
fn attach_with_nodes(
    world: &mut World,
    position: u64,
    nodes: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
) {
    let bytes = oasis7_world_state::codec::snapshot_cbor(&world.snapshot()).unwrap();
    let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_governance_snapshot(
        world.state.retained_history_root.clone().unwrap(),
        &bytes,
        FormalWorldIdentityV1 {
            world_id: "governance.fixture".into(),
            chain_id: "fixture.chain".into(),
            genesis_digest: "a".repeat(64),
        },
        FormalHeadRefV1 {
            position,
            record_id: "b".repeat(64),
        },
    );
    world.attach_verified_formal_history(parent, nodes).unwrap();
}
fn world() -> World {
    let mut world = World::new();
    let key = super::super::governance_authorization_test_support::fixture_key();
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "controller".into(),
            controller_signer_policies: BTreeMap::from([(
                "controller".into(),
                GovernanceThresholdSignerPolicy {
                    threshold: 1,
                    allowed_public_keys: [hex::encode(key.verifying_key().to_bytes())].into(),
                },
            )]),
            ..Default::default()
        })
        .unwrap();
    world.initialize_empty_formal_history().unwrap();
    attach(&mut world, 1000);
    world
}
fn command() -> Action {
    Action::SubmitGovernanceValidatorAdmission {
        controller_account_id: "controller".into(),
        candidate_id: "candidate".into(),
        node_id: "node".into(),
        finality_signer_public_key: hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&[9; 32])
                .verifying_key()
                .to_bytes(),
        ),
        stake: 25,
        operator_owner: "owner".into(),
        public_manifest_hash: "a".repeat(64),
    }
}
fn successor(parent: &World, action: Action) -> World {
    use ed25519_dalek::Signer;
    let nonce = parent
        .state
        .governance_authorization_nonces
        .get("controller")
        .copied()
        .unwrap_or(0)
        + 1;
    let mut proof = parent
        .governance_authorization_for_next_commit(nonce)
        .unwrap();
    let key = super::super::governance_authorization_test_support::fixture_key();
    proof.signatures.insert(
        hex::encode(key.verifying_key().to_bytes()),
        hex::encode(key.sign(&proof.signing_bytes(&action).unwrap()).to_bytes()),
    );
    let event = parent
        .action_to_event_policy_contract(0, &action, Some(&proof))
        .unwrap();
    let mut next = parent.clone();
    next.governance_replaying = true;
    let WorldEventBody::Governance(governance) = &event else {
        panic!("governance");
    };
    next.apply_governance_event(governance).unwrap();
    next.journal.events.push(crate::WorldEvent {
        id: next.next_event_id,
        time: next.state.time,
        caused_by: None,
        body: event,
    });
    (next.next_event_id_era, next.next_event_id) =
        super::advance((next.next_event_id_era, next.next_event_id)).unwrap();
    let position = parent
        .formal_history_reader
        .as_ref()
        .unwrap()
        .parent
        .as_ref()
        .unwrap()
        .head()
        .unwrap()
        .position
        + 1;
    attach(&mut next, position);
    next
}
fn pair() -> (World, World) {
    let parent = world();
    let next = successor(&parent, command());
    (parent, next)
}
fn projection(world: &World) -> Result<crate::VerifiedGovernanceHistorySnapshot, String> {
    let history = world
        .formal_history_reader
        .as_ref()
        .ok_or("missing test parent")?;
    crate::VerifiedGovernanceHistorySnapshot::from_verified_snapshot_bytes(
        &oasis7_world_state::codec::snapshot_cbor(&world.snapshot()).unwrap(),
        world.journal.clone(),
        history
            .parent
            .clone()
            .ok_or("missing qualified test parent")?,
        history.governance_test_reader(),
    )
}
fn verify(parent: &World, next: &World) -> Result<(), String> {
    let complete = parent.verify_formal_governance_successor(next, || Ok(()));
    let projected = projection(parent).and_then(|before| {
        projection(next).and_then(|after| before.verify_successor(&after, || Ok(())))
    });
    assert_eq!(
        complete.is_ok(),
        projected.is_ok(),
        "full={complete:?}, projection={projected:?}"
    );
    complete
}
#[test]
fn governance_history_rejects_unqualified_worlds() {
    assert!(verify(&World::new(), &World::new()).is_err());
}
#[test]
fn governance_history_revalidates_signed_submission_against_original_parent() {
    let (parent, next) = pair();
    verify(&parent, &next).unwrap();
    assert_eq!(
        next.state.governance_authorization_nonces.get("controller"),
        Some(&1)
    );
    let mut wrong_parent = parent.clone();
    attach(&mut wrong_parent, 999);
    assert!(
        verify(&wrong_parent, &next)
            .unwrap_err()
            .contains("not adjacent")
    );
    let mut same_position_wrong_policy = parent.clone();
    same_position_wrong_policy
        .state
        .governance_main_token_controller_registry
        .as_mut()
        .unwrap()
        .controller_signer_policies
        .get_mut("controller")
        .unwrap()
        .allowed_public_keys
        .clear();
    attach(&mut same_position_wrong_policy, 1000);
    assert!(verify(&same_position_wrong_policy, &next).is_err());
}
#[test]
fn governance_history_rejects_silent_snapshot_projection_and_binding_changes() {
    let (parent, next) = pair();
    let mut changed = next.clone();
    changed
        .state
        .governance_validator_admissions
        .get_mut("candidate")
        .unwrap()
        .stake += 1;
    attach(&mut changed, 1001);
    assert!(
        verify(&parent, &changed)
            .unwrap_err()
            .contains("projection mismatch")
    );
    let mut changed = next.clone();
    changed
        .state
        .governance_authorization_nonces
        .insert("controller".into(), 2);
    attach(&mut changed, 1001);
    assert!(
        verify(&parent, &changed)
            .unwrap_err()
            .contains("projection mismatch")
    );
    let mut changed = next.clone();
    changed
        .state
        .node_identity_bindings
        .insert("node".into(), "silent".into());
    attach(&mut changed, 1001);
    assert!(
        verify(&parent, &changed)
            .unwrap_err()
            .contains("mapping mismatch")
    );
    let mut changed = next.clone();
    changed
        .state
        .node_main_token_account_bindings
        .insert("node".into(), "silent".into());
    attach(&mut changed, 1001);
    assert!(
        verify(&parent, &changed)
            .unwrap_err()
            .contains("mapping mismatch")
    );
}
#[test]
fn governance_history_zero_event_successor_checks_projection_and_exact_snapshot() {
    let parent = world();
    let mut next = parent.clone();
    next.state.time += 1;
    attach(&mut next, 1001);
    verify(&parent, &next).unwrap();
    next.state
        .governance_authorization_nonces
        .insert("controller".into(), 1);
    assert!(
        verify(&parent, &next)
            .unwrap_err()
            .contains("differs from qualified parent")
    );
    attach(&mut next, 1001);
    assert!(
        verify(&parent, &next)
            .unwrap_err()
            .contains("projection mismatch")
    );
}
#[test]
fn governance_history_rejects_missing_out_of_order_and_bad_time_events() {
    let (parent, next) = pair();
    let mut missing = next.clone();
    missing.journal.events.clear();
    attach(&mut missing, 1001);
    assert!(
        verify(&parent, &missing)
            .unwrap_err()
            .contains("event missing")
    );
    let mut reordered = next.clone();
    reordered.journal.events.last_mut().unwrap().id += 1;
    attach(&mut reordered, 1001);
    assert!(
        verify(&parent, &reordered)
            .unwrap_err()
            .contains("not contiguous")
    );
    let mut future = next.clone();
    future.journal.events.last_mut().unwrap().time += 1;
    attach(&mut future, 1001);
    assert!(
        verify(&parent, &future)
            .unwrap_err()
            .contains("time mismatch")
    );
}
#[test]
fn governance_history_enforces_event_byte_and_external_deadline_budgets() {
    let parent = world();
    let mut next = parent.clone();
    next.next_event_id += 65_537;
    attach(&mut next, 1001);
    assert!(
        verify(&parent, &next)
            .unwrap_err()
            .contains("event capacity")
    );
    assert!(matches!(
        parent.verify_governance_projection_successor_typed(
            &next,
            parent
                .formal_history_reader
                .as_ref()
                .unwrap()
                .parent
                .as_ref()
                .unwrap(),
            next.formal_history_reader
                .as_ref()
                .unwrap()
                .parent
                .as_ref()
                .unwrap(),
            || Ok(())
        ),
        Err(crate::GovernanceHistoryVerificationError::Capacity(_))
    ));
    let (parent, next) = pair();
    let mut calls = 0;
    let result = parent.verify_formal_governance_successor(&next, || {
        calls += 1;
        if calls == 4 {
            Err("operation deadline".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(result, Err("operation deadline".into()));
    assert!(matches!(
        projection(&parent)
            .unwrap()
            .verify_successor_typed(&projection(&next).unwrap(), || Err("deadline".into())),
        Err(crate::GovernanceHistoryVerificationError::Budget(_))
    ));
    let mut budget = super::ByteBudget(3, false);
    assert!(serde_cbor::to_writer(&mut budget, &"large event").is_err());
}
#[test]
fn governance_history_rejects_actual_event_over_the_full_byte_budget() {
    let parent = world();
    let mut next = parent.clone();
    next.journal.events.push(crate::WorldEvent {
        id: next.next_event_id,
        time: next.state.time,
        caused_by: None,
        body: WorldEventBody::ModuleEmitted(oasis7_wasm_abi::ModuleEmitEvent {
            module_id: "budget.fixture".into(),
            trace_id: "budget".into(),
            kind: "budget".into(),
            payload: serde_json::Value::String("x".repeat(super::MAX_BYTES + 1)),
        }),
    });
    (next.next_event_id_era, next.next_event_id) =
        super::advance((next.next_event_id_era, next.next_event_id)).unwrap();
    attach(&mut next, 1001);
    let before = parent.snapshot();
    let after = next.snapshot();
    assert!(
        verify(&parent, &next)
            .unwrap_err()
            .contains("byte capacity")
    );
    assert!(matches!(
        parent.verify_governance_projection_successor_typed(
            &next,
            parent
                .formal_history_reader
                .as_ref()
                .unwrap()
                .parent
                .as_ref()
                .unwrap(),
            next.formal_history_reader
                .as_ref()
                .unwrap()
                .parent
                .as_ref()
                .unwrap(),
            || Ok(())
        ),
        Err(crate::GovernanceHistoryVerificationError::Capacity(_))
    ));
    assert_eq!(parent.snapshot(), before);
    assert_eq!(next.snapshot(), after);
}
#[test]
fn governance_history_signed_effect_tampering_cannot_hide_behind_matching_snapshot() {
    let (parent, mut next) = pair();
    let WorldEventBody::Governance(GovernanceEvent::ValidatorAdmissionSubmitted { stake, .. }) =
        &mut next.journal.events.last_mut().unwrap().body
    else {
        panic!("submission");
    };
    *stake += 1;
    attach(&mut next, 1001);
    assert!(
        verify(&parent, &next)
            .unwrap_err()
            .contains("signed action/effect mismatch")
    );
}

#[test]
fn governance_history_rechecks_each_admission_transition_and_rejects_unsigned_suffix() {
    let (parent, submitted) = pair();
    verify(&parent, &submitted).unwrap();
    let approved = successor(
        &submitted,
        Action::ApproveGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "candidate".into(),
        },
    );
    verify(&submitted, &approved).unwrap();
    let activated = successor(
        &approved,
        Action::ActivateGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "candidate".into(),
            activation_epoch: 1,
        },
    );
    verify(&approved, &activated).unwrap();
    let revoked = successor(
        &activated,
        Action::RevokeGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "candidate".into(),
            node_id: "node".into(),
            reason: "test".into(),
        },
    );
    verify(&activated, &revoked).unwrap();
    let mut unsigned = approved.clone();
    let WorldEventBody::Governance(GovernanceEvent::ValidatorAdmissionApproved {
        authorization,
        ..
    }) = &mut unsigned.journal.events.last_mut().unwrap().body
    else {
        panic!("approval");
    };
    *authorization = None;
    attach(&mut unsigned, 1002);
    assert!(
        verify(&submitted, &unsigned)
            .unwrap_err()
            .contains("unsigned governance event")
    );
}

#[test]
fn governance_history_preserves_legacy_unsigned_prefix_and_cursor_rollover() {
    let (parent, mut legacy) = pair();
    let WorldEventBody::Governance(GovernanceEvent::ValidatorAdmissionSubmitted {
        authorization,
        ..
    }) = &mut legacy.journal.events.last_mut().unwrap().body
    else {
        panic!("submission");
    };
    *authorization = None;
    legacy.state.governance_authorization_nonces.clear();
    attach(&mut legacy, 1001);
    verify(&parent, &legacy).unwrap();
    let signed = successor(
        &legacy,
        Action::ApproveGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "candidate".into(),
        },
    );
    verify(&legacy, &signed).unwrap();
    let mut parent = world();
    parent.next_event_id = u64::MAX;
    parent.next_event_id_era = 5;
    attach(&mut parent, 1000);
    let next = successor(&parent, command());
    assert_eq!((next.next_event_id_era, next.next_event_id), (6, 1));
    verify(&parent, &next).unwrap();
}

#[derive(Clone, Default)]
struct MemoryNodes(Arc<Mutex<BTreeMap<String, Vec<u8>>>>);
impl BusinessIndexNodeReader for MemoryNodes {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "poisoned test nodes")?
            .get(digest)
            .cloned())
    }
}
struct CorruptNodes;
impl BusinessIndexNodeReader for CorruptNodes {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(Some(vec![0xff]))
    }
}

#[test]
fn governance_history_reads_real_cold_archive_and_rejects_corrupt_cas_without_mutation() {
    let (parent, mut next) = pair();
    let submission = next.journal.events.last().unwrap().clone();
    // Use the real retention planner and sparse-index writes, while the
    // qualification token remains explicitly synthetic in this runtime test.
    for _ in 0..256 {
        let id = next.next_event_id;
        next.journal.events.push(crate::WorldEvent {
            id,
            time: next.state.time,
            caused_by: None,
            body: WorldEventBody::SnapshotCreated(crate::SnapshotMeta {
                journal_len: id as usize,
            }),
        });
        (next.next_event_id_era, next.next_event_id) =
            super::advance((next.next_event_id_era, id)).unwrap();
    }
    let nodes = MemoryNodes::default();
    attach_with_nodes(&mut next, 1001, Arc::new(nodes.clone()));
    let plan = next.prepare_formal_retention_candidate().unwrap();
    assert!(plan.journal_events_to_prune.contains(&(0, submission.id)));
    for write in &plan.delta.as_ref().unwrap().node_writes {
        nodes
            .0
            .lock()
            .unwrap()
            .insert(write.digest.clone(), write.bytes.clone());
    }
    next.apply_formal_retention_plan(&plan).unwrap();
    attach_with_nodes(&mut next, 1001, Arc::new(nodes.clone()));
    assert!(
        next.journal
            .events
            .iter()
            .all(|event| event.id != submission.id)
    );
    assert_eq!(
        next.read_retained_journal_event(0, submission.id).unwrap(),
        Some(submission)
    );
    let parent_before = parent.snapshot();
    let parent_journal = parent.journal.clone();
    let next_before = next.snapshot();
    let next_journal = next.journal.clone();
    let nodes_before = nodes.0.lock().unwrap().clone();
    verify(&parent, &next).unwrap();
    assert_eq!(parent.snapshot(), parent_before);
    assert_eq!(parent.journal, parent_journal);
    assert_eq!(next.snapshot(), next_before);
    assert_eq!(next.journal, next_journal);
    assert_eq!(*nodes.0.lock().unwrap(), nodes_before);

    let mut corrupt = next.clone();
    attach_with_nodes(&mut corrupt, 1001, Arc::new(CorruptNodes));
    assert!(
        verify(&parent, &corrupt)
            .unwrap_err()
            .contains("cold journal")
    );
    assert_eq!(corrupt.snapshot(), next_before);
    assert_eq!(corrupt.journal, next_journal);
    let mut missing = next.clone();
    attach_with_nodes(&mut missing, 1001, Arc::new(EmptyNodes));
    assert!(
        verify(&parent, &missing)
            .unwrap_err()
            .contains("cold journal")
    );
    assert_eq!(missing.snapshot(), next_before);
    assert_eq!(missing.journal, next_journal);
    assert_eq!(*nodes.0.lock().unwrap(), nodes_before);
}

#[test]
fn governance_history_projection_preserves_full_restore_policy_rejection() {
    let base = world();
    for invalid in 0..4 {
        let mut snapshot = base.snapshot();
        match invalid {
            0 => snapshot.governance_execution_policy.epoch_length_ticks = 0,
            1 => {
                snapshot
                    .governance_execution_policy
                    .emergency_brake_guardian_threshold = 0
            }
            2 => {
                snapshot
                    .governance_execution_policy
                    .emergency_veto_guardian_threshold = 0
            }
            _ => {
                snapshot
                    .governance_execution_policy
                    .emergency_brake_max_ticks = 0
            }
        }
        let bytes = oasis7_world_state::codec::snapshot_cbor(&snapshot).unwrap();
        let original = base
            .formal_history_reader
            .as_ref()
            .unwrap()
            .parent
            .as_ref()
            .unwrap();
        let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_governance_snapshot(
            snapshot.state.retained_history_root.clone().unwrap(),
            &bytes,
            original.effect_world().unwrap().clone(),
            original.head().unwrap().clone(),
        );
        let projected = crate::VerifiedGovernanceHistorySnapshot::from_verified_snapshot_bytes(
            &bytes,
            base.journal.clone(),
            parent.clone(),
            Arc::new(EmptyNodes),
        );
        assert!(projected.err().unwrap().contains("GovernancePolicyInvalid"));
        let complete = World::from_verified_formal_snapshot(
            snapshot,
            base.journal.clone(),
            parent,
            Arc::new(EmptyNodes),
        );
        assert!(matches!(
            complete,
            Err(crate::WorldError::GovernancePolicyInvalid { .. })
        ));
    }
}

#[test]
fn governance_history_projection_rejects_missing_admission_bindings_and_allocator_normalization() {
    let base = successor(&world(), command());
    for invalid in 0..3 {
        let mut snapshot = base.snapshot();
        match invalid {
            0 => {
                snapshot.state.node_identity_bindings.remove("node");
            }
            1 => {
                snapshot
                    .state
                    .node_main_token_account_bindings
                    .remove("node");
            }
            _ => snapshot.next_governance_identity_penalty_id = 0,
        }
        let bytes = oasis7_world_state::codec::snapshot_cbor(&snapshot).unwrap();
        let original = base
            .formal_history_reader
            .as_ref()
            .unwrap()
            .parent
            .as_ref()
            .unwrap();
        let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_governance_snapshot(
            snapshot.state.retained_history_root.clone().unwrap(),
            &bytes,
            original.effect_world().unwrap().clone(),
            original.head().unwrap().clone(),
        );
        let projected = crate::VerifiedGovernanceHistorySnapshot::from_verified_snapshot_bytes(
            &bytes,
            base.journal.clone(),
            parent.clone(),
            Arc::new(EmptyNodes),
        );
        assert!(projected.err().unwrap().contains("normalization"));
        let complete = World::from_verified_formal_snapshot(
            snapshot,
            base.journal.clone(),
            parent,
            Arc::new(EmptyNodes),
        );
        match complete {
            Err(crate::WorldError::DistributedValidationFailed { reason }) => {
                assert_eq!(
                    reason,
                    "World snapshot does not match the verified retained-history parent"
                );
            }
            _ => panic!("complete restore must reject the changed qualified root"),
        }
    }
}
