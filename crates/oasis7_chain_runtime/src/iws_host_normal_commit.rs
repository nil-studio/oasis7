//! Fixed consuming normal tail. Candidate bytes remain DATA until the actual
//! dual Frozen endpoints stage and verify them. No input execution is replayed.
use super::normal::HostNormalIoWorkV1;
use super::*;
use crate::iws_normal_input::PaidNormalTickSourcesV1;
use crate::iws_normal_input_origin::PaidNodeInputOriginV1;
use oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1;
use oasis7_distfs::controlled_authority::formal::endpoint::IwsCandidateSourceWorkV1;
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FormalEndpointPairPublicationRefsV1, NormalReadyPairFailureV1, NormalReadyPublicationFailureV1,
    PaidNormalAcceptedPairV1, PaidNormalFrozenPairV1, PaidNormalReadyPairV1,
};
use oasis7_proto::memory_budget::{MemoryReservation, ReservedValue};
use oasis7_runtime::{NormalHostWorkV1, PreparedNormalHostResidentV1, PreparedNormalHostRuntimeV1};

/// Original allowances move with the consuming phase across callbacks. A retry
/// cannot replace physical, logical or candidate-source work. Core is only
/// borrowed during a call; keeping work does not require sharing a !Sync Core.
pub(crate) struct NormalCommitWorkV1 {
    pub canonical: oasis7_wasm_abi::CanonicalCborWorkV1,
    pub input_material:
        oasis7_formal_protocol::controlled_authority::formal::IncrementalFrozenMaterialWorkV1,
    pub primary_source: Option<IwsCandidateSourceWorkV1>,
    pub replica_source: Option<IwsCandidateSourceWorkV1>,
    pub physical: HostNormalIoWorkV1,
    pub runtime: NormalHostWorkV1,
    pub cold: cold_recovery::NormalColdRecoveryWorkV1,
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub fault: Option<NormalCommitFaultV1>,
}
#[cfg(any(test, feature = "iws-host-native-test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NormalCommitFaultV1 {
    BeforeResident,
    BeforeSourceWorkReturn,
    BeforeReplicaSource,
    AfterPublication,
}

enum Phase {
    BeforeReady {
        frozen: PaidNormalFrozenPairV1,
        prepared: PreparedNormalHostRuntimeV1,
    },
    ReadyFailure {
        failure: NormalReadyPairFailureV1,
        prepared: PreparedNormalHostRuntimeV1,
    },
    Ready {
        ready: PaidNormalReadyPairV1,
        prepared: PreparedNormalHostRuntimeV1,
    },
    ReplicaPreflight {
        ready: PaidNormalReadyPairV1,
        primary: PreparedNormalHostResidentV1,
    },
    Publication {
        failure: NormalReadyPublicationFailureV1,
        primary: PreparedNormalHostResidentV1,
        replica: PreparedNormalHostResidentV1,
    },
    /// Durable acceptance already happened. Retain both original sources and
    /// residents until the two actual staged accounts have been returned.
    Accepted {
        accepted: PaidNormalAcceptedPairV1,
        primary: PreparedNormalHostResidentV1,
        replica: PreparedNormalHostResidentV1,
    },
    /// The Runtime preparation was consumed by primary resident preflight.
    /// Keep real Ready for explicit cold recovery; never rerun committed input.
    ColdBeforePublication {
        ready: PaidNormalReadyPairV1,
    },
    ColdPublication {
        failure: NormalReadyPublicationFailureV1,
    },
    ColdAccepted {
        accepted: PaidNormalAcceptedPairV1,
    },
    /// Existing install API consumes accepted FDs/residents even on error.
    /// Durable publication has succeeded: only actual cold recovery may resume.
    ColdAfterPublication,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NormalCommitRecoveryDispositionV1 {
    RetryActualReady,
    RetryReplicaPreflight,
    RecoverActualPublicationOrColdReconcile,
    RetryAcceptedSourceWork,
    ColdBeforePublication,
    DurableAcceptedHostNotInstalled,
}
#[derive(Debug)]
pub(crate) enum NormalCommitFailureCauseV1<'a> {
    Host(&'a IwsHostRuntimeErrorV1),
    Ready(&'a NormalReadyPairFailureV1),
    Publication(&'a NormalReadyPublicationFailureV1),
}
/// Owns the true Node provenance, remaining work and original failure
/// owners. This is neither a Ready claim nor an accepted/WorldReady capability.
pub(crate) struct NormalCommitFailureV1 {
    package: super::normal_package::PersistedNormalCandidatePackageV1,
    node: PaidNodeInputOriginV1,
    work: NormalCommitWorkV1,
    phase: Option<Phase>,
    cause: Option<IwsHostRuntimeErrorV1>,
    _source_fee: ReservedValue<()>,
    // The initial admission includes the complete Phase layout and remains
    // held until every retained source, resident, error and work owner drops.
    fee: Option<MemoryReservation>,
}
impl std::fmt::Debug for NormalCommitFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NormalCommitFailureV1")
            .field("disposition", &self.disposition())
            .field("cause", &self.cause())
            .finish_non_exhaustive()
    }
}
impl NormalCommitFailureV1 {
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn retained_source_work_targets_fixture_v1(&self) -> Option<[u64; 2]> {
        let retained = match self.phase.as_ref()? {
            Phase::Accepted { .. } | Phase::ColdAccepted { .. } => true,
            Phase::Publication { failure, .. } | Phase::ColdPublication { failure } => {
                failure.has_observable_primary_source_for_fixture_v1()
            }
            _ => false,
        };
        if !retained {
            return None;
        }
        Some([
            self.work.primary_source.as_ref()?.history_hash_bytes,
            self.work.replica_source.as_ref()?.history_hash_bytes,
        ])
    }
    pub(crate) fn cause(&self) -> Option<NormalCommitFailureCauseV1<'_>> {
        if let Some(error) = self.cause.as_ref() {
            return Some(NormalCommitFailureCauseV1::Host(error));
        }
        match self.phase.as_ref() {
            Some(Phase::ReadyFailure { failure, .. }) => {
                Some(NormalCommitFailureCauseV1::Ready(failure))
            }
            Some(Phase::Publication { failure, .. } | Phase::ColdPublication { failure }) => {
                Some(NormalCommitFailureCauseV1::Publication(failure))
            }
            _ => None,
        }
    }

    pub(crate) fn disposition(&self) -> NormalCommitRecoveryDispositionV1 {
        match self
            .phase
            .as_ref()
            .expect("private normal phase is present between calls")
        {
            Phase::BeforeReady { .. } | Phase::ReadyFailure { .. } | Phase::Ready { .. } => {
                NormalCommitRecoveryDispositionV1::RetryActualReady
            }
            Phase::ReplicaPreflight { .. } => {
                NormalCommitRecoveryDispositionV1::RetryReplicaPreflight
            }
            Phase::Publication { .. } | Phase::ColdPublication { .. } => {
                NormalCommitRecoveryDispositionV1::RecoverActualPublicationOrColdReconcile
            }
            Phase::Accepted { .. } | Phase::ColdAccepted { .. } => {
                NormalCommitRecoveryDispositionV1::RetryAcceptedSourceWork
            }
            Phase::ColdBeforePublication { .. } => {
                NormalCommitRecoveryDispositionV1::ColdBeforePublication
            }
            Phase::ColdAfterPublication => {
                NormalCommitRecoveryDispositionV1::DurableAcceptedHostNotInstalled
            }
        }
    }
}
impl NormalCommitFailureV1 {
    fn publish_ready_bounded(
        &mut self,
        ready: PaidNormalReadyPairV1,
        publication: FormalEndpointPairPublicationRefsV1<'_>,
        cancelled: &AtomicBool,
    ) -> Result<PaidNormalAcceptedPairV1, NormalReadyPublicationFailureV1> {
        #[cfg(any(test, feature = "iws-host-native-test-support"))]
        if self.work.fault == Some(NormalCommitFaultV1::BeforeReplicaSource) {
            self.work.fault = None;
            return ready
                .publish_with_replica_source_cancellation_for_fixture_v1(publication, cancelled);
        }
        let FormalEndpointPairPublicationRefsV1 {
            primary_provider,
            replica_provider,
            primary_policy,
            replica_policy,
            memory,
            work,
        } = publication;
        ready.publish_bounded(
            primary_provider,
            replica_provider,
            primary_policy,
            replica_policy,
            memory,
            work,
            &mut || cancelled.load(Ordering::Acquire),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn recover_bounded(
        mut self,
        primary: &mut IwsHostRuntimeV1,
        replica: &mut IwsHostRuntimeV1,
        primary_provider: &RemoteMonotonicEndpointProvider,
        replica_provider: &RemoteMonotonicEndpointProvider,
        primary_policy: &TrustedFencingPolicyV1,
        replica_policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<NormalCommitWorkV1, Self> {
        if matches!(
            self.phase,
            Some(
                Phase::ColdBeforePublication { .. }
                    | Phase::ColdPublication { .. }
                    | Phase::ColdAccepted { .. }
                    | Phase::ColdAfterPublication
            )
        ) {
            return Err(self);
        }
        {
            let mut stop = || cancelled.load(Ordering::Acquire);
            if let Err(error) = self
                .node
                .validate_context(&mut archive.bounded_context(memory, &mut stop))
            {
                self.cause = Some(error.into());
                return Err(self);
            }
        }
        if self.fee.is_none() {
            match memory.reserve(size_of::<Self>() + 4096, MemoryClass::Ordinary) {
                Ok(fee) => self.fee = Some(fee),
                Err(error) => {
                    self.cause = Some(IwsHostRuntimeErrorV1::Move(error.into()));
                    return Err(self);
                }
            }
        }
        loop {
            self.cause = None;
            let phase = self.phase.take().expect("private consuming normal phase");
            match phase {
                Phase::BeforeReady { frozen, prepared } => {
                    match frozen.produce_ready_from_candidate_bounded(
                        self.package.material_data(),
                        &mut self.work.primary_source,
                        &mut self.work.replica_source,
                        primary_provider,
                        replica_provider,
                        primary_policy,
                        replica_policy,
                        memory,
                        archive,
                        &mut || cancelled.load(Ordering::Acquire),
                    ) {
                        Ok(ready) => self.phase = Some(Phase::Ready { ready, prepared }),
                        Err(failure) => {
                            self.phase = Some(Phase::ReadyFailure { failure, prepared });
                            return Err(self);
                        }
                    }
                }
                Phase::ReadyFailure { failure, prepared } => {
                    match failure.recover_from_candidate_bounded(
                        self.package.material_data(),
                        &mut self.work.primary_source,
                        &mut self.work.replica_source,
                        primary_provider,
                        replica_provider,
                        primary_policy,
                        replica_policy,
                        memory,
                        archive,
                        &mut || cancelled.load(Ordering::Acquire),
                    ) {
                        Ok(ready) => self.phase = Some(Phase::Ready { ready, prepared }),
                        Err(failure) => {
                            self.phase = Some(Phase::ReadyFailure { failure, prepared });
                            return Err(self);
                        }
                    }
                }
                Phase::Ready { ready, prepared } => {
                    if let Err(error) = self
                        .work
                        .cold
                        .bind_ready_bounded(&ready, memory, archive, cancelled)
                    {
                        self.cause = Some(error);
                        self.phase = Some(Phase::Ready { ready, prepared });
                        return Err(self);
                    }
                    #[cfg(any(test, feature = "iws-host-native-test-support"))]
                    if self.work.fault == Some(NormalCommitFaultV1::BeforeResident) {
                        self.work.fault = None;
                        cancelled.store(true, Ordering::Release);
                    }
                    match primary.prepare_normal_resident_successor_bounded(
                        &ready,
                        prepared,
                        memory,
                        archive,
                        &mut self.work.physical,
                        &mut self.work.runtime,
                        cancelled,
                    ) {
                        Ok(primary) => {
                            self.phase = Some(Phase::ReplicaPreflight { ready, primary })
                        }
                        Err(error) => {
                            self.cause = Some(error);
                            self.phase = Some(Phase::ColdBeforePublication { ready });
                            return Err(self);
                        }
                    }
                }
                Phase::ReplicaPreflight {
                    ready,
                    primary: resident,
                } => {
                    let second = match replica.prepare_replica_normal_resident_successor_bounded(
                        &ready, &resident, memory, archive, cancelled,
                    ) {
                        Ok(value) => value,
                        Err(error) => {
                            self.cause = Some(error);
                            self.phase = Some(Phase::ReplicaPreflight {
                                ready,
                                primary: resident,
                            });
                            return Err(self);
                        }
                    };
                    match self.publish_ready_bounded(
                        ready,
                        FormalEndpointPairPublicationRefsV1 {
                            primary_provider,
                            replica_provider,
                            primary_policy,
                            replica_policy,
                            memory,
                            work: archive,
                        },
                        cancelled,
                    ) {
                        Err(failure) => {
                            self.phase = Some(Phase::Publication {
                                failure,
                                primary: resident,
                                replica: second,
                            });
                            return Err(self);
                        }
                        Ok(accepted) => {
                            self.phase = Some(Phase::Accepted {
                                accepted,
                                primary: resident,
                                replica: second,
                            });
                        }
                    }
                }
                Phase::Publication {
                    mut failure,
                    primary: resident,
                    replica: second,
                } => {
                    if failure.requires_authenticated_cold_reopen_v1() {
                        if let Err(error) = failure.return_observable_source_work_bounded(
                            &mut self.work.primary_source,
                            memory,
                            archive,
                            &mut || cancelled.load(Ordering::Acquire),
                        ) {
                            self.cause = Some(error.into());
                            self.phase = Some(Phase::Publication {
                                failure,
                                primary: resident,
                                replica: second,
                            });
                            return Err(self);
                        }
                        drop(failure);
                        self.phase = Some(Phase::ColdAfterPublication);
                        continue;
                    }
                    match failure.recover_bounded(
                        primary_provider,
                        replica_provider,
                        primary_policy,
                        replica_policy,
                        memory,
                        archive,
                        &mut || cancelled.load(Ordering::Acquire),
                    ) {
                        Err(failure) => {
                            self.phase = Some(Phase::Publication {
                                failure,
                                primary: resident,
                                replica: second,
                            });
                            return Err(self);
                        }
                        Ok(accepted) => {
                            self.phase = Some(Phase::Accepted {
                                accepted,
                                primary: resident,
                                replica: second,
                            });
                        }
                    }
                }
                Phase::Accepted {
                    mut accepted,
                    primary: resident,
                    replica: second,
                } => {
                    #[cfg(any(test, feature = "iws-host-native-test-support"))]
                    if self.work.fault == Some(NormalCommitFaultV1::BeforeSourceWorkReturn) {
                        self.work.fault = None;
                        cancelled.store(true, Ordering::Release);
                    }
                    if let Err(error) = accepted.return_source_work_bounded(
                        &mut self.work.primary_source,
                        &mut self.work.replica_source,
                        memory,
                        archive,
                        &mut || cancelled.load(Ordering::Acquire),
                    ) {
                        self.cause = Some(error.into());
                        self.phase = Some(Phase::Accepted {
                            accepted,
                            primary: resident,
                            replica: second,
                        });
                        return Err(self);
                    }
                    #[cfg(any(test, feature = "iws-host-native-test-support"))]
                    if self.work.fault == Some(NormalCommitFaultV1::AfterPublication) {
                        self.work.fault = None;
                        cancelled.store(true, Ordering::Release);
                    }
                    match IwsHostRuntimeV1::install_accepted_normal_pair_with_residents_bounded(
                        primary, replica, accepted, resident, second, memory, archive, cancelled,
                    ) {
                        Ok(()) => {
                            self.work.cold.completed();
                            return Ok(self.work);
                        }
                        Err(error) => {
                            self.cause = Some(error);
                            self.phase = Some(Phase::ColdAfterPublication);
                            return Err(self);
                        }
                    }
                }
                phase @ (Phase::ColdBeforePublication { .. }
                | Phase::ColdPublication { .. }
                | Phase::ColdAccepted { .. }
                | Phase::ColdAfterPublication) => {
                    self.phase = Some(phase);
                    return Err(self);
                }
            }
        }
    }
}
/// Concrete fixed tail from the real Node/Frozen owner and one actual Runtime
/// preparation. The two resident preflights complete before Qualified publish.
/// The supplied package is DATA: existing staged-source and resident gates must
/// independently match it to that preparation and its actual Frozen endpoints.
#[allow(clippy::too_many_arguments)]
/// A failed package admission still owns the exact unpublished sources and
/// remaining work. No caller supplied candidate may enter the publication tail.
#[derive(Debug)]
pub(crate) enum NormalCommitStartCauseV1 {
    Material(FormalError),
    Package(super::normal_package::NormalPackageErrorV1),
}
pub(crate) enum NormalCommitStartFailureV1 {
    Package {
        cause: NormalCommitStartCauseV1,
        package: super::normal_package::PersistedNormalCandidatePackageV1,
        sources: PaidNormalTickSourcesV1,
        prepared: PreparedNormalHostRuntimeV1,
        work: NormalCommitWorkV1,
    },
    Publication(NormalCommitFailureV1),
}
pub(crate) fn publish_prepared_normal_runtime_bounded(
    primary: &mut IwsHostRuntimeV1,
    replica: &mut IwsHostRuntimeV1,
    sources: PaidNormalTickSourcesV1,
    prepared: PreparedNormalHostRuntimeV1,
    package: super::normal_package::PersistedNormalCandidatePackageV1,
    mut work: NormalCommitWorkV1,
    primary_provider: &RemoteMonotonicEndpointProvider,
    replica_provider: &RemoteMonotonicEndpointProvider,
    primary_policy: &TrustedFencingPolicyV1,
    replica_policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<NormalCommitWorkV1, NormalCommitStartFailureV1> {
    let checked = (|| {
        let input = sources
            .input(memory, archive, &mut work.input_material, &mut || {
                cancelled.load(Ordering::Acquire)
            })
            .map_err(NormalCommitStartCauseV1::Material)?;
        package
            .candidate_bounded_v1(&prepared, &input, memory, archive, cancelled)
            .map(|_| ())
            .map_err(NormalCommitStartCauseV1::Package)
    })();
    match checked {
        Ok(()) => (),
        Err(cause) => {
            return Err(NormalCommitStartFailureV1::Package {
                cause,
                package,
                sources,
                prepared,
                work,
            });
        }
    };
    let (frozen, node, fee) = sources.into_publication_parts();
    NormalCommitFailureV1 {
        package,
        node,
        _source_fee: fee,
        fee: None,
        work,
        phase: Some(Phase::BeforeReady { frozen, prepared }),
        cause: None,
    }
    .recover_bounded(
        primary,
        replica,
        primary_provider,
        replica_provider,
        primary_policy,
        replica_policy,
        memory,
        archive,
        cancelled,
    )
    .map_err(NormalCommitStartFailureV1::Publication)
}

#[path = "iws_host_normal_commit_package.rs"]
mod package_tail;
pub(crate) use package_tail::{
    NormalOutputCommitFailureV1, commit_prepared_normal_output_bounded_v1,
};

#[path = "iws_host_normal_commit_cold.rs"]
mod cold_recovery;
pub(crate) use cold_recovery::{NormalColdRecoveryInputsV1, NormalColdRecoveryWorkV1};
