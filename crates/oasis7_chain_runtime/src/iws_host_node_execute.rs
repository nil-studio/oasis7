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
    NormalCommitHeaderLimitsV1, NormalHostCognitionParentSourcesV1, RuntimeSourceCompiler,
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
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn requires_preparation_fixture_v1(&self) -> bool {
        matches!(
            self.phase,
            Some(Phase::Freeze { .. } | Phase::Execute { .. })
        )
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn advance_bounded(
        mut self,
        primary: &mut IwsHostRuntimeV1,
        replica: &mut IwsHostRuntimeV1,
        context: &NodeExecutionCommitContext,
        cognition: NormalHostCognitionParentSourcesV1<'_>,
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
                    let result = execute_and_commit_frozen_normal_bounded_v1(
                        primary,
                        replica,
                        sources,
                        work,
                        e.writer,
                        e.input_limits,
                        e.action_limits,
                        cognition
                            .take()
                            .expect("one Runtime dispatch per retained attempt"),
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

    pub(crate) fn complete_bounded(
        self,
        context: &NodeExecutionCommitContext,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<NormalCommitWorkV1, (IwsHostRuntimeErrorV1, Self)> {
        match same_context(&self.original, context, self.core, common) {
            Ok(()) => Ok(self.work),
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
