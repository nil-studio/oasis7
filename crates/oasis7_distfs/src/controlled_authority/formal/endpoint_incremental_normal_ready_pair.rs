//! Fixed two-endpoint normal Ready producer. The pair keeps the actual parent
//! namespace FDs, complete staged jobs and immutable Frozen originals alive.
use super::*;
use crate::controlled_authority::formal::endpoint::incremental::genesis_receipts::normal_ready as receipts;
use oasis7_formal_protocol::controlled_authority::formal::{
    PaidVerifiedNormalCommitV1, verify_normal_decision_bounded, verify_normal_qualified_bounded,
};
use oasis7_proto::memory_budget::MemoryReservation;

pub struct PaidNormalReadyPairV1 {
    pair: PaidNormalFrozenPairV1,
    quorum: PaidVerifiedNormalCommitV1,
    original: store::PaidRequestStateV1,
    _outer: MemoryReservation,
}
pub struct NormalReadyPairFailureV1 {
    pair: Option<PaidNormalFrozenPairV1>,
    cause: BoundedAcquireErrorV1,
}
impl std::fmt::Debug for NormalReadyPairFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NormalReadyPairFailureV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for NormalReadyPairFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "normal Ready: {:?}", self.cause)
    }
}
impl std::error::Error for NormalReadyPairFailureV1 {}
fn graph_fee<T: serde::Serialize>(
    value: &T,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<MemoryReservation, FormalError> {
    let wire = store::context_wire::encode_paid(
        value,
        64 << 20,
        &mut work.bounded_context(memory, stop),
        "normal proof component bound",
        "normal proof component sizing",
    )?;
    let n = wire
        .len()
        .checked_add(1024)
        .ok_or_else(|| invalid("normal proof clone layout"))?;
    // Same original RequestState decoder bound covers the strict subgraph being
    // cloned (record/certificate/receipts), including every dynamic child.
    let fee = memory
        .reserve(
            store::bounded_request::decode_workspace(n)?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("normal proof graph admission"))?;
    work.charge(1, n as u64, stop)?;
    Ok(fee)
}
impl PaidNormalFrozenPairV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn produce_ready_bounded(
        mut self,
        candidate: &FormalCandidateMaterialV1,
        primary_provider: &RemoteMonotonicEndpointProvider,
        replica_provider: &RemoteMonotonicEndpointProvider,
        primary_policy: &TrustedFencingPolicyV1,
        replica_policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNormalReadyPairV1, NormalReadyPairFailureV1> {
        let prepared = (|| {
            work.consume_steps(1, stop)?;
            if !self.uses_budget(memory)
                || self.primary.source.identity
                    != work.bounded_context(memory, stop).work().identity()
                || self.primary.source.identity != self.replica.source.identity
                || self.primary.source.endpoint.role != EndpointRole::Primary
                || self.replica.source.endpoint.role != EndpointRole::Replica
            {
                return Err(invalid("normal Ready actual pair/Core/pool/roles").into());
            }
            if self.primary.source.lease.is_none() || self.replica.source.lease.is_none() {
                return Err(invalid("normal Ready source requires actual recovery").into());
            }
            // Both complete source conjunctions precede the first Prepare mutation.
            let _p = receipts::admit_normal(
                &self.primary.source.endpoint,
                candidate,
                &self.certificate,
                memory,
                work,
                stop,
            )?;
            let _r = receipts::admit_normal(
                &self.replica.source.endpoint,
                candidate,
                &self.certificate,
                memory,
                work,
                stop,
            )?;
            let outer = memory
                .reserve(size_of::<PaidNormalReadyPairV1>(), MemoryClass::Ordinary)
                .map_err(|_| invalid("normal Ready pair admission"))?;
            let primary = receipts::prepare(
                &mut self.primary.source.endpoint,
                &mut self.primary.source.lease,
                candidate,
                &self.certificate,
                receipts::ReceiptContext {
                    provider: primary_provider,
                    policy: primary_policy,
                    memory,
                    work,
                    stop: &mut *stop,
                },
            )?;
            let replica = receipts::prepare(
                &mut self.replica.source.endpoint,
                &mut self.replica.source.lease,
                candidate,
                &self.certificate,
                receipts::ReceiptContext {
                    provider: replica_provider,
                    policy: replica_policy,
                    memory,
                    work,
                    stop: &mut *stop,
                },
            )?;
            let pfinal = receipts::finalize(
                &mut self.primary.source.endpoint,
                &mut self.primary.source.lease,
                candidate,
                &self.certificate,
                &primary,
                &replica,
                receipts::ReceiptContext {
                    provider: primary_provider,
                    policy: primary_policy,
                    memory,
                    work,
                    stop: &mut *stop,
                },
            )?;
            let rfinal = receipts::finalize(
                &mut self.replica.source.endpoint,
                &mut self.replica.source.lease,
                candidate,
                &self.certificate,
                &primary,
                &replica,
                receipts::ReceiptContext {
                    provider: replica_provider,
                    policy: replica_policy,
                    memory,
                    work,
                    stop: &mut *stop,
                },
            )?;
            let fee = graph_fee(
                &(
                    &candidate.candidate,
                    &*self.certificate,
                    pfinal.receipt(),
                    rfinal.receipt(),
                ),
                memory,
                work,
                stop,
            )?;
            let decision = fee.retain(FormalDecisionProofV1 {
                schema_version: 1,
                record: candidate.candidate.record.clone(),
                branch: FormalCompletionBranchV1::Normal,
                writer_signature_hex: candidate.candidate.writer_signature_hex.clone(),
                frozen: Some((*self.certificate).clone()),
                primary: pfinal.receipt().clone(),
                replica: rfinal.receipt().clone(),
            });
            let parent = self
                .primary
                .source
                .endpoint
                .parent
                .as_ref()
                .ok_or_else(|| invalid("normal Ready real parent missing"))?;
            let verified = verify_normal_decision_bounded(
                &decision,
                &self.primary.source.endpoint.authority,
                parent,
                &mut work.bounded_context(memory, stop),
            )?;
            let pcomplete = receipts::complete(
                &mut self.primary.source.endpoint,
                &mut self.primary.source.lease,
                candidate,
                &self.certificate,
                &verified,
                receipts::ReceiptContext {
                    provider: primary_provider,
                    policy: primary_policy,
                    memory,
                    work,
                    stop: &mut *stop,
                },
            )?;
            let rcomplete = receipts::complete(
                &mut self.replica.source.endpoint,
                &mut self.replica.source.lease,
                candidate,
                &self.certificate,
                &verified,
                receipts::ReceiptContext {
                    provider: replica_provider,
                    policy: replica_policy,
                    memory,
                    work,
                    stop: &mut *stop,
                },
            )?;
            let fee = graph_fee(
                &(
                    verified.decision(),
                    pcomplete.receipt(),
                    rcomplete.receipt(),
                ),
                memory,
                work,
                stop,
            )?;
            let proof = fee.retain(FormalQualifiedCommitProofV1 {
                schema_version: 1,
                decision: verified.decision().clone(),
                primary_complete: pcomplete.receipt().clone(),
                replica_complete: rcomplete.receipt().clone(),
            });
            let parent = self
                .primary
                .source
                .endpoint
                .parent
                .as_ref()
                .ok_or_else(|| invalid("normal Ready original parent missing"))?;
            let quorum = verify_normal_qualified_bounded(
                &proof,
                &self.primary.source.endpoint.authority,
                parent,
                &mut work.bounded_context(memory, stop),
            )?;
            let original = self
                .primary
                .source
                .endpoint
                .store
                .read_request_bounded(
                    &candidate.candidate.record.request_digest,
                    memory,
                    work,
                    stop,
                )?
                .ok_or_else(|| invalid("normal actual Ready missing after publication"))?;
            match original.state() {
                FormalRequestStateV1::Ready {
                    candidate: actual,
                    decision,
                    complete: Some(complete),
                    ..
                } => {
                    if !store::bounded_transaction::same(actual, candidate, memory, work, stop)?
                        || !store::bounded_transaction::same(
                            decision,
                            &quorum.proof().decision,
                            memory,
                            work,
                            stop,
                        )?
                        || !store::bounded_transaction::same(
                            complete,
                            &quorum.proof().primary_complete,
                            memory,
                            work,
                            stop,
                        )?
                    {
                        return Err(invalid("normal Ready original readback binding").into());
                    }
                }
                _ => return Err(invalid("normal Ready original readback shape").into()),
            }
            Ok((quorum, original, outer))
        })();
        match prepared {
            Ok((quorum, original, outer)) => Ok(PaidNormalReadyPairV1 {
                pair: self,
                quorum,
                original,
                _outer: outer,
            }),
            Err(cause) => Err(NormalReadyPairFailureV1 {
                pair: Some(self),
                cause,
            }),
        }
    }
}
impl PaidNormalReadyPairV1 {
    pub fn proof(&self) -> &FormalQualifiedCommitProofV1 {
        self.quorum.proof()
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.pair.uses_budget(memory)
            && self.quorum.uses_budget(memory)
            && self.original.uses_budget(memory)
            && self._outer.uses_budget(memory)
    }
    fn candidate(&self) -> Result<&FormalCandidateMaterialV1, FormalError> {
        match self.original.state() {
            FormalRequestStateV1::Ready { candidate, .. } => Ok(candidate),
            _ => Err(invalid("normal Ready original owner")),
        }
    }
}
pub struct PaidNormalAcceptedPairV1 {
    primary: PaidAcceptedGenesisHostSourceV1,
    replica: PaidAcceptedGenesisHostSourceV1,
    quorum: PaidVerifiedNormalCommitV1,
    _outer: MemoryReservation,
}
impl PaidNormalAcceptedPairV1 {
    pub fn into_sources(
        self,
    ) -> (
        PaidAcceptedGenesisHostSourceV1,
        PaidAcceptedGenesisHostSourceV1,
        PaidVerifiedNormalCommitV1,
        MemoryReservation,
    ) {
        (self.primary, self.replica, self.quorum, self._outer)
    }
}
pub struct NormalReadyPublicationFailureV1 {
    ready: Option<PaidNormalReadyPairV1>,
    _primary: Option<PaidAcceptedGenesisHostSourceV1>,
    cause: BoundedAcquireErrorV1,
}
impl std::fmt::Debug for NormalReadyPublicationFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NormalReadyPublicationFailureV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for NormalReadyPublicationFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "normal Qualified publication: {:?}", self.cause)
    }
}
impl std::error::Error for NormalReadyPublicationFailureV1 {}
impl PaidNormalReadyPairV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn publish_bounded(
        mut self,
        primary_provider: &RemoteMonotonicEndpointProvider,
        replica_provider: &RemoteMonotonicEndpointProvider,
        primary_policy: &TrustedFencingPolicyV1,
        replica_policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNormalAcceptedPairV1, NormalReadyPublicationFailureV1> {
        let published = (|| {
            self.quorum
                .validate_context(&mut work.bounded_context(memory, stop))?;
            if !self.uses_budget(memory) {
                return Err(invalid("normal Ready publication pool").into());
            }
            let output_fee = memory
                .reserve(size_of::<PaidNormalAcceptedPairV1>(), MemoryClass::Ordinary)
                .map_err(|_| invalid("normal accepted pair admission"))?;
            let candidate = match self.original.state() {
                FormalRequestStateV1::Ready { candidate, .. } => candidate,
                _ => return Err(invalid("normal Ready actual source").into()),
            };
            self.pair
                .primary
                .source
                .endpoint
                .publish_normal_qualified_with_lease_bounded(
                    candidate,
                    self.quorum.proof(),
                    &mut self.pair.primary.source.lease,
                    primary_provider,
                    primary_policy,
                    memory,
                    work,
                    stop,
                )?;
            self.pair
                .replica
                .source
                .endpoint
                .publish_normal_qualified_with_lease_bounded(
                    candidate,
                    self.quorum.proof(),
                    &mut self.pair.replica.source.lease,
                    replica_provider,
                    replica_policy,
                    memory,
                    work,
                    stop,
                )?;
            let primary = self
                .pair
                .primary
                .source
                .lease
                .take()
                .ok_or_else(|| invalid("normal primary Finish lease missing"))?;
            let replica = self
                .pair
                .replica
                .source
                .lease
                .take()
                .ok_or_else(|| invalid("normal replica Finish lease missing"))?;
            Ok((primary, replica, output_fee))
        })();
        let (please, rlease, output_fee) = match published {
            Ok(leases) => leases,
            Err(cause) => {
                return Err(NormalReadyPublicationFailureV1 {
                    ready: Some(self),
                    _primary: None,
                    cause,
                });
            }
        };
        // Partial moves preserve every old source/graph reservation until both new
        // source gates complete. Neither endpoint reacquires a provider FD here.
        let p = self.pair.primary.source;
        let r = self.pair.replica.source;
        let primary = match (*p.endpoint).into_accepted_normal_host_source_with_lease_bounded(
            please,
            primary_provider,
            primary_policy,
            memory,
            work,
            stop,
        ) {
            Ok(source) => source,
            Err(cause) => {
                return Err(NormalReadyPublicationFailureV1 {
                    ready: None,
                    _primary: None,
                    cause,
                });
            }
        };
        let replica = match (*r.endpoint).into_accepted_normal_host_source_with_lease_bounded(
            rlease,
            replica_provider,
            replica_policy,
            memory,
            work,
            stop,
        ) {
            Ok(source) => source,
            Err(cause) => {
                return Err(NormalReadyPublicationFailureV1 {
                    ready: None,
                    _primary: Some(primary),
                    cause,
                });
            }
        };
        Ok(PaidNormalAcceptedPairV1 {
            primary,
            replica,
            quorum: self.quorum,
            _outer: output_fee,
        })
    }
}

/// Unknown outcomes close their old lease. Recovery below always reopens and
/// reconciles actual signed disk/provider state; it never restores old live.
fn recover_pair_leases(
    pair: &mut PaidNormalFrozenPairV1,
    primary: receipts::ReceiptContext<'_, '_, '_, '_, '_>,
    replica_provider: &RemoteMonotonicEndpointProvider,
    replica_policy: &TrustedFencingPolicyV1,
) -> Result<(), BoundedAcquireErrorV1> {
    let receipts::ReceiptContext {
        provider: primary_provider,
        policy: primary_policy,
        memory,
        work,
        stop,
    } = primary;
    for (source, provider, policy) in [
        (&mut pair.primary.source, primary_provider, primary_policy),
        (&mut pair.replica.source, replica_provider, replica_policy),
    ] {
        work.consume_steps(1, stop)?;
        if source.identity != work.bounded_context(memory, stop).work().identity()
            || !source.endpoint_fee.uses_budget(memory)
            || !source._outer.uses_budget(memory)
        {
            return Err(invalid("normal Ready recovery actual source/Core/pool").into());
        }
        if source.lease.is_some() {
            continue;
        }
        let endpoint = &mut *source.endpoint;
        let instance = match endpoint.role {
            EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
            EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
        };
        endpoint.store.reconcile_pending_update_with_scope(
            provider,
            instance,
            policy,
            memory,
            work,
            stop,
            &store::bounded_transaction::RecoveryScope::Cold {
                authority: &endpoint.authority,
                role: endpoint.role,
            },
        )?;
        let lease = crate::controlled_authority::fencing::acquire_provider_lease_bounded_v1(
            provider,
            instance,
            &endpoint.store.checkpoint,
            policy,
            &mut work.bounded_context(memory, stop),
        )?;
        lease.validate_current_binding(
            instance,
            &endpoint.store.checkpoint,
            policy,
            &mut work.bounded_context(memory, stop),
        )?;
        if lease.pending().is_some() || lease.seal().is_some() {
            return Err(FormalError::FencingUnproven.into());
        }
        source.lease = Some(lease);
    }
    Ok(())
}
impl NormalReadyPairFailureV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn recover_bounded(
        mut self,
        candidate: &FormalCandidateMaterialV1,
        primary_provider: &RemoteMonotonicEndpointProvider,
        replica_provider: &RemoteMonotonicEndpointProvider,
        primary_policy: &TrustedFencingPolicyV1,
        replica_policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNormalReadyPairV1, Self> {
        let Some(pair) = self.pair.as_mut() else {
            return Err(self);
        };
        if let Err(cause) = recover_pair_leases(
            pair,
            receipts::ReceiptContext {
                provider: primary_provider,
                policy: primary_policy,
                memory,
                work,
                stop: &mut *stop,
            },
            replica_provider,
            replica_policy,
        ) {
            self.cause = cause;
            return Err(self);
        }
        self.pair
            .take()
            .expect("checked actual normal pair")
            .produce_ready_bounded(
                candidate,
                primary_provider,
                replica_provider,
                primary_policy,
                replica_policy,
                memory,
                work,
                stop,
            )
    }
}
impl NormalReadyPublicationFailureV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn recover_bounded(
        mut self,
        primary_provider: &RemoteMonotonicEndpointProvider,
        replica_provider: &RemoteMonotonicEndpointProvider,
        primary_policy: &TrustedFencingPolicyV1,
        replica_policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNormalAcceptedPairV1, Self> {
        let Some(ready) = self.ready.as_mut() else {
            return Err(self);
        };
        if let Err(cause) = recover_pair_leases(
            &mut ready.pair,
            receipts::ReceiptContext {
                provider: primary_provider,
                policy: primary_policy,
                memory,
                work,
                stop: &mut *stop,
            },
            replica_provider,
            replica_policy,
        ) {
            self.cause = cause;
            return Err(self);
        }
        self.ready
            .take()
            .expect("checked actual normal Ready pair")
            .publish_bounded(
                primary_provider,
                replica_provider,
                primary_policy,
                replica_policy,
                memory,
                work,
                stop,
            )
    }
}

fn stage_source(
    source: &mut super::super::source::NormalFrozenTransactionV1,
    candidate: &FormalCandidateMaterialV1,
    certificate: &FormalFrozenCertificateV1,
    source_work: &mut Option<
        crate::controlled_authority::formal::endpoint::incremental::IwsCandidateSourceWorkV1,
    >,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), BoundedAcquireErrorV1> {
    work.consume_steps(1, stop)?;
    if source.identity != work.bounded_context(memory, stop).work().identity()
        || !source.endpoint_fee.uses_budget(memory)
        || !source._outer.uses_budget(memory)
        || source.lease.is_none()
    {
        return Err(invalid("normal stage actual source/lease/Core/pool").into());
    }
    if !store::bounded_transaction::same(
        &candidate.frozen.as_ref(),
        &Some(certificate),
        memory,
        work,
        stop,
    )? {
        return Err(invalid("normal stage actual Frozen certificate").into());
    }
    candidate
        .material
        .validate_iws_bounded(&mut work.bounded_context(memory, stop))?;
    let endpoint = &mut *source.endpoint;
    let parent = endpoint
        .parent
        .as_ref()
        .ok_or_else(|| invalid("normal stage actual accepted parent missing"))?;
    let context =
        oasis7_formal_protocol::controlled_authority::formal::normal_candidate_context_bounded(
            &candidate.candidate,
            certificate,
            &endpoint.authority,
            parent,
            &mut work.bounded_context(memory, stop),
        )?;
    context.validate_context(&mut work.bounded_context(memory, stop))?;
    let already = {
        let capability = endpoint
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("normal stage real page capability missing"))?;
        if !capability.pages.memory_budget().same_pool(memory) {
            return Err(invalid("normal stage page pool").into());
        }
        capability
            .jobs
            .matches_bounded(candidate, memory, work, stop)?
    };
    if !already {
        let supplied = source_work
            .take()
            .ok_or_else(|| invalid("normal stage explicit continuous source work required"))?;
        endpoint.supply_iws_candidate_work(supplied)?;
        // The original complete Core/Current/dualHistory/router/original fsync and
        // genuine local closure pipeline creates the only PreparedCandidateInstall.
        endpoint.verify_iws_page_backed_material(candidate, context.value(), &mut || stop())?;
    }
    let capability = endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("normal stage complete capability missing"))?;
    let parent = endpoint
        .parent
        .as_ref()
        .ok_or_else(|| invalid("normal stage actual parent missing"))?;
    let _job = capability.jobs.normal_job(parent, memory)?;
    Ok(())
}
impl PaidNormalFrozenPairV1 {
    /// Actual full-source staging on the same two consumed endpoints and FDs.
    /// Unused supplied work stays in its original Option; no allowance is cloned,
    /// initialized or reset on a cached retry.
    pub fn stage_normal_candidate_sources_bounded(
        &mut self,
        candidate: &FormalCandidateMaterialV1,
        primary_work: &mut Option<
            crate::controlled_authority::formal::endpoint::incremental::IwsCandidateSourceWorkV1,
        >,
        replica_work: &mut Option<
            crate::controlled_authority::formal::endpoint::incremental::IwsCandidateSourceWorkV1,
        >,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), BoundedAcquireErrorV1> {
        if !self.uses_budget(memory) {
            return Err(invalid("normal stage pair pool").into());
        }
        stage_source(
            &mut self.primary.source,
            candidate,
            &self.certificate,
            primary_work,
            memory,
            work,
            stop,
        )?;
        stage_source(
            &mut self.replica.source,
            candidate,
            &self.certificate,
            replica_work,
            memory,
            work,
            stop,
        )
    }
    /// One fixed reachable consumer from actual FrozenPair and explicit remaining
    /// source work to real dual durable Ready/Complete. No caller-ready flag.
    #[allow(clippy::too_many_arguments)]
    pub fn produce_ready_from_candidate_bounded(
        mut self,
        candidate: &FormalCandidateMaterialV1,
        primary_work: &mut Option<
            crate::controlled_authority::formal::endpoint::incremental::IwsCandidateSourceWorkV1,
        >,
        replica_work: &mut Option<
            crate::controlled_authority::formal::endpoint::incremental::IwsCandidateSourceWorkV1,
        >,
        primary_provider: &RemoteMonotonicEndpointProvider,
        replica_provider: &RemoteMonotonicEndpointProvider,
        primary_policy: &TrustedFencingPolicyV1,
        replica_policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNormalReadyPairV1, NormalReadyPairFailureV1> {
        if let Err(cause) = self.stage_normal_candidate_sources_bounded(
            candidate,
            primary_work,
            replica_work,
            memory,
            work,
            stop,
        ) {
            return Err(NormalReadyPairFailureV1 {
                pair: Some(self),
                cause,
            });
        }
        self.produce_ready_bounded(
            candidate,
            primary_provider,
            replica_provider,
            primary_policy,
            replica_policy,
            memory,
            work,
            stop,
        )
    }
}
impl NormalReadyPairFailureV1 {
    pub fn cause(&self) -> &BoundedAcquireErrorV1 {
        &self.cause
    }
}
impl NormalReadyPublicationFailureV1 {
    pub fn cause(&self) -> &BoundedAcquireErrorV1 {
        &self.cause
    }
}

impl NormalReadyPairFailureV1 {
    /// Also resumes a failed full-source stage using only explicitly supplied
    /// remaining work. Already staged sides leave their original loan untouched.
    #[allow(clippy::too_many_arguments)]
    pub fn recover_from_candidate_bounded(
        mut self,
        candidate: &FormalCandidateMaterialV1,
        primary_work: &mut Option<
            crate::controlled_authority::formal::endpoint::incremental::IwsCandidateSourceWorkV1,
        >,
        replica_work: &mut Option<
            crate::controlled_authority::formal::endpoint::incremental::IwsCandidateSourceWorkV1,
        >,
        primary_provider: &RemoteMonotonicEndpointProvider,
        replica_provider: &RemoteMonotonicEndpointProvider,
        primary_policy: &TrustedFencingPolicyV1,
        replica_policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNormalReadyPairV1, Self> {
        let Some(pair) = self.pair.as_mut() else {
            return Err(self);
        };
        if let Err(cause) = recover_pair_leases(
            pair,
            receipts::ReceiptContext {
                provider: primary_provider,
                policy: primary_policy,
                memory,
                work,
                stop: &mut *stop,
            },
            replica_provider,
            replica_policy,
        ) {
            self.cause = cause;
            return Err(self);
        }
        self.pair
            .take()
            .expect("checked actual normal pair")
            .produce_ready_from_candidate_bounded(
                candidate,
                primary_work,
                replica_work,
                primary_provider,
                replica_provider,
                primary_policy,
                replica_policy,
                memory,
                work,
                stop,
            )
    }
}

#[path = "endpoint_incremental_normal_ready_candidate.rs"]
mod candidate_read;
pub use candidate_read::NormalReadyCandidateReadViewV1;
