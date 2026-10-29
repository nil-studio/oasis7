use serde::{Deserialize, Serialize};

use oasis7_consensus::tendermint_v1::{
    BftValidatorSetV1, BftValidatorV1, VerifiedBftCommitCertificateV1,
};
pub use oasis7_proto::distributed_bft::BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1;
use oasis7_proto::distributed_bft::{BftBlockV1, BftCommitCertificateV1, BftScopeV1};

use crate::NodeConsensusAction;

#[path = "execution_hook_bft_v2.rs"]
mod bft_v2;
#[path = "execution_hook_bft_v3.rs"]
mod bft_v3;
pub use bft_v2::*;
pub use bft_v3::*;

/// Reserved consensus action identity for execution inputs that are authored
/// by the block proposer and then replayed by every materializing node.
///
/// These inputs deliberately travel through `NodeConsensusAction`: the action
/// payload is covered by the consensus action root and by the replicated
/// commit message.  A node-local file can therefore only provide an input to
/// its own proposal; it cannot mutate a committed block after the fact.
pub const REPLICATED_EXECUTION_INPUT_ACTION_ID: u64 = u64::MAX;
pub const REPLICATED_EXECUTION_INPUT_SUBMITTER: &str = "system:replicated-execution";
pub const REPLICATED_EXECUTION_INPUT_VERSION: u8 = 1;
pub const PROVIDER_BACKED_BOOTSTRAP_EXECUTION_INPUT_KIND: &str = "provider_backed_bootstrap.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeReplicatedExecutionInputV1 {
    pub version: u8,
    pub kind: String,
    /// Zero is an unbound producer-side input. The node binds it to its next
    /// canonical proposal height before the worker starts. Replicated inputs
    /// must carry the concrete committed height when they reach a hook.
    pub target_height: u64,
    /// Kind-specific canonical CBOR payload. The chain runtime owns decoding
    /// this opaque field; the node layer only transports and commits it.
    pub payload_cbor: Vec<u8>,
}

impl NodeReplicatedExecutionInputV1 {
    pub fn new(kind: impl Into<String>, target_height: u64, payload_cbor: Vec<u8>) -> Self {
        Self {
            version: REPLICATED_EXECUTION_INPUT_VERSION,
            kind: kind.into(),
            target_height,
            payload_cbor,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.version != REPLICATED_EXECUTION_INPUT_VERSION {
            return Err(format!(
                "unsupported replicated execution input version {}",
                self.version
            ));
        }
        if self.kind.trim().is_empty() {
            return Err("replicated execution input kind cannot be empty".to_string());
        }
        if self.payload_cbor.is_empty() {
            return Err("replicated execution input payload cannot be empty".to_string());
        }
        serde_cbor::to_vec(self)
            .map_err(|err| format!("encode replicated execution input failed: {err}"))
    }

    /// Decode only the reserved input envelope. Ordinary runtime/simulator
    /// action payloads return `Ok(None)` so existing action admission remains
    /// unchanged.
    pub fn decode(bytes: &[u8]) -> Result<Option<Self>, String> {
        let Ok(input) = serde_cbor::from_slice::<Self>(bytes) else {
            return Ok(None);
        };
        if input.version != REPLICATED_EXECUTION_INPUT_VERSION {
            return Err(format!(
                "unsupported replicated execution input version {}",
                input.version
            ));
        }
        if input.kind.trim().is_empty() {
            return Err("replicated execution input kind cannot be empty".to_string());
        }
        if input.payload_cbor.is_empty() {
            return Err("replicated execution input payload cannot be empty".to_string());
        }
        Ok(Some(input))
    }

    pub fn bind_target_height(&mut self, target_height: u64) -> Result<(), String> {
        if target_height == 0 {
            return Err("replicated execution input target height must be > 0".to_string());
        }
        if self.target_height == 0 {
            self.target_height = target_height;
        } else if self.target_height != target_height {
            return Err(format!(
                "replicated execution input target height mismatch: expected={} actual={}",
                target_height, self.target_height
            ));
        }
        Ok(())
    }
}

pub fn decode_replicated_execution_input_action(
    action: &NodeConsensusAction,
) -> Result<Option<NodeReplicatedExecutionInputV1>, String> {
    if action.action_id != REPLICATED_EXECUTION_INPUT_ACTION_ID
        || action.submitter_player_id != REPLICATED_EXECUTION_INPUT_SUBMITTER
    {
        return Ok(None);
    }
    NodeReplicatedExecutionInputV1::decode(action.payload_cbor.as_slice())
}

/// Validate the reserved execution-input identity and its committed height.
/// A mismatched reserved identity must be rejected as malformed input instead
/// of falling through to ordinary runtime-action decoding.
pub fn validate_replicated_execution_input_actions(
    actions: &[NodeConsensusAction],
    expected_height: u64,
) -> Result<(), String> {
    let mut found_input = false;
    for action in actions {
        let reserved_identity = action.action_id == REPLICATED_EXECUTION_INPUT_ACTION_ID
            || action.submitter_player_id == REPLICATED_EXECUTION_INPUT_SUBMITTER;
        if !reserved_identity {
            continue;
        }
        let Some(input) = decode_replicated_execution_input_action(action)? else {
            return Err(format!(
                "reserved replicated execution input identity is malformed action_id={} submitter={}",
                action.action_id, action.submitter_player_id
            ));
        };
        if found_input {
            return Err(format!(
                "multiple replicated execution inputs are not allowed at height {}",
                expected_height
            ));
        }
        found_input = true;
        if input.target_height == 0 {
            return Err(
                "committed replicated execution input must be bound to a height".to_string(),
            );
        }
        if expected_height != 0 && input.target_height != expected_height {
            return Err(format!(
                "replicated execution input height mismatch: expected={} actual={}",
                expected_height, input.target_height
            ));
        }
    }
    Ok(())
}

pub fn bind_replicated_execution_input_action(
    action: &mut NodeConsensusAction,
    target_height: u64,
) -> Result<bool, String> {
    let Some(mut input) = decode_replicated_execution_input_action(action)? else {
        return Ok(false);
    };
    input.bind_target_height(target_height)?;
    let payload_cbor = input.encode()?;
    *action = NodeConsensusAction::from_payload(
        REPLICATED_EXECUTION_INPUT_ACTION_ID,
        REPLICATED_EXECUTION_INPUT_SUBMITTER,
        payload_cbor,
    )
    .map_err(|err| format!("rebuild replicated execution input action failed: {err}"))?;
    Ok(true)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeExecutionCommitContext {
    pub world_id: String,
    pub node_id: String,
    pub proposer_id: String,
    pub height: u64,
    pub slot: u64,
    pub epoch: u64,
    pub node_block_hash: String,
    pub action_root: String,
    #[serde(default)]
    pub committed_actions: Vec<NodeConsensusAction>,
    pub committed_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeExecutionCommitResult {
    pub execution_height: u64,
    pub execution_block_hash: String,
    pub execution_state_root: String,
}

/// Runtime identity and predecessor that every BFT proposal must execute
/// against. The driver compares these values with its live parent before it
/// previews or applies a candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionContextV1 {
    pub scope: BftScopeV1,
    pub height: u64,
    pub parent_commit_ref: String,
    pub authority_epoch: u64,
    pub manifest_hash: String,
}

/// The fixed configuration the execution driver must durably pin before the
/// NodeRuntime restores BFT history or opens its network listener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionConfigV1 {
    pub scope: BftScopeV1,
    pub authority_epoch: u64,
    pub validators: Vec<BftValidatorV1>,
    pub validator_set_hash: String,
}

impl NodeBftExecutionConfigV1 {
    pub fn from_validator_set(set: &BftValidatorSetV1, authority_epoch: u64) -> Self {
        Self {
            scope: set.scope().clone(),
            authority_epoch,
            validators: set.validators().values().cloned().collect(),
            validator_set_hash: set.set_hash().to_string(),
        }
    }

    pub fn validate(&self) -> Result<BftValidatorSetV1, String> {
        let set = BftValidatorSetV1::new(self.scope.clone(), self.validators.clone())?;
        if set.set_hash() != self.validator_set_hash {
            return Err("BFT execution config validator set hash mismatch".into());
        }
        Ok(set)
    }
}

/// Durable execution head reported only after the fixed BFT configuration has
/// been checked and pinned. NodeRuntime independently replays every raw
/// certificate from the configured genesis through this exact head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionHeadV1 {
    pub height: u64,
    pub block_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftBuildContextV1 {
    pub execution: NodeBftExecutionContextV1,
    pub actions: Vec<NodeConsensusAction>,
}

/// Result returned only after the execution driver has atomically installed
/// the certificate's deterministic execution result and BFT-specific proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftExecutionCommitResultV1 {
    pub height: u64,
    pub bft_commit_ref: String,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub state_root: String,
}

/// Local execution may wait for durable evidence after consensus has decided.
/// Deferred does not revoke that decision or authorize a replacement proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeExecutionCommitOutcome {
    Applied(NodeExecutionCommitResult),
    Deferred,
}

/// Exact local decision continuation captured by the Node before guarded
/// execution. This is local recovery material, never a network finality proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeLocalExecutionContinuation {
    pub schema_version: u32,
    pub context: NodeExecutionCommitContext,
    pub predecessor_committed_height: u64,
    pub predecessor_node_block_hash: Option<String>,
    pub predecessor_execution_height: u64,
    pub predecessor_execution_block_hash: Option<String>,
    pub predecessor_execution_state_root: Option<String>,
    pub next_height: u64,
    pub next_slot: u64,
    pub reserved_action_bytes: usize,
    pub approved_stake: u64,
    pub rejected_stake: u64,
    pub required_stake: u64,
    pub total_stake: u64,
}

/// A durable execution boundary established by an explicit local bootstrap.
///
/// The boundary is derived from the persisted runtime world and is not a
/// consensus block. It lets a DevLocal chain whose setup ticks already
/// advanced the world start its first consensus proposal at the successor
/// height without weakening committed execution validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeExecutionBootstrap {
    pub height: u64,
    pub consensus_block_hash: String,
    pub execution_block_hash: String,
    pub execution_state_root: String,
}

/// A genesis-only boundary issued by an execution hook after it has verified
/// the qualified InitialActivation for this exact world. Unlike
/// `NodeExecutionBootstrap`, this capability is restricted to height zero and
/// carries the immutable initial record identity so startup can revalidate it
/// after a stop/restart. Implementations must never construct one from a raw
/// service pin or unsigned bootstrap DTO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeExecutionGenesisBootstrap {
    world_id: String,
    initial_record_id: String,
    boundary: NodeExecutionBootstrap,
}

impl NodeExecutionGenesisBootstrap {
    /// Construct a zero-height capability only after independently verifying
    /// the corresponding InitialActivation proof and World snapshot.
    pub fn from_qualified_initial_activation(
        world_id: impl Into<String>,
        initial_record_id: impl Into<String>,
        boundary: NodeExecutionBootstrap,
    ) -> Result<Self, String> {
        let world_id = world_id.into();
        let initial_record_id = initial_record_id.into();
        if world_id.trim().is_empty()
            || initial_record_id.trim().is_empty()
            || boundary.height != 0
            || boundary.consensus_block_hash.trim().is_empty()
            || boundary.execution_block_hash.trim().is_empty()
            || boundary.execution_state_root.trim().is_empty()
        {
            return Err("qualified Node genesis bootstrap is incomplete or nonzero".into());
        }
        Ok(Self {
            world_id,
            initial_record_id,
            boundary,
        })
    }

    pub fn world_id(&self) -> &str {
        &self.world_id
    }

    pub fn initial_record_id(&self) -> &str {
        &self.initial_record_id
    }

    pub fn boundary(&self) -> &NodeExecutionBootstrap {
        &self.boundary
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeExecutionCheckpointBlob {
    pub content_hash: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeExecutionCheckpointBlobRef {
    pub content_hash: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeExecutionCheckpointDescriptor {
    pub height: u64,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub manifest_ref: String,
    pub manifest_size_bytes: u64,
    #[serde(default)]
    pub blobs: Vec<NodeExecutionCheckpointBlobRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeExecutionCheckpointBundle {
    pub height: u64,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub manifest_json: Vec<u8>,
    #[serde(default)]
    pub blobs: Vec<NodeExecutionCheckpointBlob>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeExecutionCheckpointInstallContext {
    pub world_id: String,
    pub node_id: String,
    pub height: u64,
    pub node_block_hash: String,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub committed_at_unix_ms: i64,
}

pub trait NodeExecutionHook: Send {
    /// Check governance against the current immutable qualified parent before enqueueing.
    /// Unsupported hooks fail closed; execution must verify again at commit time.
    fn validate_governance_action_payload(&self, _payload_cbor: &[u8]) -> Result<(), String> {
        Err("execution hook governance authorization preflight unsupported".into())
    }

    /// Advance one storage maintenance page without producing a business tick.
    /// NodeRuntime exposes this only after its worker has completely stopped.
    /// Completion requires a full scan with no pending work or errors.
    fn advance_stopped_storage_maintenance(&mut self) -> Result<bool, String> {
        Err("execution hook does not support stopped storage maintenance".into())
    }

    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String>;

    fn on_commit_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        _expected_execution_block_hash: Option<&str>,
        _expected_execution_state_root: Option<&str>,
    ) -> Result<NodeExecutionCommitResult, String> {
        self.on_commit(context)
    }

    /// Recover the timestamp of an already durable local decision only after
    /// verifying its complete identity. Network and guarded execution do not
    /// use this seam; ordinary hooks without retained history return None.
    fn retained_local_commit_time(
        &mut self,
        _context: &NodeExecutionCommitContext,
    ) -> Result<Option<i64>, String> {
        Ok(None)
    }

    /// Opt-in engineering seam. Its captured local decision must be durable
    /// before candidate execution begins. Ordinary/network hooks do not opt in.
    fn guarded_local_execution(&self) -> bool {
        false
    }

    /// Return a genesis capability only while this hook can re-verify the same
    /// qualified InitialActivation. The default deliberately denies genesis
    /// bootstrap, including for ordinary guarded local hooks.
    fn qualified_local_execution_genesis_bootstrap(
        &mut self,
    ) -> Result<Option<NodeExecutionGenesisBootstrap>, String> {
        Ok(None)
    }

    fn prepare_local_continuation(
        &mut self,
        _continuation: &NodeLocalExecutionContinuation,
    ) -> Result<(), String> {
        Err("guarded local execution continuation is unsupported".into())
    }

    fn pending_local_continuation(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        Ok(None)
    }

    /// Called only after the original decision has been locally applied.
    fn complete_local_continuation(
        &mut self,
        _context: &NodeExecutionCommitContext,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Ordinary hooks remain synchronous. Guarded engineering hooks override
    /// this seam and retain the exact original candidate while waiting.
    fn on_commit_outcome_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        expected_execution_block_hash: Option<&str>,
        expected_execution_state_root: Option<&str>,
    ) -> Result<NodeExecutionCommitOutcome, String> {
        self.on_commit_with_expected(
            context,
            expected_execution_block_hash,
            expected_execution_state_root,
        )
        .map(NodeExecutionCommitOutcome::Applied)
    }

    fn restore_to_height(&mut self, _world_id: &str, _height: u64) -> Result<bool, String> {
        Ok(false)
    }

    fn export_checkpoint_bundle(
        &mut self,
        _height: u64,
    ) -> Result<Option<NodeExecutionCheckpointBundle>, String> {
        Ok(None)
    }

    fn install_checkpoint_bundle(
        &mut self,
        _context: NodeExecutionCheckpointInstallContext,
        _bundle: NodeExecutionCheckpointBundle,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("execution checkpoint install is not supported by this hook".to_string())
    }

    /// Build a complete BFT block from ordered actions using an isolated,
    /// deterministic preview. Implementations must not publish live effects.
    fn build_bft_candidate(
        &mut self,
        _context: NodeBftBuildContextV1,
    ) -> Result<BftBlockV1, String> {
        Err("BFT candidate construction is unsupported by this execution hook".into())
    }

    /// Independently preview a received BFT block against the exact local
    /// world, manifest, and parent. `Ok(())` means every execution root matched;
    /// it must not modify the canonical world or journal.
    fn preview_bft_candidate(
        &mut self,
        _expected: &NodeBftExecutionContextV1,
        _candidate: &BftBlockV1,
    ) -> Result<(), String> {
        Err("BFT candidate preview is unsupported by this execution hook".into())
    }

    /// Validate one action payload before BFT queue admission. This check is
    /// independent of configuration and World state; implementations must
    /// reject payload classes the deterministic BFT executor cannot replay.
    fn validate_bft_action_payload(&mut self, _payload: &[u8]) -> Result<(), String> {
        Err("BFT action payload admission is unsupported by this execution hook".into())
    }

    /// Apply or recover one fully verified fixed-set BFT certificate. The
    /// opaque token is produced only by the consensus verifier; implementations
    /// must persist BFT evidence separately from legacy PoS proof and make an
    /// exact retry idempotent.
    fn commit_bft_certificate(
        &mut self,
        _certificate: VerifiedBftCommitCertificateV1,
    ) -> Result<NodeBftExecutionCommitResultV1, String> {
        Err("BFT certificate execution is unsupported by this execution hook".into())
    }

    /// Persistently bind the exact local BFT scope, fixed set, and epoch. This
    /// runs before recovery and before any listener is opened; ordinary hooks
    /// fail closed until they implement a separate durable BFT evidence store.
    fn configure_bft_execution(
        &mut self,
        _config: &NodeBftExecutionConfigV1,
    ) -> Result<Option<NodeBftExecutionHeadV1>, String> {
        Err("durable BFT execution configuration is unsupported by this hook".into())
    }

    /// Return the execution driver's current authenticated context for the
    /// requested next height. In particular, `manifest_hash` must come from
    /// the installed canonical World, never from a proposal or caller.
    fn current_bft_execution_context(
        &mut self,
        _scope: &BftScopeV1,
        _height: u64,
        _parent_commit_ref: &str,
        _authority_epoch: u64,
    ) -> Result<NodeBftExecutionContextV1, String> {
        Err("BFT execution context lookup is unsupported by this hook".into())
    }

    /// Return a bounded, height-ordered page of raw evidence after an exclusive
    /// cursor. The NodeRuntime verifies the full contiguous chain itself and
    /// rejects pages larger than `max_entries` or with missing heights.
    fn load_bft_commit_history_page(
        &mut self,
        _config: &NodeBftExecutionConfigV1,
        _after_height: u64,
        _max_entries: usize,
    ) -> Result<Vec<BftCommitCertificateV1>, String> {
        Err("durable BFT commit history is unsupported by this execution hook".into())
    }

    /// Pin a distinct V2 execution ledger and replay it from its exact genesis.
    fn configure_bft_v2_execution(
        &mut self,
        _config: &NodeBftExecutionConfigV2,
    ) -> Result<Option<NodeBftExecutionHeadV2>, String> {
        Err("dynamic-set BFT execution is unsupported by this hook".into())
    }

    /// Compare this protocol cursor with the driver's independently recovered
    /// cursor before deriving any execution context from the canonical World.
    fn current_bft_v2_execution_context(
        &mut self,
        _schedule: &oasis7_consensus::tendermint_v2::BftHeightProposerScheduleV2,
    ) -> Result<NodeBftExecutionContextV2, String> {
        Err("dynamic-set BFT context is unsupported by this hook".into())
    }

    fn build_bft_v2_candidate(
        &mut self,
        _context: NodeBftBuildContextV2,
    ) -> Result<oasis7_proto::distributed_bft::v2::BftBlockV2, String> {
        Err("dynamic-set BFT construction is unsupported by this hook".into())
    }

    /// Independently reexecute actions and recompute the complete transition.
    fn preview_bft_v2_candidate(
        &mut self,
        _expected: &NodeBftExecutionContextV2,
        _candidate: &oasis7_proto::distributed_bft::v2::BftBlockV2,
    ) -> Result<(), String> {
        Err("dynamic-set BFT preview is unsupported by this hook".into())
    }

    fn validate_bft_v2_action_payload(&mut self, _payload: &[u8]) -> Result<(), String> {
        Err("dynamic-set BFT action admission is unsupported by this hook".into())
    }

    /// State-dependent admission uses the current committed parent policy;
    /// received candidates must still pass independent execution preview.
    fn validate_bft_v2_action_batch(
        &mut self,
        _schedule: &oasis7_consensus::tendermint_v2::BftHeightProposerScheduleV2,
        _actions: &[NodeConsensusAction],
    ) -> Result<(), String> {
        Err("dynamic-set BFT batch admission is unsupported by this hook".into())
    }

    fn commit_bft_v2_certificate(
        &mut self,
        _certificate: oasis7_consensus::tendermint_v2::VerifiedBftCommitCertificateV2,
    ) -> Result<NodeBftExecutionCommitResultV2, String> {
        Err("dynamic-set BFT commit is unsupported by this hook".into())
    }

    fn load_bft_v2_commit_history_page(
        &mut self,
        _config: &NodeBftExecutionConfigV2,
        _after_height: u64,
        _max_entries: usize,
    ) -> Result<Vec<oasis7_proto::distributed_bft::v2::BftCommitCertificateV2>, String> {
        Err("dynamic-set BFT history is unsupported by this hook".into())
    }
    fn configure_bft_v3_execution(
        &mut self,
        _binding: &oasis7_distfs::protected_bft_v3::BftSignerBindingV3,
    ) -> Result<Option<NodeBftExecutionHeadV3>, String> {
        Err("wire-3 execution is unsupported by this hook".into())
    }

    fn current_bft_v3_execution_context(
        &mut self,
        _schedule: &oasis7_consensus::tendermint_v3::BftHeaderHeightScheduleV3,
    ) -> Result<NodeBftExecutionContextV3, String> {
        Err("wire-3 execution is unsupported by this hook".into())
    }

    fn build_bft_v3_candidate(
        &mut self,
        _context: NodeBftBuildContextV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        Err("wire-3 execution is unsupported by this hook".into())
    }

    fn preview_bft_v3_candidate(
        &mut self,
        _context: &NodeBftExecutionContextV3,
        _candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        Err("wire-3 execution is unsupported by this hook".into())
    }

    fn retain_bft_v3_private_candidate(
        &mut self,
        _context: &NodeBftExecutionContextV3,
        _candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        Err("wire-3 execution is unsupported by this hook".into())
    }

    /// Export only an exact original retained value and its recorded closure.
    fn export_bft_v3_private_candidate(
        &mut self,
        _value: &oasis7_consensus::tendermint_v3::BftHeaderValueV3,
    ) -> Result<crate::NodeBftPrivateCandidatePackageV3, String> {
        Err("wire-3 original private candidate export is unsupported by this hook".into())
    }

    /// Independently execute against the strict local parent, then retain the
    /// exact received value without changing the local original selection.
    fn retain_received_bft_v3_private_candidate(
        &mut self,
        _context: &NodeBftExecutionContextV3,
        _package: &crate::NodeBftPrivateCandidatePackageV3,
    ) -> Result<String, String> {
        Err("wire-3 received private candidate retention is unsupported by this hook".into())
    }

    /// Recover the original private reservation before deriving a new value
    /// from the volatile queue. Missing recovery support is not an empty slot.
    fn pending_bft_v3_private_candidate(
        &mut self,
        _context: &NodeBftExecutionContextV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        Err("wire-3 private candidate recovery is unsupported by this hook".into())
    }

    /// Resolve only independently retained local metadata for a compact peer target.
    fn enable_bft_v3_membership_execution(&mut self) -> Result<(), String> {
        Err("wire-3 membership execution is unsupported by this hook".into())
    }

    fn lookup_bft_v3_public_value(
        &mut self,
        _height: u64,
        _header_id: &str,
    ) -> Result<Option<oasis7_consensus::tendermint_v3::BftHeaderValueV3>, String> {
        Err("wire-3 retained public value lookup is unsupported by this hook".into())
    }

    fn load_bft_v3_private_candidate(
        &mut self,
        _value: &oasis7_consensus::tendermint_v3::BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        Err("wire-3 execution is unsupported by this hook".into())
    }

    fn commit_bft_v3_certificate(
        &mut self,
        _certificate: oasis7_consensus::tendermint_v3::VerifiedBftHeaderCertificateV3,
    ) -> Result<NodeBftExecutionCommitResultV3, String> {
        Err("wire-3 execution is unsupported by this hook".into())
    }

    fn load_bft_v3_commit_history_page(
        &mut self,
        _binding: &oasis7_distfs::protected_bft_v3::BftSignerBindingV3,
        _after_height: u64,
        _max_entries: usize,
    ) -> Result<Vec<oasis7_consensus::tendermint_v3::FormalBftCertifiedHeaderV3>, String> {
        Err("wire-3 execution is unsupported by this hook".into())
    }

    fn validate_bft_v3_action_payload(&mut self, _payload: &[u8]) -> Result<(), String> {
        Err("wire-3 execution is unsupported by this hook".into())
    }

    fn validate_bft_v3_action_batch(
        &mut self,
        _schedule: &oasis7_consensus::tendermint_v3::BftHeaderHeightScheduleV3,
        _actions: &[NodeConsensusAction],
    ) -> Result<(), String> {
        Err("wire-3 execution is unsupported by this hook".into())
    }
}
