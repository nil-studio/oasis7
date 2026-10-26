use ed25519_dalek::{Signer, SigningKey};
use oasis7::consensus_action_payload::{
    ConsensusActionAuthEnvelope, ConsensusActionPayloadBody, ConsensusActionPayloadEnvelope,
    encode_consensus_action_payload,
};
use oasis7::geometry::GeoPos;
use oasis7_consensus::tendermint_v2::{
    BftHeightProposerScheduleV2, BftSchedulePolicyV2, verify_commit_certificate_token_v2,
};
use oasis7_distfs::protected_bft_v2::{
    BftSignerAcquireOutcomeV2, BftSignerBackendSnapshotV2, BftSignerBackendV2, BftSignerBindingV2,
    BftSignerErrorV2, BftSignerKeyIdentityV2, BftSignerServiceV2, BftSignerStateV2,
    BftSignerTrustV2, BftSigningKeyHandleV2, RemoteBftSignerProviderV2,
};
use oasis7_node::{
    BftBlockV2, BftCommitCertificateV2, BftProposalV2, BftScopeV2, BftValidatorV2,
    NodeBftBuildContextV2, NodeBftExecutionConfigV2, NodeConfig, NodeConsensusAction,
    NodeExecutionHook, NodeReplicationConfig, NodeRole, NodeRuntime,
};
use oasis7_proto::distributed_bft::v2::{
    BftProposalHeaderV2, BftVotePhaseV2, BftVoteV2, BftVoteValueV2,
};
use oasis7_runtime::Action as RuntimeAction;
use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const AUTHORITY_EPOCH: u64 = 9;

#[path = "driver_bft_v2_recovery.rs"]
mod recovery;
#[path = "driver_bft_v2_transition.rs"]
mod transition;

struct NodeTestSignerKey(SigningKey);

impl BftSigningKeyHandleV2 for NodeTestSignerKey {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }

    fn sign_preimage(&self, preimage: &[u8]) -> Result<[u8; 64], BftSignerErrorV2> {
        Ok(self.0.sign(preimage).to_bytes())
    }
}

struct NodeTestSignerBackend {
    binding_digest: String,
    snapshot: Arc<Mutex<BftSignerBackendSnapshotV2>>,
}

impl NodeTestSignerBackend {
    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, BftSignerBackendSnapshotV2>, BftSignerErrorV2> {
        self.snapshot
            .lock()
            .map_err(|_| BftSignerErrorV2("Node test signer backend lock poisoned".into()))
    }

    fn check_binding(&self, binding_digest: &str) -> Result<(), BftSignerErrorV2> {
        if binding_digest != self.binding_digest {
            return Err(BftSignerErrorV2("Node test signer binding mismatch".into()));
        }
        Ok(())
    }

    fn check_lease(
        snapshot: &BftSignerBackendSnapshotV2,
        generation: u64,
        token_digest: &str,
    ) -> Result<(), BftSignerErrorV2> {
        if generation != snapshot.lease_generation
            || token_digest.is_empty()
            || token_digest != snapshot.lease_token_digest
        {
            return Err(BftSignerErrorV2("Node test signer lease fenced".into()));
        }
        Ok(())
    }
}

impl BftSignerBackendV2 for NodeTestSignerBackend {
    fn load_provisioned(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        self.check_binding(binding_digest)?;
        Ok(self.lock()?.clone())
    }

    fn acquire_lease(
        &mut self,
        binding_digest: &str,
        expected_generation: u64,
        new_lease_token_digest: &str,
    ) -> Result<BftSignerAcquireOutcomeV2, BftSignerErrorV2> {
        self.check_binding(binding_digest)?;
        let mut snapshot = self.lock()?;
        if snapshot.lease_generation != expected_generation
            || !snapshot.lease_token_digest.is_empty()
        {
            return Ok(BftSignerAcquireOutcomeV2::Conflict(snapshot.clone()));
        }
        snapshot.lease_generation = snapshot
            .lease_generation
            .checked_add(1)
            .ok_or_else(|| BftSignerErrorV2("Node test signer generation overflow".into()))?;
        snapshot.lease_token_digest = new_lease_token_digest.to_string();
        Ok(BftSignerAcquireOutcomeV2::Acquired(snapshot.clone()))
    }

    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        let mut snapshot = self.lock()?;
        if snapshot.lease_generation == expected_generation
            && snapshot.lease_token_digest == expected_lease_token_digest
        {
            snapshot.lease_generation = snapshot
                .lease_generation
                .checked_add(1)
                .ok_or_else(|| BftSignerErrorV2("Node test signer generation overflow".into()))?;
            snapshot.lease_token_digest.clear();
        }
        Ok(snapshot.clone())
    }

    fn read_for_lease(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        let snapshot = self.lock()?;
        Self::check_lease(&snapshot, lease_generation, lease_token_digest)?;
        Ok(snapshot.clone())
    }

    fn protected_time_ms(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<i64, BftSignerErrorV2> {
        let mut snapshot = self.lock()?;
        Self::check_lease(&snapshot, lease_generation, lease_token_digest)?;
        snapshot.protected_time_floor_ms = snapshot
            .protected_time_floor_ms
            .checked_add(1)
            .ok_or_else(|| BftSignerErrorV2("Node test signer clock overflow".into()))?;
        Ok(snapshot.protected_time_floor_ms)
    }

    fn compare_and_swap(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: Option<i64>,
        next_state: BftSignerStateV2,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        let mut snapshot = self.lock()?;
        Self::check_lease(&snapshot, lease_generation, lease_token_digest)?;
        if snapshot.state.revision != expected_revision
            || snapshot.state.state_digest != expected_state_digest
            || next_state.revision
                != expected_revision
                    .checked_add(1)
                    .ok_or_else(|| BftSignerErrorV2("Node test signer revision overflow".into()))?
        {
            return Err(BftSignerErrorV2("Node test signer CAS mismatch".into()));
        }
        if let Some(event_time) = minimum_event_time_ms {
            snapshot.protected_time_floor_ms = snapshot.protected_time_floor_ms.max(event_time);
        }
        snapshot.state = next_state;
        Ok(snapshot.clone())
    }

    fn complete_transition_install(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: i64,
        next_lease_token_digest: &str,
        next_state: BftSignerStateV2,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        let mut snapshot = self.lock()?;
        Self::check_lease(&snapshot, lease_generation, lease_token_digest)?;
        if snapshot.state.revision != expected_revision
            || snapshot.state.state_digest != expected_state_digest
            || next_state.revision
                != expected_revision
                    .checked_add(1)
                    .ok_or_else(|| BftSignerErrorV2("Node test signer revision overflow".into()))?
        {
            return Err(BftSignerErrorV2(
                "Node test signer install CAS mismatch".into(),
            ));
        }
        snapshot.protected_time_floor_ms =
            snapshot.protected_time_floor_ms.max(minimum_event_time_ms);
        snapshot.state = next_state;
        snapshot.lease_generation = lease_generation
            .checked_add(1)
            .ok_or_else(|| BftSignerErrorV2("Node test signer generation overflow".into()))?;
        snapshot.lease_token_digest = next_lease_token_digest.to_string();
        Ok(snapshot.clone())
    }
}

fn spawn_node_test_signer(
    service: Arc<BftSignerServiceV2>,
) -> (SocketAddr, thread::JoinHandle<Result<(), String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind Node test signer TCP listener");
    listener
        .set_nonblocking(true)
        .expect("set Node signer listener nonblocking");
    let address = listener.local_addr().expect("Node signer listener address");
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err("Node did not connect to the protected signer".into());
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(format!("accept Node signer connection: {error}")),
            }
        };
        stream
            .set_nonblocking(false)
            .map_err(|error| format!("set Node signer stream blocking: {error}"))?;
        service
            .serve_connection(&mut stream)
            .map_err(|error| format!("serve Node protected signer session: {error}"))
    });
    (address, worker)
}

fn v2_scope() -> BftScopeV2 {
    BftScopeV2 {
        world_id: "driver-bft-v2-world".into(),
        chain_id: "driver-bft-v2-chain".into(),
        genesis_hash: "11".repeat(32),
    }
}

fn v2_genesis_config(
    scope: &BftScopeV2,
    keys: &[SigningKey],
) -> (NodeBftExecutionConfigV2, oasis7_runtime::World) {
    let genesis_world = super::driver_bft::bft_genesis_world(&oasis7_node::BftScopeV1 {
        world_id: scope.world_id.clone(),
        chain_id: scope.chain_id.clone(),
        genesis_hash: scope.genesis_hash.clone(),
    });
    let epoch_length_ticks = genesis_world
        .governance_execution_policy()
        .epoch_length_ticks;
    v2_config_for_world(scope, keys, genesis_world, epoch_length_ticks)
}

fn v2_config_for_world(
    scope: &BftScopeV2,
    keys: &[SigningKey],
    genesis_world: oasis7_runtime::World,
    epoch_length_ticks: u64,
) -> (NodeBftExecutionConfigV2, oasis7_runtime::World) {
    let genesis_state_root = genesis_world
        .execution_state_identity()
        .expect("genesis state identity")
        .state_root_hash;
    let genesis_manifest_hash = genesis_world
        .current_manifest_hash()
        .expect("genesis manifest hash");
    let initial_validators = keys
        .iter()
        .take(4)
        .enumerate()
        .map(|(index, key)| BftValidatorV2 {
            validator_id: format!("v{index}"),
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            voting_power: 1,
        })
        .collect();
    let config = NodeBftExecutionConfigV2 {
        scope: scope.clone(),
        authority_epoch: AUTHORITY_EPOCH,
        initial_validators,
        schedule_policy: BftSchedulePolicyV2::new(epoch_length_ticks, "driver-bft-v2-test")
            .expect("schedule policy"),
        genesis_state_root,
        genesis_manifest_hash,
    };
    (config, genesis_world)
}

fn v2_runtime_action(height: u64) -> NodeConsensusAction {
    let envelope =
        ConsensusActionPayloadEnvelope::from_runtime_action(RuntimeAction::RegisterAgent {
            agent_id: format!("bft-v2-agent-{height}"),
            pos: GeoPos::new(1, 2, 3),
        });
    let payload = encode_consensus_action_payload(&envelope).expect("encode runtime action");
    NodeConsensusAction::from_payload(1, "v0", payload).expect("build action")
}

fn v2_signed_governance_action(
    schedule: &BftHeightProposerScheduleV2,
    parent: &oasis7_runtime::World,
    controller_key: &SigningKey,
    action: RuntimeAction,
) -> NodeConsensusAction {
    let controller_account_id = match &action {
        RuntimeAction::SubmitGovernanceValidatorAdmission {
            controller_account_id,
            ..
        }
        | RuntimeAction::ApproveGovernanceValidatorAdmission {
            controller_account_id,
            ..
        }
        | RuntimeAction::ActivateGovernanceValidatorAdmission {
            controller_account_id,
            ..
        }
        | RuntimeAction::RevokeGovernanceValidatorAdmission {
            controller_account_id,
            ..
        } => controller_account_id,
        _ => panic!("test governance signer only accepts validator admission actions"),
    };
    let next_nonce = parent
        .snapshot()
        .state
        .governance_authorization_nonces
        .get(controller_account_id)
        .copied()
        .unwrap_or_default()
        .checked_add(1)
        .expect("test governance nonce increment");
    let mut authorized_parent = parent.clone();
    authorized_parent
        .attach_bft_governance_parent_v2(schedule)
        .expect("attach exact governance parent");
    let mut authorization = authorized_parent
        .governance_authorization_for_next_commit(next_nonce)
        .expect("build exact governance authorization");
    let signature = controller_key.sign(
        &authorization
            .signing_bytes(&action)
            .expect("encode governance authorization bytes"),
    );
    authorization.signatures.insert(
        hex::encode(controller_key.verifying_key().to_bytes()),
        hex::encode(signature.to_bytes()),
    );
    let envelope = ConsensusActionPayloadEnvelope::from_runtime_action_with_auth(
        action,
        ConsensusActionAuthEnvelope::GovernanceAction(authorization),
    );
    let payload = encode_consensus_action_payload(&envelope).expect("encode governance payload");
    NodeConsensusAction::from_payload(1, "controller", payload).expect("build action")
}

fn v2_certificate(
    schedule: &BftHeightProposerScheduleV2,
    block: BftBlockV2,
    keys: &[SigningKey],
) -> BftCommitCertificateV2 {
    let set = schedule.active_set();
    let block_id = block.block_id().expect("candidate block id");
    let proposer_id = schedule
        .expected_proposer_for_round(0)
        .expect("round-zero proposer");
    let proposer_key_index = proposer_id
        .strip_prefix('v')
        .expect("test validator id")
        .parse::<usize>()
        .expect("validator index");
    let header = BftProposalHeaderV2::sign(
        schedule.scope().clone(),
        schedule.height(),
        0,
        set.set_hash().to_string(),
        schedule.policy_hash().to_string(),
        proposer_id,
        block_id.clone(),
        None,
        &keys[proposer_key_index],
    )
    .expect("sign proposal header");
    let proposal = BftProposalV2 {
        header,
        block,
        valid_round_evidence: None,
    };
    let mut precommit_power = 0u64;
    let mut precommits = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        let validator_id = format!("v{index}");
        let Some(power) = set.voting_power(validator_id.as_str()) else {
            continue;
        };
        if precommit_power >= set.quorum_voting_power() {
            break;
        }
        precommits.push(
            BftVoteV2::sign(
                schedule.scope().clone(),
                schedule.height(),
                0,
                BftVotePhaseV2::Precommit,
                set.set_hash().to_string(),
                schedule.policy_hash().to_string(),
                BftVoteValueV2::Block {
                    block_id: block_id.clone(),
                },
                validator_id,
                key,
            )
            .expect("sign precommit"),
        );
        precommit_power += power;
    }
    assert!(precommit_power >= set.quorum_voting_power());
    BftCommitCertificateV2 {
        proposal: Box::new(proposal),
        precommits,
    }
}

#[test]
fn dynamic_set_bft_driver_builds_previews_commits_and_replays_exact_qc_history() {
    let allocated = super::temp_dir("bft-v2-driver-replay");
    let basename = allocated
        .file_name()
        .expect("test path basename")
        .to_string_lossy();
    let root = std::env::temp_dir()
        .canonicalize()
        .expect("canonical temp root")
        .join(basename.as_ref());
    let keys = super::driver_bft::keys();
    let scope = v2_scope();
    let (config, genesis_world) = v2_genesis_config(&scope, &keys);
    let mut driver = super::driver_bft::driver_for_world(&root, genesis_world);
    let initial_schedule = config.validate().expect("configured genesis schedule");
    assert!(
        driver
            .configure_bft_v2_execution(&config)
            .expect("initialize V2 execution")
            .is_none()
    );

    let execution = driver
        .current_bft_v2_execution_context(&initial_schedule)
        .expect("exact genesis execution context");
    let candidate = driver
        .build_bft_v2_candidate(NodeBftBuildContextV2 {
            execution: execution.clone(),
            actions: vec![v2_runtime_action(1)],
        })
        .expect("build V2 execution candidate");
    driver
        .preview_bft_v2_candidate(&execution, &candidate)
        .expect("independently replay V2 candidate");

    let certificate = v2_certificate(&initial_schedule, candidate, &keys);
    let token = verify_commit_certificate_token_v2(&initial_schedule, &certificate)
        .expect("verify V2 commit certificate");
    let expected_result = driver
        .commit_bft_v2_certificate(token)
        .expect("durably commit V2 execution");
    assert_eq!(expected_result.height, 1);
    assert_eq!(expected_result.driver_install_digest.len(), 64);
    assert!(
        expected_result
            .driver_install_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "durable install receipt is a lowercase hexadecimal digest"
    );

    drop(driver);
    let (_, restored_genesis) = v2_genesis_config(&scope, &keys);
    let mut reopened = super::driver_bft::driver_for_world(&root, restored_genesis);
    assert!(
        reopened
            .configure_bft_v2_execution(&config)
            .expect("reopen exact V2 execution ledger")
            .is_none(),
        "a structural durable tip is not exposed as a verified head before raw QC replay"
    );
    let history = reopened
        .load_bft_v2_commit_history_page(&config, 0, 8)
        .expect("load durable raw V2 QC history");
    assert_eq!(history.len(), 1);
    let token = verify_commit_certificate_token_v2(&initial_schedule, &history[0])
        .expect("independently verify restored QC");
    let replayed = reopened
        .commit_bft_v2_certificate(token)
        .expect("replay exact V2 QC into restored World");
    assert_eq!(replayed, expected_result);

    drop(reopened);
    let _ = std::fs::remove_dir_all(root);
}

fn dynamic_genesis_world(scope: &BftScopeV2, keys: &[SigningKey]) -> oasis7_runtime::World {
    let base = super::driver_bft::bft_genesis_world(&oasis7_node::BftScopeV1 {
        world_id: scope.world_id.clone(),
        chain_id: scope.chain_id.clone(),
        genesis_hash: scope.genesis_hash.clone(),
    });
    let mut snapshot = base.snapshot();
    snapshot.governance_execution_policy.epoch_length_ticks = 3;
    snapshot.state.governance_finality_signer_registry =
        Some(oasis7_runtime::GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".into(),
            threshold: 3,
            signer_bindings: (0..4)
                .map(|index| {
                    (
                        format!("v{index}"),
                        hex::encode(keys[index].verifying_key().to_bytes()),
                    )
                })
                .collect(),
            validator_stakes: (0..4).map(|index| (format!("v{index}"), 1)).collect(),
            ..Default::default()
        });
    snapshot.state.governance_validator_admissions.insert(
        "candidate-v4".into(),
        oasis7_runtime::GovernanceValidatorAdmissionRecord {
            candidate_id: "candidate-v4".into(),
            node_id: "v4".into(),
            finality_signer_public_key: hex::encode(keys[4].verifying_key().to_bytes()),
            stake: 1,
            operator_owner: "test-operator".into(),
            public_manifest_hash: "test-manifest".into(),
            requested_at_epoch: 0,
            last_transition_tick: 0,
            activation_epoch: Some(1),
            status: oasis7_runtime::GovernanceValidatorAdmissionStatus::ProbationReady,
            ..Default::default()
        },
    );
    oasis7_runtime::World::from_snapshot_with_release_security_policy(
        snapshot,
        base.journal().clone(),
        base.release_security_policy().clone(),
    )
    .expect("restore deterministic dynamic-set genesis World")
}

#[test]
fn v2_governance_payload_ingress_and_set_changes_use_the_verified_parent() {
    let allocated = super::temp_dir("bft-v2-missing-governance-registry");
    let basename = allocated
        .file_name()
        .expect("test path basename")
        .to_string_lossy();
    let root = std::env::temp_dir()
        .canonicalize()
        .expect("canonical temp root")
        .join(basename.as_ref());
    let keys = (1..=5)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let controller_key = SigningKey::from_bytes(&[73; 32]);
    let scope = v2_scope();
    let base = super::driver_bft::bft_genesis_world(&oasis7_node::BftScopeV1 {
        world_id: scope.world_id.clone(),
        chain_id: scope.chain_id.clone(),
        genesis_hash: scope.genesis_hash.clone(),
    });
    let mut snapshot = base.snapshot();
    snapshot.governance_execution_policy.epoch_length_ticks = 3;
    snapshot.state.governance_finality_signer_registry = None;
    let mut genesis_world = oasis7_runtime::World::from_snapshot_with_release_security_policy(
        snapshot,
        base.journal().clone(),
        base.release_security_policy().clone(),
    )
    .expect("restore no-registry BFT genesis World");
    genesis_world
        .set_governance_main_token_controller_registry(
            oasis7_runtime::GovernanceMainTokenControllerRegistry {
                genesis_controller_account_id: "controller".into(),
                controller_signer_policies: BTreeMap::from([(
                    "controller".into(),
                    oasis7_runtime::GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: [hex::encode(
                            controller_key.verifying_key().to_bytes(),
                        )]
                        .into(),
                    },
                )]),
                ..Default::default()
            },
        )
        .expect("install action signing authority");
    let (config, genesis_world) = v2_config_for_world(&scope, &keys, genesis_world, 3);
    let mut driver = super::driver_bft::driver_for_world(&root, genesis_world.clone());
    let schedule = config.validate().expect("configured genesis schedule");
    driver
        .configure_bft_v2_execution(&config)
        .expect("initialize no-registry V2 execution");

    let membership_actions = [
        RuntimeAction::SubmitGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "candidate-v4".into(),
            node_id: "v4".into(),
            finality_signer_public_key: hex::encode(keys[4].verifying_key().to_bytes()),
            stake: 1,
            operator_owner: "test-operator".into(),
            public_manifest_hash: "test-manifest".into(),
        },
        RuntimeAction::ApproveGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "candidate-v4".into(),
        },
        RuntimeAction::ActivateGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "candidate-v4".into(),
            activation_epoch: 1,
        },
        RuntimeAction::RevokeGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "candidate-v4".into(),
            node_id: "v4".into(),
            reason: "no registry".into(),
        },
    ];
    let signed_actions = membership_actions
        .into_iter()
        .map(|action| {
            v2_signed_governance_action(&schedule, &genesis_world, &controller_key, action)
        })
        .collect::<Vec<_>>();

    let unconfigured_root = root.join("preconfigured-ingress");
    std::fs::create_dir_all(&unconfigured_root).expect("create unconfigured ingress root");
    let unconfigured_driver =
        super::driver_bft::driver_for_world(&unconfigured_root, genesis_world.clone());
    unconfigured_driver
        .validate_bft_v2_action_payload(v2_runtime_action(99).payload_cbor.as_slice())
        .expect("ordinary closed runtime action can be admitted before V2 setup");
    for action in &signed_actions {
        assert!(
            unconfigured_driver
                .validate_bft_v2_action_payload(action.payload_cbor.as_slice())
                .is_err(),
            "governance action admission requires a configured verified parent"
        );
    }

    let mut tampered_envelope: ConsensusActionPayloadEnvelope =
        serde_cbor::from_slice(signed_actions[0].payload_cbor.as_slice())
            .expect("decode valid governance payload");
    if let Some(ConsensusActionAuthEnvelope::GovernanceAction(proof)) =
        tampered_envelope.auth.as_mut()
    {
        proof.nonce = proof.nonce.checked_add(1).expect("test nonce increment");
    } else {
        panic!("signed governance payload must contain a governance proof");
    }
    let tampered_payload = encode_consensus_action_payload(&tampered_envelope)
        .expect("encode tampered governance proof");
    let tampered_action = NodeConsensusAction::from_payload(99, "system:test", tampered_payload)
        .expect("build tampered governance action");
    assert!(
        driver
            .validate_bft_v2_action_payload(tampered_action.payload_cbor.as_slice())
            .is_err(),
        "current V2 ingress must authenticate governance signatures and parent"
    );

    let mut wrong_parent_envelope: ConsensusActionPayloadEnvelope =
        serde_cbor::from_slice(signed_actions[0].payload_cbor.as_slice())
            .expect("decode valid governance payload");
    let action = match &wrong_parent_envelope.body {
        ConsensusActionPayloadBody::RuntimeAction { action } => action.clone(),
        _ => panic!("signed governance payload must contain a runtime action"),
    };
    let proof = match wrong_parent_envelope.auth.as_mut() {
        Some(ConsensusActionAuthEnvelope::GovernanceAction(proof)) => proof,
        _ => panic!("signed governance payload must contain a governance proof"),
    };
    proof.parent_position = 1;
    proof.parent_record_id = "34".repeat(32);
    proof.parent_state_root = "35".repeat(32);
    proof.valid_from_position = 2;
    proof.valid_through_position = 2;
    proof.signatures.clear();
    let signature = controller_key.sign(
        &proof
            .signing_bytes(&action)
            .expect("encode different-parent signing bytes"),
    );
    proof.signatures.insert(
        hex::encode(controller_key.verifying_key().to_bytes()),
        hex::encode(signature.to_bytes()),
    );
    let wrong_parent_payload = encode_consensus_action_payload(&wrong_parent_envelope)
        .expect("encode validly signed wrong-parent proof");
    let wrong_parent_action =
        NodeConsensusAction::from_payload(100, "system:test", wrong_parent_payload)
            .expect("build wrong-parent governance action");
    assert!(
        driver
            .validate_bft_v2_action_payload(wrong_parent_action.payload_cbor.as_slice())
            .is_err(),
        "a valid signature for a different governance parent must fail V2 ingress"
    );

    let node_root = root.join("node-ingress");
    std::fs::create_dir_all(&node_root).expect("create actual Node ingress root");
    let mut node_driver = super::driver_bft::driver_for_world(&node_root, genesis_world.clone());
    node_driver
        .configure_bft_v2_execution(&config)
        .expect("configure actual Node execution driver");
    let requester_key = SigningKey::from_bytes(&[101; 32]);
    let provider_key = SigningKey::from_bytes(&[102; 32]);
    let binding = BftSignerBindingV2 {
        schema_version: 2,
        scope: scope.clone(),
        genesis_authority_epoch: config.authority_epoch,
        genesis_validators: config.initial_validators.clone(),
        genesis_state_root: config.genesis_state_root.clone(),
        genesis_manifest_hash: config.genesis_manifest_hash.clone(),
        schedule_policy: config.schedule_policy.clone(),
        local_validator_id: "v0".into(),
        provisioned_signing_keys: vec![BftSignerKeyIdentityV2 {
            validator_id: "v0".into(),
            public_key_hex: hex::encode(keys[0].verifying_key().to_bytes()),
        }],
        timeouts: oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
            propose_ms: 100,
            prevote_ms: 100,
            precommit_ms: 100,
            round_delta_ms: 10,
        },
        operator_instance_id: "driver-node-ingress-instance".into(),
        requester_auth_public_key: hex::encode(requester_key.verifying_key().to_bytes()),
    };
    let trust = BftSignerTrustV2 {
        schema_version: 2,
        provider_public_key: hex::encode(provider_key.verifying_key().to_bytes()),
        binding_digest: binding.digest().expect("digest provider binding"),
    };
    let binding_digest = binding.digest().expect("digest Node signer binding");
    let signer_state = BftSignerStateV2::genesis(&binding).expect("initialize signer genesis");
    let backend = NodeTestSignerBackend {
        binding_digest,
        snapshot: Arc::new(Mutex::new(BftSignerBackendSnapshotV2 {
            state: signer_state,
            lease_generation: 1,
            lease_token_digest: String::new(),
            protected_time_floor_ms: 100,
        })),
    };
    let signer_service = Arc::new(
        BftSignerServiceV2::open(
            binding.clone(),
            Box::new(NodeTestSignerKey(provider_key.clone())),
            vec![Box::new(NodeTestSignerKey(keys[0].clone()))],
            Box::new(backend),
        )
        .expect("open actual TCP signer service"),
    );
    let (signer_address, signer_thread) = spawn_node_test_signer(signer_service);
    let provider = Arc::new(
        RemoteBftSignerProviderV2::new(
            signer_address,
            Duration::from_secs(3),
            binding.clone(),
            requester_key,
            trust,
        )
        .expect("construct remote protected signer provider"),
    );
    let node_config = NodeConfig::new("v0", scope.world_id.clone(), NodeRole::Sequencer)
        .expect("build V2 Node config")
        .with_gossip_optional(SocketAddr::from(([127, 0, 0, 1], 0)), Vec::new())
        .with_replication(
            NodeReplicationConfig::new(node_root.join("replication"))
                .expect("build durable Node replication config"),
        );
    let mut runtime = NodeRuntime::new(node_config)
        .with_replication_network_consensus_enabled(false)
        .with_execution_hook(node_driver)
        .with_tendermint_v2(binding, provider)
        .expect("select BFT V2 with the configured real driver");
    if let Err(start_error) = runtime.start() {
        let signer_result = signer_thread.join();
        panic!("start actual Node and acquire TCP signer: {start_error}; signer={signer_result:?}");
    }
    runtime
        .submit_consensus_action_payload(1, signed_actions[0].payload_cbor.clone())
        .expect("real Node ingress accepts correctly signed current-parent governance proof");
    assert!(
        runtime
            .submit_consensus_action_payload(2, tampered_action.payload_cbor.clone())
            .is_err()
    );
    assert!(
        runtime
            .submit_consensus_action_payload(3, wrong_parent_action.payload_cbor.clone())
            .is_err()
    );
    runtime.stop().expect("stop actual Node runtime cleanly");
    signer_thread
        .join()
        .expect("join actual TCP signer service")
        .expect("serve complete actual TCP signer session");

    for (index, action) in signed_actions.iter().enumerate() {
        let payload_admission =
            driver.validate_bft_v2_action_payload(action.payload_cbor.as_slice());
        let batch_admission =
            driver.validate_bft_v2_action_batch(&schedule, std::slice::from_ref(action));
        if index < 2 {
            payload_admission.expect("Submit/Approve future intent does not change the set");
            batch_admission.expect("Submit/Approve future intent remains admissible");
            if index == 0 {
                let execution = driver
                    .current_bft_v2_execution_context(&schedule)
                    .expect("exact no-registry execution context");
                let candidate = driver
                    .build_bft_v2_candidate(NodeBftBuildContextV2 {
                        execution,
                        actions: vec![action.clone()],
                    })
                    .expect("Submit remains executable without an installed finality registry");
                assert!(
                    candidate.transition.is_none() && candidate.transition_ref.is_none(),
                    "a missing projection cannot fabricate or install a validator set"
                );
            }
        } else {
            assert!(
                payload_admission
                    .expect_err("queued Activate/Revoke requires a governed registry")
                    .contains("require a configured governance finality registry")
            );
            assert!(
                batch_admission
                    .expect_err("signed Activate/Revoke requires a governed registry")
                    .contains("require a configured governance finality registry")
            );
            let execution = driver
                .current_bft_v2_execution_context(&schedule)
                .expect("exact no-registry execution context");
            assert!(
                driver
                    .build_bft_v2_candidate(NodeBftBuildContextV2 {
                        execution,
                        actions: vec![action.clone()],
                    })
                    .expect_err("candidate construction must not discard a set change")
                    .contains("require a configured governance finality registry")
            );
        }
    }

    let execution = driver
        .current_bft_v2_execution_context(&schedule)
        .expect("exact no-registry execution context");
    let ordinary = v2_runtime_action(1);
    let mut candidate = driver
        .build_bft_v2_candidate(NodeBftBuildContextV2 {
            execution: execution.clone(),
            actions: vec![ordinary],
        })
        .expect("ordinary execution remains valid without a governed registry");
    candidate.actions_cbor = serde_cbor::to_vec(&[signed_actions[2].clone()])
        .expect("encode signed Activate block actions");
    candidate.action_root =
        oasis7_node::compute_consensus_action_root(std::slice::from_ref(&signed_actions[2]))
            .expect("compute signed Activate block action root");
    assert!(
        driver
            .preview_bft_v2_candidate(&execution, &candidate)
            .expect_err("independent candidate execution must reject missing set registry")
            .contains("require a configured governance finality registry")
    );

    drop(driver);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn dynamic_set_transition_is_derived_from_candidate_world_and_activates_at_exact_height() {
    let allocated = super::temp_dir("bft-v2-dynamic-set-transition");
    let basename = allocated
        .file_name()
        .expect("test path basename")
        .to_string_lossy();
    let root = std::env::temp_dir()
        .canonicalize()
        .expect("canonical temp root")
        .join(basename.as_ref());
    let keys = (1..=5)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let scope = v2_scope();
    let (config, genesis_world) =
        v2_config_for_world(&scope, &keys, dynamic_genesis_world(&scope, &keys), 3);
    let mut driver = super::driver_bft::driver_for_world(&root, genesis_world);
    let mut schedule = config.validate().expect("configured genesis schedule");
    driver
        .configure_bft_v2_execution(&config)
        .expect("initialize V2 execution");

    let mut results = Vec::new();
    for height in 1..=3 {
        let execution = driver
            .current_bft_v2_execution_context(&schedule)
            .expect("exact height context");
        let actions = if height == 2 {
            Vec::new()
        } else {
            vec![v2_runtime_action(height)]
        };
        if height == 2 {
            driver
                .validate_bft_v2_action_batch(&schedule, &actions)
                .expect("empty action batch is a valid idle BFT tick");
        }
        let candidate = driver
            .build_bft_v2_candidate(NodeBftBuildContextV2 {
                execution: execution.clone(),
                actions,
            })
            .expect("build candidate from current World");
        match height {
            1 => {
                let transition = candidate
                    .transition
                    .as_ref()
                    .expect("candidate projection should schedule future set");
                assert_eq!(transition.scheduled_height, 1);
                assert_eq!(transition.activation_height, 3);
                assert!(candidate.transition_ref.is_none());
            }
            2 => {
                assert!(candidate.transition.is_none());
                assert!(candidate.transition_ref.is_some());
                assert_eq!(schedule.active_set().validators().len(), 4);
            }
            3 => {
                assert!(candidate.transition.is_none());
                assert!(candidate.transition_ref.is_none());
                assert_eq!(schedule.active_set().validators().len(), 5);
            }
            _ => unreachable!(),
        }
        driver
            .preview_bft_v2_candidate(&execution, &candidate)
            .expect("candidate projection independently replays");
        let certificate = v2_certificate(&schedule, candidate, &keys);
        let token = verify_commit_certificate_token_v2(&schedule, &certificate)
            .expect("verify exact height QC");
        schedule = token.next_height_schedule().clone();
        results.push(
            driver
                .commit_bft_v2_certificate(token)
                .expect("commit exact height"),
        );
    }
    assert_eq!(results[2].height, 3);
    assert_eq!(schedule.active_set().validators().len(), 5);

    drop(driver);
    let (_, restored_genesis) =
        v2_config_for_world(&scope, &keys, dynamic_genesis_world(&scope, &keys), 3);
    let mut reopened = super::driver_bft::driver_for_world(&root, restored_genesis);
    assert!(
        reopened
            .configure_bft_v2_execution(&config)
            .expect("reopen dynamic-set V2 ledger")
            .is_none(),
        "dynamic-set head remains unreported until full QC and execution replay"
    );
    let history = reopened
        .load_bft_v2_commit_history_page(&config, 0, 8)
        .expect("load raw QC history");
    let mut replay_schedule = config.validate().expect("genesis replay schedule");
    for certificate in history {
        let token = verify_commit_certificate_token_v2(&replay_schedule, &certificate)
            .expect("replay QC chain including transition activation");
        replay_schedule = token.next_height_schedule().clone();
        reopened
            .commit_bft_v2_certificate(token)
            .expect("replay QC into exact Runtime World");
    }
    assert_eq!(replay_schedule.active_set().validators().len(), 5);

    drop(reopened);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn early_validator_activation_waits_for_its_exact_boundary_and_pending_set_is_immutable() {
    let allocated = super::temp_dir("bft-v2-early-validator-activation");
    let basename = allocated
        .file_name()
        .expect("test path basename")
        .to_string_lossy();
    let root = std::env::temp_dir()
        .canonicalize()
        .expect("canonical temp root")
        .join(basename.as_ref());
    let keys = (1..=5)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let controller_key = SigningKey::from_bytes(&[72; 32]);
    let scope = v2_scope();
    let mut genesis_world = dynamic_genesis_world(&scope, &keys);
    genesis_world
        .set_governance_main_token_controller_registry(
            oasis7_runtime::GovernanceMainTokenControllerRegistry {
                genesis_controller_account_id: "controller".into(),
                controller_signer_policies: BTreeMap::from([(
                    "controller".into(),
                    oasis7_runtime::GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: [hex::encode(
                            controller_key.verifying_key().to_bytes(),
                        )]
                        .into(),
                    },
                )]),
                ..Default::default()
            },
        )
        .expect("install controller test authority");
    let (config, genesis_world) = v2_config_for_world(&scope, &keys, genesis_world, 3);
    let mut driver = super::driver_bft::driver_for_world(&root, genesis_world.clone());
    let mut schedule = config.validate().expect("configured genesis schedule");
    driver
        .configure_bft_v2_execution(&config)
        .expect("initialize V2 execution");

    let activation = RuntimeAction::ActivateGovernanceValidatorAdmission {
        controller_account_id: "controller".into(),
        candidate_id: "candidate-v4".into(),
        activation_epoch: 2,
    };
    let early_activation =
        v2_signed_governance_action(&schedule, &genesis_world, &controller_key, activation);
    driver
        .validate_bft_v2_action_batch(&schedule, std::slice::from_ref(&early_activation))
        .expect("early signed activation is admitted before its scheduling boundary");

    for height in 1..=6 {
        let execution = driver
            .current_bft_v2_execution_context(&schedule)
            .expect("exact height context");
        if height == 2 {
            let late = v2_signed_governance_action(
                &schedule,
                &driver.execution_world,
                &controller_key,
                RuntimeAction::ActivateGovernanceValidatorAdmission {
                    controller_account_id: "controller".into(),
                    candidate_id: "candidate-v4".into(),
                    activation_epoch: 1,
                },
            );
            let error = driver
                .validate_bft_v2_action_batch(&schedule, &[late])
                .expect_err("late activation cannot be silently rescheduled");
            assert!(
                error.contains("after its exact scheduling height"),
                "unexpected late activation error: {error}"
            );
        }
        let actions = if height == 1 {
            vec![early_activation.clone()]
        } else {
            vec![v2_runtime_action(height)]
        };
        let candidate = driver
            .build_bft_v2_candidate(NodeBftBuildContextV2 {
                execution: execution.clone(),
                actions: actions.clone(),
            })
            .expect("build candidate from current World");
        match height {
            1..=3 => assert!(
                candidate.transition.is_none() && candidate.transition_ref.is_none(),
                "early activation must not change the validator set before epoch 2"
            ),
            4 => {
                let transition = candidate
                    .transition
                    .as_ref()
                    .expect("height 4 schedules activation at height 6");
                assert_eq!(transition.scheduled_height, 4);
                assert_eq!(transition.activation_height, 6);
                assert_eq!(transition.target_epoch, 2);
            }
            5 => {
                assert!(candidate.transition.is_none());
                assert!(candidate.transition_ref.is_some());
                assert_eq!(schedule.active_set().validators().len(), 4);
                let conflicting = v2_signed_governance_action(
                    &schedule,
                    &driver.execution_world,
                    &controller_key,
                    RuntimeAction::ActivateGovernanceValidatorAdmission {
                        controller_account_id: "controller".into(),
                        candidate_id: "candidate-v4".into(),
                        activation_epoch: 3,
                    },
                );
                assert!(
                    driver
                        .validate_bft_v2_action_batch(&schedule, &[conflicting])
                        .expect_err("pending transition forbids overlapping set change")
                        .contains("pending")
                );
                let submit = v2_signed_governance_action(
                    &schedule,
                    &driver.execution_world,
                    &controller_key,
                    RuntimeAction::SubmitGovernanceValidatorAdmission {
                        controller_account_id: "controller".into(),
                        candidate_id: "candidate-v5".into(),
                        node_id: "v5".into(),
                        finality_signer_public_key: hex::encode(keys[4].verifying_key().to_bytes()),
                        stake: 1,
                        operator_owner: "test-operator".into(),
                        public_manifest_hash: "test-manifest".into(),
                    },
                );
                driver
                    .validate_bft_v2_action_batch(&schedule, &[submit])
                    .expect("pending transition still allows non-membership-changing submit");
            }
            6 => {
                assert!(candidate.transition.is_none());
                assert!(candidate.transition_ref.is_none());
                assert_eq!(schedule.active_set().validators().len(), 5);
            }
            _ => unreachable!(),
        }
        driver
            .preview_bft_v2_candidate(&execution, &candidate)
            .expect("candidate projection independently replays");
        let certificate = v2_certificate(&schedule, candidate, &keys);
        let token = verify_commit_certificate_token_v2(&schedule, &certificate)
            .expect("verify exact height QC");
        schedule = token.next_height_schedule().clone();
        driver
            .commit_bft_v2_certificate(token)
            .expect("commit exact height");
    }
    assert_eq!(schedule.active_set().validators().len(), 5);

    drop(driver);
    let _ = std::fs::remove_dir_all(root);
}
