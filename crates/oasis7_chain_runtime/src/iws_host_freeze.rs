//! Consuming Host -> real Frozen persistence. A failed/unknown publication
//! leaves this Host empty and preserves the endpoint's typed recovery owner.
use super::*;
use oasis7_distfs::controlled_authority::formal::endpoint::PaidNormalFrozenEndpointV1;
use oasis7_formal_protocol::controlled_authority::formal::{
    IncrementalFrozenMaterialWorkV1, SignedFormalFrozenV1,
};
use oasis7_formal_protocol::controlled_authority::replicated_protocol::ClosedRecord;

pub(super) struct HostNormalContinuationV1 {
    pub(super) resident: Option<
        oasis7_proto::memory_budget::ReservedValue<oasis7_runtime::NormalHostResidentSourcesV1>,
    >,
    pub(super) identity:
        oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    pub(super) fee: oasis7_proto::memory_budget::ReservedValue<()>,
    pub(super) record: [u8; 32],
    pub(super) role:
        oasis7_formal_protocol::controlled_authority::replicated_protocol::EndpointRole,
    pub(super) fence: ExecutionFence,
    pub(super) environment: [u8; 32],
    pub(super) page_namespace: (u64, u64),
    pub(super) native_namespace: (u64, u64),
    pub(super) generation: u64,
    pub(super) limits: WorkerLimitsV1,
}

impl IwsHostRuntimeV1 {
    /// Consumes the installed live endpoint and its unique FD. The signed
    /// reservation and material are checked by the actual endpoint publisher;
    /// no caller-supplied root or fence can restore the consumed Host source.
    #[allow(clippy::too_many_arguments)]
    pub fn freeze_current_normal_bounded(
        &mut self,
        input: &SignedFormalFrozenV1,
        material: &ClosedRecord,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        material_work: &mut IncrementalFrozenMaterialWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<PaidNormalFrozenEndpointV1, IwsHostRuntimeErrorV1> {
        // Cancellation/Core admission can fail before taking the current
        // owner. Once taken, every failure leaves the execution slot empty.
        work.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
        let current = self
            .current
            .current()
            .ok_or_else(|| FormalError::Invalid("Host current owner missing".into()))?;
        let identity = {
            let mut stop = || cancelled.load(Ordering::Acquire);
            let mut context = work.bounded_context(memory, &mut stop);
            current.source.validate_context(&mut context)?;
            context.work().identity()
        };
        let fee = memory
            .reserve(size_of::<HostNormalContinuationV1>(), MemoryClass::Ordinary)
            .map_err(|e| IwsHostRuntimeErrorV1::Move(e.into()))?
            .retain(());
        let next_generation =
            self.current
                .generation()
                .checked_add(1)
                .ok_or(IwsHostRuntimeErrorV1::Move(
                    ExecutionError::ResourceLimited("Host generation"),
                ))?;
        let continuation = &mut self.continuation;
        self.current.consume(|current| {
            // These expectations come only from the actual source just removed
            // from the slot. They cannot install a source or reopen its FD.
            *continuation = Some(HostNormalContinuationV1 {
                identity,
                fee,
                record: current.source.record_id(),
                role: current.source.endpoint_role(),
                fence: current.source.fence(),
                environment: current.source.execution_environment(),
                page_namespace: current.source.page_namespace_identity(),
                native_namespace: current.source.native_namespace_identity(),
                generation: next_generation,
                limits: current.limits,
                resident: current.resident,
            });
            current
                .source
                .freeze_normal_bounded(
                    input,
                    material,
                    provider,
                    policy,
                    memory,
                    work,
                    material_work,
                    &mut || cancelled.load(Ordering::Acquire),
                )
                .map_err(IwsHostRuntimeErrorV1::Frozen)
        })
    }

    /// Install only a durable accepted successor of the source consumed by
    /// this Host. All worker checks and allocation precede the no-fail move.
    pub fn install_accepted_normal_source_bounded(
        &mut self,
        source: PaidAcceptedGenesisHostSourceV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        let expected = self
            .continuation
            .as_ref()
            .ok_or_else(|| FormalError::Invalid("Host normal continuation missing".into()))?;
        let prepared = prepare_normal_current(
            &mut self.current,
            expected,
            source,
            None,
            memory,
            work,
            cancelled,
        )?;
        prepared.finish();
        self.continuation = None;
        Ok(())
    }
}

fn prepare_normal_current<'a>(
    slot: &'a mut slot::HostCurrentSlotV1,
    expected: &HostNormalContinuationV1,
    source: PaidAcceptedGenesisHostSourceV1,
    resident: Option<
        oasis7_proto::memory_budget::ReservedValue<oasis7_runtime::NormalHostResidentSourcesV1>,
    >,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<slot::PreparedHostCurrentV1<'a>, IwsHostRuntimeErrorV1> {
    let mut stop = || cancelled.load(Ordering::Acquire);
    {
        let mut context = work.bounded_context(memory, &mut stop);
        source.validate_context(&mut context)?;
        if let Some(resident) = resident.as_ref() {
            resident
                .validate_live_binding_v1(source.record_id(), &source.fence(), &mut context)
                .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
        }
        match (expected.resident.as_ref(), resident.as_ref()) {
            (Some(previous), Some(next)) => next
                .validate_same_host_process_v1(previous, &mut context)
                .map_err(IwsHostRuntimeErrorV1::NormalSource)?,
            (None, None) => (),
            _ => {
                return Err(
                    FormalError::Invalid("normal Host process resident continuity".into()).into(),
                );
            }
        }
        if expected.identity != context.work().identity() || !expected.fee.uses_budget(memory) {
            return Err(FormalError::Invalid("Host normal continuation pool/Core".into()).into());
        }
    }
    work.charge(1, 192, &mut stop)?;
    if !slot.is_empty()
        || slot.generation() != expected.generation
        || source.predecessor_record_id() != Some(expected.record)
        || source.endpoint_role() != expected.role
        || source.predecessor_fence() != Some(expected.fence)
        || source.execution_environment() != expected.environment
        || source.page_namespace_identity() != expected.page_namespace
        || source.native_namespace_identity() != expected.native_namespace
    {
        return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
    }
    let current = HostCurrentV1 {
        source,
        resident,
        limits: WorkerLimitsV1 {
            address_space_bytes: expected.limits.address_space_bytes,
            max_request_bytes: expected.limits.max_request_bytes,
            max_output_bytes: expected.limits.max_output_bytes,
            wall_time_ms: expected.limits.wall_time_ms,
        },
    };
    admit_worker(&current, memory, work, cancelled)?;
    slot.prepare_empty(current, memory, cancelled)
}

impl IwsHostRuntimeV1 {
    /// Prepare both genuine accepted sources before either no-fail cache move.
    /// A failed second admission leaves both consumed Host slots empty.
    pub fn install_accepted_normal_pair_bounded(
        primary: &mut Self,
        replica: &mut Self,
        pair: oasis7_distfs::controlled_authority::formal::endpoint::PaidNormalAcceptedPairV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        let (p, r, quorum, _pair_fee) = pair.into_sources();
        quorum.validate_context(
            &mut work.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        )?;
        let pexpected = primary.continuation.as_ref().ok_or_else(|| {
            FormalError::Invalid("primary Host normal continuation missing".into())
        })?;
        let rexpected = replica.continuation.as_ref().ok_or_else(|| {
            FormalError::Invalid("replica Host normal continuation missing".into())
        })?;
        let p = prepare_normal_current(
            &mut primary.current,
            pexpected,
            p,
            None,
            memory,
            work,
            cancelled,
        )?;
        let r = prepare_normal_current(
            &mut replica.current,
            rexpected,
            r,
            None,
            memory,
            work,
            cancelled,
        )?;
        p.finish();
        r.finish();
        primary.continuation = None;
        replica.continuation = None;
        Ok(())
    }
}

impl IwsHostRuntimeV1 {
    /// Complete normal installation consumes both preaccepted resident owners;
    /// no source-only slot is silently upgraded or reconstructed from history.
    #[allow(clippy::too_many_arguments)]
    pub fn install_accepted_normal_pair_with_residents_bounded(
        primary: &mut Self,
        replica: &mut Self,
        pair: oasis7_distfs::controlled_authority::formal::endpoint::PaidNormalAcceptedPairV1,
        primary_resident: oasis7_runtime::PreparedNormalHostResidentV1,
        replica_resident: oasis7_runtime::PreparedNormalHostResidentV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        let (p, r, quorum, _pair_fee) = pair.into_sources();
        quorum.validate_context(
            &mut work.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        )?;
        let p_resident = primary_resident
            .finish(quorum.verified())
            .map_err(IwsHostRuntimeErrorV1::Move)?;
        let r_resident = replica_resident
            .finish(quorum.verified())
            .map_err(IwsHostRuntimeErrorV1::Move)?;
        let pe = primary
            .continuation
            .as_ref()
            .ok_or_else(|| FormalError::Invalid("primary normal continuation missing".into()))?;
        let re = replica
            .continuation
            .as_ref()
            .ok_or_else(|| FormalError::Invalid("replica normal continuation missing".into()))?;
        let p = prepare_normal_current(
            &mut primary.current,
            pe,
            p,
            Some(p_resident),
            memory,
            work,
            cancelled,
        )?;
        let r = prepare_normal_current(
            &mut replica.current,
            re,
            r,
            Some(r_resident),
            memory,
            work,
            cancelled,
        )?;
        p.finish();
        r.finish();
        primary.continuation = None;
        replica.continuation = None;
        Ok(())
    }
}
