//! Four continuously running Nodes first admit Node3, then perform its normal
//! finality rotation through actual worker/provider installation. All QC votes
//! are real worker outputs. Issuer, keys, backend and elapsed clock are explicit
//! test doubles; no physical custody or independent fault domains are claimed.
use super::key_rotation_cases::{make_body, signed_rotation};
use super::rotation_live_support::{FourLiveRotationNodes, diagnostics, public, wait_until};
use super::*;
use std::sync::atomic::Ordering;

const PLAYER: &str = "admitted-live-rotation-player";
#[test]
fn wire3_four_live_admitted_nodes_rotate_install_and_require_the_new_key() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(admitted_rotation)
        .unwrap()
        .join()
        .unwrap();
}
fn admitted_rotation() {
    let h = super::super::handoff();
    let mut fixture = FourLiveRotationNodes::new(&h, 7, 9, Some(PLAYER));
    let proposer = fixture.bindings[0]
        .initial_schedule()
        .expected_proposer_for_round(0)
        .unwrap();
    let proposer = fixture
        .bindings
        .iter()
        .position(|b| b.local_validator_id == proposer)
        .unwrap();
    // Actual controller-signed submit/approve/activate actions, authenticated at
    // the original H. Node3 is not initially an active bootstrap validator.
    super::super::submit_admission(&fixture.nodes[proposer], &fixture.actual[proposer]);
    fixture.start();
    let FourLiveRotationNodes {
        mut nodes,
        actual,
        bindings,
        snapshots,
        origin,
        healed,
        old_calls,
        new_calls,
        servers,
        stage_budget,
        _roots,
        _stores,
        _networks,
    } = fixture;
    let diagnostic = || diagnostics(&bindings, &snapshots, &origin);
    wait_until(
        Instant::now() + stage_budget,
        &nodes,
        "real Admission activates Node3 at h6",
        &diagnostic,
        || {
            nodes
                .iter()
                .all(|node| node.bft_v3_status().unwrap().committed_height == 6)
                && snapshots.iter().all(|snapshot| {
                    let state = &snapshot.lock().unwrap().state;
                    state.pending_driver_apply.is_none()
                        && state
                            .history
                            .last()
                            .is_some_and(|c| c.execution_commitment.height == 6)
                        && state.membership_install.as_ref().is_some_and(|m| {
                            m.receipts.iter().any(|r| r.body.activation_height == 6)
                        })
                })
                && actual
                    .iter()
                    .all(|driver| driver.lock().unwrap().world().state().time == 6)
        },
    );
    let immutable = actual
        .iter()
        .map(|driver| {
            let driver = driver.lock().unwrap();
            let world = driver.world();
            let admission = &world.state().governance_validator_admissions["new-formal-validator"];
            assert_eq!(admission.node_id, validator_id(3));
            assert_eq!(admission.finality_signer_public_key, public(123));
            assert_eq!(admission.stake, 1);
            assert_eq!(admission.activation_epoch, Some(2));
            assert_eq!(
                world
                    .resolve_governance_effective_finality_signer_registry()
                    .unwrap()
                    .unwrap()
                    .signer_bindings
                    .len(),
                4
            );
            (
                serde_cbor::to_vec(&world.state().governance_finality_signer_registry).unwrap(),
                serde_cbor::to_vec(&world.state().governance_validator_admissions).unwrap(),
                world
                    .node_identity_public_key(&validator_id(3))
                    .map(str::to_owned),
                world
                    .node_main_token_account(&validator_id(3))
                    .map(str::to_owned),
            )
        })
        .collect::<Vec<_>>();
    let mut schedule = bindings[3].initial_schedule().clone();
    let mut tip = None;
    for certificate in &snapshots[3].lock().unwrap().state.history {
        let verified = verify_commit_certificate_token_v3(&schedule, certificate).unwrap();
        schedule = verified.next_height_schedule().clone();
        tip = Some(verified);
    }
    let tip = tip.unwrap();
    assert_eq!(schedule.height(), 7);
    let body = make_body(&actual[3], &tip);
    let (old, new) = nodes[3]
        .request_bft_v3_finality_key_rotation_proofs(body.clone(), Duration::from_secs(120))
        .unwrap();
    let (id, action_payload) = signed_rotation(&body, old, new, 32007);
    let expected_rotation =
        oasis7_node::NodeConsensusAction::from_payload(id, PLAYER, action_payload.clone()).unwrap();
    for node in &nodes {
        node.submit_consensus_action_payload(id, action_payload.clone())
            .unwrap();
    }
    wait_until(
        Instant::now() + stage_budget,
        &nodes,
        "all four actual workers ACK normal rotation h9",
        &diagnostic,
        || {
            snapshots.iter().enumerate().all(|(index, snapshot)| {
                snapshot
                    .lock()
                    .unwrap()
                    .state
                    .membership_install
                    .as_ref()
                    .is_some_and(|m| {
                        m.pending.is_none()
                            && m.receipts.iter().any(|receipt| {
                                receipt.body.activation_height == 9
                                    && receipt.body.local_signing_public_key
                                        == public(if index == 3 { 124 } else { 120 + index as u8 })
                            })
                    })
            })
        },
    );
    let retired_count = old_calls.load(Ordering::SeqCst);
    wait_until(
        Instant::now() + stage_budget,
        &nodes,
        "three actual survivors require K124 quorum at h9",
        &diagnostic,
        || {
            snapshots[1..].iter().all(|snapshot| {
                snapshot
                    .lock()
                    .unwrap()
                    .state
                    .history
                    .last()
                    .is_some_and(|c| c.execution_commitment.height >= 9)
            })
        },
    );
    {
        let snapshot = snapshots[3].lock().unwrap();
        let qc = snapshot
            .state
            .history
            .iter()
            .find(|c| c.execution_commitment.height == 9)
            .unwrap();
        assert_eq!(qc.certificate.precommits.len(), 3);
        assert!(
            qc.certificate
                .precommits
                .iter()
                .any(|v| v.validator_id == validator_id(3) && v.public_key_hex == public(124))
        );
        assert!(
            qc.certificate
                .precommits
                .iter()
                .all(|v| v.validator_id != validator_id(0))
        );
    }
    healed.store(true, Ordering::Release);
    wait_until(
        Instant::now() + stage_budget,
        &nodes,
        "healed fourth Node independently installs new-key head",
        &diagnostic,
        || {
            snapshots.iter().all(|snapshot| {
                let state = &snapshot.lock().unwrap().state;
                state.pending_driver_apply.is_none()
                    && state
                        .history
                        .last()
                        .is_some_and(|c| c.execution_commitment.height >= 9)
            })
        },
    );
    let action_id = 32010;
    let marker = "admitted-live-healed-continued";
    let expected =
        oasis7_node::NodeConsensusAction::from_payload(action_id, PLAYER, payload(marker)).unwrap();
    for node in &nodes {
        node.submit_consensus_action_payload(action_id, payload(marker))
            .unwrap();
    }
    let mut receipts = [None, None, None, None];
    let mut rotation_receipts = [None, None, None, None];
    wait_until(
        Instant::now() + stage_budget,
        &nodes,
        "exact original action and four World effects after healing",
        &diagnostic,
        || {
            for (index, node) in nodes.iter().enumerate() {
                for batch in node.drain_committed_action_batches() {
                    if let Some(action) = batch.actions.iter().find(|a| a.action_id == id) {
                        assert_eq!(
                            action, &expected_rotation,
                            "exact original rotation receipt"
                        );
                        assert_eq!(batch.height, 7);
                        assert!(
                            rotation_receipts[index].is_none(),
                            "duplicate rotation receipt"
                        );
                        rotation_receipts[index] = Some((
                            batch.height,
                            batch.block_hash.clone(),
                            batch.action_root.clone(),
                        ));
                    }
                    if let Some(action) = batch.actions.iter().find(|a| a.action_id == action_id) {
                        assert_eq!(action, &expected, "exact original payload/player receipt");
                        assert!(
                            receipts[index].is_none(),
                            "duplicate original action receipt"
                        );
                        receipts[index] = Some((batch.height, batch.block_hash, batch.action_root));
                    }
                }
            }
            receipts.iter().all(Option::is_some)
                && rotation_receipts.iter().all(Option::is_some)
                && actual.iter().all(|driver| {
                    driver
                        .lock()
                        .unwrap()
                        .world()
                        .state()
                        .agents
                        .contains_key(marker)
                })
        },
    );
    for receipt in &receipts {
        assert_eq!(receipt.as_ref(), receipts[0].as_ref());
    }
    for receipt in &rotation_receipts {
        assert_eq!(receipt.as_ref(), rotation_receipts[0].as_ref());
    }
    assert_eq!(
        old_calls.load(Ordering::SeqCst),
        retired_count,
        "retired handle has zero calls after h9 ACK"
    );
    assert!(new_calls.load(Ordering::SeqCst) > 0);
    for node in &mut nodes {
        assert!(node.bft_v3_status().unwrap().running);
        node.stop().unwrap();
    }
    for (index, snapshot) in snapshots.iter().enumerate() {
        let state = &snapshot.lock().unwrap().state;
        let decision = state
            .history
            .iter()
            .find(|c| c.execution_commitment.height == 7)
            .unwrap();
        assert!(
            decision.certificate.proposal.round >= 1,
            "dropped r0 must use real elapsed consensus timers"
        );
        let confirmation = state
            .history
            .iter()
            .find(|c| c.execution_commitment.height == 8)
            .unwrap();
        for certificate in [decision, confirmation] {
            assert!(
                certificate
                    .certificate
                    .precommits
                    .iter()
                    .all(|vote| vote.validator_id != validator_id(3)
                        || vote.public_key_hex == public(123))
            );
        }
        if index == 3 {
            for journal in state
                .completed_journals
                .iter()
                .flatten()
                .filter(|j| j.height() == 7 || j.height() == 8)
            {
                assert_eq!(journal.local_signing_public_key(), public(123));
            }
            for journal in state
                .completed_journals
                .iter()
                .flatten()
                .chain(state.journal.iter())
                .filter(|j| j.height() >= 9)
            {
                assert_eq!(journal.local_signing_public_key(), public(124));
            }
        }
        let driver = actual[index].lock().unwrap();
        let world = driver.world();
        assert_eq!(
            serde_cbor::to_vec(&world.state().governance_finality_signer_registry).unwrap(),
            immutable[index].0
        );
        assert_eq!(
            serde_cbor::to_vec(&world.state().governance_validator_admissions).unwrap(),
            immutable[index].1
        );
        assert_eq!(
            world.node_identity_public_key(&validator_id(3)),
            immutable[index].2.as_deref()
        );
        assert_eq!(
            world.node_main_token_account(&validator_id(3)),
            immutable[index].3.as_deref()
        );
        assert_eq!(world.state().governance_finality_key_rotations.len(), 1);
        assert!(
            world
                .state()
                .governance_bootstrap_finality_key_rotations
                .is_empty()
        );
        let effective = world
            .resolve_governance_effective_finality_signer_registry()
            .unwrap()
            .unwrap();
        assert_eq!(effective.signer_bindings[&validator_id(3)], public(124));
        assert_eq!(effective.validator_stakes[&validator_id(3)], 1);
    }
    for server in servers {
        server.join().unwrap().unwrap();
    }
}
