use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v1::{BftHeightProposerScheduleV1, BftTimeoutsV1, BftValidatorV1};
use oasis7_proto::distributed_bft::{BftBlockV1, BftScopeV1};
use serde_json::Value;

use super::super::*;
use crate::{
    NodeBftBuildContextV1, NodeBftExecutionContextV1, NodeExecutionCommitContext,
    NodeExecutionCommitResult, NodeExecutionHook, NodeRole, NodeRuntime,
    compute_consensus_action_root,
};

const WORLD_ID: &str = "bft-local-queue-test-world";
const VALIDATOR_ID: &str = "bft-local-queue-test-validator";
const GENESIS_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const MANIFEST_HASH: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const FIXED_ROOT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let serial = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-bft-local-queue-{}-{serial}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test root");
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

struct ProposalHook {
    governance_authorized: Arc<AtomicBool>,
}

impl ProposalHook {
    fn block(
        context: &NodeBftExecutionContextV1,
        actions: &[crate::NodeConsensusAction],
    ) -> Result<BftBlockV1, String> {
        Ok(BftBlockV1 {
            scope: context.scope.clone(),
            height: context.height,
            parent_commit_ref: context.parent_commit_ref.clone(),
            authority_epoch: context.authority_epoch,
            manifest_hash: context.manifest_hash.clone(),
            actions_cbor: serde_cbor::to_vec(&actions).map_err(|error| error.to_string())?,
            action_root: compute_consensus_action_root(actions)
                .map_err(|error| error.to_string())?,
            event_root: FIXED_ROOT.into(),
            execution_block_hash: FIXED_ROOT.into(),
            execution_state_root: FIXED_ROOT.into(),
            state_root: FIXED_ROOT.into(),
        })
    }
}

impl NodeExecutionHook for ProposalHook {
    fn validate_governance_action_payload(&self, _payload: &[u8]) -> Result<(), String> {
        if self.governance_authorized.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err("governance is no longer authorized by the current parent".into())
        }
    }

    fn on_commit(
        &mut self,
        _context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("the local queue fixture never applies a commit".into())
    }

    fn validate_bft_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        serde_cbor::from_slice::<Value>(payload)
            .map(|_| ())
            .map_err(|error| format!("invalid deterministic test action: {error}"))
    }

    fn current_bft_execution_context(
        &mut self,
        scope: &BftScopeV1,
        height: u64,
        parent_commit_ref: &str,
        authority_epoch: u64,
    ) -> Result<NodeBftExecutionContextV1, String> {
        if scope.world_id != WORLD_ID || height != 1 || parent_commit_ref != GENESIS_HASH {
            return Err("local queue test context changed unexpectedly".into());
        }
        Ok(NodeBftExecutionContextV1 {
            scope: scope.clone(),
            height,
            parent_commit_ref: parent_commit_ref.to_string(),
            authority_epoch,
            manifest_hash: MANIFEST_HASH.into(),
        })
    }

    fn build_bft_candidate(
        &mut self,
        context: NodeBftBuildContextV1,
    ) -> Result<BftBlockV1, String> {
        Self::block(&context.execution, context.actions.as_slice())
    }

    fn preview_bft_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV1,
        candidate: &BftBlockV1,
    ) -> Result<(), String> {
        let actions: Vec<crate::NodeConsensusAction> =
            serde_cbor::from_slice(&candidate.actions_cbor).map_err(|error| error.to_string())?;
        if candidate != &Self::block(expected, actions.as_slice())? {
            return Err("local queue candidate differs from deterministic preview".into());
        }
        Ok(())
    }
}

fn bft_config(key: &SigningKey) -> super::NodeBftConfigV1 {
    super::NodeBftConfigV1 {
        scope: BftScopeV1 {
            world_id: WORLD_ID.into(),
            chain_id: "bft-local-queue-test-chain".into(),
            genesis_hash: GENESIS_HASH.into(),
        },
        authority_epoch: 1,
        validators: vec![BftValidatorV1 {
            validator_id: VALIDATOR_ID.into(),
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            voting_power: 1,
        }],
        local_validator_id: VALIDATOR_ID.into(),
        timeouts: BftTimeoutsV1 {
            propose_ms: 10_000,
            prevote_ms: 10_000,
            precommit_ms: 10_000,
            round_delta_ms: 1_000,
        },
    }
}

fn ordinary_action(id: u64, marker: &str) -> crate::NodeConsensusAction {
    action(
        id,
        serde_json::json!({"kind": "closed-action", "marker": marker}),
    )
}

fn governance_action(id: u64, nonce: u64) -> crate::NodeConsensusAction {
    action(
        id,
        serde_json::json!({
            "version": 1,
            "auth": {"type": "governance_action", "data": {
                "schema_version": 1,
                "controller_account_id": "controller",
                "nonce": nonce,
                "signatures": {"test-key": "test-signature"}
            }},
            "body": {"type": "runtime_action", "data": {"action": {
                "type": "ApproveGovernanceValidatorAdmission",
                "data": {"candidate_id": format!("candidate-{id}")}
            }}}
        }),
    )
}

fn action(id: u64, value: Value) -> crate::NodeConsensusAction {
    crate::NodeConsensusAction::from_payload(
        id,
        "bft-local-queue-test-player",
        serde_cbor::to_vec(&value).expect("encode action"),
    )
    .expect("valid action")
}

fn node_config() -> crate::NodeConfig {
    let mut config =
        crate::NodeConfig::new(VALIDATOR_ID, WORLD_ID, NodeRole::Sequencer).expect("node config");
    config.player_id = "bft-local-queue-test-player".into();
    config
}

fn worker(
    root: &TempRoot,
    actions: Vec<crate::NodeConsensusAction>,
    authorized: Arc<AtomicBool>,
) -> Worker {
    let key = SigningKey::from_bytes(&[71; 32]);
    let bft = bft_config(&key);
    let set = bft.validate(WORLD_ID).expect("fixed validator set");
    let schedule =
        BftHeightProposerScheduleV1::genesis(&set, bft.authority_epoch).expect("genesis schedule");
    let wal = BftWalStore::open_for_recovery(
        &root.path().join("replication/tendermint-v1"),
        &schedule,
        VALIDATOR_ID,
        hex::encode(key.verifying_key().to_bytes()).as_str(),
        false,
    )
    .expect("open BFT WAL");
    let bytes = actions.iter().map(|action| action.payload_cbor.len()).sum();
    Worker::restore(WorkerSetup {
        config: node_config(),
        bft,
        set,
        schedule,
        key,
        hook: Arc::new(Mutex::new(Box::new(ProposalHook {
            governance_authorized: authorized,
        }))),
        wal,
        pending: Arc::new(Mutex::new(actions)),
        pending_bytes: Arc::new(AtomicUsize::new(bytes)),
        state: Arc::new(Mutex::new(crate::node_runtime_core::RuntimeState::default())),
        tip: None,
    })
    .expect("restore local worker")
}

fn local_proposal_actions(worker: &Worker) -> Vec<crate::NodeConsensusAction> {
    let proposal = worker
        .setup
        .wal
        .state()
        .events
        .iter()
        .find_map(|event| match event {
            BftWalEventV1::Proposal {
                proposal,
                local: true,
                ..
            } => Some(proposal),
            _ => None,
        })
        .expect("worker persisted a local proposal");
    serde_cbor::from_slice(&proposal.block.actions_cbor).expect("decode proposal actions")
}

#[test]
fn maybe_propose_sorts_actions_and_collapses_exact_duplicates() {
    let root = TempRoot::new();
    let authorized = Arc::new(AtomicBool::new(true));
    let low = governance_action(10, 1);
    let high = governance_action(20, 2);
    let mut worker = worker(
        &root,
        vec![high.clone(), low.clone(), high.clone()],
        authorized,
    );
    let original_bytes = high.payload_cbor.len() * 2 + low.payload_cbor.len();

    worker
        .maybe_propose(None)
        .expect("valid local queue proposal");

    let actions = local_proposal_actions(&worker);
    assert_eq!(actions, vec![low.clone(), high.clone()]);
    assert_eq!(
        worker
            .setup
            .pending
            .lock()
            .expect("pending queue")
            .as_slice(),
        [low.clone(), high.clone()]
    );
    assert_eq!(
        worker.setup.pending_bytes.load(Ordering::Acquire),
        low.payload_cbor.len() + high.payload_cbor.len()
    );
    assert!(original_bytes > worker.setup.pending_bytes.load(Ordering::Acquire));
    let proposal = worker
        .setup
        .wal
        .state()
        .events
        .iter()
        .find_map(|event| match event {
            BftWalEventV1::Proposal {
                proposal,
                local: true,
                ..
            } => Some(proposal),
            _ => None,
        })
        .expect("durable local proposal");
    assert_eq!(
        proposal.block.action_root,
        compute_consensus_action_root(&actions).unwrap()
    );
}

#[test]
fn maybe_propose_respects_engine_batch_limit_without_dropping_queued_actions() {
    let root = TempRoot::new();
    let first = ordinary_action(2, "first");
    let second = ordinary_action(9, "second");
    let expected_bytes = first.payload_cbor.len() + second.payload_cbor.len();
    let mut worker = worker(
        &root,
        vec![second.clone(), first.clone()],
        Arc::new(AtomicBool::new(true)),
    );
    worker.setup.config.max_engine_pending_consensus_actions = 1;

    worker
        .maybe_propose(None)
        .expect("one bounded proposal from a larger valid queue");

    assert_eq!(local_proposal_actions(&worker), vec![first.clone()]);
    assert_eq!(
        worker
            .setup
            .pending
            .lock()
            .expect("pending queue")
            .as_slice(),
        [first, second]
    );
    assert_eq!(
        worker.setup.pending_bytes.load(Ordering::Acquire),
        expected_bytes
    );
}

#[test]
fn stale_governance_is_removed_and_local_worker_still_proposes() {
    let root = TempRoot::new();
    let authorized = Arc::new(AtomicBool::new(true));
    let stale = governance_action(4, 1);
    let normal = ordinary_action(8, "keep");
    let mut worker = worker(
        &root,
        vec![stale.clone(), normal.clone()],
        Arc::clone(&authorized),
    );
    authorized.store(false, Ordering::Release);

    worker
        .maybe_propose(None)
        .expect("stale authorization is a local rejection, not a worker failure");

    assert_eq!(local_proposal_actions(&worker), vec![normal.clone()]);
    assert_eq!(
        worker
            .setup
            .pending
            .lock()
            .expect("pending queue")
            .as_slice(),
        std::slice::from_ref(&normal)
    );
    assert_eq!(
        worker.setup.pending_bytes.load(Ordering::Acquire),
        normal.payload_cbor.len()
    );
    let state = worker.setup.state.lock().expect("runtime status");
    let status = state.bft_status.as_ref().expect("BFT status");
    assert_eq!(status.rejected_local_actions, 1);
    assert!(
        status
            .last_local_action_rejection
            .as_deref()
            .is_some_and(|reason| reason.contains("4"))
    );
}

#[test]
fn preexisting_conflicting_action_ids_fail_before_local_proposal_mutation() {
    let root = TempRoot::new();
    let actions = vec![ordinary_action(3, "first"), ordinary_action(3, "different")];
    let expected_bytes: usize = actions.iter().map(|action| action.payload_cbor.len()).sum();
    let mut worker = worker(&root, actions.clone(), Arc::new(AtomicBool::new(true)));

    let error = worker
        .maybe_propose(None)
        .expect_err("conflicting preexisting IDs must fail closed");

    assert!(
        error
            .to_string()
            .contains("conflicting preexisting BFT queued actions")
    );
    assert_eq!(
        worker
            .setup
            .pending
            .lock()
            .expect("pending queue")
            .as_slice(),
        actions
    );
    assert_eq!(
        worker.setup.pending_bytes.load(Ordering::Acquire),
        expected_bytes
    );
    assert!(worker.setup.wal.state().events.is_empty());
}

#[test]
fn bft_ingress_is_idempotent_and_start_rejects_legacy_queue_conflicts() {
    let key = SigningKey::from_bytes(&[72; 32]);
    let authorized = Arc::new(AtomicBool::new(true));
    let hook = || ProposalHook {
        governance_authorized: Arc::clone(&authorized),
    };
    let configured = NodeRuntime::new(node_config())
        .with_execution_hook(hook())
        .with_tendermint_v1(bft_config(&key))
        .expect("select BFT");
    let first = ordinary_action(5, "first");
    configured
        .submit_consensus_action_payload(5, first.payload_cbor.clone())
        .expect("queue first BFT action");
    configured
        .submit_consensus_action_payload(5, first.payload_cbor.clone())
        .expect("exact BFT retry is idempotent");
    let before = configured
        .pending_consensus_actions
        .lock()
        .expect("queue")
        .clone();
    let before_bytes = configured
        .pending_consensus_action_queue_bytes
        .load(Ordering::Acquire);
    assert_eq!(before.as_slice(), std::slice::from_ref(&first));
    assert!(
        configured
            .submit_consensus_action_payload(5, ordinary_action(5, "conflict").payload_cbor)
            .is_err()
    );
    assert_eq!(
        configured
            .pending_consensus_actions
            .lock()
            .expect("queue")
            .as_slice(),
        before
    );
    assert_eq!(
        configured
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        before_bytes
    );

    let mut legacy = NodeRuntime::new(node_config()).with_execution_hook(hook());
    legacy
        .submit_consensus_action_payload(9, ordinary_action(9, "legacy-a").payload_cbor)
        .expect("legacy queue first action");
    legacy
        .submit_consensus_action_payload(9, ordinary_action(9, "legacy-b").payload_cbor)
        .expect("legacy queue preserves prior behavior");
    let legacy_before = legacy
        .pending_consensus_actions
        .lock()
        .expect("legacy queue")
        .clone();
    let legacy_bytes = legacy
        .pending_consensus_action_queue_bytes
        .load(Ordering::Acquire);
    legacy.bft_config = Some(bft_config(&key));

    let error = legacy
        .start()
        .expect_err("BFT startup rejects preexisting conflicting IDs");

    assert!(
        error
            .to_string()
            .contains("conflicting preexisting BFT queued actions")
    );
    assert!(legacy.worker.is_none());
    assert_eq!(
        legacy
            .pending_consensus_actions
            .lock()
            .expect("legacy queue")
            .as_slice(),
        legacy_before
    );
    assert_eq!(
        legacy
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        legacy_bytes
    );
}
