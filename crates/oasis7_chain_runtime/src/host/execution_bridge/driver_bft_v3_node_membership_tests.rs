//! Actual signed governance, four Noise nodes and native provider install ACK.
//! Issuer witness/clock and signer backend remain explicit memory test doubles.
use super::*;
use oasis7::consensus_action_payload::{
    ConsensusActionAuthEnvelope, ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7_runtime::Action as RuntimeAction;
use oasis7_runtime::{GovernanceFinalitySignerRegistry, GovernanceValidatorAdmissionStatus};

#[path = "driver_bft_v3_node_membership_fault_tests.rs"]
mod fault_cases;
#[path = "driver_bft_v3_node_membership_rotation_tests.rs"]
mod rotation_cases;

pub(super) fn two_session_provider(
    service: Arc<BftSignerServiceV3>,
) -> (SocketAddr, thread::JoinHandle<Result<(), String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let join = thread::spawn(move || {
        for _ in 0..2 {
            let deadline = Instant::now() + Duration::from_secs(180);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return Err(
                                "membership Node did not reopen its native provider session".into(),
                            );
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => return Err(error.to_string()),
                }
            };
            stream.set_nonblocking(false).unwrap();
            service
                .serve_connection(&mut stream)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    });
    (address, join)
}

#[test]
fn wire3_membership_actual_world_noise_follower_installs_at_h_plus_two() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| network_case_membership(false, false, false, false, true))
        .unwrap()
        .join()
        .unwrap();
}

pub(super) fn handoff() -> QualifiedH<BftProfileTargetV2> {
    qualified_handoff_v3_with_count_and_setup(3, |world| {
        let mut policy = world.governance_execution_policy().clone();
        policy.epoch_length_ticks = 3;
        world.set_governance_execution_policy(policy).unwrap();
        world
            .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
                slot_id: "governance.finality.v1".into(),
                threshold: 3,
                signer_bindings: (0..3)
                    .map(|i| {
                        (
                            validator_id(i),
                            hex::encode(fixtures::key(120 + i).verifying_key().to_bytes()),
                        )
                    })
                    .collect(),
                validator_stakes: (0..3).map(|i| (validator_id(i), 1)).collect(),
                ..Default::default()
            })
            .unwrap();
    })
}

pub(super) fn submit_admission(
    node: &NodeRuntime,
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
) {
    let payloads = admission_payloads(&actual.lock().unwrap());
    for (id, payload) in payloads {
        node.submit_consensus_action_payload(id, payload).unwrap();
    }
}

fn admission_payloads(driver: &FormalBftHeaderNodeDriverV3) -> Vec<(u64, Vec<u8>)> {
    let mut world = driver.world().clone();
    world.attach_formal_bft_header_handoff_parent_v3(driver.handoff(), driver.initial_schedule())
        .expect("authorize against the independently qualified header parent, not the controlled raw root");
    let actions = [
        RuntimeAction::SubmitGovernanceValidatorAdmission {
            controller_account_id: "formal-bft-controller".into(),
            candidate_id: "new-formal-validator".into(),
            node_id: validator_id(3),
            finality_signer_public_key: hex::encode(fixtures::key(123).verifying_key().to_bytes()),
            stake: 1,
            operator_owner: "local-membership-test".into(),
            public_manifest_hash: "local-membership-manifest".into(),
        },
        RuntimeAction::ApproveGovernanceValidatorAdmission {
            controller_account_id: "formal-bft-controller".into(),
            candidate_id: "new-formal-validator".into(),
        },
        RuntimeAction::ActivateGovernanceValidatorAdmission {
            controller_account_id: "formal-bft-controller".into(),
            candidate_id: "new-formal-validator".into(),
            activation_epoch: 2,
        },
    ];
    actions
        .into_iter()
        .enumerate()
        .map(|(index, action)| {
            let mut auth = world
                .governance_authorization_for_next_commit(index as u64 + 1)
                .unwrap();
            let key = fixtures::key(102);
            auth.signatures.insert(
                hex::encode(key.verifying_key().to_bytes()),
                hex::encode(key.sign(&auth.signing_bytes(&action).unwrap()).to_bytes()),
            );
            let envelope = ConsensusActionPayloadEnvelope::from_runtime_action_with_auth(
                action,
                ConsensusActionAuthEnvelope::GovernanceAction(auth),
            );
            (
                9600 + index as u64,
                encode_consensus_action_payload(&envelope).unwrap(),
            )
        })
        .collect()
}

pub(super) fn exercise(
    nodes: &mut [NodeRuntime],
    actual: &[Arc<Mutex<FormalBftHeaderNodeDriverV3>>],
    snapshots: &[Arc<Mutex<BftSignerBackendSnapshotV3>>],
    views: &[Arc<NodeBftPeerExecutionStateV3>],
    isolation: &std::sync::atomic::AtomicBool,
    clock_start: &std::sync::OnceLock<Instant>,
) {
    // Node zero is isolated at height 6 by the transport fixture, before it
    // can contribute a vote or deliver a locally assembled certificate.
    // Keep the fault-free signed governance prefix on the original fixed test
    // clock. Start elapsed timeout recovery only at the injected height-6
    // partition, after every actual driver installed the transition prefix.
    // Governance actions remain bound to their original qualified parent.
    wait_height(nodes, snapshots, 5);
    clock_start.set(Instant::now()).unwrap();
    wait_height(&nodes[1..], &snapshots[1..], 6);
    assert_eq!(
        snapshots[0]
            .lock()
            .unwrap()
            .state
            .history
            .last()
            .unwrap()
            .execution_commitment
            .height,
        5
    );
    for snapshot in snapshots {
        let snapshot = snapshot.lock().unwrap();
        let source = snapshot
            .state
            .history
            .iter()
            .find(|c| c.execution_commitment.height == 4)
            .unwrap();
        let transition = source.execution_commitment.transition.as_ref().unwrap();
        assert_eq!(transition.activation_height, 6);
        assert_eq!(transition.new_validators.len(), 4);
        let confirmation = snapshot
            .state
            .history
            .iter()
            .find(|c| c.execution_commitment.height == 5)
            .unwrap();
        assert_eq!(
            confirmation
                .execution_commitment
                .transition_ref
                .as_ref()
                .unwrap()
                .source_block_id,
            source.header.block_id().unwrap()
        );
        for old in snapshot
            .state
            .history
            .iter()
            .filter(|c| c.execution_commitment.height <= 5)
        {
            assert!(
                !old.certificate
                    .precommits
                    .iter()
                    .any(|v| v.validator_id == validator_id(3))
            );
        }
        let extension = snapshot.state.membership_install.as_ref().unwrap();
        assert!(extension.pending.is_none());
        assert_eq!(extension.receipts.len(), 1);
    }
    let future = snapshots[3].lock().unwrap();
    for (certificate, journal) in future
        .state
        .history
        .iter()
        .zip(&future.state.completed_journals)
    {
        if certificate.execution_commitment.height <= 5 {
            assert!(journal.is_none());
        }
    }
    let activated = future
        .state
        .history
        .iter()
        .find(|c| c.execution_commitment.height == 6)
        .unwrap();
    assert!(
        activated
            .certificate
            .precommits
            .iter()
            .any(|v| v.validator_id == validator_id(3))
    );
    assert!(
        !activated
            .certificate
            .precommits
            .iter()
            .any(|v| v.validator_id == validator_id(0))
    );
    drop(future);
    for view in &views[1..] {
        assert_eq!(
            view.view()
                .unwrap()
                .unwrap()
                .schedule
                .active_set()
                .validators()
                .len(),
            4
        );
    }
    // Empty blocks are legal and can already have an original proposal.
    // Bind the positive outcome to this exact action, not to a guessed height.
    // Retry the identical player action through every healthy ingress.
    // No mempool gossip is claimed; committed equality clears every copy.
    for node in &nodes[1..] {
        node.submit_consensus_action_payload(9706, payload("membership-agent-6"))
            .unwrap();
    }
    wait_action(&nodes[1..], &snapshots[1..], 9706, "membership-agent-6");
    for driver in &actual[1..] {
        let driver = driver.lock().unwrap();
        assert!(
            driver
                .world()
                .state()
                .agents
                .contains_key("membership-agent-6")
        );
        // Epoch activation projects the effective registry without rewriting
        // the original admission decision's audit status.
        let admission =
            &driver.world().state().governance_validator_admissions["new-formal-validator"];
        assert_eq!(
            admission.status,
            GovernanceValidatorAdmissionStatus::ProbationReady
        );
        assert_eq!(admission.activation_epoch, Some(2));
        let effective = driver
            .world()
            .resolve_governance_effective_finality_signer_registry()
            .unwrap()
            .unwrap();
        assert_eq!(effective.signer_bindings.len(), 4);
        assert_eq!(
            effective.signer_bindings.get(&validator_id(3)),
            Some(&hex::encode(fixtures::key(123).verifying_key().to_bytes()))
        );
    }
    for node in nodes.iter_mut() {
        node.stop().unwrap();
    }
    isolation.store(false, std::sync::atomic::Ordering::Release);
    for node in nodes.iter_mut() {
        node.start().unwrap();
    }
    wait_height(nodes, snapshots, 6);
    for node in nodes.iter() {
        node.submit_consensus_action_payload(9807, payload("membership-after-restart"))
            .unwrap();
    }
    wait_action(nodes, snapshots, 9807, "membership-after-restart");
    for driver in actual {
        assert!(
            driver
                .lock()
                .unwrap()
                .world()
                .state()
                .agents
                .contains_key("membership-after-restart")
        );
    }
    for node in nodes.iter_mut() {
        node.stop().unwrap();
    }
}
fn wait_action(
    nodes: &[NodeRuntime],
    snapshots: &[Arc<Mutex<BftSignerBackendSnapshotV3>>],
    id: u64,
    marker: &str,
) {
    let expected = oasis7_node::NodeConsensusAction::from_payload(
        id,
        "membership-test-player",
        payload(marker),
    )
    .unwrap();
    let mut completed = vec![false; nodes.len()];
    let mut observed_heights = nodes
        .iter()
        .map(|node| node.bft_v3_status().unwrap().committed_height)
        .collect::<Vec<_>>();
    // One original empty proposal may already be retained when input arrives.
    // Every healthy ingress now has the same action, so the following height
    // must include it. Empty progress cannot postpone this oracle indefinitely.
    let maximum_height = observed_heights
        .iter()
        .max()
        .unwrap()
        .checked_add(2)
        .unwrap();
    let mut deadlines = vec![Instant::now() + Duration::from_secs(180); nodes.len()];
    loop {
        for (index, node) in nodes.iter().enumerate() {
            let status = node.bft_v3_status().unwrap();
            assert!(status.running, "{status:?}");
            for batch in node.drain_committed_action_batches() {
                if let Some(action) = batch.actions.iter().find(|action| action.action_id == id) {
                    assert_eq!(
                        action, &expected,
                        "receipt must name the exact original player action"
                    );
                    assert!(!completed[index], "duplicate original action receipt");
                    completed[index] = true;
                }
            }
            if !completed[index] {
                assert!(
                    status.committed_height <= maximum_height,
                    "original action {id} missing beyond bounded next proposal: {status:?}"
                );
                if status.committed_height > observed_heights[index] {
                    observed_heights[index] = status.committed_height;
                    deadlines[index] = Instant::now() + Duration::from_secs(180);
                }
            }
        }
        if completed.iter().all(|done| *done) {
            return;
        }
        assert!(
            completed
                .iter()
                .zip(&deadlines)
                .all(|(done, deadline)| *done || Instant::now() < *deadline),
            "timeout exact action {id}; completed={completed:?}; nodes={:?}; journals={:?}",
            nodes
                .iter()
                .map(NodeRuntime::bft_v3_status)
                .collect::<Vec<_>>(),
            snapshots
                .iter()
                .map(|snapshot| {
                    let snapshot = snapshot.lock().unwrap();
                    let events = snapshot.state.journal.as_ref().map(|journal| {
                        journal
                            .events
                            .iter()
                            .map(|event| {
                                use oasis7_consensus::tendermint_v3::BftSigningJournalEventV3 as E;
                                match event {
                                    E::Proposal {
                                        proposal,
                                        local,
                                        now_ms,
                                    } => format!(
                                        "proposal r={} by={} value={} local={} time={}",
                                        proposal.header.round,
                                        proposal.header.proposer_id,
                                        proposal.header.block_id,
                                        local,
                                        now_ms
                                    ),
                                    E::Vote {
                                        vote,
                                        local,
                                        now_ms,
                                    } => format!(
                                        "vote r={} by={} phase={:?} value={:?} local={} time={}",
                                        vote.round,
                                        vote.validator_id,
                                        vote.phase,
                                        vote.value,
                                        local,
                                        now_ms
                                    ),
                                    E::Timeout { now_ms } => format!("timeout time={now_ms}"),
                                    E::ProposalEquivocation { now_ms, .. } => {
                                        format!("equivocation time={now_ms}")
                                    }
                                }
                            })
                            .collect::<Vec<_>>()
                    });
                    (
                        snapshot.protected_time_floor_ms,
                        snapshot
                            .state
                            .history
                            .last()
                            .map(|c| c.execution_commitment.height),
                        snapshot.state.pending_driver_apply.clone(),
                        events,
                    )
                })
                .collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

pub(super) struct HeightPartition {
    pub inner: Arc<dyn oasis7_node::NodeBftPeerTransportV3>,
    pub isolated: Arc<std::sync::atomic::AtomicBool>,
}
impl HeightPartition {
    fn blocked(&self, message: &oasis7_consensus::tendermint_v3::BftPublicPeerMessageV3) -> bool {
        use oasis7_consensus::tendermint_v3::BftPublicPeerMessageV3 as M;
        self.isolated.load(std::sync::atomic::Ordering::Acquire)
            && match message {
                M::Proposal(p) => p.header.height >= 6,
                M::Vote(v) => v.height >= 6,
                M::Commit(c) => c.execution_commitment.height >= 6,
                M::HistoryReply(r) => r
                    .certificates
                    .iter()
                    .any(|c| c.execution_commitment.height >= 6),
                M::HistoryRequest(_) => false,
            }
    }
}
impl oasis7_node::NodeBftPeerTransportV3 for HeightPartition {
    fn context(&self) -> &oasis7_consensus::tendermint_v3::BftPeerContextV3 {
        self.inner.context()
    }
    fn peers(&self) -> Vec<String> {
        self.inner.peers()
    }
    fn broadcast_public(
        &self,
        m: &oasis7_consensus::tendermint_v3::BftPublicPeerMessageV3,
    ) -> Result<(), String> {
        if self.blocked(m) {
            Ok(())
        } else {
            self.inner.broadcast_public(m)
        }
    }
    fn send_public(
        &self,
        p: &str,
        m: &oasis7_consensus::tendermint_v3::BftPublicPeerMessageV3,
    ) -> Result<(), String> {
        if self.blocked(m) {
            Ok(())
        } else {
            self.inner.send_public(p, m)
        }
    }
    fn drain_public(
        &self,
        max: usize,
    ) -> Result<Vec<oasis7_node::NodeBftReceivedPublicV3>, String> {
        Ok(self
            .inner
            .drain_public(max)?
            .into_iter()
            .filter(|m| !self.blocked(&m.message))
            .collect())
    }
}

fn wait_height(
    nodes: &[NodeRuntime],
    snapshots: &[Arc<Mutex<BftSignerBackendSnapshotV3>>],
    height: u64,
) {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        for node in nodes {
            assert!(
                node.bft_v3_status().unwrap().running,
                "membership Node blocked: {:?}",
                node.bft_v3_status()
            );
        }
        let ready = nodes
            .iter()
            .all(|node| node.bft_v3_status().unwrap().committed_height >= height)
            && snapshots.iter().all(|snapshot| {
                let snapshot = snapshot.lock().unwrap();
                snapshot
                    .state
                    .applied_installs
                    .iter()
                    .any(|receipt| receipt.height == height)
                    && snapshot.state.pending_driver_apply.is_none()
            });
        if ready {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timeout membership height {height}; nodes={:?}; clocks={:?}",
            nodes
                .iter()
                .map(NodeRuntime::bft_v3_status)
                .collect::<Vec<_>>(),
            snapshots
                .iter()
                .map(|snapshot| {
                    let snapshot = snapshot.lock().unwrap();
                    (
                        snapshot.protected_time_floor_ms,
                        snapshot
                            .state
                            .journal
                            .as_ref()
                            .and_then(|j| j.height_started_at_ms()),
                        snapshot
                            .state
                            .history
                            .last()
                            .map(|c| c.execution_commitment.height),
                    )
                })
                .collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(20));
    }
}
