//! Node adapter for the formal writer's proof-bearing execution path.
//!
//! Raw Node commit callbacks and peer-supplied expected hashes cannot authorize
//! a formal execution. The adapter accepts only the exact durable local
//! continuation prepared by the Node and reports execution hashes from the
//! committed `ExecutionBridgeRecord` result artifact.
use super::{
    EXECUTION_BRIDGE_RECORD_SCHEMA_V3, ExecutionBridgeRecord, formal_writer::FormalWriter, to_cbor,
};
use oasis7_distfs::controlled_authority::formal::{
    FormalRecordKindV1, SignedNormalDurableSetTransitionV1, VerifiedFormalReadAnchorV1,
    coordinator::FormalDurabilityEndpoint,
};
use oasis7_node::{
    NodeExecutionBootstrap, NodeExecutionCommitContext, NodeExecutionCommitOutcome,
    NodeExecutionCommitResult, NodeExecutionGenesisBootstrap, NodeExecutionHook,
    NodeLocalExecutionContinuation,
};
use oasis7_runtime::{World, blake3_hex};
use std::sync::{Arc, Mutex};

#[cfg(test)]
#[path = "formal_node_hook_tests.rs"]
mod tests;

#[derive(Clone)]
pub(crate) struct CommittedExecutionEvidence {
    world_id: String,
    canonical_after_state_root: String,
    world_snapshot_root: String,
    formal_record_id: String,
    result: ExecutionBridgeRecord,
}

#[derive(Clone)]
pub(crate) enum CurrentHeadEvidence {
    InitialActivation(NodeExecutionBootstrap),
    Execution(Box<CommittedExecutionEvidence>),
}

pub(crate) trait FormalNodeWriter: Send {
    fn world(&self) -> &World;
    fn world_id(&self) -> &str;
    /// Return the current immutable World cut and its opaque verified formal
    /// anchor. Implementations must not promote endpoint-supplied raw pins.
    fn committed_read_cut(&self) -> Result<(World, VerifiedFormalReadAnchorV1), String> {
        Err("formal writer does not expose a verified committed read cut".into())
    }
    fn retained_initial_read_cut(
        &self,
    ) -> Result<(World, VerifiedFormalReadAnchorV1, NodeExecutionBootstrap), String> {
        Err("formal writer does not expose qualified original genesis history".into())
    }
    /// One bounded storage maintenance page, independent of business commit.
    fn advance_storage_maintenance(&mut self) {}
    fn current_head_evidence(&self) -> Result<CurrentHeadEvidence, String>;
    fn pending_continuation(&self) -> Result<Option<NodeLocalExecutionContinuation>, String>;
    fn prepare(&mut self, continuation: &NodeLocalExecutionContinuation) -> Result<(), String>;
    fn prepare_successor(
        &mut self,
        continuation: &NodeLocalExecutionContinuation,
        exact_terminal_record_id: &str,
    ) -> Result<(), String>;
    fn prepare_with_durable_set_transition(
        &mut self,
        continuation: &NodeLocalExecutionContinuation,
        transition: Option<&SignedNormalDurableSetTransitionV1>,
    ) -> Result<(), String> {
        if transition.is_some() {
            return Err("formal writer does not support healthy durable-set transitions".into());
        }
        self.prepare(continuation)
    }
    fn prepare_successor_with_durable_set_transition(
        &mut self,
        continuation: &NodeLocalExecutionContinuation,
        exact_terminal_record_id: &str,
        transition: Option<&SignedNormalDurableSetTransitionV1>,
    ) -> Result<(), String> {
        if transition.is_some() {
            return Err("formal writer does not support healthy durable-set transitions".into());
        }
        self.prepare_successor(continuation, exact_terminal_record_id)
    }
    fn resume_original(&mut self) -> Result<String, String>;
    fn committed_execution(&self) -> Result<CommittedExecutionEvidence, String>;
}

impl<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint> FormalNodeWriter
    for FormalWriter<P, R>
{
    fn world(&self) -> &World {
        FormalWriter::world(self)
    }

    fn world_id(&self) -> &str {
        &self.anchor().head().record().world.world_id
    }

    fn advance_storage_maintenance(&mut self) {
        FormalWriter::advance_storage_maintenance(self)
    }

    fn committed_read_cut(&self) -> Result<(World, VerifiedFormalReadAnchorV1), String> {
        let anchor = self.anchor().clone();
        let world = self.world().clone();
        let snapshot_root = blake3_hex(&to_cbor(world.snapshot())?);
        if snapshot_root != anchor.head().record().after_state_root
            || anchor.head().record().world.world_id != self.world_id()
        {
            return Err("formal publication World differs from qualified writer anchor".into());
        }
        Ok((world, anchor))
    }

    fn current_head_evidence(&self) -> Result<CurrentHeadEvidence, String> {
        let anchor = self.anchor();
        let canonical = anchor.head().record();
        match &canonical.kind {
            FormalRecordKindV1::InitialActivation => Ok(CurrentHeadEvidence::InitialActivation(
                FormalWriter::committed_initial_bootstrap(self)?,
            )),
            FormalRecordKindV1::Execution => {
                let result = self.committed_execution_record()?;
                let snapshot_root = blake3_hex(&to_cbor(self.world().snapshot())?);
                Ok(CurrentHeadEvidence::Execution(Box::new(
                    CommittedExecutionEvidence {
                        world_id: canonical.world.world_id.clone(),
                        canonical_after_state_root: canonical.after_state_root.clone(),
                        world_snapshot_root: snapshot_root,
                        formal_record_id: anchor.head().head().record_id.clone(),
                        result,
                    },
                )))
            }
        }
    }

    fn retained_initial_read_cut(
        &self,
    ) -> Result<(World, VerifiedFormalReadAnchorV1, NodeExecutionBootstrap), String> {
        FormalWriter::retained_initial_read_cut(self)
    }

    fn pending_continuation(&self) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        FormalWriter::pending_continuation(self)
    }

    fn prepare(&mut self, continuation: &NodeLocalExecutionContinuation) -> Result<(), String> {
        FormalWriter::prepare(self, continuation)
    }

    fn prepare_successor(
        &mut self,
        continuation: &NodeLocalExecutionContinuation,
        exact_terminal_record_id: &str,
    ) -> Result<(), String> {
        FormalWriter::prepare_successor(self, continuation, exact_terminal_record_id)
    }

    fn prepare_with_durable_set_transition(
        &mut self,
        continuation: &NodeLocalExecutionContinuation,
        transition: Option<&SignedNormalDurableSetTransitionV1>,
    ) -> Result<(), String> {
        FormalWriter::prepare_with_durable_set_transition(self, continuation, transition)
    }

    fn prepare_successor_with_durable_set_transition(
        &mut self,
        continuation: &NodeLocalExecutionContinuation,
        exact_terminal_record_id: &str,
        transition: Option<&SignedNormalDurableSetTransitionV1>,
    ) -> Result<(), String> {
        FormalWriter::prepare_successor_with_durable_set_transition(
            self,
            continuation,
            exact_terminal_record_id,
            transition,
        )
    }

    fn resume_original(&mut self) -> Result<String, String> {
        let proof = FormalWriter::resume_original(self)?;
        proof
            .decision
            .record
            .record_id()
            .map_err(|error| error.to_string())
    }

    fn committed_execution(&self) -> Result<CommittedExecutionEvidence, String> {
        match self.current_head_evidence()? {
            CurrentHeadEvidence::Execution(evidence) => Ok(*evidence),
            CurrentHeadEvidence::InitialActivation(_) => {
                Err("formal InitialActivation has no committed Node execution result".into())
            }
        }
    }
}

/// Publishes an immutable, already-qualified World cut to read-only services.
/// The implementation must swap the complete `(World, anchor)` pair atomically.
pub(crate) trait FormalPublicationSink: Send + Sync {
    fn publish(&self, world: World, anchor: VerifiedFormalReadAnchorV1) -> Result<(), String>;
}

/// An actual `NodeExecutionHook` backed by dual-durable Frozen/qualified formal
/// commits. An initial-activation bootstrap is accepted only through the
/// writer's typed accessor for the signed InitialInput closure; this adapter
/// never accepts raw bootstrap values or invents Node counters and hashes.
pub(crate) struct FormalNodeHook<W: FormalNodeWriter> {
    writer: W,
    consumed_terminal: Option<ConsumedTerminal>,
    publication_sink: Option<Arc<dyn FormalPublicationSink>>,
    durable_set_arm: Option<Arc<super::formal_durable_set_arm::FormalDurableSetArmControlV1>>,
    admission_gate: Option<Arc<Mutex<()>>>,
}

struct ConsumedTerminal {
    continuation: NodeLocalExecutionContinuation,
    formal_record_id: String,
}

impl<W: FormalNodeWriter> FormalNodeHook<W> {
    pub(crate) fn new(writer: W) -> Result<Self, String> {
        let hook = Self {
            writer,
            consumed_terminal: None,
            publication_sink: None,
            durable_set_arm: None,
            admission_gate: None,
        };
        // Refuse unsupported initial anchors and zero-height pending recovery at
        // construction. A zero-height InitialActivation must pass the distinct
        // qualified-genesis path before the caller can start a NodeRuntime.
        if hook.writer.pending_continuation()?.is_some() {
            hook.bootstrap()?;
        } else {
            match hook.writer.current_head_evidence()? {
                CurrentHeadEvidence::InitialActivation(bootstrap) if bootstrap.height == 0 => {
                    hook.genesis_bootstrap()?.ok_or(
                        "formal zero-height InitialActivation lacks qualified genesis evidence",
                    )?;
                }
                _ => {
                    hook.bootstrap()?;
                }
            }
        }
        Ok(hook)
    }

    /// Attach the public-read publisher before the Node starts. The initial
    /// immutable cut comes only from the writer's verified anchor.
    pub(crate) fn with_publication_sink(
        mut self,
        sink: Arc<dyn FormalPublicationSink>,
    ) -> Result<Self, String> {
        let (world, anchor) = self.writer.committed_read_cut()?;
        sink.publish(world, anchor)?;
        self.publication_sink = Some(sink);
        Ok(self)
    }

    /// Attach the private healthy-transition arm control before Node start.
    /// The hook and in-process operator owner share the same admission gate,
    /// so an arm cannot race an already admitted continuation.
    pub(crate) fn with_durable_set_arm(
        mut self,
        arm: Arc<super::formal_durable_set_arm::FormalDurableSetArmControlV1>,
        admission_gate: Arc<Mutex<()>>,
    ) -> Result<Self, String> {
        arm.set_pending_open(self.writer.pending_continuation()?.is_some())?;
        self.durable_set_arm = Some(arm);
        self.admission_gate = Some(admission_gate);
        Ok(self)
    }

    fn mark_pending_open(&self, open: bool) -> Result<(), String> {
        if let Some(arm) = &self.durable_set_arm {
            arm.set_pending_open(open)?;
        }
        Ok(())
    }

    fn armed_transition_for_next_boundary(
        &self,
    ) -> Result<Option<SignedNormalDurableSetTransitionV1>, String> {
        let Some(arm) = &self.durable_set_arm else {
            return Ok(None);
        };
        let (_, anchor) = self.writer.committed_read_cut()?;
        Ok(arm
            .for_next_boundary(&anchor)?
            .map(|intent| intent.transition))
    }

    fn publish_committed_read_cut(&self, expected_record_id: &str) -> Result<(), String> {
        let Some(sink) = &self.publication_sink else {
            return Ok(());
        };
        let (world, anchor) = self.writer.committed_read_cut()?;
        let actual_head = anchor.head().head();
        let actual_record_id = actual_head.record_id.as_str();
        if actual_record_id != expected_record_id {
            return Err(
                "formal read publisher is not at the exact qualified execution head".into(),
            );
        }
        sink.publish(world, anchor)
    }

    pub(crate) fn bootstrap(&self) -> Result<NodeExecutionBootstrap, String> {
        if self.consumed_terminal.is_none()
            && let Some(continuation) = self.writer.pending_continuation()?
        {
            match self.writer.current_head_evidence()? {
                CurrentHeadEvidence::Execution(evidence)
                    if evidence.result.height == continuation.context.height =>
                {
                    result_for_continuation(&continuation, &evidence)?;
                    let bootstrap = self.pending_predecessor_bootstrap(&continuation)?;
                    self.validate_continuation(&continuation, &bootstrap)?;
                    return Ok(bootstrap);
                }
                CurrentHeadEvidence::Execution(evidence)
                    if evidence.result.height > continuation.context.height =>
                {
                    return Err(
                        "formal pending continuation is behind the qualified execution head".into(),
                    );
                }
                CurrentHeadEvidence::InitialActivation(bootstrap)
                    if continuation.predecessor_committed_height == 0 && bootstrap.height == 0 =>
                {
                    let genesis = self
                        .genesis_bootstrap()?
                        .ok_or("formal pending genesis continuation lacks its InitialActivation")?;
                    self.validate_continuation(&continuation, genesis.boundary())?;
                    return Ok(genesis.boundary().clone());
                }
                _ => {
                    if continuation.context.world_id != self.writer.world_id() {
                        return Err(
                            "formal pending continuation world differs from writer anchor".into(),
                        );
                    }
                    return bootstrap_from_pending(&continuation);
                }
            }
        }
        match self.writer.current_head_evidence()? {
            CurrentHeadEvidence::InitialActivation(bootstrap) => {
                bootstrap_from_initial(self.writer.world(), &bootstrap)
            }
            CurrentHeadEvidence::Execution(evidence) => bootstrap_from_execution(&evidence),
        }
    }

    /// Return the dedicated zero-height capability only for the exact
    /// qualified InitialActivation at native position one. The formal writer's
    /// typed initial-result accessor supplies the hashes; the Node adapter
    /// confirms that they bind this immutable World snapshot and record.
    pub(crate) fn genesis_bootstrap(
        &self,
    ) -> Result<Option<NodeExecutionGenesisBootstrap>, String> {
        if self.consumed_terminal.is_some() {
            return Ok(None);
        }
        let (world, anchor, bootstrap) = match self.writer.current_head_evidence()? {
            CurrentHeadEvidence::InitialActivation(bootstrap) => {
                let (world, anchor) = self.writer.committed_read_cut()?;
                (world, anchor, bootstrap)
            }
            CurrentHeadEvidence::Execution(_) => {
                let Some(original) = self.writer.pending_continuation()? else {
                    return Ok(None);
                };
                if original.predecessor_committed_height != 0 || original.context.height != 1 {
                    return Ok(None);
                }
                self.writer.retained_initial_read_cut()?
            }
        };
        if bootstrap.height != 0 {
            return Ok(None);
        }

        let record = anchor.head().record();
        let head = anchor.head().head();
        let world_root = blake3_hex(&to_cbor(world.snapshot())?);
        if !matches!(&record.kind, FormalRecordKindV1::InitialActivation)
            || record.position != 1
            || head.position != 1
            || head.record_id != record.record_id().map_err(|error| error.to_string())?
            || record.world.world_id != self.writer.world_id()
            || record.world != anchor.authority().definition().world
            || world.state().time != 0
            || record.after_state_root != world_root
            || bootstrap.execution_state_root != world_root
        {
            return Err(
                "formal genesis evidence differs from qualified InitialActivation World cut".into(),
            );
        }
        nonempty(
            "genesis consensus block hash",
            &bootstrap.consensus_block_hash,
        )?;
        nonempty(
            "genesis execution block hash",
            &bootstrap.execution_block_hash,
        )?;
        let capability = NodeExecutionGenesisBootstrap::from_qualified_initial_activation(
            record.world.world_id.clone(),
            head.record_id,
            bootstrap,
        )?;
        if let Some(continuation) = self.writer.pending_continuation()? {
            self.validate_continuation(&continuation, capability.boundary())?;
        }
        Ok(Some(capability))
    }

    fn pending_predecessor_bootstrap(
        &self,
        continuation: &NodeLocalExecutionContinuation,
    ) -> Result<NodeExecutionBootstrap, String> {
        if continuation.predecessor_committed_height == 0 {
            let genesis = self
                .genesis_bootstrap()?
                .ok_or("formal zero-height predecessor is not a qualified genesis")?;
            return Ok(genesis.boundary().clone());
        }
        bootstrap_from_pending(continuation)
    }

    fn validate_continuation(
        &self,
        continuation: &NodeLocalExecutionContinuation,
        bootstrap: &NodeExecutionBootstrap,
    ) -> Result<(), String> {
        if continuation.context.world_id != self.writer.world_id()
            || continuation.context.node_id.trim().is_empty()
            || continuation.context.proposer_id != continuation.context.node_id
            || continuation.context.node_block_hash.trim().is_empty()
            || continuation.schema_version != 1
            || continuation.next_height != continuation.context.height
            || continuation.context.height
                != continuation
                    .predecessor_committed_height
                    .checked_add(1)
                    .ok_or("formal Node continuation height overflow")?
            || continuation.predecessor_committed_height != bootstrap.height
            || continuation.predecessor_execution_height != bootstrap.height
            || continuation.predecessor_node_block_hash.as_deref()
                != Some(bootstrap.consensus_block_hash.as_str())
            || continuation.predecessor_execution_block_hash.as_deref()
                != Some(bootstrap.execution_block_hash.as_str())
            || continuation.predecessor_execution_state_root.as_deref()
                != Some(bootstrap.execution_state_root.as_str())
        {
            return Err(
                "formal Node continuation differs from its verified predecessor boundary".into(),
            );
        }
        Ok(())
    }
}

fn nonempty(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("formal Node {label} is empty"))
    } else {
        Ok(())
    }
}

fn bootstrap_from_initial(
    world: &World,
    bootstrap: &NodeExecutionBootstrap,
) -> Result<NodeExecutionBootstrap, String> {
    if bootstrap.height == 0 || bootstrap.height != world.state().time {
        return Err(
            "formal InitialActivation bootstrap height differs from positive World time".into(),
        );
    }
    let world_root = blake3_hex(&to_cbor(world.snapshot())?);
    if bootstrap.execution_state_root != world_root {
        return Err("formal InitialActivation bootstrap root differs from verified World".into());
    }
    nonempty(
        "initial consensus block hash",
        &bootstrap.consensus_block_hash,
    )?;
    nonempty(
        "initial execution block hash",
        &bootstrap.execution_block_hash,
    )?;
    Ok(bootstrap.clone())
}

fn bootstrap_from_pending(
    continuation: &NodeLocalExecutionContinuation,
) -> Result<NodeExecutionBootstrap, String> {
    let height = continuation.predecessor_committed_height;
    if height == 0 {
        return Err(
            "formal pending continuation has predecessor height zero, unsupported by NodeExecutionBootstrap".into(),
        );
    }
    if continuation.schema_version != 1
        || continuation.predecessor_execution_height != height
        || continuation.next_height != continuation.context.height
        || height.checked_add(1) != Some(continuation.context.height)
    {
        return Err("formal pending continuation counter boundary is inconsistent".into());
    }
    let consensus_block_hash = continuation
        .predecessor_node_block_hash
        .as_deref()
        .ok_or("formal pending continuation predecessor Node hash missing")?;
    let execution_block_hash = continuation
        .predecessor_execution_block_hash
        .as_deref()
        .ok_or("formal pending continuation predecessor execution hash missing")?;
    let execution_state_root = continuation
        .predecessor_execution_state_root
        .as_deref()
        .ok_or("formal pending continuation predecessor state root missing")?;
    for (label, value) in [
        ("predecessor Node hash", consensus_block_hash),
        ("predecessor execution hash", execution_block_hash),
        ("predecessor state root", execution_state_root),
    ] {
        nonempty(label, value)?;
    }
    Ok(NodeExecutionBootstrap {
        height,
        consensus_block_hash: consensus_block_hash.into(),
        execution_block_hash: execution_block_hash.into(),
        execution_state_root: execution_state_root.into(),
    })
}

fn bootstrap_from_execution(
    evidence: &CommittedExecutionEvidence,
) -> Result<NodeExecutionBootstrap, String> {
    let record = &evidence.result;
    let node_hash = record
        .node_block_hash
        .as_deref()
        .ok_or("formal execution result Node block hash missing")?;
    if record.schema_version != EXECUTION_BRIDGE_RECORD_SCHEMA_V3
        || record.world_id != evidence.world_id
        || record.world_id.trim().is_empty()
        || record.height == 0
        || record.execution_state_root != evidence.canonical_after_state_root
        || record.execution_state_root != evidence.world_snapshot_root
    {
        return Err("formal ExecutionBridgeRecord does not match its qualified World head".into());
    }
    nonempty("Node block hash", node_hash)?;
    nonempty("execution block hash", &record.execution_block_hash)?;
    nonempty("execution state root", &record.execution_state_root)?;
    Ok(NodeExecutionBootstrap {
        height: record.height,
        consensus_block_hash: node_hash.into(),
        execution_block_hash: record.execution_block_hash.clone(),
        execution_state_root: record.execution_state_root.clone(),
    })
}

fn result_for_continuation(
    continuation: &NodeLocalExecutionContinuation,
    evidence: &CommittedExecutionEvidence,
) -> Result<NodeExecutionCommitResult, String> {
    let record = &evidence.result;
    let context = &continuation.context;
    if record.schema_version != EXECUTION_BRIDGE_RECORD_SCHEMA_V3
        || evidence.world_id != context.world_id
        || record.world_id != context.world_id
        || record.height != context.height
        || record.node_block_hash.as_deref() != Some(context.node_block_hash.as_str())
        || record.prev_node_block_hash != continuation.predecessor_node_block_hash
        || record.proposer_id.as_deref() != Some(context.proposer_id.as_str())
        || record.action_root.as_deref() != Some(context.action_root.as_str())
        || record.execution_state_root != evidence.canonical_after_state_root
        || record.execution_state_root != evidence.world_snapshot_root
    {
        return Err(
            "formal committed ExecutionBridgeRecord differs from the exact Node continuation"
                .into(),
        );
    }
    nonempty("execution block hash", &record.execution_block_hash)?;
    nonempty("execution state root", &record.execution_state_root)?;
    Ok(NodeExecutionCommitResult {
        execution_height: record.height,
        execution_block_hash: record.execution_block_hash.clone(),
        execution_state_root: record.execution_state_root.clone(),
    })
}

impl<W: FormalNodeWriter> NodeExecutionHook for FormalNodeHook<W> {
    fn guarded_local_execution(&self) -> bool {
        true
    }

    fn qualified_local_execution_genesis_bootstrap(
        &mut self,
    ) -> Result<Option<NodeExecutionGenesisBootstrap>, String> {
        self.genesis_bootstrap()
    }

    fn on_commit(
        &mut self,
        _: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("formal Node execution requires the exact prepared local continuation".into())
    }

    fn on_commit_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        expected_execution_block_hash: Option<&str>,
        expected_execution_state_root: Option<&str>,
    ) -> Result<NodeExecutionCommitResult, String> {
        if expected_execution_block_hash.is_some() || expected_execution_state_root.is_some() {
            return Err(
                "formal Node execution rejects network-supplied expected continuation".into(),
            );
        }
        self.on_commit(context)
    }

    fn prepare_local_continuation(
        &mut self,
        continuation: &NodeLocalExecutionContinuation,
    ) -> Result<(), String> {
        let mut timing = super::formal_execution_timing::FormalExecutionTiming::begin(
            "node_prepare_continuation",
            continuation.context.height,
        );
        let admission_gate = self.admission_gate.clone();
        let _admission = admission_gate
            .as_ref()
            .map(|gate| {
                gate.lock()
                    .map_err(|_| "formal Node admission gate poisoned".to_string())
            })
            .transpose()?;
        timing.mark("admission_gate_acquired");
        let pending = self.writer.pending_continuation()?;
        timing.mark("pending_read");
        if let Some(consumed) = &self.consumed_terminal {
            let original = pending
                .as_ref()
                .ok_or("formal consumed terminal disappeared before successor preparation")?;
            if original != &consumed.continuation || original == continuation {
                return Err(
                    "formal successor cannot replace a different or already-consumed request"
                        .into(),
                );
            }
            let evidence = self.writer.committed_execution()?;
            if evidence.formal_record_id != consumed.formal_record_id {
                return Err("formal consumed terminal differs from current qualified head".into());
            }
            result_for_continuation(original, &evidence)?;
            let bootstrap = bootstrap_from_execution(&evidence)?;
            self.validate_continuation(continuation, &bootstrap)?;
            let transition = self.armed_transition_for_next_boundary()?;
            self.mark_pending_open(true)?;
            let prepared = self.writer.prepare_successor_with_durable_set_transition(
                continuation,
                &consumed.formal_record_id,
                transition.as_ref(),
            );
            let pending_open = self.writer.pending_continuation()?.is_some();
            self.mark_pending_open(pending_open)?;
            prepared?;
            self.consumed_terminal = None;
            return Ok(());
        }
        if let Some(original) = pending.as_ref() {
            if original != continuation {
                return Err("formal original continuation has priority over a successor".into());
            }
            let bootstrap = self.pending_predecessor_bootstrap(original)?;
            self.validate_continuation(continuation, &bootstrap)?;
            // Reasserting the original Frozen request is idempotent and does not
            // execute it. Node will resume it through on_commit_outcome below.
            self.mark_pending_open(true)?;
            let prepared = self
                .writer
                .prepare_with_durable_set_transition(continuation, None);
            let pending_open = self.writer.pending_continuation()?.is_some();
            self.mark_pending_open(pending_open)?;
            return prepared;
        }
        let bootstrap = if continuation.predecessor_committed_height == 0 {
            self.genesis_bootstrap()?
                .ok_or("formal zero-height continuation lacks qualified genesis")?
                .boundary()
                .clone()
        } else {
            self.bootstrap()?
        };
        self.validate_continuation(continuation, &bootstrap)?;
        // The writer persists the exact Frozen material and dual acknowledgements
        // here. It does not execute World until resume_original is called.
        let transition = self.armed_transition_for_next_boundary()?;
        self.mark_pending_open(true)?;
        let prepared = self
            .writer
            .prepare_with_durable_set_transition(continuation, transition.as_ref());
        let pending_open = self.writer.pending_continuation()?.is_some();
        self.mark_pending_open(pending_open)?;
        prepared
    }

    fn pending_local_continuation(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        // The Node calls this once per tick, including idle ticks. Reclamation
        // never changes a continuation or blocks it on closure completion.
        self.writer.advance_storage_maintenance();
        let pending = self.writer.pending_continuation()?;
        if let Some(consumed) = &self.consumed_terminal {
            return match pending {
                Some(pending) if pending == consumed.continuation => Ok(None),
                Some(_) => Err(
                    "formal pending changed before consumed terminal successor preparation".into(),
                ),
                None => {
                    Err("formal consumed terminal disappeared before successor preparation".into())
                }
            };
        }
        if let Some(original) = &pending {
            match self.writer.current_head_evidence()? {
                CurrentHeadEvidence::Execution(evidence)
                    if evidence.result.height == original.context.height =>
                {
                    result_for_continuation(original, &evidence)?;
                }
                CurrentHeadEvidence::Execution(evidence)
                    if evidence.result.height > original.context.height =>
                {
                    return Err(
                        "formal pending continuation is behind the qualified execution head".into(),
                    );
                }
                _ => {}
            }
        }
        Ok(pending)
    }

    fn on_commit_outcome_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        expected_execution_block_hash: Option<&str>,
        expected_execution_state_root: Option<&str>,
    ) -> Result<NodeExecutionCommitOutcome, String> {
        let mut timing = super::formal_execution_timing::FormalExecutionTiming::begin(
            "node_commit_outcome",
            context.height,
        );
        if expected_execution_block_hash.is_some() || expected_execution_state_root.is_some() {
            return Err(
                "formal Node execution rejects network-supplied expected continuation".into(),
            );
        }
        if self.consumed_terminal.is_some() {
            return Err(
                "formal terminal continuation was already consumed by this Node runtime".into(),
            );
        }
        let continuation = self
            .writer
            .pending_continuation()?
            .ok_or("formal Node execution has no durably prepared original continuation")?;
        if continuation.context != context {
            return Err("formal pending Node continuation differs from requested commit".into());
        }
        let proof_record_id = self.writer.resume_original()?;
        timing.mark("original_resumed");
        let evidence = self.writer.committed_execution()?;
        timing.mark("committed_evidence_read");
        if evidence.formal_record_id != proof_record_id {
            return Err("formal writer proof and qualified publication head differ".into());
        }
        let result = result_for_continuation(&continuation, &evidence)?;
        self.publish_committed_read_cut(&proof_record_id)?;
        timing.mark("read_cut_published");
        #[cfg(all(test, unix))]
        super::formal_crash_test_boundary::stop_after("read_published")?;
        Ok(NodeExecutionCommitOutcome::Applied(result))
    }

    fn complete_local_continuation(
        &mut self,
        context: &NodeExecutionCommitContext,
    ) -> Result<(), String> {
        let mut timing = super::formal_execution_timing::FormalExecutionTiming::begin(
            "node_complete_continuation",
            context.height,
        );
        let admission_gate = self.admission_gate.clone();
        let _admission = admission_gate
            .as_ref()
            .map(|gate| {
                gate.lock()
                    .map_err(|_| "formal Node admission gate poisoned".to_string())
            })
            .transpose()?;
        timing.mark("admission_gate_acquired");
        if self
            .consumed_terminal
            .as_ref()
            .is_some_and(|consumed| consumed.continuation.context == *context)
        {
            return Ok(());
        }
        let continuation = self
            .writer
            .pending_continuation()?
            .ok_or("formal completed Node continuation missing")?;
        if continuation.context != *context {
            return Err(
                "formal completion context differs from original pending Node decision".into(),
            );
        }
        let evidence = self.writer.committed_execution()?;
        timing.mark("committed_evidence_read");
        result_for_continuation(&continuation, &evidence)?;
        // Node invokes completion before persisting its next-slot and action
        // reservation counters. Retain the durable terminal receipt; only the
        // in-memory runtime view marks it consumed. Restart replays the same
        // original until an atomic successor prepare replaces this receipt.
        self.consumed_terminal = Some(ConsumedTerminal {
            continuation,
            formal_record_id: evidence.formal_record_id,
        });
        self.mark_pending_open(false)?;
        Ok(())
    }

    fn restore_to_height(&mut self, _: &str, _: u64) -> Result<bool, String> {
        Err("formal qualified history cannot be rolled back through Node legacy restore".into())
    }
}
