//! Consuming actual Node provenance -> dual durable Frozen -> fixed Runtime
//! input. Every partial publication retains its original endpoint recovery
//! owner; retries never recapture Node bytes or reset work.
use super::*;
use crate::iws_normal_input::{
    NormalTickSourcesAdmissionFailureV1, PaidNormalTickSourcesAdmissionV1, PaidNormalTickSourcesV1,
};
use oasis7_distfs::controlled_authority::formal::endpoint::{
    NormalFrozenFailureV1, NormalFrozenPairFailureV1, PaidNormalFrozenEndpointV1,
    PaidNormalFrozenPairV1, join_normal_frozen_endpoints_bounded,
};
use oasis7_formal_protocol::controlled_authority::formal::IncrementalFrozenMaterialWorkV1;
use oasis7_node::{VerifiedNodeLocalContinuationV1, VerifiedNodeRestoredContinuationV1};
use oasis7_proto::memory_budget::MemoryReservation;

enum Phase {
    Fresh,
    RecoverPrimary(NormalFrozenFailureV1),
    Primary(PaidNormalFrozenEndpointV1),
    RecoverReplica {
        primary: PaidNormalFrozenEndpointV1,
        failure: NormalFrozenFailureV1,
    },
    Join(PaidNormalFrozenEndpointV1, PaidNormalFrozenEndpointV1),
    RecoverPair(NormalFrozenPairFailureV1),
    Joined(PaidNormalFrozenPairV1),
    RecoverAdmission(NormalTickSourcesAdmissionFailureV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeNormalFreezeDispositionV1 {
    BeforePrimary,
    RecoverPrimary,
    BeforeReplica,
    RecoverReplica,
    JoinOrRecoverCertificate,
    BindOriginalNode,
}

pub(crate) struct NodeNormalFreezeAttemptV1 {
    input: Option<normal_package::PaidPreparedNodeNormalInputV1>,
    admission: Option<PaidNormalTickSourcesAdmissionV1>,
    phase: Option<Phase>,
    host_error: Option<IwsHostRuntimeErrorV1>,
    package_error: Option<NormalPackageErrorV1>,
    _fee: MemoryReservation,
}

impl NodeNormalFreezeAttemptV1 {
    pub(crate) fn fresh_bounded(
        primary: &IwsHostRuntimeV1,
        source: &VerifiedNodeLocalContinuationV1<'_>,
        writer: &ed25519_dalek::SigningKey,
        input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<Self, NormalPackageErrorV1> {
        let input = primary.prepare_fresh_normal_input_bounded(
            source,
            writer,
            input_limits,
            memory,
            archive,
            cancelled,
        )?;
        Self::prepared(input, memory, archive, cancelled)
    }

    pub(crate) fn restored_bounded(
        primary: &IwsHostRuntimeV1,
        source: &VerifiedNodeRestoredContinuationV1<'_>,
        writer: &ed25519_dalek::SigningKey,
        input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<Self, NormalPackageErrorV1> {
        let input = primary.prepare_restored_normal_input_bounded(
            source,
            writer,
            input_limits,
            memory,
            archive,
            cancelled,
        )?;
        Self::prepared(input, memory, archive, cancelled)
    }

    fn prepared(
        input: normal_package::PaidPreparedNodeNormalInputV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<Self, NormalPackageErrorV1> {
        let mut stop = || cancelled.load(Ordering::Acquire);
        let common = archive.bounded_context(memory, &mut stop);
        let fee = memory
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(|e| {
                NormalPackageErrorV1::Value(
                    oasis7_formal_protocol::incremental_value::ValueTreeError::Memory(e),
                )
            })?;
        let admission = PaidNormalTickSourcesAdmissionV1::reserve(&common).map_err(|e| {
            NormalPackageErrorV1::Value(
                oasis7_formal_protocol::incremental_value::ValueTreeError::Memory(e),
            )
        })?;
        Ok(Self {
            input: Some(input),
            admission: Some(admission),
            phase: Some(Phase::Fresh),
            host_error: None,
            package_error: None,
            _fee: fee,
        })
    }

    pub(crate) fn disposition(&self) -> NodeNormalFreezeDispositionV1 {
        use NodeNormalFreezeDispositionV1 as D;
        match self
            .phase
            .as_ref()
            .expect("normal Frozen phase between calls")
        {
            Phase::Fresh => D::BeforePrimary,
            Phase::RecoverPrimary(_) => D::RecoverPrimary,
            Phase::Primary(_) => D::BeforeReplica,
            Phase::RecoverReplica { .. } => D::RecoverReplica,
            Phase::Join(..) | Phase::RecoverPair(_) => D::JoinOrRecoverCertificate,
            Phase::Joined(_) | Phase::RecoverAdmission(_) => D::BindOriginalNode,
        }
    }

    /// This bounded serial coordinator consumes only the original allowances
    /// supplied by the containing normal attempt. Runtime has not executed yet.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn advance_bounded(
        mut self,
        primary: &mut IwsHostRuntimeV1,
        replica: &mut IwsHostRuntimeV1,
        primary_provider: &RemoteMonotonicEndpointProvider,
        replica_provider: &RemoteMonotonicEndpointProvider,
        primary_policy: &TrustedFencingPolicyV1,
        replica_policy: &TrustedFencingPolicyV1,
        input_work: &mut IncrementalFrozenMaterialWorkV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<PaidNormalTickSourcesV1, Self> {
        loop {
            if let Err(error) = archive.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))
            {
                self.host_error = Some(error.into());
                return Err(self);
            }
            self.host_error = None;
            self.package_error = None;
            let phase = self
                .phase
                .take()
                .expect("normal Frozen phase between calls");
            self.phase = Some(match phase {
                Phase::Fresh => {
                    let input = self.input.as_ref().expect("retained original Node package");
                    let check = (|| {
                        let mut stop = || cancelled.load(Ordering::Acquire);
                        let mut common = archive.bounded_context(memory, &mut stop);
                        input.validate_primary_bounded(primary, &mut common)?;
                        let p = primary.current.current().ok_or_else(|| {
                            FormalError::Invalid("normal primary source missing".into())
                        })?;
                        let r = replica.current.current().ok_or_else(|| {
                            FormalError::Invalid("normal replica source missing".into())
                        })?;
                        r.source.validate_context(&mut common)?;
                        common.charge(1, 192)?;
                        if p.source.endpoint_role() != oasis7_formal_protocol::controlled_authority::replicated_protocol::EndpointRole::Primary
                            || r.source.endpoint_role() != oasis7_formal_protocol::controlled_authority::replicated_protocol::EndpointRole::Replica
                            || p.source.record_id() != r.source.record_id()
                            || p.source.fence() != r.source.fence()
                            || p.source.execution_environment() != r.source.execution_environment()
                        {
                            return Err(NormalPackageErrorV1::Invalid("normal Frozen actual dual parent mismatch"));
                        }
                        Ok(())
                    })();
                    if let Err(error) = check {
                        self.phase = Some(Phase::Fresh);
                        self.package_error = Some(error);
                        return Err(self);
                    }
                    match primary.freeze_current_normal_bounded(
                        &input.reservation,
                        &input.material,
                        primary_provider,
                        primary_policy,
                        memory,
                        archive,
                        input_work,
                        cancelled,
                    ) {
                        Ok(value) => Phase::Primary(value),
                        Err(IwsHostRuntimeErrorV1::Frozen(error)) => {
                            self.phase = Some(Phase::RecoverPrimary(error));
                            return Err(self);
                        }
                        Err(error) => {
                            self.phase = Some(Phase::Fresh);
                            self.host_error = Some(error);
                            return Err(self);
                        }
                    }
                }
                Phase::RecoverPrimary(failure) => match failure.recover_bounded(
                    primary_provider,
                    primary_policy,
                    memory,
                    archive,
                    &mut || cancelled.load(Ordering::Acquire),
                ) {
                    Ok(value) => Phase::Primary(value),
                    Err(failure) => {
                        self.phase = Some(Phase::RecoverPrimary(failure));
                        return Err(self);
                    }
                },
                Phase::Primary(p) => {
                    let input = self.input.as_ref().expect("retained original Node package");
                    match replica.freeze_current_normal_bounded(
                        &input.reservation,
                        &input.material,
                        replica_provider,
                        replica_policy,
                        memory,
                        archive,
                        input_work,
                        cancelled,
                    ) {
                        Ok(r) => Phase::Join(p, r),
                        Err(IwsHostRuntimeErrorV1::Frozen(failure)) => {
                            self.phase = Some(Phase::RecoverReplica {
                                primary: p,
                                failure,
                            });
                            return Err(self);
                        }
                        Err(error) => {
                            self.phase = Some(Phase::Primary(p));
                            self.host_error = Some(error);
                            return Err(self);
                        }
                    }
                }
                Phase::RecoverReplica {
                    primary: p,
                    failure,
                } => match failure.recover_bounded(
                    replica_provider,
                    replica_policy,
                    memory,
                    archive,
                    &mut || cancelled.load(Ordering::Acquire),
                ) {
                    Ok(r) => Phase::Join(p, r),
                    Err(failure) => {
                        self.phase = Some(Phase::RecoverReplica {
                            primary: p,
                            failure,
                        });
                        return Err(self);
                    }
                },
                Phase::Join(p, r) => match join_normal_frozen_endpoints_bounded(
                    p,
                    r,
                    primary_provider,
                    replica_provider,
                    primary_policy,
                    replica_policy,
                    memory,
                    archive,
                    &mut || cancelled.load(Ordering::Acquire),
                ) {
                    Ok(pair) => Phase::Joined(pair),
                    Err(failure) => {
                        self.phase = Some(Phase::RecoverPair(failure));
                        return Err(self);
                    }
                },
                Phase::RecoverPair(failure) => match failure.recover_bounded(
                    primary_provider,
                    replica_provider,
                    primary_policy,
                    replica_policy,
                    memory,
                    archive,
                    &mut || cancelled.load(Ordering::Acquire),
                ) {
                    Ok(pair) => Phase::Joined(pair),
                    Err(failure) => {
                        self.phase = Some(Phase::RecoverPair(failure));
                        return Err(self);
                    }
                },
                Phase::Joined(pair) => {
                    let origin = self
                        .input
                        .take()
                        .expect("retained original Node package")
                        .into_origin();
                    let admission = self.admission.take().expect("prepaid source carrier");
                    match PaidNormalTickSourcesV1::from_captured_node(
                        admission,
                        pair,
                        origin,
                        &mut archive
                            .bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
                    ) {
                        Ok(sources) => return Ok(sources),
                        Err(failure) => {
                            self.phase = Some(Phase::RecoverAdmission(failure));
                            return Err(self);
                        }
                    }
                }
                Phase::RecoverAdmission(failure) => match failure.recover_bounded(
                    &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
                ) {
                    Ok(sources) => return Ok(sources),
                    Err(failure) => {
                        self.phase = Some(Phase::RecoverAdmission(failure));
                        return Err(self);
                    }
                },
            });
        }
    }
}

impl std::fmt::Debug for NodeNormalFreezeAttemptV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("NodeNormalFreezeAttemptV1");
        d.field("disposition", &self.disposition())
            .field("host_error", &self.host_error)
            .field("package_error", &self.package_error);
        match self
            .phase
            .as_ref()
            .expect("normal Frozen phase between calls")
        {
            Phase::RecoverPrimary(e) | Phase::RecoverReplica { failure: e, .. } => {
                d.field("endpoint", e);
            }
            Phase::RecoverPair(e) => {
                d.field("pair", e);
            }
            Phase::RecoverAdmission(e) => {
                d.field("admission", e.cause());
            }
            _ => (),
        }
        d.finish_non_exhaustive()
    }
}
