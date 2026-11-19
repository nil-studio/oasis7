//! Continuous production ownership of the fixed IWS Node execution path.
//! Only genuine accepted Host owners enter. In particular, their endpoint
//! constructor checks the signed hot checkpoint for decision/pending/seal;
//! unresolved cross-process Frozen work must use its dedicated recovery path
//! before startup, never an invented `None` continuation or a fresh decision.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_cbor_data::CborDataLimitsV1, bounded_context::FormalBoundedContextV1,
    incremental::IWS_AUTHORITY_PROFILE_VERSION,
};
use oasis7_formal_protocol::controlled_authority::replicated_protocol::EndpointRole;
use oasis7_node::{
    NodeExecutionBootstrap, NodeExecutionCommitContext, NodeExecutionCommitOutcome,
    NodeExecutionCommitResult, NodeExecutionGenesisBootstrap, NodeExecutionHook,
    NodeLocalExecutionBootstrapV1, NodeLocalExecutionContinuation, VerifiedNodeLocalContinuationV1,
    VerifiedNodeRestoredContinuationV1,
};
use oasis7_proto::memory_budget::{MemoryReservation, ReservedValue};
use oasis7_runtime::PaidNormalHostCognitionParentSourcesV1;
use std::sync::{Arc, Mutex};

#[path = "iws_host_node_live_cognition.rs"]
mod cognition;
#[path = "iws_host_node_live_original.rs"]
mod original;
#[path = "iws_host_node_live_settings.rs"]
mod settings;
use cognition::CognitionState;
pub use cognition::IwsLiveNodeCognitionV1;
use original::PaidNodeOriginalV1;
pub use settings::{IwsLiveNodeColdSettingsV1, IwsLiveNodeSettingsV1};

pub struct IwsLiveNodeOwnersV1 {
    pub primary: IwsHostRuntimeV1,
    pub replica: IwsHostRuntimeV1,
    pub memory: MemoryBudget,
    pub archive: FormalArchiveReadWorkV1,
    pub work: NormalCommitWorkV1,
}

pub(crate) struct IwsLiveNodeStartFailureV1 {
    pub cause: String,
    pub owners: IwsLiveNodeOwnersV1,
    pub settings: IwsLiveNodeSettingsV1,
    pub cognition: IwsLiveNodeCognitionV1,
}

pub(crate) enum IwsLiveNodeBootstrapV1 {
    Genesis(NodeExecutionGenesisBootstrap),
    Normal(NodeExecutionBootstrap),
}

/// The real atomic and its allocation charge live together, including when a
/// cancellation handle outlives the service owner. Cancellation needs no lock.
#[derive(Clone)]
pub(crate) struct IwsLiveNodeCancellationV1(Arc<ReservedValue<AtomicBool>>);
impl IwsLiveNodeCancellationV1 {
    pub(crate) fn set_cancelled(&self, cancelled: bool) {
        self.0.store(cancelled, Ordering::Release);
    }
    fn flag(&self) -> &AtomicBool {
        &self.0
    }
}

enum Phase {
    Idle(NormalCommitWorkV1),
    Attempt(NodeNormalExecutionAttemptV1),
    Applied(PaidNodeAppliedNormalV1),
    StartFailure(NodeNormalStartFailureV1),
}

struct State {
    primary: IwsHostRuntimeV1,
    replica: IwsHostRuntimeV1,
    memory: MemoryBudget,
    archive: FormalArchiveReadWorkV1,
    settings: IwsLiveNodeSettingsV1,
    cognition: CognitionState,
    boundary: PaidIwsNodeBoundaryV1,
    phase: Option<Phase>,
    original: Option<PaidNodeOriginalV1>,
    returned: MemoryReservation,
    retained_hints_fee: MemoryReservation,
}

struct Shared {
    state: Mutex<State>,
    cancelled: IwsLiveNodeCancellationV1,
    // Drops after the charged Arc/Mutex payload and all its source owners.
    _fee: MemoryReservation,
}

pub(crate) struct IwsLiveNodeOwnerV1(Arc<Shared>);
pub(crate) struct IwsLiveNodeHookV1(Arc<Shared>);
#[cfg(any(test, feature = "iws-host-native-test-support"))]
pub(crate) struct IwsLiveNodePartsV1 {
    owners: IwsLiveNodeOwnersV1,
    settings: IwsLiveNodeSettingsV1,
    cognition: CognitionState,
    _returned: MemoryReservation,
    _hints: MemoryReservation,
    _fee: MemoryReservation,
}

#[cfg(any(test, feature = "iws-host-native-test-support"))]
impl IwsLiveNodePartsV1 {
    pub(crate) fn owners(&self) -> &IwsLiveNodeOwnersV1 {
        &self.owners
    }
    pub(crate) fn settings(&self) -> &IwsLiveNodeSettingsV1 {
        &self.settings
    }
    pub(crate) fn cognition_work(&self) -> (&HostColdIoWorkV1, &HostCognitionStartupWorkV1) {
        self.cognition.work()
    }
}

fn invalid(message: &'static str) -> String {
    message.into()
}

fn validate_pair(
    primary: &IwsHostRuntimeV1,
    replica: &IwsHostRuntimeV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), String> {
    let p = primary
        .current
        .current()
        .ok_or_else(|| invalid("live Node primary accepted owner missing"))?;
    let r = replica
        .current
        .current()
        .ok_or_else(|| invalid("live Node replica accepted owner missing"))?;
    p.source
        .validate_context(common)
        .map_err(|e| e.to_string())?;
    r.source
        .validate_context(common)
        .map_err(|e| e.to_string())?;
    common.charge(1, 256).map_err(|e| e.to_string())?;
    if primary.continuation.is_some()
        || replica.continuation.is_some()
        || p.source.endpoint_role() != EndpointRole::Primary
        || r.source.endpoint_role() != EndpointRole::Replica
        || p.source.record_id() != r.source.record_id()
        || p.source.fence() != r.source.fence()
        || p.source.execution_environment() != r.source.execution_environment()
    {
        return Err(invalid("live Node dual accepted source mismatch"));
    }
    for current in [p, r] {
        current
            .resident
            .as_ref()
            .ok_or_else(|| invalid("live Node actual resident missing"))?
            .validate_live_binding_v1(current.source.record_id(), &current.source.fence(), common)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl IwsLiveNodeOwnerV1 {
    /// No raw pending DTO or accepted-head claim is accepted here. Both real
    /// endpoint sources must already have passed the no-pending Host gate.
    pub(crate) fn new_bounded(
        owners: IwsLiveNodeOwnersV1,
        settings: IwsLiveNodeSettingsV1,
        cognition: IwsLiveNodeCognitionV1,
    ) -> Result<Self, IwsLiveNodeStartFailureV1> {
        let cancelled = AtomicBool::new(false);
        let prepared = (|| -> Result<_, String> {
            let mut stop = || cancelled.load(Ordering::Acquire);
            let mut common = owners.archive.bounded_context(&owners.memory, &mut stop);
            validate_pair(&owners.primary, &owners.replica, &mut common)?;
            owners
                .work
                .runtime
                .validate_context_bounded_v1(&mut common)
                .map_err(|e| e.to_string())?;
            owners
                .work
                .cold
                .validate_context(&mut common)
                .map_err(|e| e.to_string())?;
            let p = owners.primary.current.current().expect("validated primary");
            let r = owners.replica.current.current().expect("validated replica");
            let authority = p.source.authority();
            let definition = authority.definition();
            common
                .charge(
                    1,
                    (settings.world_id.len() + definition.world.world_id.len() + 64) as u64,
                )
                .map_err(|e| e.to_string())?;
            let mut writer = [0u8; 32];
            if definition.profile_version != IWS_AUTHORITY_PROFILE_VERSION
                || definition.world.world_id != settings.world_id
                || definition.predecessor_authority_id.is_some()
                || authority.durable_set_activation_digest().is_some()
                || hex::decode_to_slice(&definition.writer_public_key, &mut writer).is_err()
                || writer != settings.writer.verifying_key().to_bytes()
            {
                return Err(invalid(
                    "live Node requires the exact same-epoch IWS initial authority/writer",
                ));
            }
            let p_authority = authority
                .context_digest_bounded(&mut common)
                .map_err(|e| e.to_string())?;
            let r_authority = r
                .source
                .authority()
                .context_digest_bounded(&mut common)
                .map_err(|e| e.to_string())?;
            let cold_authority = settings
                .cold
                .authority
                .context_digest_bounded(&mut common)
                .map_err(|e| e.to_string())?;
            common.charge(1, 192).map_err(|e| e.to_string())?;
            if *p_authority != *r_authority || *p_authority != *cold_authority {
                return Err(invalid("live Node dual/cold authority context changed"));
            }
            let fee = owners
                .memory
                .reserve(
                    size_of::<Shared>() + 2 * size_of::<usize>(),
                    MemoryClass::Ordinary,
                )
                .map_err(|e| e.to_string())?;
            let cancellation_fee = owners
                .memory
                .reserve(
                    size_of::<ReservedValue<AtomicBool>>() + 2 * size_of::<usize>(),
                    MemoryClass::Ordinary,
                )
                .map_err(|e| e.to_string())?;
            let returned = owners
                .memory
                .reserve(0, MemoryClass::Ordinary)
                .map_err(|e| e.to_string())?;
            // Caller-owned hints move here. Retain a conservative complete
            // BTree node/link/key/value layout; later inserts retain their own
            // additional charges in this same reservation.
            let hint_bytes =
                super::node_hints::mailbox_hint_heap_bytes_v1(settings.cold.mailbox_hints.len())
                    .map_err(|e| e.to_string())?;
            common
                .charge(settings.cold.mailbox_hints.len() as u64, hint_bytes as u64)
                .map_err(|e| e.to_string())?;
            let hints_fee = owners
                .memory
                .reserve(hint_bytes, MemoryClass::Ordinary)
                .map_err(|e| e.to_string())?;
            drop(common);
            owners
                .primary
                .cognition_parent_sources_bounded(
                    &cognition.startup,
                    &owners.memory,
                    &owners.archive,
                    &cancelled,
                )
                .map_err(|e| e.to_string())?;
            let boundary = owners
                .primary
                .read_node_execution_boundary_bounded(
                    &owners.memory,
                    &owners.archive,
                    &settings.input_limits,
                    &cancelled,
                )
                .map_err(|e| e.to_string())?;
            let replica = owners
                .replica
                .read_node_execution_boundary_bounded(
                    &owners.memory,
                    &owners.archive,
                    &settings.input_limits,
                    &cancelled,
                )
                .map_err(|e| e.to_string())?;
            owners
                .archive
                .charge(1, 512, &mut || cancelled.load(Ordering::Acquire))
                .map_err(|e| e.to_string())?;
            if boundary.boundary() != replica.boundary() || boundary.genesis() != replica.genesis()
            {
                return Err(invalid("live Node dual accepted bootstrap mismatch"));
            }
            Ok((boundary, fee, cancellation_fee, returned, hints_fee))
        })();
        match prepared {
            Err(cause) => Err(IwsLiveNodeStartFailureV1 {
                cause,
                owners,
                settings,
                cognition,
            }),
            Ok((boundary, fee, cancellation_fee, returned, retained_hints_fee)) => {
                let IwsLiveNodeOwnersV1 {
                    primary,
                    replica,
                    memory,
                    archive,
                    work,
                } = owners;
                let record = primary
                    .current_record_id()
                    .expect("validated current owner");
                Ok(Self(Arc::new(Shared {
                    state: Mutex::new(State {
                        primary,
                        replica,
                        memory,
                        archive,
                        settings,
                        cognition: CognitionState::new(cognition, record),
                        boundary,
                        phase: Some(Phase::Idle(work)),
                        original: None,
                        returned,
                        retained_hints_fee,
                    }),
                    cancelled: IwsLiveNodeCancellationV1(Arc::new(
                        cancellation_fee.retain(cancelled),
                    )),
                    _fee: fee,
                })))
            }
        }
    }

    pub(crate) fn hook(&self) -> IwsLiveNodeHookV1 {
        IwsLiveNodeHookV1(Arc::clone(&self.0))
    }
    pub(crate) fn cancellation_handle(&self) -> IwsLiveNodeCancellationV1 {
        self.0.cancelled.clone()
    }

    /// The consumer sees only a completed cut under the same owner lock, pool,
    /// Common and cancellation flag. It cannot take or replace either Host.
    pub(crate) fn with_current_bounded(
        &self,
        run: impl FnOnce(
            &IwsHostRuntimeV1,
            &IwsHostRuntimeV1,
            &MemoryBudget,
            &FormalArchiveReadWorkV1,
            &AtomicBool,
        ) -> Result<(), String>,
    ) -> Result<(), String> {
        let state = self
            .0
            .state
            .lock()
            .map_err(|_| invalid("live Node owner poisoned"))?;
        if !matches!(state.phase, Some(Phase::Idle(_))) || state.original.is_some() {
            return Err(invalid("live Node completed cut is not yet available"));
        }
        let mut stop = || self.0.cancelled.flag().load(Ordering::Acquire);
        let mut common = state.archive.bounded_context(&state.memory, &mut stop);
        validate_pair(&state.primary, &state.replica, &mut common)?;
        state
            .boundary
            .validate_current_bounded(&state.primary, &mut common)
            .map_err(|e| e.to_string())?;
        drop(common);
        run(
            &state.primary,
            &state.replica,
            &state.memory,
            &state.archive,
            self.0.cancelled.flag(),
        )
    }

    /// Available after Node stop/join and hook drop, only at a completed cut.
    /// Pending or failed work stays inside the returned original owner.
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn into_parts(self) -> Result<IwsLiveNodePartsV1, Self> {
        let shared = match Arc::try_unwrap(self.0) {
            Ok(shared) => shared,
            Err(shared) => return Err(Self(shared)),
        };
        let Shared {
            state,
            cancelled,
            _fee: fee,
        } = shared;
        if state.is_poisoned() {
            return Err(Self(Arc::new(Shared {
                state,
                cancelled,
                _fee: fee,
            })));
        }
        let mut state = state
            .into_inner()
            .unwrap_or_else(|_| unreachable!("exclusive checked Node mutex"));
        if state.original.is_some() || !matches!(state.phase, Some(Phase::Idle(_))) {
            return Err(Self(Arc::new(Shared {
                state: Mutex::new(state),
                cancelled,
                _fee: fee,
            })));
        }
        let Some(Phase::Idle(work)) = state.phase.take() else {
            unreachable!()
        };
        Ok(IwsLiveNodePartsV1 {
            owners: IwsLiveNodeOwnersV1 {
                primary: state.primary,
                replica: state.replica,
                memory: state.memory,
                archive: state.archive,
                work,
            },
            settings: state.settings,
            cognition: state.cognition,
            _returned: state.returned,
            _hints: state.retained_hints_fee,
            _fee: fee,
        })
    }
}

impl std::fmt::Debug for IwsLiveNodeOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IwsLiveNodeOwnerV1").finish_non_exhaustive()
    }
}

enum VerifiedContinuation<'a, 's> {
    Fresh(&'a VerifiedNodeLocalContinuationV1<'s>),
    Restored(&'a VerifiedNodeRestoredContinuationV1<'s>),
}
impl VerifiedContinuation<'_, '_> {
    fn original(&self) -> &NodeLocalExecutionContinuation {
        match self {
            Self::Fresh(v) => v.original(),
            Self::Restored(v) => v.original(),
        }
    }
}

impl State {
    fn copy_bootstrap(&mut self, cancelled: &AtomicBool) -> Result<IwsLiveNodeBootstrapV1, String> {
        let mut stop = || cancelled.load(Ordering::Acquire);
        let mut common = self.archive.bounded_context(&self.memory, &mut stop);
        match self
            .phase
            .as_ref()
            .ok_or_else(|| invalid("live Node phase missing"))?
        {
            Phase::Idle(_) => {
                validate_pair(&self.primary, &self.replica, &mut common)?;
                self.boundary
                    .validate_current_bounded(&self.primary, &mut common)
                    .map_err(|e| e.to_string())?;
            }
            Phase::Attempt(attempt) => {
                let original = self
                    .original
                    .as_ref()
                    .ok_or_else(|| invalid("live Node pending original missing"))?;
                attempt
                    .validate_callback_bounded(&original.value().context, &mut common)
                    .map_err(|e| e.to_string())?;
                self.validate_pending_boundary(original, &mut common)?;
            }
            Phase::Applied(applied) => {
                let original = self
                    .original
                    .as_ref()
                    .ok_or_else(|| invalid("live Node applied original missing"))?;
                applied
                    .validate_callback_bounded(&original.value().context, &mut common)
                    .map_err(|e| e.to_string())?;
                self.validate_pending_boundary(original, &mut common)?;
            }
            Phase::StartFailure(failure) => return Err(format!("{:?}", failure.cause)),
        }
        let boundary = self.boundary.boundary();
        let mut bytes = size_of::<IwsLiveNodeBootstrapV1>();
        for text in [
            &boundary.consensus_block_hash,
            &boundary.execution_block_hash,
            &boundary.execution_state_root,
        ] {
            bytes = bytes
                .checked_add(text.len())
                .ok_or_else(|| invalid("live Node bootstrap layout"))?;
        }
        if let Some(genesis) = self.boundary.genesis() {
            bytes = bytes
                .checked_add(genesis.world_id().len())
                .and_then(|n| n.checked_add(genesis.initial_record_id().len()))
                .ok_or_else(|| invalid("live Node genesis bootstrap layout"))?;
        }
        common.charge(1, bytes as u64).map_err(|e| e.to_string())?;
        let mut fee = self
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|e| e.to_string())?;
        self.returned.merge(&mut fee).map_err(|e| e.to_string())?;
        Ok(match self.boundary.genesis() {
            Some(value) => IwsLiveNodeBootstrapV1::Genesis(value.clone()),
            None => IwsLiveNodeBootstrapV1::Normal(boundary.clone()),
        })
    }

    fn validate_pending_boundary(
        &self,
        original: &PaidNodeOriginalV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), String> {
        original
            .validate_callback(&original.value().context, common)
            .map_err(|e| e.to_string())?;
        let original = original.value();
        let boundary = self.boundary.boundary();
        common.charge(1, 512).map_err(|e| e.to_string())?;
        if original.predecessor_committed_height != boundary.height
            || original.predecessor_execution_height != boundary.height
            || original.predecessor_node_block_hash.as_deref()
                != Some(boundary.consensus_block_hash.as_str())
            || original.predecessor_execution_block_hash.as_deref()
                != Some(boundary.execution_block_hash.as_str())
            || original.predecessor_execution_state_root.as_deref()
                != Some(boundary.execution_state_root.as_str())
            || boundary.height.checked_add(1) != Some(original.context.height)
            || original.context.world_id != self.settings.world_id
        {
            return Err(invalid("live Node retained predecessor changed"));
        }
        Ok(())
    }

    fn prepare(
        &mut self,
        source: VerifiedContinuation<'_, '_>,
        cancelled: &AtomicBool,
    ) -> Result<(), String> {
        let mut stop = || cancelled.load(Ordering::Acquire);
        let mut common = self.archive.bounded_context(&self.memory, &mut stop);
        if let Some(original) = &self.original {
            original
                .validate_same(source.original(), &mut common)
                .map_err(|e| e.to_string())?;
            return match self.phase.as_ref() {
                Some(Phase::StartFailure(failure)) => Err(format!("{:?}", failure.cause)),
                Some(Phase::Attempt(_) | Phase::Applied(_)) => Ok(()),
                _ => Err(invalid("live Node original lost its execution owner")),
            };
        }
        if !matches!(self.phase, Some(Phase::Idle(_))) {
            return Err(invalid("live Node original missing from active phase"));
        }
        let original = PaidNodeOriginalV1::capture(
            source.original(),
            self.settings.input_limits.max_graph_bytes,
            &mut common,
        )
        .map_err(|e| e.to_string())?;
        self.validate_pending_boundary(&original, &mut common)?;
        drop(common);
        let Some(Phase::Idle(work)) = self.phase.take() else {
            unreachable!()
        };
        self.original = Some(original);
        let environment = self
            .settings
            .environment(&self.memory, &self.archive, cancelled);
        let result = match source {
            VerifiedContinuation::Fresh(source) => NodeNormalExecutionAttemptV1::fresh_bounded(
                &self.primary,
                source,
                work,
                &environment,
            ),
            VerifiedContinuation::Restored(source) => {
                NodeNormalExecutionAttemptV1::restored_bounded(
                    &self.primary,
                    source,
                    work,
                    &environment,
                )
            }
        };
        match result {
            Ok(attempt) => {
                self.phase = Some(Phase::Attempt(attempt));
                Ok(())
            }
            Err(failure) => {
                let cause = format!("{:?}", failure.cause);
                self.phase = Some(Phase::StartFailure(failure));
                Err(cause)
            }
        }
    }
}

impl NodeExecutionHook for IwsLiveNodeHookV1 {
    fn guarded_local_execution(&self) -> bool {
        true
    }

    fn current_guarded_local_execution_bootstrap(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionBootstrapV1>, String> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| invalid("live Node owner poisoned"))?;
        Ok(Some(
            match state.copy_bootstrap(self.0.cancelled.flag())? {
                IwsLiveNodeBootstrapV1::Genesis(value) => {
                    NodeLocalExecutionBootstrapV1::Genesis(value)
                }
                IwsLiveNodeBootstrapV1::Normal(value) => {
                    NodeLocalExecutionBootstrapV1::Normal(value)
                }
            },
        ))
    }

    fn qualified_local_execution_genesis_bootstrap(
        &mut self,
    ) -> Result<Option<NodeExecutionGenesisBootstrap>, String> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| invalid("live Node owner poisoned"))?;
        match state.copy_bootstrap(self.0.cancelled.flag())? {
            IwsLiveNodeBootstrapV1::Genesis(value) => Ok(Some(value)),
            IwsLiveNodeBootstrapV1::Normal(_) => Ok(None),
        }
    }

    fn prepare_verified_local_continuation(
        &mut self,
        source: &VerifiedNodeLocalContinuationV1<'_>,
    ) -> Result<(), String> {
        self.0
            .state
            .lock()
            .map_err(|_| invalid("live Node owner poisoned"))?
            .prepare(VerifiedContinuation::Fresh(source), self.0.cancelled.flag())
    }

    fn restored_verified_local_continuation(
        &mut self,
        source: &VerifiedNodeRestoredContinuationV1<'_>,
    ) -> Result<(), String> {
        self.0
            .state
            .lock()
            .map_err(|_| invalid("live Node owner poisoned"))?
            .prepare(
                VerifiedContinuation::Restored(source),
                self.0.cancelled.flag(),
            )
    }

    fn pending_local_continuation(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| invalid("live Node owner poisoned"))?;
        let State {
            original,
            phase,
            memory,
            archive,
            ..
        } = &mut *state;
        match original {
            Some(original) => original
                .copy_bounded(&mut archive.bounded_context(memory, &mut || {
                    self.0.cancelled.flag().load(Ordering::Acquire)
                }))
                .map(Some)
                .map_err(|e| e.to_string()),
            None if matches!(phase, Some(Phase::Idle(_))) => Ok(None),
            None => Err(invalid("live Node pending owner is incomplete")),
        }
    }

    fn on_commit(
        &mut self,
        _context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err(invalid(
            "live IWS Node requires its verified guarded continuation",
        ))
    }

    fn on_commit_outcome_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        expected_block: Option<&str>,
        expected_root: Option<&str>,
    ) -> Result<NodeExecutionCommitOutcome, String> {
        if expected_block.is_some() || expected_root.is_some() {
            return Err(invalid(
                "live IWS Node rejects external execution expectations",
            ));
        }
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| invalid("live Node owner poisoned"))?;
        let State {
            primary,
            replica,
            memory,
            archive,
            settings,
            cognition,
            original,
            phase,
            ..
        } = &mut *state;
        let cancelled = self.0.cancelled.flag();
        original
            .as_ref()
            .ok_or_else(|| invalid("live Node commit lacks its original decision"))?
            .validate_callback(
                &context,
                &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
            )
            .map_err(|e| e.to_string())?;
        let applied = match phase
            .take()
            .ok_or_else(|| invalid("live Node phase missing"))?
        {
            Phase::Attempt(attempt) => match attempt.advance_with_startup_bounded(
                primary,
                replica,
                &context,
                cognition.startup(),
                &settings.environment(memory, archive, cancelled),
            ) {
                Ok(applied) => applied,
                Err(attempt) => {
                    let terminal = attempt.terminal_failure().map(ToString::to_string);
                    *phase = Some(Phase::Attempt(attempt));
                    return match terminal {
                        Some(cause) => Err(cause),
                        None => Ok(NodeExecutionCommitOutcome::Deferred),
                    };
                }
            },
            Phase::Applied(applied) => applied,
            Phase::StartFailure(failure) => {
                let cause = format!("{:?}", failure.cause);
                *phase = Some(Phase::StartFailure(failure));
                return Err(cause);
            }
            retained @ Phase::Idle(_) => {
                *phase = Some(retained);
                return Err(invalid("live Node commit has no prepared attempt"));
            }
        };
        let mut applied = applied;
        let result = applied.copy_result_bounded(
            &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        );
        *phase = Some(Phase::Applied(applied));
        // A response-copy failure does not discard an accepted result, replace
        // its work, or re-enter Runtime on the next callback.
        result
            .map(NodeExecutionCommitOutcome::Applied)
            .map_err(|e| e.to_string())
    }

    fn complete_local_continuation(
        &mut self,
        context: &NodeExecutionCommitContext,
    ) -> Result<(), String> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| invalid("live Node owner poisoned"))?;
        let State {
            primary,
            memory,
            archive,
            settings,
            original,
            phase,
            cognition,
            boundary,
            retained_hints_fee,
            ..
        } = &mut *state;
        let cancelled = self.0.cancelled.flag();
        let mut stop = || cancelled.load(Ordering::Acquire);
        let mut common = archive.bounded_context(memory, &mut stop);
        original
            .as_ref()
            .ok_or_else(|| invalid("live Node completion lacks original decision"))?
            .validate_callback(context, &mut common)
            .map_err(|e| e.to_string())?;
        match phase.as_ref() {
            Some(Phase::Applied(applied)) => applied
                .validate_callback_bounded(context, &mut common)
                .map_err(|e| e.to_string())?,
            _ => {
                return Err(invalid(
                    "live Node completion precedes accepted application",
                ));
            }
        }
        drop(common);
        if let Some(Phase::Applied(applied)) = phase.as_mut() {
            cognition.prepare_next_parent(
                primary,
                applied,
                context,
                &settings.environment(memory, archive, cancelled),
            )?;
        }
        // Save genuine current Event occurrences before releasing this applied
        // owner. A partial/cancelled hint update retains both the result and
        // all charged DATA; its retry cannot re-enter Runtime or refill work.
        super::node_hints::retain_current_mailbox_hints_bounded_v1(
            primary,
            &mut settings.cold.mailbox_hints,
            retained_hints_fee,
            memory,
            archive,
            cancelled,
        )
        .map_err(|e| e.to_string())?;
        let mut common = archive.bounded_context(memory, &mut stop);
        let applied = match phase
            .take()
            .ok_or_else(|| invalid("live Node phase missing"))?
        {
            Phase::Applied(applied) => applied,
            retained => {
                *phase = Some(retained);
                return Err(invalid(
                    "live Node completion precedes accepted application",
                ));
            }
        };
        match applied.complete_with_boundary_bounded(context, &mut common) {
            Ok((work, accepted)) => {
                *boundary = accepted;
                *original = None;
                *phase = Some(Phase::Idle(work));
                Ok(())
            }
            Err((cause, applied)) => {
                *phase = Some(Phase::Applied(applied));
                Err(cause.to_string())
            }
        }
    }
}
