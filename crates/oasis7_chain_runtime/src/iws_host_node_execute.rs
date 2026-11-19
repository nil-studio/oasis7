//! Fixed actual Node -> Frozen -> Runtime -> dual publication consumer.
//! A prepared execution is never re-entered during output/publication recovery.
use super::*;
use crate::iws_normal_input::PaidNormalTickSourcesV1;
use crate::iws_normal_input_decode::NormalInputDecodeLimitsV1;
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_cbor_data::CborDataLimitsV1,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    context_wire,
};
use oasis7_node::{
    NodeExecutionCommitContext, NodeExecutionCommitResult, VerifiedNodeLocalContinuationV1,
    VerifiedNodeRestoredContinuationV1,
};
use oasis7_proto::memory_budget::{MemoryReservation, ReservedValue};
use oasis7_runtime::{
    NormalCommitHeaderLimitsV1, NormalHostCognitionParentSourcesV1,
    PaidNormalHostCognitionParentSourcesV1, RuntimeSourceCompiler,
    incremental_normal_sources::{NormalTickParentRefsV1, bind_normal_tick_parent_v1},
};

/// Configuration and original borrowed resources, not a replacement work bank.
pub(crate) struct NodeNormalExecutionEnvironmentV1<'a> {
    pub writer: &'a ed25519_dalek::SigningKey,
    pub input_limits: &'a NormalInputDecodeLimitsV1,
    pub action_limits: &'a CborDataLimitsV1,
    pub compiler: &'a RuntimeSourceCompiler,
    pub policy: NormalCommitHeaderLimitsV1,
    pub max_action_bytes: usize,
    pub max_grant_payload_bytes: u64,
    pub primary_provider: &'a RemoteMonotonicEndpointProvider,
    pub replica_provider: &'a RemoteMonotonicEndpointProvider,
    pub primary_policy: &'a TrustedFencingPolicyV1,
    pub replica_policy: &'a TrustedFencingPolicyV1,
    pub memory: &'a MemoryBudget,
    pub archive: &'a FormalArchiveReadWorkV1,
    pub cancelled: &'a AtomicBool,
    pub cold: super::normal_commit::NormalColdRecoveryInputsV1<'a>,
}

enum Phase {
    Freeze {
        attempt: NodeNormalFreezeAttemptV1,
        work: NormalCommitWorkV1,
    },
    Execute {
        sources: PaidNormalTickSourcesV1,
        work: NormalCommitWorkV1,
    },
    /// Preserve the first failed Runtime preparation. No callback silently
    /// reruns a partially evaluated tick or replaces any original allowance.
    FailedPreparation {
        cause: IwsHostRuntimeErrorV1,
        _sources: PaidNormalTickSourcesV1,
        _work: NormalCommitWorkV1,
    },
    Output(NormalOutputCommitFailureV1),
    ReadAcceptedBoundary(NormalCommitWorkV1),
}

pub(crate) struct NodeNormalExecutionAttemptV1 {
    original: ReservedValue<Vec<u8>>,
    core: WorkIdentityV1,
    phase: Option<Phase>,
    cause: Option<IwsHostRuntimeErrorV1>,
    fee: MemoryReservation,
}

pub(crate) struct NodeNormalStartFailureV1 {
    pub cause: NormalPackageErrorV1,
    pub work: NormalCommitWorkV1,
}

/// Only the pre-Runtime phases may borrow the current Cognition parent. A
/// retained output already owns its Runtime result, including on cold retry.
#[cfg(any(test, feature = "iws-host-native-test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeNormalRuntimeInputV1 {
    Required,
    NotRequired,
}

fn invalid() -> FormalError {
    FormalError::Invalid("normal Node attempt original context/Core/pool changed".into())
}

fn same_context(
    original: &ReservedValue<Vec<u8>>,
    context: &NodeExecutionCommitContext,
    core: WorkIdentityV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), IwsHostRuntimeErrorV1> {
    common.consume_steps(1)?;
    if core != common.work().identity() || !original.uses_budget(common.memory()) {
        return Err(invalid().into());
    }
    let actual = context_wire::encode_paid(
        context,
        64 << 20,
        common,
        "normal callback context bound",
        "original normal Node context",
    )?;
    if actual.len() != original.len() {
        return Err(invalid().into());
    }
    for (left, right) in actual.chunks(8192).zip(original.chunks(8192)) {
        common.charge(1, (left.len() as u64) * 2)?;
        if left != right {
            return Err(invalid().into());
        }
    }
    Ok(())
}

impl NodeNormalExecutionAttemptV1 {
    /// Runtime preparation has already consumed its one attempt. The original
    /// failure remains owned with its sources/work and cannot become a retry.
    pub(crate) fn terminal_failure(&self) -> Option<&IwsHostRuntimeErrorV1> {
        match self.phase.as_ref() {
            Some(Phase::FailedPreparation { cause, .. }) => Some(cause),
            _ => None,
        }
    }

    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn runtime_input(&self) -> NodeNormalRuntimeInputV1 {
        if matches!(
            self.phase,
            Some(Phase::Freeze { .. } | Phase::Execute { .. })
        ) {
            NodeNormalRuntimeInputV1::Required
        } else {
            NodeNormalRuntimeInputV1::NotRequired
        }
    }
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn requires_preparation_fixture_v1(&self) -> bool {
        self.runtime_input() == NodeNormalRuntimeInputV1::Required
    }
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn cold_disposition_fixture_v1(&self) -> Option<NormalCommitRecoveryDispositionV1> {
        match self.phase.as_ref()? {
            Phase::Output(NormalOutputCommitFailureV1::Publication(
                NormalCommitStartFailureV1::Publication(failure),
            )) => Some(failure.disposition()),
            _ => None,
        }
    }
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn retained_source_work_targets_fixture_v1(&self) -> Option<[u64; 2]> {
        match self.phase.as_ref()? {
            Phase::Output(NormalOutputCommitFailureV1::Publication(
                NormalCommitStartFailureV1::Publication(failure),
            )) => failure.retained_source_work_targets_fixture_v1(),
            _ => None,
        }
    }
    pub(crate) fn fresh_bounded(
        primary: &IwsHostRuntimeV1,
        original: &VerifiedNodeLocalContinuationV1<'_>,
        work: NormalCommitWorkV1,
        environment: &NodeNormalExecutionEnvironmentV1<'_>,
    ) -> Result<Self, NodeNormalStartFailureV1> {
        let e = environment;
        let preparation = (|| {
            let mut stop = || e.cancelled.load(Ordering::Acquire);
            let mut common = e.archive.bounded_context(e.memory, &mut stop);
            work.runtime.validate_context_bounded_v1(&mut common)?;
            work.cold
                .validate_context(&mut common)
                .map_err(NormalPackageErrorV1::Boundary)?;
            let fee = e
                .memory
                .reserve(size_of::<Self>(), MemoryClass::Ordinary)
                .map_err(|error| NormalPackageErrorV1::Value(error.into()))?;
            let wire = context_wire::encode_paid(
                &original.original().context,
                64 << 20,
                &mut common,
                "normal callback context bound",
                "original normal Node context",
            )?;
            let core = common.work().identity();
            let attempt = NodeNormalFreezeAttemptV1::fresh_bounded(
                primary,
                original,
                e.writer,
                e.input_limits,
                e.memory,
                e.archive,
                e.cancelled,
            )?;
            Ok::<_, NormalPackageErrorV1>((fee, wire, core, attempt))
        })();
        match preparation {
            Ok((fee, original, core, attempt)) => Ok(Self {
                original,
                core,
                phase: Some(Phase::Freeze { attempt, work }),
                cause: None,
                fee,
            }),
            Err(cause) => Err(NodeNormalStartFailureV1 { cause, work }),
        }
    }

    pub(crate) fn restored_bounded(
        primary: &IwsHostRuntimeV1,
        original: &VerifiedNodeRestoredContinuationV1<'_>,
        work: NormalCommitWorkV1,
        environment: &NodeNormalExecutionEnvironmentV1<'_>,
    ) -> Result<Self, NodeNormalStartFailureV1> {
        let e = environment;
        let preparation = (|| {
            let mut stop = || e.cancelled.load(Ordering::Acquire);
            let mut common = e.archive.bounded_context(e.memory, &mut stop);
            work.runtime.validate_context_bounded_v1(&mut common)?;
            work.cold
                .validate_context(&mut common)
                .map_err(NormalPackageErrorV1::Boundary)?;
            let fee = e
                .memory
                .reserve(size_of::<Self>(), MemoryClass::Ordinary)
                .map_err(|error| NormalPackageErrorV1::Value(error.into()))?;
            let wire = context_wire::encode_paid(
                &original.original().context,
                64 << 20,
                &mut common,
                "normal callback context bound",
                "original normal Node context",
            )?;
            let core = common.work().identity();
            let attempt = NodeNormalFreezeAttemptV1::restored_bounded(
                primary,
                original,
                e.writer,
                e.input_limits,
                e.memory,
                e.archive,
                e.cancelled,
            )?;
            Ok::<_, NormalPackageErrorV1>((fee, wire, core, attempt))
        })();
        match preparation {
            Ok((fee, original, core, attempt)) => Ok(Self {
                original,
                core,
                phase: Some(Phase::Freeze { attempt, work }),
                cause: None,
                fee,
            }),
            Err(cause) => Err(NodeNormalStartFailureV1 { cause, work }),
        }
    }

    pub(crate) fn validate_callback_bounded(
        &self,
        context: &NodeExecutionCommitContext,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        if !self.fee.uses_budget(common.memory()) {
            return Err(invalid().into());
        }
        same_context(&self.original, context, self.core, common)
    }

    pub(crate) fn advance_bounded(
        self,
        primary: &mut IwsHostRuntimeV1,
        replica: &mut IwsHostRuntimeV1,
        context: &NodeExecutionCommitContext,
        cognition: NormalHostCognitionParentSourcesV1<'_>,
        environment: &NodeNormalExecutionEnvironmentV1<'_>,
    ) -> Result<PaidNodeAppliedNormalV1, Self> {
        self.advance_with_input_bounded(primary, replica, context, |_| Ok(cognition), environment)
    }

    /// Bind the startup evidence only after both Frozen endpoints are retained.
    /// A partial Freeze retry may have consumed the mutable Host slots already;
    /// its genuine Frozen parent, rather than a reconstructed Host, is the
    /// source for this borrow. Output recovery never evaluates this resolver.
    pub(crate) fn advance_with_startup_bounded(
        self,
        primary: &mut IwsHostRuntimeV1,
        replica: &mut IwsHostRuntimeV1,
        context: &NodeExecutionCommitContext,
        startup: Option<&PaidNormalHostCognitionParentSourcesV1>,
        environment: &NodeNormalExecutionEnvironmentV1<'_>,
    ) -> Result<PaidNodeAppliedNormalV1, Self> {
        let e = environment;
        self.advance_with_input_bounded(
            primary,
            replica,
            context,
            |sources| {
                let Some(evidence) = startup else {
                    return Ok(NormalHostCognitionParentSourcesV1::Resident);
                };
                sources
                    .frozen_sources()
                    .with_primary_parent_read_view_bounded(
                        e.memory,
                        e.archive,
                        &mut || e.cancelled.load(Ordering::Acquire),
                        |parent| {
                            let mut stop = || e.cancelled.load(Ordering::Acquire);
                            let mut common = e.archive.bounded_context(e.memory, &mut stop);
                            let parent = bind_normal_tick_parent_v1(
                                NormalTickParentRefsV1 {
                                    core: parent.core(),
                                    storage: parent.storage(),
                                    roots: parent.roots(),
                                    runtime: parent.runtime(),
                                    cognition: parent.cognition(),
                                    retention: parent.retention_ledger(),
                                    head: parent.head(),
                                    authority: parent.authority(),
                                },
                                &mut common,
                            )?;
                            evidence
                                .as_sources_bounded(&parent, &mut common)
                                .map_err(IwsHostRuntimeErrorV1::NormalSource)
                        },
                    )
            },
            environment,
        )
    }

    fn advance_with_input_bounded<'c>(
        mut self,
        primary: &mut IwsHostRuntimeV1,
        replica: &mut IwsHostRuntimeV1,
        context: &NodeExecutionCommitContext,
        cognition: impl FnOnce(
            &PaidNormalTickSourcesV1,
        )
            -> Result<NormalHostCognitionParentSourcesV1<'c>, IwsHostRuntimeErrorV1>,
        environment: &NodeNormalExecutionEnvironmentV1<'_>,
    ) -> Result<PaidNodeAppliedNormalV1, Self> {
        let e = environment;
        if let Err(cause) = self.validate_callback_bounded(
            context,
            &mut e
                .archive
                .bounded_context(e.memory, &mut || e.cancelled.load(Ordering::Acquire)),
        ) {
            self.cause = Some(cause);
            return Err(self);
        }
        let mut cognition = Some(cognition);
        loop {
            if let Err(cause) = e
                .archive
                .consume_steps(1, &mut || e.cancelled.load(Ordering::Acquire))
            {
                self.cause = Some(cause.into());
                return Err(self);
            }
            self.cause = None;
            match self.phase.take().expect("retained fixed Node phase") {
                Phase::Freeze { attempt, mut work } => {
                    match attempt.advance_bounded(
                        primary,
                        replica,
                        e.primary_provider,
                        e.replica_provider,
                        e.primary_policy,
                        e.replica_policy,
                        &mut work.input_material,
                        e.memory,
                        e.archive,
                        e.cancelled,
                    ) {
                        Ok(sources) => self.phase = Some(Phase::Execute { sources, work }),
                        Err(attempt) => {
                            self.phase = Some(Phase::Freeze { attempt, work });
                            return Err(self);
                        }
                    }
                }
                Phase::Execute { sources, work } => {
                    let cognition = match cognition
                        .take()
                        .expect("one Runtime input binding per retained attempt")(
                        &sources
                    ) {
                        Ok(cognition) => cognition,
                        Err(cause) => {
                            self.phase = Some(Phase::Execute { sources, work });
                            self.cause = Some(cause);
                            return Err(self);
                        }
                    };
                    let result = execute_and_commit_frozen_normal_bounded_v1(
                        primary,
                        replica,
                        sources,
                        work,
                        e.writer,
                        e.input_limits,
                        e.action_limits,
                        cognition,
                        e.compiler,
                        NormalCommitHeaderLimitsV1 {
                            transaction: e.policy.transaction,
                            checkpoint: e.policy.checkpoint,
                        },
                        e.max_action_bytes,
                        e.max_grant_payload_bytes,
                        e.primary_provider,
                        e.replica_provider,
                        e.primary_policy,
                        e.replica_policy,
                        e.memory,
                        e.archive,
                        e.cancelled,
                    );
                    match result {
                        Ok(work) => self.phase = Some(Phase::ReadAcceptedBoundary(work)),
                        Err(NormalExecutionCommitFailureV1::BeforePreparation {
                            cause,
                            sources,
                            work,
                        }) => {
                            self.phase = Some(Phase::FailedPreparation {
                                cause,
                                _sources: sources,
                                _work: work,
                            });
                            return Err(self);
                        }
                        Err(NormalExecutionCommitFailureV1::AfterPreparation(failure)) => {
                            self.phase = Some(Phase::Output(failure));
                            return Err(self);
                        }
                    }
                }
                phase @ Phase::FailedPreparation { .. } => {
                    self.phase = Some(phase);
                    return Err(self);
                }
                Phase::Output(failure) => {
                    match failure.recover_bounded_v1(
                        primary,
                        replica,
                        e.writer,
                        e.input_limits,
                        e.primary_provider,
                        e.replica_provider,
                        e.primary_policy,
                        e.replica_policy,
                        e.memory,
                        e.archive,
                        e.cancelled,
                        &e.cold,
                        e.compiler,
                    ) {
                        Ok(work) => self.phase = Some(Phase::ReadAcceptedBoundary(work)),
                        Err(failure) => {
                            self.phase = Some(Phase::Output(failure));
                            return Err(self);
                        }
                    }
                }
                Phase::ReadAcceptedBoundary(work) => {
                    let result = (|| {
                        let p = primary.read_node_execution_boundary_bounded(
                            e.memory,
                            e.archive,
                            e.input_limits,
                            e.cancelled,
                        )?;
                        let r = replica.read_node_execution_boundary_bounded(
                            e.memory,
                            e.archive,
                            e.input_limits,
                            e.cancelled,
                        )?;
                        e.archive
                            .charge(1, 512, &mut || e.cancelled.load(Ordering::Acquire))?;
                        if primary.current_record_id() != replica.current_record_id()
                            || p.boundary() != r.boundary()
                            || p.genesis().is_some()
                            || p.boundary().height != context.height
                            || p.boundary().consensus_block_hash != context.node_block_hash
                        {
                            return Err(invalid().into());
                        }
                        Ok::<_, IwsHostRuntimeErrorV1>(p)
                    })();
                    match result {
                        Ok(boundary) => {
                            return Ok(PaidNodeAppliedNormalV1 {
                                boundary,
                                work,
                                original: self.original,
                                core: self.core,
                                fee: self.fee,
                            });
                        }
                        Err(cause) => {
                            self.phase = Some(Phase::ReadAcceptedBoundary(work));
                            self.cause = Some(cause);
                            return Err(self);
                        }
                    }
                }
            }
        }
    }
}

/// Retains the original continuing work until Node acknowledges this exact
/// applied context. The actual installed two-host result precedes this owner.
pub(crate) struct PaidNodeAppliedNormalV1 {
    boundary: node_boundary::PaidIwsNodeBoundaryV1,
    work: NormalCommitWorkV1,
    original: ReservedValue<Vec<u8>>,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
impl PaidNodeAppliedNormalV1 {
    pub(crate) fn validate_callback_bounded(
        &self,
        context: &NodeExecutionCommitContext,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        if !self.fee.uses_budget(common.memory()) {
            return Err(invalid().into());
        }
        same_context(&self.original, context, self.core, common)
    }
    pub(crate) fn copy_result_bounded(
        &mut self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<NodeExecutionCommitResult, IwsHostRuntimeErrorV1> {
        common.charge(1, 128)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(invalid().into());
        }
        let value = self.boundary.boundary();
        let bytes = size_of::<NodeExecutionCommitResult>()
            .checked_add(value.execution_block_hash.len())
            .and_then(|n| n.checked_add(value.execution_state_root.len()))
            .ok_or_else(invalid)?;
        let mut copy_fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|_| invalid())?;
        self.fee.merge(&mut copy_fee).map_err(|_| invalid())?;
        // Each result copy remains charged until the matching Node completion.
        Ok(NodeExecutionCommitResult {
            execution_height: value.height,
            execution_block_hash: value.execution_block_hash.clone(),
            execution_state_root: value.execution_state_root.clone(),
        })
    }

    /// A genuine child reopened by the fixed cold recovery path has a Built
    /// resident. Its next tick requires a new Cold Cognition admission, using
    /// the caller's original source/IO accounts and this same remaining Runtime
    /// work. This never dispatches the accepted tick's actions again.
    pub(crate) fn prepare_cold_cognition_bounded(
        &mut self,
        primary: &IwsHostRuntimeV1,
        context: &NodeExecutionCommitContext,
        io: &mut HostColdIoWorkV1,
        source_work: &mut HostCognitionStartupWorkV1,
        environment: &NodeNormalExecutionEnvironmentV1<'_>,
    ) -> Result<PaidNormalHostCognitionParentSourcesV1, HostCognitionStartupErrorV1> {
        let e = environment;
        self.validate_callback_bounded(
            context,
            &mut e
                .archive
                .bounded_context(e.memory, &mut || e.cancelled.load(Ordering::Acquire)),
        )?;
        primary.prepare_cognition_startup_bounded(
            io,
            source_work,
            &mut self.work.runtime,
            e.policy.transaction,
            e.memory,
            e.archive,
            e.cancelled,
        )
    }

    pub(crate) fn complete_bounded(
        self,
        context: &NodeExecutionCommitContext,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<NormalCommitWorkV1, (IwsHostRuntimeErrorV1, Self)> {
        self.complete_with_boundary_bounded(context, common)
            .map(|(work, _boundary)| work)
    }

    /// Continuous Node ownership moves the actual accepted boundary together
    /// with the original remaining work. It does not reread or mint a successor.
    pub(crate) fn complete_with_boundary_bounded(
        self,
        context: &NodeExecutionCommitContext,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        (NormalCommitWorkV1, node_boundary::PaidIwsNodeBoundaryV1),
        (IwsHostRuntimeErrorV1, Self),
    > {
        match same_context(&self.original, context, self.core, common) {
            Ok(()) => Ok((self.work, self.boundary)),
            Err(cause) => Err((cause, self)),
        }
    }
}

impl std::fmt::Debug for NodeNormalExecutionAttemptV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = f.debug_struct("NodeNormalExecutionAttemptV1");
        out.field("cause", &self.cause);
        match self.phase.as_ref().expect("retained Node phase") {
            Phase::Freeze { attempt, .. } => {
                out.field("frozen", attempt);
            }
            Phase::Execute { .. } => {
                out.field("phase", &"before Runtime");
            }
            Phase::FailedPreparation { cause, .. } => {
                out.field("failed_preparation", cause);
            }
            Phase::Output(failure) => {
                out.field("output", failure);
            }
            Phase::ReadAcceptedBoundary(_) => {
                out.field("phase", &"durable accepted result read");
            }
        }
        out.finish_non_exhaustive()
    }
}
