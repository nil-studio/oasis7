#![cfg(unix)]

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v1::{BftTimeoutsV1, BftValidatorV1};
use oasis7_proto::distributed_bft::{BftBlockV1, BftCommitCertificateV1, BftScopeV1};
use serde::{Deserialize, Serialize};

use crate::{
    NodeBftBuildContextV1, NodeBftConfigV1, NodeBftExecutionCommitResultV1,
    NodeBftExecutionConfigV1, NodeBftExecutionContextV1, NodeBftExecutionHeadV1, NodeConfig,
    NodeExecutionCommitContext, NodeExecutionCommitResult, NodeExecutionHook,
    NodeReplicationConfig, NodeRole, NodeRuntime, compute_consensus_action_root,
};

const WORLD_ID: &str = "node-bft-runtime-integration-world";
const CHAIN_ID: &str = "node-bft-runtime-integration-chain";
const GENESIS_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const MANIFEST_HASH: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const ACTION_ID: u64 = 77;
const TEST_TIMEOUTS: BftTimeoutsV1 = BftTimeoutsV1 {
    propose_ms: 180,
    prevote_ms: 180,
    precommit_ms: 180,
    round_delta_ms: 40,
};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-node-bft-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create isolated BFT integration root");
        Self(path)
    }

    fn path(&self) -> &Path {
        self.0.as_path()
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PinnedTestConfig {
    scope: BftScopeV1,
    authority_epoch: u64,
    validators: Vec<BftValidatorV1>,
    validator_set_hash: String,
}

impl From<&NodeBftExecutionConfigV1> for PinnedTestConfig {
    fn from(config: &NodeBftExecutionConfigV1) -> Self {
        Self {
            scope: config.scope.clone(),
            authority_epoch: config.authority_epoch,
            validators: config.validators.clone(),
            validator_set_hash: config.validator_set_hash.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DurableTestEvidence {
    config: Option<PinnedTestConfig>,
    certificates: Vec<BftCommitCertificateV1>,
}

/// A deterministic, durable test driver. It exercises the NodeRuntime hook
/// contract and local recovery only; it is not a production execution driver.
struct DurableTestHook {
    directory: PathBuf,
    evidence: DurableTestEvidence,
    newly_applied_heights: Arc<Mutex<Vec<u64>>>,
}

impl DurableTestHook {
    fn open(directory: PathBuf, newly_applied_heights: Arc<Mutex<Vec<u64>>>) -> Self {
        let path = directory.join("bft-test-evidence.cbor");
        let evidence = match fs::read(path) {
            Ok(bytes) => serde_cbor::from_slice(&bytes).expect("decode test BFT evidence"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                DurableTestEvidence::default()
            }
            Err(error) => panic!("read test BFT evidence: {error}"),
        };
        Self {
            directory,
            evidence,
            newly_applied_heights,
        }
    }

    fn evidence_path(&self) -> PathBuf {
        self.directory.join("bft-test-evidence.cbor")
    }

    fn persist(&self, evidence: &DurableTestEvidence) -> Result<(), String> {
        fs::create_dir_all(&self.directory)
            .map_err(|error| format!("create test BFT evidence directory: {error}"))?;
        let bytes = serde_cbor::to_vec(evidence)
            .map_err(|error| format!("encode test BFT evidence: {error}"))?;
        let temporary_path = self.directory.join("bft-test-evidence.cbor.tmp");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .map_err(|error| format!("create test BFT evidence temporary file: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("write test BFT evidence: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync test BFT evidence: {error}"))?;
        drop(file);
        fs::rename(&temporary_path, self.evidence_path())
            .map_err(|error| format!("replace test BFT evidence: {error}"))?;
        File::open(&self.directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("sync test BFT evidence directory: {error}"))
    }

    fn current_context(
        &self,
        scope: &BftScopeV1,
        height: u64,
        parent_commit_ref: &str,
        authority_epoch: u64,
    ) -> Result<NodeBftExecutionContextV1, String> {
        let config = self
            .evidence
            .config
            .as_ref()
            .ok_or_else(|| "test BFT driver is not configured".to_string())?;
        let expected_height = u64::try_from(self.evidence.certificates.len())
            .map_err(|_| "test BFT evidence height overflow".to_string())?
            .checked_add(1)
            .ok_or_else(|| "test BFT evidence height overflow".to_string())?;
        let expected_parent = match self.evidence.certificates.last() {
            Some(certificate) => certificate.proposal.block.block_id()?,
            None => config.scope.genesis_hash.clone(),
        };
        if config.scope != *scope
            || config.authority_epoch != authority_epoch
            || height != expected_height
            || parent_commit_ref != expected_parent
        {
            return Err("test BFT execution context differs from its durable head".into());
        }
        Ok(NodeBftExecutionContextV1 {
            scope: scope.clone(),
            height,
            parent_commit_ref: parent_commit_ref.to_string(),
            authority_epoch,
            manifest_hash: MANIFEST_HASH.to_string(),
        })
    }

    fn deterministic_block(
        context: &NodeBftExecutionContextV1,
        actions: &[crate::NodeConsensusAction],
    ) -> Result<BftBlockV1, String> {
        let actions_cbor = serde_cbor::to_vec(&actions)
            .map_err(|error| format!("encode deterministic test actions: {error}"))?;
        let action_root = compute_consensus_action_root(actions)
            .map_err(|error| format!("compute deterministic test action root: {error}"))?;
        Ok(BftBlockV1 {
            scope: context.scope.clone(),
            height: context.height,
            parent_commit_ref: context.parent_commit_ref.clone(),
            authority_epoch: context.authority_epoch,
            manifest_hash: context.manifest_hash.clone(),
            event_root: test_root(1, context, actions_cbor.as_slice()),
            execution_block_hash: test_root(2, context, actions_cbor.as_slice()),
            execution_state_root: test_root(3, context, actions_cbor.as_slice()),
            state_root: test_root(4, context, actions_cbor.as_slice()),
            actions_cbor,
            action_root,
        })
    }

    fn result(
        certificate: &oasis7_consensus::tendermint_v1::VerifiedBftCommitCertificateV1,
    ) -> NodeBftExecutionCommitResultV1 {
        let block = certificate.block();
        NodeBftExecutionCommitResultV1 {
            height: block.height,
            bft_commit_ref: certificate.block_id().to_string(),
            execution_block_hash: block.execution_block_hash.clone(),
            execution_state_root: block.execution_state_root.clone(),
            state_root: block.state_root.clone(),
        }
    }
}

fn test_root(domain: u8, context: &NodeBftExecutionContextV1, actions_cbor: &[u8]) -> String {
    // This deterministic test-only mixer is not an execution-security hash.
    let mut output = [domain; 32];
    let height = context.height.to_be_bytes();
    let parent = context.parent_commit_ref.as_bytes();
    for (index, byte) in height
        .iter()
        .chain(parent.iter())
        .chain(actions_cbor.iter())
        .enumerate()
    {
        let slot = index % output.len();
        output[slot] = output[slot].wrapping_mul(31).wrapping_add(*byte);
    }
    hex::encode(output)
}

impl NodeExecutionHook for DurableTestHook {
    fn validate_bft_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        let value: serde_json::Value = serde_cbor::from_slice(payload)
            .map_err(|error| format!("test action decode: {error}"))?;
        if value == serde_json::json!({"kind": "bft-integration-noop"}) {
            Ok(())
        } else {
            Err("unsupported deterministic BFT test action".into())
        }
    }

    fn on_commit(
        &mut self,
        _context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("legacy Pos commit is outside the BFT test driver".into())
    }

    fn build_bft_candidate(
        &mut self,
        context: NodeBftBuildContextV1,
    ) -> Result<BftBlockV1, String> {
        let expected = self.current_context(
            &context.execution.scope,
            context.execution.height,
            context.execution.parent_commit_ref.as_str(),
            context.execution.authority_epoch,
        )?;
        if expected != context.execution {
            return Err("test BFT candidate context is not the current durable context".into());
        }
        Self::deterministic_block(&context.execution, context.actions.as_slice())
    }

    fn preview_bft_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV1,
        candidate: &BftBlockV1,
    ) -> Result<(), String> {
        let current = self.current_context(
            &expected.scope,
            expected.height,
            expected.parent_commit_ref.as_str(),
            expected.authority_epoch,
        )?;
        if &current != expected {
            return Err("test BFT preview context is stale".into());
        }
        let actions: Vec<crate::NodeConsensusAction> =
            serde_cbor::from_slice(&candidate.actions_cbor)
                .map_err(|error| format!("decode test BFT actions: {error}"))?;
        if serde_cbor::to_vec(&actions)
            .map_err(|error| format!("re-encode test BFT actions: {error}"))?
            != candidate.actions_cbor
        {
            return Err("test BFT actions are not canonical".into());
        }
        if candidate != &Self::deterministic_block(expected, actions.as_slice())? {
            return Err("test BFT candidate roots do not match isolated preview".into());
        }
        Ok(())
    }

    fn configure_bft_execution(
        &mut self,
        config: &NodeBftExecutionConfigV1,
    ) -> Result<Option<NodeBftExecutionHeadV1>, String> {
        config.validate()?;
        let configured = PinnedTestConfig::from(config);
        match &self.evidence.config {
            Some(existing) if existing != &configured => {
                return Err("test BFT fixed configuration changed".into());
            }
            Some(_) => {}
            None => {
                let mut next = self.evidence.clone();
                next.config = Some(configured);
                self.persist(&next)?;
                self.evidence = next;
            }
        }
        Ok(self
            .evidence
            .certificates
            .last()
            .map(|certificate| NodeBftExecutionHeadV1 {
                height: certificate.proposal.block.height,
                block_id: certificate
                    .proposal
                    .block
                    .block_id()
                    .expect("test certificate block id"),
            }))
    }

    fn current_bft_execution_context(
        &mut self,
        scope: &BftScopeV1,
        height: u64,
        parent_commit_ref: &str,
        authority_epoch: u64,
    ) -> Result<NodeBftExecutionContextV1, String> {
        self.current_context(scope, height, parent_commit_ref, authority_epoch)
    }

    fn commit_bft_certificate(
        &mut self,
        certificate: oasis7_consensus::tendermint_v1::VerifiedBftCommitCertificateV1,
    ) -> Result<NodeBftExecutionCommitResultV1, String> {
        let block = certificate.block();
        let existing_index = usize::try_from(block.height.saturating_sub(1))
            .map_err(|_| "test BFT certificate height overflow".to_string())?;
        if let Some(existing) = self.evidence.certificates.get(existing_index) {
            if existing == certificate.certificate() {
                return Ok(Self::result(&certificate));
            }
            return Err("test BFT durable history conflicts at an existing height".into());
        }
        let context = self.current_context(
            &block.scope,
            block.height,
            block.parent_commit_ref.as_str(),
            block.authority_epoch,
        )?;
        self.preview_bft_candidate(&context, block)?;
        if block.scope != self.evidence.config.as_ref().expect("configured").scope {
            return Err("test BFT certificate scope differs from pin".into());
        }
        let mut next = self.evidence.clone();
        next.certificates.push(certificate.certificate().clone());
        self.persist(&next)?;
        self.evidence = next;
        self.newly_applied_heights
            .lock()
            .map_err(|_| "test BFT apply counter poisoned".to_string())?
            .push(block.height);
        Ok(Self::result(&certificate))
    }

    fn load_bft_commit_history_page(
        &mut self,
        config: &NodeBftExecutionConfigV1,
        after_height: u64,
        max_entries: usize,
    ) -> Result<Vec<BftCommitCertificateV1>, String> {
        if self.evidence.config.as_ref() != Some(&PinnedTestConfig::from(config)) {
            return Err("test BFT history request differs from pinned config".into());
        }
        let start = usize::try_from(after_height)
            .map_err(|_| "test BFT history cursor overflow".to_string())?;
        Ok(self
            .evidence
            .certificates
            .iter()
            .skip(start)
            .take(max_entries)
            .cloned()
            .collect())
    }
}

fn fixed_keys() -> Vec<SigningKey> {
    (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect()
}

fn scope() -> BftScopeV1 {
    BftScopeV1 {
        world_id: WORLD_ID.into(),
        chain_id: CHAIN_ID.into(),
        genesis_hash: GENESIS_HASH.to_string(),
    }
}

fn validator_set(keys: &[SigningKey]) -> Vec<BftValidatorV1> {
    keys.iter()
        .enumerate()
        .map(|(index, key)| BftValidatorV1 {
            validator_id: format!("bft-v{index}"),
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            voting_power: 1,
        })
        .collect()
}

fn socket_addrs() -> Vec<SocketAddr> {
    (0..4)
        .map(|_| {
            UdpSocket::bind("127.0.0.1:0")
                .expect("reserve local test UDP address")
                .local_addr()
                .expect("test UDP address")
        })
        .collect()
}

fn make_runtime(
    index: usize,
    keys: &[SigningKey],
    addresses: &[SocketAddr],
    peers: Vec<SocketAddr>,
    root: &Path,
    newly_applied_heights: Arc<Mutex<Vec<u64>>>,
) -> NodeRuntime {
    let validator_id = format!("bft-v{index}");
    let key = &keys[index];
    let replication = NodeReplicationConfig::new(root.join(format!("node-{index}/replication")))
        .expect("replication config")
        .with_signing_keypair(
            hex::encode(key.to_bytes()),
            hex::encode(key.verifying_key().to_bytes()),
        )
        .expect("BFT node signer");
    let mut config = NodeConfig::new(validator_id.clone(), WORLD_ID, NodeRole::Sequencer)
        .expect("NodeRuntime config");
    config.player_id = "shared-bft-test-player".into();
    config = config
        .with_tick_interval(Duration::from_millis(10))
        .expect("short BFT test tick")
        .with_replication(replication)
        .with_gossip_optional(addresses[index], peers);
    let bft = NodeBftConfigV1 {
        scope: scope(),
        authority_epoch: 1,
        validators: validator_set(keys),
        local_validator_id: validator_id,
        timeouts: TEST_TIMEOUTS,
    };
    NodeRuntime::new(config)
        .with_execution_hook(DurableTestHook::open(
            root.join(format!("node-{index}/driver")),
            newly_applied_heights,
        ))
        .with_replication_network_consensus_enabled(false)
        .with_tendermint_v1(bft)
        .expect("fixed-set BFT mode")
}

fn all_peers_except(addresses: &[SocketAddr], index: usize) -> Vec<SocketAddr> {
    addresses
        .iter()
        .enumerate()
        .filter_map(|(peer_index, address)| (peer_index != index).then_some(*address))
        .collect()
}

fn wait_for_height(nodes: &[NodeRuntime], height: u64, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        let snapshots = nodes.iter().map(NodeRuntime::snapshot).collect::<Vec<_>>();
        let diagnostics = || {
            snapshots
                .iter()
                .zip(nodes)
                .map(|(snapshot, node)| {
                    let gossip = node.gossip_traffic_snapshot().map(|metrics| {
                        format!("totals={:?}, by_kind={:?}", metrics.totals, metrics.by_kind)
                    });
                    let wal_path = node.config.replication.as_ref().expect("replication").root_dir.join("tendermint-v1/tendermint-v1.wal.cbor");
                    let wal: crate::node_bft_wal::BftWalStateV1 = serde_cbor::from_slice(&fs::read(wal_path).expect("read diagnostic WAL")).expect("decode diagnostic WAL");
                    let events = wal.events.iter().map(|e| match e {
                        crate::node_bft_wal::BftWalEventV1::Proposal { proposal, local, .. } => format!("proposal:r{}:{}:local{}", proposal.header.round, proposal.header.proposer_id, local),
                        crate::node_bft_wal::BftWalEventV1::Vote { vote, local, .. } => format!("vote:r{}:{:?}:{}:{:?}:local{}", vote.round, vote.phase, vote.validator_id, vote.value, local),
                        crate::node_bft_wal::BftWalEventV1::Timeout { .. } => "timeout".into(),
                    }).collect::<Vec<_>>();
                    format!(
                        "{} running={} committed_height={} round/slot={} tick_count={} last_error={:?} gossip={gossip:?} wal_height={} wal_events={events:?}",
                        snapshot.node_id,
                        snapshot.running,
                        snapshot.consensus.committed_height,
                        snapshot.consensus.slot,
                        snapshot.tick_count,
                        snapshot.last_error,
                        wal.height,
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        };
        if snapshots.iter().any(|snapshot| !snapshot.running) {
            panic!(
                "node stopped while waiting for {label} at BFT height {height}: {}",
                diagnostics()
            );
        }
        if snapshots
            .iter()
            .all(|snapshot| snapshot.consensus.committed_height >= height)
        {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for {label} at BFT height {height}: {}",
                diagnostics()
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn submit_shared_action(nodes: &[NodeRuntime]) {
    let payload = serde_cbor::to_vec(&serde_json::json!({
        "kind": "bft-integration-noop"
    }))
    .expect("encode deterministic action payload");
    for node in nodes {
        node.submit_consensus_action_payload(ACTION_ID, payload.clone())
            .expect("queue the same deterministic action on participating nodes");
    }
}

fn load_evidence(root: &Path, index: usize) -> DurableTestEvidence {
    let path = root
        .join(format!("node-{index}/driver"))
        .join("bft-test-evidence.cbor");
    let bytes = fs::read(path).expect("read durable test driver history");
    serde_cbor::from_slice(&bytes).expect("decode durable test driver history")
}

#[test]
fn node_runtime_needs_quorum_and_late_fourth_node_catches_up_then_reopens() {
    let keys = fixed_keys();

    let quorum_root = TempRoot::new("q-minus-one");
    let quorum_addresses = socket_addrs();
    let mut two_nodes = (0..2)
        .map(|index| {
            make_runtime(
                index,
                &keys,
                &quorum_addresses,
                all_peers_except(&quorum_addresses, index),
                quorum_root.path(),
                Arc::new(Mutex::new(Vec::new())),
            )
        })
        .collect::<Vec<_>>();
    for node in &mut two_nodes {
        node.start().expect("start one of two sub-quorum BFT nodes");
    }
    thread::sleep(Duration::from_millis(900));
    for (index, node) in two_nodes.iter().enumerate() {
        let snapshot = node.snapshot();
        assert!(
            snapshot.running,
            "sub-quorum validator stopped: {snapshot:?}"
        );
        assert_eq!(snapshot.consensus.committed_height, 0);
        assert!(
            load_evidence(quorum_root.path(), index)
                .certificates
                .is_empty(),
            "sub-quorum node {index} must not persist a BFT certificate"
        );
    }
    for node in &mut two_nodes {
        node.stop().expect("stop sub-quorum BFT node");
    }

    let root = TempRoot::new("offline-catchup");
    let addresses = socket_addrs();
    let apply_counts = (0..4)
        .map(|_| Arc::new(Mutex::new(Vec::new())))
        .collect::<Vec<_>>();
    let mut nodes = (0..3)
        .map(|index| {
            make_runtime(
                index,
                &keys,
                &addresses,
                all_peers_except(&addresses, index),
                root.path(),
                Arc::clone(&apply_counts[index]),
            )
        })
        .collect::<Vec<_>>();
    submit_shared_action(&nodes);
    for node in &mut nodes {
        node.start().expect("start three-of-four BFT nodes");
    }
    wait_for_height(&nodes, 1, "three-validator quorum commit");

    // Stop two replicas. A single node remains as a history source, so the
    // fourth validator can recover a real QC but cannot form a new quorum yet.
    nodes[1].stop().expect("partition validator one");
    nodes[2].stop().expect("partition validator two");
    let server_height = nodes[0].snapshot().consensus.committed_height;
    assert!(server_height >= 1);
    let expected_tip = load_evidence(root.path(), 0)
        .certificates
        .last()
        .expect("server committed history")
        .proposal
        .block
        .block_id()
        .expect("server BFT head");

    let mut late_node = make_runtime(
        3,
        &keys,
        &addresses,
        vec![addresses[0]],
        root.path(),
        Arc::clone(&apply_counts[3]),
    );
    late_node.start().expect("start offline fourth validator");
    wait_for_height(
        std::slice::from_ref(&late_node),
        server_height,
        "fourth-node history catch-up",
    );
    assert_eq!(
        load_evidence(root.path(), 3)
            .certificates
            .last()
            .expect("late validator recovered history")
            .proposal
            .block
            .block_id()
            .expect("late validator BFT head"),
        expected_tip,
        "catch-up must install the exact serving QC chain tip"
    );
    let recovered_actions: Vec<crate::NodeConsensusAction> = serde_cbor::from_slice(
        &load_evidence(root.path(), 3)
            .certificates
            .first()
            .expect("first recovered QC")
            .proposal
            .block
            .actions_cbor,
    )
    .expect("decode recovered application actions");
    assert_eq!(recovered_actions.len(), 1);
    assert_eq!(recovered_actions[0].action_id, ACTION_ID);
    let late_applied = apply_counts[3].lock().expect("late apply counter");
    assert_eq!(
        late_applied.iter().filter(|height| **height == 1).count(),
        1
    );
    assert_eq!(late_applied.len() as u64, server_height);
    drop(late_applied);

    nodes[1].start().expect("reopen first active validator");
    nodes[2].start().expect("reopen second active validator");
    let target_height = server_height.saturating_add(1);
    wait_for_height(
        &nodes,
        target_height,
        "all validators resume after catch-up",
    );
    wait_for_height(
        std::slice::from_ref(&late_node),
        target_height,
        "late validator joins the next height",
    );

    for (index, apply_count) in apply_counts.iter().enumerate() {
        let evidence = load_evidence(root.path(), index);
        assert!(evidence.certificates.len() >= target_height as usize);
        assert_eq!(
            evidence.certificates[target_height as usize - 1]
                .proposal
                .block
                .block_id()
                .expect("committed BFT block at resumed height"),
            load_evidence(root.path(), 0).certificates[target_height as usize - 1]
                .proposal
                .block
                .block_id()
                .expect("canonical BFT block at resumed height")
        );
        for height in 1..=target_height {
            assert_eq!(
                apply_count
                    .lock()
                    .expect("active apply counter")
                    .iter()
                    .filter(|applied| **applied == height)
                    .count(),
                1,
                "node {index} must apply height {height} once"
            );
        }
    }

    for node in &mut nodes {
        node.stop().expect("stop active BFT node");
    }
    late_node.stop().expect("stop late BFT node");

    let before_restart = apply_counts[3]
        .lock()
        .expect("late apply counter before restart")
        .clone();
    let mut cold_reopen = make_runtime(
        3,
        &keys,
        &addresses,
        vec![addresses[0]],
        root.path(),
        Arc::clone(&apply_counts[3]),
    );
    cold_reopen
        .start()
        .expect("reopen fourth-node durable WAL and history");
    wait_for_height(
        std::slice::from_ref(&cold_reopen),
        target_height,
        "fourth-node cold replay",
    );
    assert_eq!(
        *apply_counts[3]
            .lock()
            .expect("late apply counter after restart"),
        before_restart,
        "exact raw-QC replay must not apply already durable BFT effects twice"
    );
    cold_reopen.stop().expect("stop cold-reopened BFT node");
}
