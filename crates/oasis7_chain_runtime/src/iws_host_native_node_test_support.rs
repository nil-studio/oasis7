//! Test-only adapter around the shipping fixed consumers. A real Node engine
//! is the sole issuer of continuation provenance; fixture originals are only
//! independent assertions/Node recovery DATA and never source qualifications.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1;
use oasis7_node::{
    NodeConfig, NodeExecutionCommitContext, NodeExecutionCommitOutcome, NodeExecutionCommitResult,
    NodeExecutionGenesisBootstrap, NodeExecutionHook, NodeLocalExecutionContinuation, NodeRole,
    NodeRuntime, VerifiedNodeLocalContinuationV1, VerifiedNodeRestoredContinuationV1,
};
use oasis7_runtime::{
    NormalCommitHeaderLimitsV1, NormalHostCognitionParentSourcesV1,
    PaidNormalHostCognitionParentSourcesV1, RuntimeSourceCompiler,
};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub(crate) struct NativeNodeOwnersV1 {
    pub primary: IwsHostRuntimeV1,
    pub replica: IwsHostRuntimeV1,
    pub memory: MemoryBudget,
    pub archive: FormalArchiveReadWorkV1,
    pub work: NormalCommitWorkV1,
}
pub(crate) enum NativeNodeCognitionV1 {
    Startup(PaidNormalHostCognitionParentSourcesV1),
    Resident,
}
pub(crate) struct NativeNodeSettingsV1 {
    pub world_id: String,
    pub writer: ed25519_dalek::SigningKey,
    pub primary_provider: Arc<RemoteMonotonicEndpointProvider>,
    pub replica_provider: Arc<RemoteMonotonicEndpointProvider>,
    pub primary_policy: TrustedFencingPolicyV1,
    pub replica_policy: TrustedFencingPolicyV1,
    pub compiler: RuntimeSourceCompiler,
    pub input_limits: CborDataLimitsV1,
    pub action_limits: CborDataLimitsV1,
    pub policy: NormalCommitHeaderLimitsV1,
    pub max_action_bytes: usize,
    pub max_grant_payload_bytes: u64,
}
impl NativeNodeSettingsV1 {
    fn environment<'a>(
        &'a self,
        memory: &'a MemoryBudget,
        archive: &'a FormalArchiveReadWorkV1,
        cancelled: &'a AtomicBool,
    ) -> NodeNormalExecutionEnvironmentV1<'a> {
        NodeNormalExecutionEnvironmentV1 {
            writer: &self.writer,
            input_limits: &self.input_limits,
            action_limits: &self.action_limits,
            compiler: &self.compiler,
            policy: NormalCommitHeaderLimitsV1 {
                transaction: self.policy.transaction,
                checkpoint: self.policy.checkpoint,
            },
            max_action_bytes: self.max_action_bytes,
            max_grant_payload_bytes: self.max_grant_payload_bytes,
            primary_provider: &self.primary_provider,
            replica_provider: &self.replica_provider,
            primary_policy: &self.primary_policy,
            replica_policy: &self.replica_policy,
            memory,
            archive,
            cancelled,
        }
    }
}
enum Phase {
    Idle(NormalCommitWorkV1),
    Attempt(NodeNormalExecutionAttemptV1),
    Applied(PaidNodeAppliedNormalV1),
    Complete(NormalCommitWorkV1),
    StartFailure(NodeNormalStartFailureV1),
}
struct State {
    primary: IwsHostRuntimeV1,
    replica: IwsHostRuntimeV1,
    memory: MemoryBudget,
    archive: FormalArchiveReadWorkV1,
    settings: NativeNodeSettingsV1,
    cognition: NativeNodeCognitionV1,
    cancelled: AtomicBool,
    boundary: PaidIwsNodeBoundaryV1,
    phase: Option<Phase>,
    // Independent fixture assertion DATA, never an accepted/Frozen producer.
    original: Option<NodeLocalExecutionContinuation>,
    applied: Option<NodeExecutionCommitResult>,
    fresh_count: usize,
    context_refusals: usize,
    execution_advances: usize,
    response_refusals: usize,
    completion_count: usize,
    failure: Option<String>,
    observed: Option<mpsc::SyncSender<()>>,
}
struct Hook(Arc<Mutex<State>>);
impl NodeExecutionHook for Hook {
    fn guarded_local_execution(&self) -> bool {
        true
    }
    fn qualified_local_execution_genesis_bootstrap(
        &mut self,
    ) -> Result<Option<NodeExecutionGenesisBootstrap>, String> {
        let state = self.0.lock().map_err(|_| "fixture state poisoned")?;
        let mut stop = || state.cancelled.load(Ordering::Acquire);
        state
            .boundary
            .validate_current_bounded(
                &state.primary,
                &mut state.archive.bounded_context(&state.memory, &mut stop),
            )
            .map_err(|e| e.to_string())?;
        // The Node API owns this test bootstrap DATA; its actual producer owner
        // remains live in State through Node stop/join, including revalidation.
        Ok(state.boundary.genesis().cloned())
    }
    fn prepare_verified_local_continuation(
        &mut self,
        source: &VerifiedNodeLocalContinuationV1<'_>,
    ) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|_| "fixture state poisoned")?;
        if matches!(state.phase, Some(Phase::Complete(_))) {
            return Ok(());
        }
        if let Some(original) = &state.original {
            if original != source.original() {
                return Err("fixture original Node decision changed".into());
            }
            return Ok(());
        }
        let State {
            primary,
            memory,
            archive,
            settings,
            cancelled,
            phase,
            original,
            fresh_count,
            context_refusals,
            failure: first_failure,
            observed,
            ..
        } = &mut *state;
        let work = match phase.take().ok_or("fixture lost initial phase")? {
            Phase::Idle(work) => work,
            retained => {
                *phase = Some(retained);
                return Err("fixture lost initial work owner".into());
            }
        };
        *original = Some(source.original().clone());
        *fresh_count += 1;
        match NodeNormalExecutionAttemptV1::fresh_bounded(
            primary,
            source,
            work,
            &settings.environment(memory, archive, cancelled),
        ) {
            Ok(attempt) => {
                let mut changed = source.original().context.clone();
                changed.committed_at_unix_ms = changed
                    .committed_at_unix_ms
                    .checked_add(1)
                    .ok_or("fixture original timestamp overflow")?;
                let before = (
                    archive.remaining_steps(),
                    archive.remaining_read_bytes(),
                    memory.used(),
                );
                let refusal = attempt.validate_callback_bounded(
                    &changed,
                    &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
                );
                assert!(
                    refusal.is_err(),
                    "altered original Node context must be refused"
                );
                assert!(
                    archive.remaining_steps() <= before.0
                        && archive.remaining_read_bytes() <= before.1
                );
                assert_eq!(
                    memory.used(),
                    before.2,
                    "refused callback releases temporary DATA only"
                );
                *context_refusals += 1;
                *phase = Some(Phase::Attempt(attempt));
                Ok(())
            }
            Err(failure) => {
                let error = format!("start: {:?}", failure.cause);
                *first_failure = Some(error.clone());
                *phase = Some(Phase::StartFailure(failure));
                if let Some(sender) = observed.take() {
                    let _ = sender.send(());
                }
                Err(error)
            }
        }
    }
    fn restored_verified_local_continuation(
        &mut self,
        source: &VerifiedNodeRestoredContinuationV1<'_>,
    ) -> Result<(), String> {
        let state = self.0.lock().map_err(|_| "fixture state poisoned")?;
        if state.original.as_ref() != Some(source.original()) {
            return Err("fixture restored original changed".into());
        }
        // A restored callback cannot recapture or replace an already retained
        // attempt. Its actual same-context check still runs before advancement.
        Ok(())
    }
    fn pending_local_continuation(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        let state = self.0.lock().map_err(|_| "fixture state poisoned")?;
        if matches!(state.phase, Some(Phase::Complete(_))) {
            Ok(None)
        } else {
            Ok(state.original.clone())
        }
    }
    fn on_commit(
        &mut self,
        _: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("fixture requires original typed Node commit outcome".into())
    }
    fn on_commit_outcome_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        expected_block: Option<&str>,
        expected_state: Option<&str>,
    ) -> Result<NodeExecutionCommitOutcome, String> {
        let mut state = self.0.lock().map_err(|_| "fixture state poisoned")?;
        if matches!(state.phase, Some(Phase::Complete(_))) {
            return Ok(NodeExecutionCommitOutcome::Deferred);
        }
        if expected_block.is_some()
            || expected_state.is_some()
            || state.original.as_ref().map(|v| &v.context) != Some(&context)
        {
            return Err("fixture original callback changed".into());
        }
        let State {
            primary,
            replica,
            memory,
            archive,
            settings,
            cancelled,
            phase,
            failure,
            observed,
            applied,
            execution_advances,
            response_refusals,
            cognition,
            ..
        } = &mut *state;
        let retained = phase.take().ok_or("fixture retained phase missing")?;
        let mut owner = match retained {
            Phase::Attempt(attempt) => {
                let source_result = match cognition {
                    NativeNodeCognitionV1::Startup(owner) => {
                        primary.cognition_parent_sources_bounded(owner, memory, archive, cancelled)
                    }
                    NativeNodeCognitionV1::Resident => {
                        Ok(NormalHostCognitionParentSourcesV1::Resident)
                    }
                };
                let sources = match source_result {
                    Ok(sources) => sources,
                    Err(error) => {
                        *failure = Some(format!("actual Cognition parent: {error}"));
                        *phase = Some(Phase::Attempt(attempt));
                        if let Some(sender) = observed.take() {
                            let _ = sender.send(());
                        }
                        return Ok(NodeExecutionCommitOutcome::Deferred);
                    }
                };
                *execution_advances += 1;
                match attempt.advance_bounded(
                    primary,
                    replica,
                    &context,
                    sources,
                    &settings.environment(memory, archive, cancelled),
                ) {
                    Ok(owner) => owner,
                    Err(attempt) => {
                        *failure = Some(format!("normal attempt: {attempt:?}"));
                        *phase = Some(Phase::Attempt(attempt));
                        if let Some(sender) = observed.take() {
                            let _ = sender.send(());
                        }
                        return Ok(NodeExecutionCommitOutcome::Deferred);
                    }
                }
            }
            Phase::Applied(owner) => owner,
            retained => {
                *phase = Some(retained);
                return Err("fixture commit phase invalid".into());
            }
        };
        if *response_refusals == 0 {
            // Fault at the real response copy, after both durable acceptances
            // and Host installs. The next actual callback must use Applied;
            // neither the engine-issued input nor Runtime may run again.
            let before = (archive.remaining_steps(), archive.remaining_read_bytes());
            cancelled.store(true, Ordering::Release);
            let refusal = owner.copy_result_bounded(
                &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
            );
            cancelled.store(false, Ordering::Release);
            assert!(
                refusal.is_err(),
                "actual response copy must observe cancellation"
            );
            assert!(
                archive.remaining_steps() <= before.0 && archive.remaining_read_bytes() <= before.1
            );
            *response_refusals += 1;
            *phase = Some(Phase::Applied(owner));
            return Ok(NodeExecutionCommitOutcome::Deferred);
        }
        let result = owner.copy_result_bounded(
            &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        );
        *phase = Some(Phase::Applied(owner));
        let result = result.map_err(|e| e.to_string())?;
        *applied = Some(result.clone());
        Ok(NodeExecutionCommitOutcome::Applied(result))
    }
    fn complete_local_continuation(
        &mut self,
        context: &NodeExecutionCommitContext,
    ) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|_| "fixture state poisoned")?;
        let State {
            phase,
            memory,
            archive,
            cancelled,
            original,
            completion_count,
            observed,
            ..
        } = &mut *state;
        if original.as_ref().map(|v| &v.context) != Some(context) {
            return Err("fixture completion context changed".into());
        }
        let owner = match phase.take().ok_or("fixture completion phase missing")? {
            Phase::Applied(owner) => owner,
            retained => {
                *phase = Some(retained);
                return Err("fixture completion without actual applied owner".into());
            }
        };
        match owner.complete_bounded(
            context,
            &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        ) {
            Ok(work) => {
                *phase = Some(Phase::Complete(work));
                *completion_count += 1;
                if let Some(sender) = observed.take() {
                    let _ = sender.send(());
                }
                Ok(())
            }
            Err((error, owner)) => {
                *phase = Some(Phase::Applied(owner));
                Err(error.to_string())
            }
        }
    }
}

/// One nonempty true Node tick. Queueing precedes start so an empty first tick
/// cannot accidentally satisfy the check; every Host execution uses the fixed
/// consumer and the actual engine-issued continuation.
pub(crate) fn run_native_node_tick_v1(
    owners: NativeNodeOwnersV1,
    settings: NativeNodeSettingsV1,
    cognition: NativeNodeCognitionV1,
    amounts: &[i64],
) -> Result<(NativeNodeOwnersV1, NativeNodeSettingsV1, serde_json::Value), String> {
    use oasis7::consensus_action_payload::{
        ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
    };
    let cancelled = AtomicBool::new(false);
    let boundary = owners
        .primary
        .read_node_execution_boundary_bounded(
            &owners.memory,
            &owners.archive,
            &settings.input_limits,
            &cancelled,
        )
        .map_err(|e| e.to_string())?;
    let expected_height = boundary
        .boundary()
        .height
        .checked_add(1)
        .ok_or("fixture height overflow")?;
    let config = NodeConfig::new("iws-normal-node", &settings.world_id, NodeRole::Sequencer)
        .map_err(|e| e.to_string())?
        .with_player_id("sender")
        .map_err(|e| e.to_string())?
        .with_tick_interval(Duration::from_millis(20))
        .map_err(|e| e.to_string())?;
    let bootstrap = boundary.genesis().cloned();
    let cold = if bootstrap.is_none() {
        Some(boundary.boundary().clone())
    } else {
        None
    };
    let (send, receive) = mpsc::sync_channel(1);
    let state = Arc::new(Mutex::new(State {
        primary: owners.primary,
        replica: owners.replica,
        memory: owners.memory,
        archive: owners.archive,
        settings,
        cognition,
        cancelled,
        boundary,
        phase: Some(Phase::Idle(owners.work)),
        original: None,
        applied: None,
        fresh_count: 0,
        context_refusals: 0,
        execution_advances: 0,
        response_refusals: 0,
        completion_count: 0,
        failure: None,
        observed: Some(send),
    }));
    let mut node = NodeRuntime::new(config).with_execution_hook(Hook(Arc::clone(&state)));
    if let Some(bootstrap) = bootstrap {
        node = node.with_local_execution_genesis_bootstrap(bootstrap);
    }
    if let Some(bootstrap) = cold {
        node = node.with_local_execution_bootstrap(bootstrap);
    }
    for (index, amount) in amounts.iter().copied().enumerate() {
        let payload =
            encode_consensus_action_payload(&ConsensusActionPayloadEnvelope::from_runtime_action(
                oasis7_runtime::Action::TransferResource {
                    from_agent_id: "sender".into(),
                    to_agent_id: "receiver".into(),
                    kind: oasis7_agent_api::world_action::ResourceKind::Electricity,
                    amount,
                },
            ))
            .map_err(|e| e.to_string())?;
        node.submit_consensus_action_payload((index as u64) + 1, payload)
            .map_err(|e| e.to_string())?;
    }
    node.start().map_err(|e| e.to_string())?;
    let observed = receive.recv_timeout(Duration::from_secs(60));
    let stopped = node.stop();
    let snapshot = node.snapshot();
    drop(node);
    stopped.map_err(|e| e.to_string())?;
    let mut state = Arc::try_unwrap(state)
        .map_err(|_| "stopped Node retained fixture hook")?
        .into_inner()
        .map_err(|_| "fixture state poisoned")?;
    if let Some(error) = state.failure.take() {
        return Err(error);
    }
    observed.map_err(|_| format!("actual nonempty Node callback deadline: {snapshot:?}"))?;
    if snapshot.consensus.last_execution_height != expected_height
        || snapshot.consensus.committed_height != expected_height
        || state.fresh_count != 1
        || state.completion_count != 1
        || state.context_refusals != 1
        || state.execution_advances != 1
        || state.response_refusals != 1
    {
        return Err(format!("actual Node completion mismatch: {snapshot:?}"));
    }
    let original = state
        .original
        .as_ref()
        .ok_or("original Node receipt missing")?;
    if original.context.committed_actions.len() != amounts.len() || amounts.is_empty() {
        return Err("fixture did not execute exact nonempty input".into());
    }
    let Some(Phase::Complete(work)) = state.phase.take() else {
        return Err("actual Node completion did not return original remaining work".into());
    };
    let receipt = serde_json::json!({"node_context":original.context,"applied":state.applied,
        "fresh_captures":state.fresh_count,"completed":state.completion_count,"amounts":amounts,
        "altered_context_refused":state.context_refusals,"runtime_advances":state.execution_advances,
        "actual_cancelled_responses":state.response_refusals,
        "remaining_common_steps":state.archive.remaining_steps(),"remaining_common_bytes":state.archive.remaining_read_bytes(),
        "primary_record":state.primary.current_record_id().map(hex::encode),"replica_record":state.replica.current_record_id().map(hex::encode)});
    Ok((
        NativeNodeOwnersV1 {
            primary: state.primary,
            replica: state.replica,
            memory: state.memory,
            archive: state.archive,
            work,
        },
        state.settings,
        receipt,
    ))
}
