//! Genuine accepted GEN sources -> actual worker admission -> resident Host.
//! The primary consumes the immutable producer's projected owner; the replica
//! rebuilds its own owners from its independent accepted endpoint. Neither path
//! manufactures a cold configuration, accepted World or execution callback.
use super::cold::{ColdProjectionReader as BootstrapProjectionReader, Loan, amounts, take};
use super::*;
use oasis7_distfs::{
    controlled_authority::formal::endpoint::HostAcceptedParentReadViewV1,
    incremental_page_store::{PageStoreError, objects::ObjectIo},
};
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{
            FormalRecordKindV1, bounded_context::FormalBoundedContextV1, incremental::is_iws_record,
        },
        replicated_protocol::EndpointRole,
    },
    incremental_execution::{ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits},
    incremental_projection_sidecar::read_projection_sidecar_bounded_v1,
    incremental_state::StateRootV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryReservation, ReservedValue};
use oasis7_runtime::{
    NormalHostGenesisAuthorityV1, NormalHostResidentSourcesV1, NormalHostWorkV1,
    PreparedNormalBusinessMetadataGenesisV1, ProvisionalAuthorizationProjectionV1,
    RuntimeSourceCompiler, WorldJournalRetentionV1,
    incremental_genesis::RuntimeGenesisSummaryV1,
    incremental_genesis_projection::RuntimeProjectedGenesisV1,
    incremental_normal_sources::{NormalTickParentRefsV1, bind_normal_tick_parent_v1},
};

/// The same explicit physical/logical startup quota carrier used by restore.
/// This alias grants no Cold source or configuration; GEN never uses its codec
/// field and never creates, copies or refills any remaining IO allowance.
pub type HostGenesisIoWorkV1 = super::cold::HostColdIoWorkV1;

fn physical(error: PageStoreError) -> IwsHostRuntimeErrorV1 {
    IwsHostRuntimeErrorV1::Physical(error)
}

fn validate_genesis_parent(
    parent: &HostAcceptedParentReadViewV1<'_>,
    role: EndpointRole,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), IwsHostRuntimeErrorV1> {
    common.consume_steps(4)?;
    let record = parent.head().record();
    if parent.endpoint_role() != role
        || record.kind != FormalRecordKindV1::InitialActivation
        || record.position != 1
        || !is_iws_record(record.schema_version)
        || parent.core().body().height != 0
        || parent.core().body().transition.is_some()
    {
        return Err(FormalError::Invalid("Host actual accepted GEN role/profile".into()).into());
    }
    Ok(())
}

/// Pay before the transaction's owned RootSet clone, using the same layout
/// envelope as authenticated cold startup. Keep the reservation across the TX.
fn reserve_roots_clone(
    roots: &RootSetV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, IwsHostRuntimeErrorV1> {
    let layout = std::mem::size_of_val(roots)
        .checked_add(roots.world_id.len())
        .and_then(|n| {
            roots
                .state
                .len()
                .checked_mul(size_of::<StateRootV1>())
                .and_then(|v| n.checked_add(v))
        })
        .and_then(|n| {
            roots
                .state
                .iter()
                .try_fold(n, |n, root| n.checked_add(root.node_root.len()))
        })
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(|| {
            physical(PageStoreError::ResourceLimited(
                "GEN accepted RootSet clone layout",
            ))
        })?;
    let bytes = u64::try_from(layout)
        .map_err(|_| physical(PageStoreError::ResourceLimited("GEN RootSet layout width")))?;
    common.charge(bytes, bytes)?;
    common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)
        .map_err(physical)
}

impl IwsHostRuntimeV1 {
    /// Install the actual primary GEN after verifying its complete stored
    /// proof, sidecar and same-producer projection through the readonly Root.
    /// The original World is not an input: its projected owner and retention
    /// DATA were captured before acceptance and remain tied to this Common.
    #[allow(clippy::too_many_arguments)]
    pub fn install_accepted_genesis_projected_bounded(
        &mut self,
        source: PaidAcceptedGenesisHostSourceV1,
        limits: WorkerLimitsV1,
        projection: ProvisionalAuthorizationProjectionV1,
        projected: RuntimeProjectedGenesisV1,
        authority: NormalHostGenesisAuthorityV1<'_>,
        retention: ReservedValue<WorldJournalRetentionV1>,
        io: &mut HostGenesisIoWorkV1,
        runtime_work: &mut NormalHostWorkV1,
        compiler: &RuntimeSourceCompiler,
        transaction: TransactionLimits,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<
        (
            RuntimeGenesisSummaryV1,
            PreparedNormalBusinessMetadataGenesisV1,
        ),
        IwsHostRuntimeErrorV1,
    > {
        archive.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
        if !self.current.is_empty() || self.continuation.is_some() {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
        }
        let (resident, genesis, metadata) = source.with_parent_read_view_bounded(
            memory,
            archive,
            &mut || cancelled.load(Ordering::Acquire),
            |parent| -> Result<_, IwsHostRuntimeErrorV1> {
                let mut stop = || cancelled.load(Ordering::Acquire);
                let mut common = archive.bounded_context(memory, &mut stop);
                source.validate_context(&mut common)?;
                validate_genesis_parent(&parent, EndpointRole::Primary, &mut common)?;
                let proof = parent.read_qualified_parent_proof_bounded(
                    memory, archive, &mut || cancelled.load(Ordering::Acquire),
                )?;
                proof.validate_parent_bounded(&parent, &mut common)?;
                let pages = parent.pages();
                let budget = take(&mut io.state);
                let ceiling = amounts(&budget);
                let state = Loan {
                    value: pages.state_session_with_cancelled(budget, MemoryClass::Ordinary, cancelled).map_err(physical)?,
                    slot: &mut io.state,
                    ceiling,
                };
                let budget = take(&mut io.values);
                let ceiling = amounts(&budget);
                let values = Loan {
                    value: pages.value_session_with_cancelled(budget, cancelled),
                    slot: &mut io.values,
                    ceiling,
                };
                let budget = take(&mut io.members);
                let ceiling = amounts(&budget);
                let mut members = Loan {
                    value: pages.value_session_with_cancelled(budget, cancelled),
                    slot: &mut io.members,
                    ceiling,
                };
                let objects = super::normal::io::open_originals_metered(pages, &mut io.originals, cancelled, &mut common)?;
                let original_holder = super::normal::originals::HostNormalOriginalsV1::new(
                    &objects,
                    ObjectIo {
                        physical: &mut io.originals,
                        logical: &mut io.original_values,
                        class: MemoryClass::Ordinary,
                        cancelled,
                    },
                    memory,
                ).map_err(physical)?;
                let mut originals = original_holder.reader();
                let side = read_projection_sidecar_bounded_v1(
                    parent.head(), parent.core(), proof.material(),
                    &mut BootstrapProjectionReader(&mut originals), &mut common,
                ).map_err(|error| IwsHostRuntimeErrorV1::NormalSource(
                    oasis7_runtime::BodyMutationError::Original(Box::new(error)),
                ))?;
                let bound = bind_normal_tick_parent_v1(NormalTickParentRefsV1 {
                    core: parent.core(), storage: parent.storage(), roots: parent.roots(),
                    runtime: parent.runtime(), cognition: parent.cognition(),
                    retention: parent.retention_ledger(), head: parent.head(), authority: parent.authority(),
                }, &mut common)?;
                let _roots = reserve_roots_clone(parent.roots(), &mut common)?;
                let fence = bound.bootstrap_fence_data_v1();
                let mut tx = ExecutionTransaction::begin(ExecutionParent {
                    commit: fence.parent_commit, roots: (**parent.roots()).clone(), authority: fence.authority,
                }, parent.head().verified_record_id(), &state.value, memory, transaction)
                    .map_err(IwsHostRuntimeErrorV1::Move)?;
                let views = super::normal::values::HostNormalValuesV1::new(&mut members.value, memory)
                    .map_err(IwsHostRuntimeErrorV1::Value)?;
                let (mut member_sink, mut fee_sink, mut event_sink) = (views.handle(), views.handle(), views.handle());
                source.with_native_worker(memory, archive, &mut || cancelled.load(Ordering::Acquire), |lease| {
                    with_admitted_native_lease(lease, source.execution_environment(), &limits, memory, archive, cancelled, |worker| {
                        oasis7_runtime::bootstrap_authenticated_genesis_normal_host_resident_v1(
                            &bound, &mut tx, &state.value, &values.value,
                            &mut member_sink, &mut fee_sink, &mut event_sink, &mut originals,
                            |event| original_holder.persist_event(event), worker,
                            projection, projected, authority, retention, side, parent.proof_ledger(),
                            compiler, runtime_work,
                            || state.value.take_read_error().map(|error| ValueTreeError::Io(std::io::Error::other(error))),
                            || cancelled.load(Ordering::Acquire), &mut common, &mut || source.fence(),
                        ).map_err(IwsHostRuntimeErrorV1::GenesisNormal)
                    })
                })
            },
        )?;
        self.finish_admitted_genesis_resident(
            source, limits, resident, memory, archive, cancelled,
        )?;
        Ok((genesis, metadata))
    }

    /// A second actual endpoint consumes its own accepted namespaces and worker
    /// lease. Only the primary's immutable checked projection DATA is shared;
    /// all replica bootstrap directories and qualifications are rebuilt from
    /// the replica's real parent, under the same continuing Common and pool.
    #[allow(clippy::too_many_arguments)]
    pub fn install_accepted_genesis_replica_from_primary_bounded(
        &mut self,
        primary: &IwsHostRuntimeV1,
        source: PaidAcceptedGenesisHostSourceV1,
        limits: WorkerLimitsV1,
        projection: ProvisionalAuthorizationProjectionV1,
        genesis: &RuntimeGenesisSummaryV1,
        authority: NormalHostGenesisAuthorityV1<'_>,
        io: &mut HostGenesisIoWorkV1,
        runtime_work: &mut NormalHostWorkV1,
        transaction: TransactionLimits,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        archive.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
        if !self.current.is_empty() || self.continuation.is_some() || primary.continuation.is_some()
        {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
        }
        let primary = primary.current.current().ok_or_else(|| {
            FormalError::Invalid("GEN replica missing actual primary Host".into())
        })?;
        let primary_resident = primary.resident.as_ref().ok_or_else(|| {
            FormalError::Invalid("GEN replica missing actual primary resident".into())
        })?;
        let resident = primary.source.with_parent_read_view_bounded(
            memory, archive, &mut || cancelled.load(Ordering::Acquire),
            |primary_parent| -> Result<_, IwsHostRuntimeErrorV1> {
                source.with_parent_read_view_bounded(
                    memory, archive, &mut || cancelled.load(Ordering::Acquire),
                    |replica_parent| -> Result<_, IwsHostRuntimeErrorV1> {
                        let mut stop = || cancelled.load(Ordering::Acquire);
                        let mut common = archive.bounded_context(memory, &mut stop);
                        primary.source.validate_context(&mut common)?;
                        source.validate_context(&mut common)?;
                        validate_genesis_parent(&primary_parent, EndpointRole::Primary, &mut common)?;
                        validate_genesis_parent(&replica_parent, EndpointRole::Replica, &mut common)?;
                        common.consume_steps(5)?;
                        if primary_parent.head().verified_record_id() != replica_parent.head().verified_record_id()
                            || primary_parent.fence() != replica_parent.fence()
                            || primary_parent.namespace_identities().0 == replica_parent.namespace_identities().0
                            || primary_parent.namespace_identities().1 == replica_parent.namespace_identities().1
                            || primary.source.execution_environment() != source.execution_environment()
                        {
                            return Err(FormalError::Invalid("GEN actual independent replica parent".into()).into());
                        }
                        let primary_proof = primary_parent.read_qualified_parent_proof_bounded(
                            memory, archive, &mut || cancelled.load(Ordering::Acquire),
                        )?;
                        primary_proof.validate_parent_bounded(&primary_parent, &mut common)?;
                        let replica_proof = replica_parent.read_qualified_parent_proof_bounded(
                            memory, archive, &mut || cancelled.load(Ordering::Acquire),
                        )?;
                        replica_proof.validate_parent_bounded(&replica_parent, &mut common)?;
                        let primary_bound = bind_normal_tick_parent_v1(NormalTickParentRefsV1 {
                            core: primary_parent.core(), storage: primary_parent.storage(), roots: primary_parent.roots(),
                            runtime: primary_parent.runtime(), cognition: primary_parent.cognition(),
                            retention: primary_parent.retention_ledger(), head: primary_parent.head(), authority: primary_parent.authority(),
                        }, &mut common)?;
                        let replica_bound = bind_normal_tick_parent_v1(NormalTickParentRefsV1 {
                            core: replica_parent.core(), storage: replica_parent.storage(), roots: replica_parent.roots(),
                            runtime: replica_parent.runtime(), cognition: replica_parent.cognition(),
                            retention: replica_parent.retention_ledger(), head: replica_parent.head(), authority: replica_parent.authority(),
                        }, &mut common)?;
                        let pages = replica_parent.pages();
                        let budget = take(&mut io.state);
                        let ceiling = amounts(&budget);
                        let state = Loan {
                            value: pages.state_session_with_cancelled(budget, MemoryClass::Ordinary, cancelled).map_err(physical)?,
                            slot: &mut io.state,
                            ceiling,
                        };
                        let budget = take(&mut io.values);
                        let ceiling = amounts(&budget);
                        let values = Loan {
                            value: pages.value_session_with_cancelled(budget, cancelled),
                            slot: &mut io.values,
                            ceiling,
                        };
                        let budget = take(&mut io.members);
                        let ceiling = amounts(&budget);
                        let mut members = Loan {
                            value: pages.value_session_with_cancelled(budget, cancelled),
                            slot: &mut io.members,
                            ceiling,
                        };
                        let objects = super::normal::io::open_originals_metered(pages, &mut io.originals, cancelled, &mut common)?;
                        let original_holder = super::normal::originals::HostNormalOriginalsV1::new(
                            &objects,
                            ObjectIo {
                                physical: &mut io.originals,
                                logical: &mut io.original_values,
                                class: MemoryClass::Ordinary,
                                cancelled,
                            },
                            memory,
                        ).map_err(physical)?;
                        let mut originals = original_holder.reader();
                        // Verify the replica's own persisted sidecar bytes even
                        // though its immutable columns come from the primary's
                        // already checked same-record projection cache.
                        let _side = read_projection_sidecar_bounded_v1(
                            replica_parent.head(), replica_parent.core(), replica_proof.material(),
                            &mut BootstrapProjectionReader(&mut originals), &mut common,
                        ).map_err(|error| IwsHostRuntimeErrorV1::NormalSource(
                            oasis7_runtime::BodyMutationError::Original(Box::new(error)),
                        ))?;
                        let _roots = reserve_roots_clone(replica_parent.roots(), &mut common)?;
                        let fence = replica_bound.bootstrap_fence_data_v1();
                        let mut tx = ExecutionTransaction::begin(ExecutionParent {
                            commit: fence.parent_commit, roots: (**replica_parent.roots()).clone(), authority: fence.authority,
                        }, replica_parent.head().verified_record_id(), &state.value, memory, transaction)
                            .map_err(IwsHostRuntimeErrorV1::Move)?;
                        let views = super::normal::values::HostNormalValuesV1::new(&mut members.value, memory)
                            .map_err(IwsHostRuntimeErrorV1::Value)?;
                        let mut member_sink = views.handle();
                        source.with_native_worker(memory, archive, &mut || cancelled.load(Ordering::Acquire), |lease| {
                            with_admitted_native_lease(lease, source.execution_environment(), &limits, memory, archive, cancelled, |_worker| {
                                oasis7_runtime::bootstrap_authenticated_genesis_replica_resident_v1(
                                    &primary_bound, primary_resident, &replica_bound,
                                    &mut tx, &state.value, &values.value, &mut member_sink, &mut originals,
                                    projection, genesis, authority, runtime_work,
                                    || state.value.take_read_error().map(|error| ValueTreeError::Io(std::io::Error::other(error))),
                                    || cancelled.load(Ordering::Acquire), &mut common, &mut || source.fence(),
                                ).map_err(IwsHostRuntimeErrorV1::NormalSource)
                            })
                        })
                    },
                )
            },
        )?;
        self.finish_admitted_genesis_resident(source, limits, resident, memory, archive, cancelled)
    }

    fn finish_admitted_genesis_resident(
        &mut self,
        source: PaidAcceptedGenesisHostSourceV1,
        limits: WorkerLimitsV1,
        resident: ReservedValue<NormalHostResidentSourcesV1>,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        {
            let mut stop = || cancelled.load(Ordering::Acquire);
            let mut common = archive.bounded_context(memory, &mut stop);
            source.validate_context(&mut common)?;
            resident
                .validate_live_binding_v1(source.record_id(), &source.fence(), &mut common)
                .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
        }
        // Native admission and every fallible source check already succeeded.
        // Empty-slot preparation is the last fallible step; finish only moves.
        let prepared = self.current.prepare_empty(
            HostCurrentV1 {
                source,
                limits,
                resident: Some(resident),
            },
            memory,
            cancelled,
        )?;
        prepared.finish();
        Ok(())
    }
}
