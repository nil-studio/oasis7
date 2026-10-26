use std::fs;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v1::BftTimeoutsV1;
use oasis7_consensus::tendermint_v2::{
    BftHeightProposerScheduleV2, BftSchedulePolicyV2, BftValidatorSetV2,
    VerifiedBftCommitCertificateV2, verify_commit_certificate_token_v2,
};
use oasis7_distfs::protected_bft_v2::{
    BftSignerBindingV2, BftSignerKeyIdentityV2, BftSignerStateV2,
};
use oasis7_proto::distributed_bft::v2::{
    BFT_PROTOCOL_VERSION_V2, BftBlockV2, BftCommitCertificateV2, BftScopeV2,
    BftValidatorTransitionV2, BftValidatorV2, BftVotePhaseV2, BftVoteValueV2,
};

use crate::node_bft_v2_test_provider::ProtectedBftV2TestProvider;
use crate::{
    NodeBftBuildContextV2, NodeBftExecutionCommitResultV2, NodeBftExecutionConfigV2,
    NodeBftExecutionContextV2, NodeConfig, NodeConsensusAction, NodeExecutionCommitContext,
    NodeExecutionCommitResult, NodeExecutionHook, NodeReplicationConfig, NodeRole, NodeRuntime,
    compute_consensus_action_root,
};

const WORLD_ID: &str = "node-v2-worker-transition-world";
const CHAIN_ID: &str = "node-v2-worker-transition-chain";
const PROFILE_ID: &str = "node-v2-worker-transition-profile";
const EPOCH_LENGTH: u64 = 2;
const TARGET_EPOCH: u64 = 3;
const SCHEDULED_HEIGHT: u64 = 4;
const ACTIVATION_HEIGHT: u64 = 6;
const TEST_TIMEOUTS: BftTimeoutsV1 = BftTimeoutsV1 {
    propose_ms: 1_000,
    prevote_ms: 1_000,
    precommit_ms: 1_000,
    round_delta_ms: 200,
};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct PrivateRoot(PathBuf);

impl PrivateRoot {
    fn new() -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-node-v2-transition-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create BFT V2 test root");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .expect("protect BFT V2 test root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PrivateRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct DriverLedger {
    config: Option<NodeBftExecutionConfigV2>,
    certificates: Vec<BftCommitCertificateV2>,
    replay_cursor: usize,
}

#[derive(Clone)]
struct TransitionTestHook {
    ledger: Arc<Mutex<DriverLedger>>,
    new_validators: Vec<BftValidatorV2>,
}

impl TransitionTestHook {
    fn schedule(&self) -> Result<BftHeightProposerScheduleV2, String> {
        let ledger = self
            .ledger
            .lock()
            .map_err(|_| "test driver ledger mutex poisoned".to_string())?;
        let config = ledger
            .config
            .as_ref()
            .ok_or_else(|| "test driver was not configured".to_string())?;
        let set = BftValidatorSetV2::new(config.scope.clone(), config.initial_validators.clone())?;
        let mut schedule = BftHeightProposerScheduleV2::genesis(
            set,
            config.schedule_policy.clone(),
            config.authority_epoch,
            config.genesis_state_root.clone(),
            config.genesis_manifest_hash.clone(),
        )?;
        for certificate in ledger.certificates.iter().take(ledger.replay_cursor) {
            let token = verify_commit_certificate_token_v2(&schedule, certificate)?;
            schedule = token.next_height_schedule().clone();
        }
        Ok(schedule)
    }

    fn make_block(
        &self,
        schedule: &BftHeightProposerScheduleV2,
        actions: &[NodeConsensusAction],
    ) -> Result<BftBlockV2, String> {
        let actions_cbor = serde_cbor::to_vec(&actions)
            .map_err(|error| format!("encode deterministic test actions: {error}"))?;
        let action_root = compute_consensus_action_root(actions)
            .map_err(|error| format!("compute deterministic test action root: {error}"))?;
        let transition = if schedule.height() == SCHEDULED_HEIGHT {
            let new_set =
                BftValidatorSetV2::new(schedule.scope().clone(), self.new_validators.clone())?;
            Some(BftValidatorTransitionV2 {
                source_commit_ref: schedule.parent_commit_ref().to_string(),
                source_state_root: schedule.parent_state_root().to_string(),
                source_profile: schedule.policy().profile_id.clone(),
                source_epoch: schedule.parent_tick() / EPOCH_LENGTH,
                target_epoch: TARGET_EPOCH,
                epoch_length_ticks: EPOCH_LENGTH,
                mapping_ruleset_hash: schedule.policy_hash().to_string(),
                scheduled_height: SCHEDULED_HEIGHT,
                activation_height: ACTIVATION_HEIGHT,
                old_validator_set_hash: schedule.active_set().set_hash().to_string(),
                new_validators: self.new_validators.clone(),
                new_validator_set_hash: new_set.set_hash().to_string(),
                governance_action_digest: digest("test-governance", &actions_cbor),
                projected_registry_digest: digest("test-projected-registry", &actions_cbor),
            })
        } else {
            None
        };
        let transition_ref = schedule.pending_transition().cloned();
        let roots_payload = serde_cbor::to_vec(&(
            schedule.height(),
            schedule.parent_commit_ref(),
            schedule.parent_state_root(),
            &actions_cbor,
            &transition,
            &transition_ref,
        ))
        .map_err(|error| format!("encode deterministic test roots: {error}"))?;
        let manifest_hash = schedule.parent_manifest_hash().to_string();
        Ok(BftBlockV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope: schedule.scope().clone(),
            height: schedule.height(),
            parent_commit_ref: schedule.parent_commit_ref().to_string(),
            authority_epoch: schedule.authority_epoch(),
            active_validator_set_hash: schedule.active_set().set_hash().to_string(),
            schedule_policy_hash: schedule.policy_hash().to_string(),
            manifest_hash: manifest_hash.clone(),
            post_manifest_hash: manifest_hash,
            action_root,
            event_root: digest("test-event-root", &roots_payload),
            execution_block_hash: digest("test-execution-block", &roots_payload),
            execution_state_root: digest("test-execution-state", &roots_payload),
            state_root: digest("test-state-root", &roots_payload),
            actions_cbor,
            transition,
            transition_ref,
        })
    }

    fn result(
        token: &VerifiedBftCommitCertificateV2,
    ) -> Result<NodeBftExecutionCommitResultV2, String> {
        let block = token.block();
        let certificate = serde_cbor::to_vec(token.certificate())
            .map_err(|error| format!("encode test certificate receipt: {error}"))?;
        Ok(NodeBftExecutionCommitResultV2 {
            height: block.height,
            bft_commit_ref: token.block_id().to_string(),
            execution_block_hash: block.execution_block_hash.clone(),
            execution_state_root: block.execution_state_root.clone(),
            state_root: block.state_root.clone(),
            post_manifest_hash: block.post_manifest_hash.clone(),
            driver_install_digest: digest("test-driver-install-record", &certificate),
        })
    }
}

impl NodeExecutionHook for TransitionTestHook {
    fn on_commit(
        &mut self,
        _: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("legacy commit is outside the BFT V2 test hook".into())
    }

    fn configure_bft_v2_execution(
        &mut self,
        config: &NodeBftExecutionConfigV2,
    ) -> Result<Option<crate::NodeBftExecutionHeadV2>, String> {
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| "test driver ledger mutex poisoned".to_string())?;
        match ledger.config.as_ref() {
            Some(existing) if existing != config => {
                Err("test driver BFT V2 configuration changed".into())
            }
            Some(_) => {
                ledger.replay_cursor = 0;
                Ok(None)
            }
            None => {
                ledger.config = Some(config.clone());
                ledger.replay_cursor = 0;
                Ok(None)
            }
        }
    }

    fn current_bft_v2_execution_context(
        &mut self,
        schedule: &BftHeightProposerScheduleV2,
    ) -> Result<NodeBftExecutionContextV2, String> {
        let expected = self.schedule()?;
        if &expected != schedule {
            return Err("test driver cursor differs from Node V2 schedule".into());
        }
        Ok(NodeBftExecutionContextV2::from_schedule(schedule))
    }

    fn build_bft_v2_candidate(
        &mut self,
        context: NodeBftBuildContextV2,
    ) -> Result<BftBlockV2, String> {
        let schedule = self.schedule()?;
        if context.execution != NodeBftExecutionContextV2::from_schedule(&schedule) {
            return Err("test driver candidate context is stale".into());
        }
        self.make_block(&schedule, &context.actions)
    }

    fn preview_bft_v2_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV2,
        candidate: &BftBlockV2,
    ) -> Result<(), String> {
        let schedule = self.schedule()?;
        if expected != &NodeBftExecutionContextV2::from_schedule(&schedule) {
            return Err("test driver preview context is stale".into());
        }
        let actions: Vec<NodeConsensusAction> = serde_cbor::from_slice(&candidate.actions_cbor)
            .map_err(|error| format!("decode test BFT V2 candidate actions: {error}"))?;
        if serde_cbor::to_vec(&actions)
            .map_err(|error| format!("re-encode test BFT V2 candidate actions: {error}"))?
            != candidate.actions_cbor
        {
            return Err("test BFT V2 actions are not canonical".into());
        }
        if self.make_block(&schedule, &actions)? != *candidate {
            return Err("test BFT V2 candidate differs from deterministic preview".into());
        }
        Ok(())
    }

    fn validate_bft_v2_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        let value: serde_json::Value = serde_cbor::from_slice(payload)
            .map_err(|error| format!("decode test BFT V2 action envelope: {error}"))?;
        if value == serde_json::json!({"kind": "transition-test-noop"}) {
            Ok(())
        } else {
            Err("unsupported test BFT V2 action".into())
        }
    }

    fn validate_bft_v2_action_batch(
        &mut self,
        schedule: &BftHeightProposerScheduleV2,
        actions: &[NodeConsensusAction],
    ) -> Result<(), String> {
        if &self.schedule()? != schedule {
            return Err("test BFT V2 action batch cursor is stale".into());
        }
        for action in actions {
            self.validate_bft_v2_action_payload(&action.payload_cbor)?;
        }
        Ok(())
    }

    fn commit_bft_v2_certificate(
        &mut self,
        token: VerifiedBftCommitCertificateV2,
    ) -> Result<NodeBftExecutionCommitResultV2, String> {
        let certificate = token.certificate().clone();
        let index = usize::try_from(token.block().height.saturating_sub(1))
            .map_err(|_| "test BFT V2 height exceeds address space".to_string())?;
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| "test driver ledger mutex poisoned".to_string())?;
        let config = ledger
            .config
            .as_ref()
            .ok_or_else(|| "test driver was not configured".to_string())?;
        let set = BftValidatorSetV2::new(config.scope.clone(), config.initial_validators.clone())?;
        let mut schedule = BftHeightProposerScheduleV2::genesis(
            set,
            config.schedule_policy.clone(),
            config.authority_epoch,
            config.genesis_state_root.clone(),
            config.genesis_manifest_hash.clone(),
        )?;

        if index < ledger.replay_cursor {
            let Some(existing) = ledger.certificates.get(index) else {
                return Err("test driver replay cursor exceeds stored history".into());
            };
            if existing != &certificate {
                return Err("test driver has a conflicting BFT V2 certificate".into());
            }
            for committed in ledger.certificates.iter().take(index) {
                schedule = verify_commit_certificate_token_v2(&schedule, committed)?
                    .next_height_schedule()
                    .clone();
            }
            if token.schedule() != &schedule {
                return Err(
                    "test driver retry does not match the stored certificate height".into(),
                );
            }
            verify_commit_certificate_token_v2(&schedule, &certificate)?;
            return Self::result(&token);
        }

        for committed in ledger.certificates.iter().take(ledger.replay_cursor) {
            schedule = verify_commit_certificate_token_v2(&schedule, committed)?
                .next_height_schedule()
                .clone();
        }
        if token.schedule() != &schedule {
            return Err("test driver commit token does not extend its exact cursor".into());
        }
        if let Some(existing) = ledger.certificates.get(index) {
            if existing != &certificate {
                return Err("test driver has a conflicting BFT V2 certificate".into());
            }
            if index == ledger.replay_cursor {
                ledger.replay_cursor += 1;
            } else if index > ledger.replay_cursor {
                return Err("test driver BFT V2 replay skipped a certificate".into());
            }
        } else {
            if index != ledger.replay_cursor || index != ledger.certificates.len() {
                return Err("test driver BFT V2 commit is not contiguous".into());
            }
            ledger.certificates.push(certificate);
            ledger.replay_cursor += 1;
        }
        Self::result(&token)
    }

    fn load_bft_v2_commit_history_page(
        &mut self,
        config: &NodeBftExecutionConfigV2,
        after_height: u64,
        max_entries: usize,
    ) -> Result<Vec<BftCommitCertificateV2>, String> {
        let ledger = self
            .ledger
            .lock()
            .map_err(|_| "test driver ledger mutex poisoned".to_string())?;
        if ledger.config.as_ref() != Some(config) {
            return Err("test driver history requested under a different config".into());
        }
        let start = usize::try_from(after_height)
            .map_err(|_| "test BFT V2 history cursor exceeds address space".to_string())?;
        if start > ledger.certificates.len() {
            return Err("test BFT V2 history cursor is beyond the head".into());
        }
        Ok(ledger
            .certificates
            .iter()
            .skip(start)
            .take(max_entries)
            .cloned()
            .collect())
    }
}

struct NodeIdentity {
    id: String,
    seed: u8,
    binding: BftSignerBindingV2,
    provider: ProtectedBftV2TestProvider,
    driver: Arc<Mutex<DriverLedger>>,
}

fn validators(seeds: &[u8]) -> Vec<BftValidatorV2> {
    seeds
        .iter()
        .map(|seed| BftValidatorV2 {
            validator_id: validator_id(*seed),
            public_key_hex: hex::encode(
                SigningKey::from_bytes(&[*seed; 32])
                    .verifying_key()
                    .to_bytes(),
            ),
            voting_power: 1,
        })
        .collect()
}

fn validator_id(seed: u8) -> String {
    format!("validator-{seed:02}")
}

fn scope() -> BftScopeV2 {
    BftScopeV2 {
        world_id: WORLD_ID.into(),
        chain_id: CHAIN_ID.into(),
        genesis_hash: hash_byte(0x11),
    }
}

fn binding(seed: u8, requester_seed: u8, provider_seed: u8) -> BftSignerBindingV2 {
    let validator_key = SigningKey::from_bytes(&[seed; 32]);
    let requester_key = SigningKey::from_bytes(&[requester_seed; 32]);
    BftSignerBindingV2 {
        schema_version: 2,
        scope: scope(),
        genesis_authority_epoch: 1,
        genesis_validators: validators(&[11, 12, 13, 14]),
        genesis_state_root: hash_byte(0x22),
        genesis_manifest_hash: hash_byte(0x33),
        schedule_policy: BftSchedulePolicyV2::new(EPOCH_LENGTH, PROFILE_ID)
            .expect("valid BFT V2 test policy"),
        local_validator_id: validator_id(seed),
        provisioned_signing_keys: vec![BftSignerKeyIdentityV2 {
            validator_id: validator_id(seed),
            public_key_hex: hex::encode(validator_key.verifying_key().to_bytes()),
        }],
        timeouts: TEST_TIMEOUTS,
        operator_instance_id: format!("node-v2-transition-test-{seed}-{provider_seed}"),
        requester_auth_public_key: hex::encode(requester_key.verifying_key().to_bytes()),
    }
}

fn identities() -> Vec<NodeIdentity> {
    (11..=16)
        .map(|seed| {
            let requester_seed = seed + 40;
            let provider_seed = seed + 80;
            let binding = binding(seed, requester_seed, provider_seed);
            let provider = ProtectedBftV2TestProvider::start(
                binding.clone(),
                SigningKey::from_bytes(&[requester_seed; 32]),
                SigningKey::from_bytes(&[provider_seed; 32]),
                vec![SigningKey::from_bytes(&[seed; 32])],
            )
            .expect("start real loopback protected BFT V2 provider");
            NodeIdentity {
                id: validator_id(seed),
                seed,
                binding,
                provider,
                driver: Arc::new(Mutex::new(DriverLedger::default())),
            }
        })
        .collect()
}

fn reserve_udp_addresses(count: usize) -> Vec<SocketAddr> {
    (0..count)
        .map(|_| {
            UdpSocket::bind("127.0.0.1:0")
                .expect("reserve BFT V2 UDP test address")
                .local_addr()
                .expect("read reserved BFT V2 UDP address")
        })
        .collect()
}

fn make_node(
    identity: &NodeIdentity,
    index: usize,
    addresses: &[SocketAddr],
    root: &Path,
) -> NodeRuntime {
    let peers = addresses
        .iter()
        .enumerate()
        .filter_map(|(peer_index, address)| (peer_index != index).then_some(*address))
        .collect();
    let replication = NodeReplicationConfig::new(root.join(format!("{}/replication", identity.id)))
        .expect("build BFT V2 replication settings");
    let config = NodeConfig::new(identity.id.clone(), WORLD_ID, NodeRole::Sequencer)
        .expect("build BFT V2 Node config")
        .with_tick_interval(Duration::from_millis(20))
        .expect("set BFT V2 worker cadence")
        .with_replication(replication)
        .with_gossip_optional(addresses[index], peers);
    NodeRuntime::new(config)
        .with_execution_hook(TransitionTestHook {
            ledger: Arc::clone(&identity.driver),
            new_validators: validators(&[11, 12, 13, 15, 16]),
        })
        .with_replication_network_consensus_enabled(false)
        .with_tendermint_v2(identity.binding.clone(), identity.provider.provider.clone())
        .expect("select protected dynamic-set BFT V2")
}

fn digest(domain: &str, bytes: &[u8]) -> String {
    let mut framed = domain.as_bytes().to_vec();
    framed.push(0);
    framed.extend_from_slice(bytes);
    oasis7_distfs::blake3_hex(&framed)
}

fn hash_byte(byte: u8) -> String {
    format!("{byte:02x}").repeat(32)
}

fn wait_all_height(nodes: &[NodeRuntime], identities: &[NodeIdentity], target: u64, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let snapshots = nodes.iter().map(NodeRuntime::snapshot).collect::<Vec<_>>();
        let provider_errors = identities
            .iter()
            .enumerate()
            .filter_map(|(index, identity)| {
                let errors = identity.provider.server.errors();
                (!errors.is_empty()).then_some((index, errors))
            })
            .collect::<Vec<_>>();
        if snapshots.iter().any(|snapshot| !snapshot.running) {
            panic!(
                "BFT V2 worker stopped while waiting for {label} at height {target}: {}; provider service errors: {provider_errors:?}",
                snapshots
                    .iter()
                    .map(|snapshot| format!(
                        "{} running={} committed={} error={:?}",
                        snapshot.node_id,
                        snapshot.running,
                        snapshot.consensus.committed_height,
                        snapshot.last_error
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
        if snapshots
            .iter()
            .all(|snapshot| snapshot.consensus.committed_height >= target)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {label} at BFT V2 height {target}: {}; provider service errors: {provider_errors:?}",
            snapshots
                .iter()
                .map(|snapshot| format!(
                    "{} committed={} error={:?}",
                    snapshot.node_id, snapshot.consensus.committed_height, snapshot.last_error
                ))
                .collect::<Vec<_>>()
                .join("; ")
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_node_height(node: &NodeRuntime, target: u64, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let snapshot = node.snapshot();
        if !snapshot.running {
            panic!(
                "BFT V2 worker {} stopped while waiting for {label} at height {target}: committed={} error={:?}",
                snapshot.node_id, snapshot.consensus.committed_height, snapshot.last_error
            );
        }
        if snapshot.consensus.committed_height >= target {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {label} at BFT V2 height {target}: node={} committed={} error={:?}",
            snapshot.node_id,
            snapshot.consensus.committed_height,
            snapshot.last_error
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_node_stopped(node: &NodeRuntime, label: &str) -> crate::NodeSnapshot {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let snapshot = node.snapshot();
        if !snapshot.running {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {label}; node={} committed={} error={:?}",
            snapshot.node_id,
            snapshot.consensus.committed_height,
            snapshot.last_error
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn stop_all(nodes: &mut Vec<NodeRuntime>) {
    for node in nodes.iter_mut() {
        node.stop().expect("stop BFT V2 test worker");
    }
    nodes.clear();
}

fn verify_certificates(
    binding: &BftSignerBindingV2,
    certificates: &[BftCommitCertificateV2],
    through_height: u64,
) {
    assert!(certificates.len() >= through_height as usize);
    let genesis = binding.validate().expect("valid genesis set");
    let mut schedule = BftHeightProposerScheduleV2::genesis(
        genesis,
        binding.schedule_policy.clone(),
        binding.genesis_authority_epoch,
        binding.genesis_state_root.clone(),
        binding.genesis_manifest_hash.clone(),
    )
    .expect("valid genesis schedule");
    let old_ids = [11, 12, 13, 14]
        .into_iter()
        .map(validator_id)
        .collect::<std::collections::BTreeSet<_>>();
    let new_ids = [11, 12, 13, 15, 16]
        .into_iter()
        .map(validator_id)
        .collect::<std::collections::BTreeSet<_>>();
    for certificate in certificates.iter().take(through_height as usize) {
        let token = verify_commit_certificate_token_v2(&schedule, certificate)
            .expect("complete certificate verifies against the preceding schedule");
        let block = token.block();
        let expected = if block.height < ACTIVATION_HEIGHT {
            &old_ids
        } else {
            &new_ids
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut voting_power = 0u64;
        for vote in &certificate.precommits {
            assert_eq!(vote.phase, BftVotePhaseV2::Precommit);
            assert_eq!(
                vote.value,
                BftVoteValueV2::Block {
                    block_id: token.block_id().to_string()
                }
            );
            assert!(expected.contains(&vote.validator_id));
            assert!(seen.insert(vote.validator_id.clone()), "duplicate signer");
            vote.verify_signature()
                .expect("precommit signature verifies");
            voting_power += token
                .active_validator_set()
                .voting_power(&vote.validator_id)
                .expect("signer belongs to the active set");
        }
        assert!(
            voting_power >= token.active_validator_set().quorum_voting_power(),
            "height {} certificate lacks active-set quorum",
            block.height
        );
        if block.height == SCHEDULED_HEIGHT {
            assert_eq!(
                block.transition.as_ref().unwrap().new_validators,
                validators(&[11, 12, 13, 15, 16])
            );
            assert_eq!(block.transition_ref, None);
        }
        if block.height == SCHEDULED_HEIGHT + 1 {
            assert!(block.transition.is_none());
            assert_eq!(
                block.transition_ref.as_ref().unwrap().scheduled_height,
                SCHEDULED_HEIGHT
            );
            assert_eq!(
                block.transition_ref.as_ref().unwrap().activation_height,
                ACTIVATION_HEIGHT
            );
        }
        if block.height == ACTIVATION_HEIGHT {
            assert_eq!(
                token
                    .active_validator_set()
                    .validators()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                new_ids.iter().cloned().collect::<Vec<_>>()
            );
        }
        schedule = token.next_height_schedule().clone();
    }
}

#[test]
fn protected_tcp_signer_workers_apply_transition_and_continue_after_node_restart() {
    let root = PrivateRoot::new();
    let mut identities = identities();
    let addresses = reserve_udp_addresses(identities.len());
    let mut nodes = identities
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, identity)| make_node(identity, index, &addresses, root.path()))
        .collect::<Vec<_>>();

    for (index, node) in nodes.iter_mut().enumerate() {
        if let Err(error) = node.start() {
            let service_errors = identities
                .iter()
                .enumerate()
                .map(|(identity_index, identity)| {
                    (identity_index, identity.provider.server.errors())
                })
                .collect::<Vec<_>>();
            panic!(
                "start actual protected BFT V2 worker {index}: {error}; provider service errors: {service_errors:?}"
            );
        }
    }
    wait_all_height(
        &nodes,
        &identities,
        ACTIVATION_HEIGHT,
        "4-to-5 set activation",
    );

    let removed = identities
        .iter()
        .position(|identity| identity.seed == 14)
        .unwrap();
    assert!(nodes[removed].bft_v2_status().expect("V2 status").running);
    assert!(
        !nodes[removed]
            .bft_v2_status()
            .expect("V2 status")
            .local_validator_active
    );

    let late_index = identities
        .iter()
        .position(|identity| identity.seed == 16)
        .expect("late validator identity");
    let before_late_join = identities[late_index].provider.backend.snapshot().state;
    assert!(before_late_join.history.is_empty());
    assert!(before_late_join.journal.is_none());
    assert!(before_late_join.install_receipts.is_empty());

    // This validator was offline through the h+1 confirmation. It must catch
    // up from the real gossip history before the provider permits activation.
    // Fail only the install CAS to prove the Node remains stopped with a
    // durable pending intent and no signature-capable receipt.
    identities[late_index]
        .provider
        .server
        .fail_next_complete_transition_install();
    let mut failed_late = make_node(&identities[late_index], late_index, &addresses, root.path());
    failed_late
        .start()
        .expect("late validator starts in read-only history catch-up mode");
    let failed_snapshot = wait_node_stopped(&failed_late, "late validator install failure");
    assert!(
        failed_snapshot
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("complete BFT V2 transition install")),
        "late catch-up must stop on the failed protected install, got {:?}",
        failed_snapshot.last_error
    );
    failed_late
        .stop()
        .expect("join stopped late validator worker");
    let failed_install_state = identities[late_index].provider.backend.snapshot().state;
    let pending_install = failed_install_state
        .pending_install
        .as_ref()
        .expect("failed install leaves its exact intent pending");
    assert_eq!(pending_install.body.activation_height, ACTIVATION_HEIGHT);
    let pending_request_id = pending_install.body.request_id.clone();
    let failed_activation_journal = failed_install_state
        .journal
        .as_ref()
        .expect("failed install retains the restored activation-height journal");
    assert_eq!(failed_activation_journal.height, ACTIVATION_HEIGHT);
    assert!(failed_activation_journal.height_started_at_ms.is_none());
    assert!(failed_activation_journal.events().is_empty());
    assert!(failed_activation_journal.pending_proposal_intent.is_none());
    assert!(failed_activation_journal.pending_vote_intent.is_none());
    assert!(failed_activation_journal.pending_commit.is_none());
    assert!(failed_install_state.install_receipts.is_empty());
    assert_eq!(
        failed_install_state.history.len(),
        ACTIVATION_HEIGHT as usize - 1,
        "late validator must stop at the h+1 confirmation before activation"
    );

    // The next completion commits remotely but loses the TCP reply. Startup
    // must fail closed for that attempt; a further restart recovers the exact
    // receipt rather than creating a new installation decision. The failed
    // CAS poisoned its service, so an explicit test operator must first stop
    // every old session and fence only the lease metadata. The same pending
    // request and signer state survive that external action.
    let before_operator_fence = identities[late_index].provider.backend.snapshot();
    let expected_generation = before_operator_fence.lease_generation;
    assert!(!before_operator_fence.lease_token_digest.is_empty());
    let late_seed = identities[late_index].seed;
    let requester_seed = late_seed + 40;
    let provider_seed = late_seed + 80;
    identities[late_index]
        .provider
        .operator_fence_and_restart_for_test(
            expected_generation,
            SigningKey::from_bytes(&[requester_seed; 32]),
            SigningKey::from_bytes(&[provider_seed; 32]),
            vec![SigningKey::from_bytes(&[late_seed; 32])],
        )
        .expect("explicitly fence the stopped poisoned service for the test");
    let after_operator_fence = identities[late_index].provider.backend.snapshot();
    assert_eq!(after_operator_fence.state, before_operator_fence.state);
    assert_eq!(
        after_operator_fence.protected_time_floor_ms,
        before_operator_fence.protected_time_floor_ms
    );
    assert_eq!(
        after_operator_fence.lease_generation,
        expected_generation + 1
    );
    assert!(after_operator_fence.lease_token_digest.is_empty());
    assert_eq!(
        after_operator_fence
            .state
            .pending_install
            .as_ref()
            .map(|intent| intent.body.request_id.as_str()),
        Some(pending_request_id.as_str()),
        "external test fencing must preserve the original pending install identity"
    );

    identities[late_index]
        .provider
        .server
        .drop_next_complete_transition_install_reply();
    let mut lost_reply = make_node(&identities[late_index], late_index, &addresses, root.path());
    let lost_reply_error = match lost_reply.start() {
        Ok(()) => panic!("startup cannot claim an install whose completion reply was lost"),
        Err(error) => error,
    };
    let committed_install_state = identities[late_index].provider.backend.snapshot().state;
    assert!(
        committed_install_state.pending_install.is_none(),
        "a committed lost reply must leave the original receipt and clear pending; startup error={lost_reply_error:?}, provider service errors={:?}, provider state={committed_install_state:?}",
        identities[late_index].provider.server.errors()
    );
    let recovered_receipt = committed_install_state
        .install_receipt_for_request(&pending_request_id)
        .expect("lost reply leaves the original durable receipt")
        .clone();

    let mut late_node = make_node(&identities[late_index], late_index, &addresses, root.path());
    late_node
        .start()
        .expect("startup recovers the exact protected install receipt");
    wait_node_height(
        &late_node,
        ACTIVATION_HEIGHT,
        "late validator activation catch-up",
    );
    let recovered_status = late_node.bft_v2_status().expect("late validator status");
    assert!(recovered_status.local_validator_active);
    assert!(recovered_status.committed_height >= ACTIVATION_HEIGHT);
    let retried_state = identities[late_index].provider.backend.snapshot().state;
    assert_eq!(
        retried_state.install_receipt_for_request(&pending_request_id),
        Some(&recovered_receipt),
        "restart must retain the original receipt byte-for-byte"
    );

    let beyond_confirmation_height = ACTIVATION_HEIGHT + 2;
    wait_all_height(
        &nodes,
        &identities,
        beyond_confirmation_height,
        "post-activation history before cold restart",
    );
    wait_node_height(
        &late_node,
        beyond_confirmation_height,
        "late validator history before cold restart",
    );
    let late_tip_height = u64::try_from(
        identities[late_index]
            .provider
            .backend
            .snapshot()
            .state
            .history
            .len(),
    )
    .expect("bounded late validator history");
    assert!(late_tip_height >= beyond_confirmation_height);
    assert_eq!(
        recovered_receipt.body.installed_head_height,
        ACTIVATION_HEIGHT - 1,
        "the receipt is anchored at the h+1 confirmation"
    );
    assert!(
        recovered_receipt.body.installed_head_height < late_tip_height,
        "cold startup must recover the old confirmation receipt from a later verified tip"
    );

    stop_all(&mut nodes);
    late_node
        .stop()
        .expect("stop late validator after catch-up");
    let histories = identities
        .iter()
        .map(|identity| identity.provider.backend.snapshot().state.history)
        .collect::<Vec<_>>();
    let (tip_index, tip_history) = histories
        .iter()
        .enumerate()
        .max_by_key(|(_, history)| history.len())
        .expect("at least one protected signer history");
    let tip_height = u64::try_from(tip_history.len()).expect("bounded test history");
    assert!(tip_height >= ACTIVATION_HEIGHT);
    verify_certificates(&identities[tip_index].binding, tip_history, tip_height);

    for identity in &identities[4..6] {
        let checkpoint = identity.provider.backend.snapshot();
        assert!(
            checkpoint.state.install_receipts.iter().any(|receipt| {
                receipt.body.activation_height == ACTIVATION_HEIGHT
                    && receipt.body.active_validator_set_hash
                        == BftValidatorSetV2::new(scope(), validators(&[11, 12, 13, 15, 16]))
                            .unwrap()
                            .set_hash()
            }),
            "new validator {} must have a protected install receipt before activation signing",
            identity.id
        );
    }

    let removed_state: BftSignerStateV2 = identities[removed].provider.backend.snapshot().state;
    assert!(
        removed_state.journal.is_none(),
        "removed member must be read-only"
    );
    for certificate in removed_state
        .history
        .iter()
        .filter(|certificate| certificate.proposal.block.height >= ACTIVATION_HEIGHT)
    {
        assert!(
            certificate
                .precommits
                .iter()
                .all(|vote| vote.validator_id != identities[removed].id)
        );
    }

    // Restart exactly two old-set validators and both new-set validators.
    // A QC can omit any one member of a larger set, and a signer need not have
    // received its own QC yet, so select the freshest history holder as a
    // source and let ordinary history catch-up reconcile differing prefixes.
    let mut restart_indices = vec![4, 5];
    let mut old_active = vec![0, 1, 2];
    if tip_index < 3 {
        old_active.retain(|index| *index != tip_index);
        restart_indices.push(tip_index);
        restart_indices.push(old_active[0]);
    } else {
        restart_indices.extend(old_active.into_iter().take(2));
    }
    if tip_index == removed {
        restart_indices.push(removed);
    }
    restart_indices.sort_unstable();
    restart_indices.dedup();
    assert_eq!(
        restart_indices
            .iter()
            .filter(|index| **index != removed)
            .count(),
        4,
        "restart exactly two old and both new active validators"
    );
    assert!(restart_indices.contains(&tip_index));
    let next_height = tip_height.checked_add(1).expect("test height bound");
    let next_height_index = usize::try_from(next_height).expect("test height fits usize");
    assert!(
        histories
            .iter()
            .all(|history| history.len() < next_height_index),
        "height {next_height} must not exist before the Node restart"
    );

    // Reopen the four active validators. If the freshest source is the
    // removed validator, include it only as a read-only history source.
    nodes = identities
        .iter()
        .enumerate()
        .filter(|(index, _)| restart_indices.contains(index))
        .map(|(index, identity)| make_node(identity, index, &addresses, root.path()))
        .collect();
    for node in &mut nodes {
        node.start().expect("replay and restart at activated set");
    }
    wait_all_height(
        &nodes,
        &identities,
        next_height,
        "post-restart new-set commit",
    );

    if restart_indices.contains(&removed) {
        let followed_removed_state = identities[removed].provider.backend.snapshot().state;
        assert!(followed_removed_state.journal.is_none());
        assert!(followed_removed_state.history.len() >= next_height as usize);
        for certificate in followed_removed_state
            .history
            .iter()
            .filter(|certificate| certificate.proposal.block.height >= ACTIVATION_HEIGHT)
        {
            assert!(
                certificate
                    .precommits
                    .iter()
                    .all(|vote| vote.validator_id != identities[removed].id),
                "removed validator must follow certificates without signing"
            );
        }
    }

    let (restart_tip_index, restarted_history) = identities
        .iter()
        .enumerate()
        .map(|(index, identity)| (index, identity.provider.backend.snapshot().state.history))
        .max_by_key(|(_, history)| history.len())
        .expect("restarted providers retain history");
    verify_certificates(
        &identities[restart_tip_index].binding,
        &restarted_history,
        next_height,
    );
    let next = &restarted_history[(next_height - 1) as usize];
    assert_eq!(next.proposal.block.height, next_height);
    let next_signers = next
        .precommits
        .iter()
        .map(|vote| vote.validator_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(next_signers.contains(validator_id(15).as_str()));
    assert!(next_signers.contains(validator_id(16).as_str()));

    for node in &mut nodes {
        node.stop().expect("stop restarted BFT V2 worker");
    }
}

#[path = "node_bft_v2_worker_transition_tests/late_install.rs"]
mod late_install;

#[path = "node_bft_v2_worker_transition_tests/proposal_evidence.rs"]
mod proposal_evidence;
