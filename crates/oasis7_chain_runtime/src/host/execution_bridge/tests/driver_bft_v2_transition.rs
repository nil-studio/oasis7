//! Real governance execution, protected transition ACK, and Node restart.
//!
//! The signer backend is the existing in-memory test double. Nodes, TCP
//! provider sessions, V2 QC verification, and the execution driver are real.

use super::*;
use oasis7_consensus::tendermint_v2::BftHeightProposerScheduleV2;
use oasis7_distfs::protected_bft_v2::BftSignerProviderV2;
use oasis7_runtime::{
    GovernanceFinalitySignerRegistry, GovernanceMainTokenControllerRegistry,
    GovernanceThresholdSignerPolicy, GovernanceValidatorAdmissionStatus,
};
use std::net::UdpSocket;

const VALIDATOR_COUNT: usize = 5;
const INITIAL_VALIDATOR_COUNT: usize = 4;
const TRANSITION_SOURCE_HEIGHT: u64 = 4;
const TRANSITION_CONFIRMATION_HEIGHT: u64 = 5;
const ACTIVATION_HEIGHT: u64 = 6;

struct SignerIdentity {
    binding: BftSignerBindingV2,
    validator_key: SigningKey,
    requester_key: SigningKey,
    provider_key: SigningKey,
    snapshot: Arc<Mutex<BftSignerBackendSnapshotV2>>,
}

struct RunningNode {
    runtime: NodeRuntime,
    signer_server: Option<thread::JoinHandle<Result<(), String>>>,
}

fn governance_genesis(
    scope: &BftScopeV2,
    keys: &[SigningKey],
) -> (oasis7_runtime::World, SigningKey) {
    let controller_key = SigningKey::from_bytes(&[73; 32]);
    let base = super::super::driver_bft::bft_genesis_world(&oasis7_node::BftScopeV1 {
        world_id: scope.world_id.clone(),
        chain_id: scope.chain_id.clone(),
        genesis_hash: scope.genesis_hash.clone(),
    });
    let mut snapshot = base.snapshot();
    snapshot.governance_execution_policy.epoch_length_ticks = 3;
    snapshot.state.governance_finality_signer_registry = Some(GovernanceFinalitySignerRegistry {
        slot_id: "governance.finality.v1".into(),
        threshold: 3,
        signer_bindings: (0..INITIAL_VALIDATOR_COUNT)
            .map(|index| {
                (
                    format!("v{index}"),
                    hex::encode(keys[index].verifying_key().to_bytes()),
                )
            })
            .collect(),
        validator_stakes: (0..INITIAL_VALIDATOR_COUNT)
            .map(|index| (format!("v{index}"), 1))
            .collect(),
        ..Default::default()
    });
    let mut world = oasis7_runtime::World::from_snapshot_with_release_security_policy(
        snapshot,
        base.journal().clone(),
        base.release_security_policy().clone(),
    )
    .expect("restore governance-enabled genesis World");
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "controller".into(),
            controller_signer_policies: BTreeMap::from([(
                "controller".into(),
                GovernanceThresholdSignerPolicy {
                    threshold: 1,
                    allowed_public_keys: [hex::encode(controller_key.verifying_key().to_bytes())]
                        .into(),
                },
            )]),
            ..Default::default()
        })
        .expect("install the actual governance action authority");
    (world, controller_key)
}

fn stage_signed_transition_history(
    root: &std::path::Path,
    config: &NodeBftExecutionConfigV2,
    genesis_world: oasis7_runtime::World,
    keys: &[SigningKey],
    controller_key: &SigningKey,
) -> (Vec<BftCommitCertificateV2>, BftHeightProposerScheduleV2) {
    let mut driver = super::super::driver_bft::driver_for_world(root, genesis_world);
    driver
        .configure_bft_v2_execution(config)
        .expect("configure the real V2 execution driver");
    let mut schedule = config.validate().expect("validate V2 genesis schedule");
    let mut history = Vec::new();

    for height in 1..=TRANSITION_CONFIRMATION_HEIGHT {
        let actions = match height {
            1 => vec![super::v2_signed_governance_action(
                &schedule,
                &driver.execution_world,
                controller_key,
                RuntimeAction::SubmitGovernanceValidatorAdmission {
                    controller_account_id: "controller".into(),
                    candidate_id: "candidate-v4".into(),
                    node_id: "v4".into(),
                    finality_signer_public_key: hex::encode(keys[4].verifying_key().to_bytes()),
                    stake: 1,
                    operator_owner: "test-operator".into(),
                    public_manifest_hash: "test-manifest".into(),
                },
            )],
            2 => vec![super::v2_signed_governance_action(
                &schedule,
                &driver.execution_world,
                controller_key,
                RuntimeAction::ApproveGovernanceValidatorAdmission {
                    controller_account_id: "controller".into(),
                    candidate_id: "candidate-v4".into(),
                },
            )],
            3 => vec![super::v2_signed_governance_action(
                &schedule,
                &driver.execution_world,
                controller_key,
                RuntimeAction::ActivateGovernanceValidatorAdmission {
                    controller_account_id: "controller".into(),
                    candidate_id: "candidate-v4".into(),
                    activation_epoch: 2,
                },
            )],
            _ => vec![super::v2_runtime_action(height)],
        };
        let execution = driver
            .current_bft_v2_execution_context(&schedule)
            .expect("derive current driver execution context");
        let candidate = driver
            .build_bft_v2_candidate(NodeBftBuildContextV2 { execution, actions })
            .expect("execute the actual signed governance/runtime action");
        if height == TRANSITION_SOURCE_HEIGHT {
            let transition = candidate
                .transition
                .as_ref()
                .expect("activation projection schedules the successor set");
            assert_eq!(transition.scheduled_height, TRANSITION_SOURCE_HEIGHT);
            assert_eq!(transition.activation_height, ACTIVATION_HEIGHT);
            assert_eq!(transition.new_validators.len(), VALIDATOR_COUNT);
        }
        if height == TRANSITION_CONFIRMATION_HEIGHT {
            let reference = candidate
                .transition_ref
                .as_ref()
                .expect("h+1 block confirms the exact transition source");
            assert_eq!(reference.scheduled_height, TRANSITION_SOURCE_HEIGHT);
            assert_eq!(reference.activation_height, ACTIVATION_HEIGHT);
        }
        let certificate = super::v2_certificate(&schedule, candidate, keys);
        let token = verify_commit_certificate_token_v2(&schedule, &certificate)
            .expect("verify the exact V2 QC before driver commit");
        schedule = token.next_height_schedule().clone();
        driver
            .commit_bft_v2_certificate(token)
            .expect("commit verified QC through the real World driver");
        history.push(certificate);
    }

    let final_world = driver.execution_world.snapshot();
    let admission = final_world
        .state
        .governance_validator_admissions
        .get("candidate-v4")
        .expect("signed Submit/Approve/Activate actions reached World state");
    assert_eq!(
        admission.status,
        GovernanceValidatorAdmissionStatus::ProbationReady
    );
    assert_eq!(schedule.height(), ACTIVATION_HEIGHT);
    assert_eq!(schedule.active_set().validators().len(), VALIDATOR_COUNT);
    drop(driver);
    (history, schedule)
}

fn signer_identities(
    config: &NodeBftExecutionConfigV2,
    keys: &[SigningKey],
) -> Vec<SignerIdentity> {
    keys.iter()
        .enumerate()
        .map(|(index, validator_key)| {
            let requester_key = SigningKey::from_bytes(&[101 + index as u8; 32]);
            let provider_key = SigningKey::from_bytes(&[121 + index as u8; 32]);
            let binding = BftSignerBindingV2 {
                schema_version: 2,
                scope: config.scope.clone(),
                genesis_authority_epoch: config.authority_epoch,
                genesis_validators: config.initial_validators.clone(),
                genesis_state_root: config.genesis_state_root.clone(),
                genesis_manifest_hash: config.genesis_manifest_hash.clone(),
                schedule_policy: config.schedule_policy.clone(),
                local_validator_id: format!("v{index}"),
                provisioned_signing_keys: vec![BftSignerKeyIdentityV2 {
                    validator_id: format!("v{index}"),
                    public_key_hex: hex::encode(validator_key.verifying_key().to_bytes()),
                }],
                timeouts: oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
                    propose_ms: 100,
                    prevote_ms: 100,
                    precommit_ms: 100,
                    round_delta_ms: 20,
                },
                operator_instance_id: format!("driver-bft-v2-transition-v{index}"),
                requester_auth_public_key: hex::encode(requester_key.verifying_key().to_bytes()),
            };
            let state = BftSignerStateV2::genesis(&binding)
                .expect("create protected signer genesis, including inactive v4");
            SignerIdentity {
                binding,
                validator_key: validator_key.clone(),
                requester_key,
                provider_key,
                snapshot: Arc::new(Mutex::new(BftSignerBackendSnapshotV2 {
                    state,
                    lease_generation: 1,
                    lease_token_digest: String::new(),
                    protected_time_floor_ms: 100,
                })),
            }
        })
        .collect()
}

fn signer_service(identity: &SignerIdentity) -> Arc<BftSignerServiceV2> {
    Arc::new(
        BftSignerServiceV2::open(
            identity.binding.clone(),
            Box::new(super::NodeTestSignerKey(identity.provider_key.clone())),
            vec![Box::new(super::NodeTestSignerKey(
                identity.validator_key.clone(),
            ))],
            Box::new(super::NodeTestSignerBackend {
                binding_digest: identity
                    .binding
                    .digest()
                    .expect("compute exact signer binding digest"),
                snapshot: Arc::clone(&identity.snapshot),
            }),
        )
        .expect("open protected V2 service over durable test snapshot"),
    )
}

fn seed_signer_history(identity: &SignerIdentity, history: &[BftCommitCertificateV2]) {
    let service = signer_service(identity);
    let (address, server) = super::spawn_node_test_signer(service);
    let trust = BftSignerTrustV2 {
        schema_version: 2,
        provider_public_key: hex::encode(identity.provider_key.verifying_key().to_bytes()),
        binding_digest: identity
            .binding
            .digest()
            .expect("compute exact signer binding digest"),
    };
    let provider = RemoteBftSignerProviderV2::new(
        address,
        Duration::from_secs(3),
        identity.binding.clone(),
        identity.requester_key.clone(),
        trust,
    )
    .expect("construct remote TCP signer for history seed");
    let mut lease = provider
        .acquire(&identity.binding)
        .expect("acquire real remote signer lease for QC history");
    for certificate in history {
        let mut checkpoint = lease.checkpoint().expect("read current signer checkpoint");
        if let Some(journal) = checkpoint.state.journal.as_ref() {
            if journal.height_started_at_ms().is_none() {
                checkpoint = lease
                    .start_height_timer(checkpoint.state.revision)
                    .expect("durably start the exact active height timer");
            }
            let current_journal = checkpoint
                .state
                .journal
                .as_ref()
                .expect("active local validator retains a journal");
            assert!(current_journal.pending_commit().is_none());
            let mut candidate = checkpoint.state.candidate();
            candidate
                .journal
                .as_mut()
                .expect("active signer journal candidate")
                .pending_commit = Some(certificate.clone());
            checkpoint = lease
                .advance(
                    checkpoint.state.revision,
                    &checkpoint.state.state_digest,
                    candidate,
                )
                .expect("reserve the exact QC in the protected journal");
        }
        checkpoint = lease
            .append_committed_certificate(
                checkpoint.state.revision,
                &checkpoint.state.state_digest,
                certificate.clone(),
            )
            .expect("append verified QC through the remote protected provider");
    }
    drop(lease);
    server
        .join()
        .expect("join history seeding provider thread")
        .expect("release seeding provider lease cleanly");
    let seeded = identity.snapshot.lock().expect("inspect seeded provider");
    assert_eq!(seeded.state.history, history);
    assert!(seeded.lease_token_digest.is_empty());
}

fn reserve_udp_addresses(count: usize) -> Vec<SocketAddr> {
    let reservations = (0..count)
        .map(|_| UdpSocket::bind("127.0.0.1:0").expect("reserve loopback BFT gossip endpoint"))
        .collect::<Vec<_>>();
    let addresses = reservations
        .iter()
        .map(|socket| socket.local_addr().expect("read reserved gossip address"))
        .collect();
    drop(reservations);
    addresses
}

fn start_node(
    index: usize,
    identity: &SignerIdentity,
    config: &NodeBftExecutionConfigV2,
    genesis_world: &oasis7_runtime::World,
    addresses: &[SocketAddr],
    root: &std::path::Path,
) -> RunningNode {
    let service = signer_service(identity);
    let (signer_address, signer_server) = super::spawn_node_test_signer(service);
    let provider = Arc::new(
        RemoteBftSignerProviderV2::new(
            signer_address,
            Duration::from_secs(3),
            identity.binding.clone(),
            identity.requester_key.clone(),
            BftSignerTrustV2 {
                schema_version: 2,
                provider_public_key: hex::encode(identity.provider_key.verifying_key().to_bytes()),
                binding_digest: identity
                    .binding
                    .digest()
                    .expect("compute exact signer binding digest"),
            },
        )
        .expect("construct the real remote protected signer provider"),
    );
    let driver_root = root.join(format!("node-{index}/driver"));
    let driver = super::super::driver_bft::driver_for_world(&driver_root, genesis_world.clone());
    let peers = addresses
        .iter()
        .enumerate()
        .filter_map(|(peer_index, address)| (peer_index != index).then_some(*address))
        .collect();
    let node_config = NodeConfig::new(
        identity.binding.local_validator_id.clone(),
        config.scope.world_id.clone(),
        NodeRole::Sequencer,
    )
    .expect("create real BFT validator Node")
    .with_tick_interval(Duration::from_millis(20))
    .expect("set BFT worker cadence")
    .with_replication(
        NodeReplicationConfig::new(root.join(format!("node-{index}/replication")))
            .expect("create durable Node replication settings"),
    )
    .with_gossip_optional(addresses[index], peers);
    let mut runtime = NodeRuntime::new(node_config)
        .with_execution_hook(driver)
        .with_replication_network_consensus_enabled(false)
        .with_tendermint_v2(identity.binding.clone(), provider)
        .expect("select V2 with the real World execution driver");
    if let Err(error) = runtime.start() {
        drop(runtime);
        let service_result = signer_server.join();
        panic!(
            "start real V2 Node {} with protected signer: {error}; signer={service_result:?}",
            identity.binding.local_validator_id
        );
    }
    RunningNode {
        runtime,
        signer_server: Some(signer_server),
    }
}

fn wait_all_at_height(nodes: &[RunningNode], target: u64, context: &str) {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let statuses = nodes
            .iter()
            .map(|node| {
                node.runtime
                    .bft_v2_status()
                    .expect("selected V2 runtime status")
            })
            .collect::<Vec<_>>();
        if statuses.iter().any(|status| !status.running) {
            panic!("a real BFT Node stopped during {context}: {statuses:?}");
        }
        if statuses
            .iter()
            .all(|status| status.committed_height >= target)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "nodes did not reach height {target} during {context}: {statuses:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn stop_nodes(nodes: &mut [RunningNode]) {
    for node in nodes.iter_mut() {
        node.runtime.stop().expect("stop real BFT Node");
    }
    for node in nodes.iter_mut() {
        node.signer_server
            .take()
            .expect("each Node owns a TCP signer session")
            .join()
            .expect("join TCP signer service thread")
            .expect("release Node signer lease cleanly");
    }
}

#[test]
fn real_driver_governance_transition_installs_over_tcp_and_nodes_restart_forward() {
    let allocated = super::super::temp_dir("bft-v2-governance-transition-node-runtime");
    let root = std::env::temp_dir()
        .canonicalize()
        .expect("canonical temporary directory")
        .join(
            allocated
                .file_name()
                .expect("generated test path has a basename"),
        );
    std::fs::create_dir_all(&root).expect("create isolated transition test root");
    let keys = (1..=VALIDATOR_COUNT)
        .map(|seed| {
            let seed = u8::try_from(seed).expect("validator seed fits Ed25519 test seed byte");
            SigningKey::from_bytes(&[seed; 32])
        })
        .collect::<Vec<_>>();
    let scope = super::v2_scope();
    let (genesis_world, controller_key) = governance_genesis(&scope, &keys);
    let (config, genesis_world) = super::v2_config_for_world(&scope, &keys, genesis_world, 3);
    let (history, activation_schedule) = stage_signed_transition_history(
        root.join("staging-driver").as_path(),
        &config,
        genesis_world.clone(),
        &keys,
        &controller_key,
    );
    assert_eq!(history.len() as u64, TRANSITION_CONFIRMATION_HEIGHT);
    assert_eq!(activation_schedule.height(), ACTIVATION_HEIGHT);
    assert_eq!(
        activation_schedule.active_set().validators().len(),
        VALIDATOR_COUNT
    );
    let identities = signer_identities(&config, &keys);
    for identity in &identities {
        seed_signer_history(identity, &history);
        let state = identity
            .snapshot
            .lock()
            .expect("inspect initial signer state");
        assert_eq!(
            state.state.history.len(),
            TRANSITION_CONFIRMATION_HEIGHT as usize
        );
        assert!(state.state.install_receipts.is_empty());
    }

    let addresses = reserve_udp_addresses(VALIDATOR_COUNT);
    let mut nodes = identities
        .iter()
        .enumerate()
        .map(|(index, identity)| {
            start_node(index, identity, &config, &genesis_world, &addresses, &root)
        })
        .collect::<Vec<_>>();

    wait_all_at_height(&nodes, ACTIVATION_HEIGHT, "first height under the new set");
    for (index, identity) in identities.iter().enumerate() {
        let status = nodes[index]
            .runtime
            .bft_v2_status()
            .expect("read transition Node status");
        assert_eq!(
            status.active_validator_set_hash,
            activation_schedule.active_set().set_hash()
        );
        let snapshot = identity.snapshot.lock().expect("inspect install ACK state");
        assert_eq!(snapshot.state.install_receipts.len(), 1);
        let receipt = &snapshot.state.install_receipts[0].body;
        assert_eq!(receipt.source_height, TRANSITION_SOURCE_HEIGHT);
        assert_eq!(receipt.confirmation_height, TRANSITION_CONFIRMATION_HEIGHT);
        assert_eq!(receipt.activation_height, ACTIVATION_HEIGHT);
        assert!(
            receipt.installed_head_height >= TRANSITION_CONFIRMATION_HEIGHT,
            "ACK must be anchored at or after the h+1 confirmation"
        );
        let installed_head = usize::try_from(receipt.installed_head_height - 1)
            .expect("installed head height fits an index");
        let installed_certificate = snapshot
            .state
            .history
            .get(installed_head)
            .expect("ACK head is present in the complete verified signer history");
        assert_eq!(
            installed_certificate
                .proposal
                .block
                .block_id()
                .expect("installed history block id"),
            receipt.installed_head_commit_ref,
            "ACK identifies the exact committed ancestor at its installed head"
        );
    }

    wait_all_at_height(&nodes, ACTIVATION_HEIGHT + 1, "post-activation block");
    let pre_restart_history = nodes[0]
        .runtime
        .raw_bft_v2_commit_history_page(0, 64)
        .expect("read original Node QC history");
    assert!(pre_restart_history.len() as u64 > ACTIVATION_HEIGHT);
    stop_nodes(&mut nodes);
    let highest_durable_height = identities
        .iter()
        .map(|identity| {
            identity
                .snapshot
                .lock()
                .expect("inspect stopped signer state")
                .state
                .history
                .len() as u64
        })
        .max()
        .expect("five durable signer histories");
    assert!(highest_durable_height > ACTIVATION_HEIGHT);
    // stop() shuts down the worker and TCP session; dropping the runtime also
    // releases the driver's exclusive ledger lock before reopening that path.
    drop(nodes);

    let mut restarted = identities
        .iter()
        .enumerate()
        .map(|(index, identity)| {
            start_node(index, identity, &config, &genesis_world, &addresses, &root)
        })
        .collect::<Vec<_>>();
    wait_all_at_height(
        &restarted,
        highest_durable_height + 1,
        "cold Node restart and subsequent BFT height",
    );
    for (index, node) in restarted.iter().enumerate() {
        let status = node
            .runtime
            .bft_v2_status()
            .expect("read restarted Node status");
        assert_eq!(
            status.active_validator_set_hash,
            activation_schedule.active_set().set_hash()
        );
        assert!(status.committed_height > highest_durable_height);
        let state = identities[index]
            .snapshot
            .lock()
            .expect("inspect restarted signer state");
        assert_eq!(state.state.install_receipts.len(), 1);
        assert!(
            state.state.history.len() as u64 >= status.committed_height,
            "protected history cannot lag the Node's reported committed height"
        );
    }
    stop_nodes(&mut restarted);
    drop(restarted);
    let subset_baseline_height = identities
        .iter()
        .map(|identity| {
            identity
                .snapshot
                .lock()
                .expect("inspect all durable histories before reducing membership")
                .state
                .history
                .len() as u64
        })
        .max()
        .expect("five durable signer histories after restart");

    // Keep the most advanced history holder online while removing the oldest
    // old-set member. With the five-member successor set, exactly four nodes
    // remain (new validator v4 plus three old validators), so a new QC above
    // every durable height requires v4's own precommit.
    let retired_old_index = (0..INITIAL_VALIDATOR_COUNT)
        .min_by_key(|index| {
            identities[*index]
                .snapshot
                .lock()
                .expect("inspect old validator history before subset restart")
                .state
                .history
                .len()
        })
        .expect("at least one old validator to retire");
    let subset_indices = (0..VALIDATOR_COUNT)
        .filter(|index| *index != retired_old_index)
        .collect::<Vec<_>>();
    assert_eq!(subset_indices.len(), INITIAL_VALIDATOR_COUNT);
    assert!(subset_indices.contains(&(VALIDATOR_COUNT - 1)));
    let mut subset_nodes = subset_indices
        .iter()
        .map(|index| {
            start_node(
                *index,
                &identities[*index],
                &config,
                &genesis_world,
                &addresses,
                &root,
            )
        })
        .collect::<Vec<_>>();
    let new_validator_height = subset_baseline_height + 1;
    wait_all_at_height(
        &subset_nodes,
        new_validator_height,
        "new validator participation with one old validator offline",
    );
    stop_nodes(&mut subset_nodes);
    drop(subset_nodes);

    for index in &subset_indices {
        let snapshot = identities[*index]
            .snapshot
            .lock()
            .expect("inspect signer after four-node continuation");
        assert!(
            snapshot.state.history.len() as u64 >= new_validator_height,
            "every online validator retains the new-height QC"
        );
    }
    let v4_snapshot = identities[VALIDATOR_COUNT - 1]
        .snapshot
        .lock()
        .expect("inspect new validator's durable history");
    let v4_certificate = v4_snapshot
        .state
        .history
        .get(usize::try_from(new_validator_height - 1).expect("QC height index"))
        .cloned()
        .expect("new validator retained the target-height QC");
    assert!(
        v4_certificate.precommits.iter().any(|vote| {
            vote.validator_id == "v4"
                && vote.height == new_validator_height
                && vote.phase == BftVotePhaseV2::Precommit
                && matches!(&vote.value, BftVoteValueV2::Block { .. })
        }),
        "the new validator must sign a block precommit above all prior durable heights"
    );

    let mut replay_driver = super::super::driver_bft::driver_for_world(
        root.join("independent-world-replay").as_path(),
        genesis_world,
    );
    replay_driver
        .configure_bft_v2_execution(&config)
        .expect("configure independent full-world replay");
    let mut replay_schedule = config.validate().expect("replay genesis schedule");
    let complete_history = identities[subset_indices[0]]
        .snapshot
        .lock()
        .expect("read final signer history")
        .state
        .history
        .clone();
    for certificate in complete_history {
        let token = verify_commit_certificate_token_v2(&replay_schedule, &certificate)
            .expect("verify all staged and live Node QCs from pinned genesis");
        replay_schedule = token.next_height_schedule().clone();
        replay_driver
            .commit_bft_v2_certificate(token)
            .expect("re-execute all governance and Node blocks in real World");
    }
    let final_state = replay_driver.execution_world.snapshot().state;
    let admission = final_state
        .governance_validator_admissions
        .get("candidate-v4")
        .expect("governance admission remains in canonical World");
    assert_eq!(
        admission.status,
        GovernanceValidatorAdmissionStatus::ProbationReady,
        "the signed record retains its raw probation state after activation"
    );
    // Raw admission status is historical; effective registry resolution applies
    // the activation epoch to the current World and is the actual membership oracle.
    let final_registry = replay_driver
        .execution_world
        .resolve_governance_effective_finality_signer_registry()
        .expect("resolve effective post-activation registry")
        .expect("effective governance registry");
    assert_eq!(
        final_registry.signer_bindings.get("v4"),
        Some(&hex::encode(keys[4].verifying_key().to_bytes()))
    );
    let active_set = replay_schedule.active_set();
    assert_eq!(active_set.validators().len(), VALIDATOR_COUNT);
    assert_eq!(
        active_set
            .validator("v4")
            .expect("the committed successor set contains v4")
            .public_key_hex,
        hex::encode(keys[4].verifying_key().to_bytes())
    );

    drop(replay_driver);
    let _ = std::fs::remove_dir_all(root);
}
