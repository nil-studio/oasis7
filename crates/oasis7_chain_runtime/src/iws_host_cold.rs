//! Fixed authenticated cold source -> actual native admission -> complete
//! Runtime resident -> no-fail Host install. No public execution callback.
use super::*;
use oasis7_distfs::{
    controlled_authority::formal::endpoint::{
        AuthenticatedColdHostErrorV1, IwsCandidateSourceWorkV1, IwsColdParentHintsV1,
        IwsColdParentWorkV1,
    },
    incremental_page_store::{
        PageIoBudget, PageStoreError, StatePageSession, ValuePageSession,
        objects::{IncrementalObjectStore, ObjectIo},
    },
};
use oasis7_formal_protocol::{
    incremental_execution::TransactionLimits,
    incremental_material::FactLocationV1,
    incremental_value::{ValueIoBudget, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{
    NormalHostStartupWorkV1, NormalHostWorkV1, ProvisionalAuthorizationProjectionV1,
    WorldRuntimeMemoryLimits,
};

/// Actual independent physical/logical cold startup quotas. No implicit
/// constructor, refill or copied balance is provided by this fixed consumer.
pub struct HostColdIoWorkV1 {
    /// The actual remaining SDK codec/signature allowance used by signed
    /// startup configuration. It is retained by this same cold work owner.
    pub process_canonical: oasis7_wasm_abi::CanonicalCborWorkV1,
    pub state: PageIoBudget,
    pub values: PageIoBudget,
    pub members: PageIoBudget,
    pub originals: PageIoBudget,
    pub original_values: ValueIoBudget,
}
#[derive(Debug)]
pub enum HostColdStartupErrorV1 {
    Source(AuthenticatedColdHostErrorV1),
    Host(IwsHostRuntimeErrorV1),
}
impl From<IwsHostRuntimeErrorV1> for HostColdStartupErrorV1 {
    fn from(e: IwsHostRuntimeErrorV1) -> Self {
        Self::Host(e)
    }
}
impl From<FormalError> for HostColdStartupErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Host(e.into())
    }
}
impl std::fmt::Display for HostColdStartupErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cold Host startup: {self:?}")
    }
}
impl std::error::Error for HostColdStartupErrorV1 {}

enum ProcessInput<'a> {
    Governed(&'a oasis7_runtime::ReleaseSecurityPolicy),
    #[cfg(feature = "node-libp2p")]
    DevLocal(&'a crate::execution_config::DevLocalParticipantConfig),
}
fn take(slot: &mut PageIoBudget) -> PageIoBudget {
    std::mem::replace(
        slot,
        PageIoBudget {
            reads_remaining: 0,
            read_bytes_remaining: 0,
            writes_remaining: 0,
            write_bytes_remaining: 0,
        },
    )
}
fn amounts(v: &PageIoBudget) -> [u64; 4] {
    [
        v.reads_remaining,
        v.read_bytes_remaining,
        v.writes_remaining,
        v.write_bytes_remaining,
    ]
}
trait Remaining {
    fn remaining(&self) -> Result<PageIoBudget, PageStoreError>;
}
impl Remaining for StatePageSession<'_> {
    fn remaining(&self) -> Result<PageIoBudget, PageStoreError> {
        self.remaining_io()
    }
}
impl Remaining for ValuePageSession<'_> {
    fn remaining(&self) -> Result<PageIoBudget, PageStoreError> {
        self.remaining_io()
    }
}
struct Loan<'a, T: Remaining> {
    value: T,
    slot: &'a mut PageIoBudget,
    ceiling: [u64; 4],
}
impl<T: Remaining> Drop for Loan<'_, T> {
    fn drop(&mut self) {
        if let Ok(after) = self.value.remaining() {
            if amounts(&after)
                .into_iter()
                .zip(self.ceiling)
                .all(|(a, b)| a <= b)
            {
                *self.slot = after;
            }
        }
    }
}
fn physical(e: PageStoreError) -> IwsHostRuntimeErrorV1 {
    IwsHostRuntimeErrorV1::Physical(e)
}

impl IwsHostRuntimeV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn install_authenticated_cold_governed_bounded<L>(
        &mut self,
        endpoint: FileFormalEndpoint,
        provider: &RemoteMonotonicEndpointProvider,
        pinned: &TrustedFencingPolicyV1,
        policy: &oasis7_runtime::ReleaseSecurityPolicy,
        limits: WorkerLimitsV1,
        configuration: WorldRuntimeMemoryLimits,
        projection: ProvisionalAuthorizationProjectionV1,
        cold: &mut IwsColdParentWorkV1,
        execution: &mut IwsCandidateSourceWorkV1,
        hints: IwsColdParentHintsV1,
        mailbox_hint: L,
        physical: &mut HostColdIoWorkV1,
        runtime_work: &mut NormalHostWorkV1<'_>,
        compiler: &oasis7_runtime::RuntimeSourceCompiler,
        startup: &mut NormalHostStartupWorkV1<'_>,
        transaction: TransactionLimits,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), HostColdStartupErrorV1>
    where
        L: FnMut(&[u8; 32]) -> Result<FactLocationV1, String>,
    {
        self.install_authenticated_cold_inner(
            endpoint,
            provider,
            pinned,
            ProcessInput::Governed(policy),
            limits,
            configuration,
            projection,
            cold,
            execution,
            hints,
            mailbox_hint,
            physical,
            runtime_work,
            compiler,
            startup,
            transaction,
            memory,
            archive,
            cancelled,
        )
    }
    #[cfg(feature = "node-libp2p")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn install_authenticated_cold_devlocal_bounded<L>(
        &mut self,
        endpoint: FileFormalEndpoint,
        provider: &RemoteMonotonicEndpointProvider,
        pinned: &TrustedFencingPolicyV1,
        control: &crate::execution_config::DevLocalParticipantConfig,
        limits: WorkerLimitsV1,
        configuration: WorldRuntimeMemoryLimits,
        projection: ProvisionalAuthorizationProjectionV1,
        cold: &mut IwsColdParentWorkV1,
        execution: &mut IwsCandidateSourceWorkV1,
        hints: IwsColdParentHintsV1,
        mailbox_hint: L,
        physical: &mut HostColdIoWorkV1,
        runtime_work: &mut NormalHostWorkV1<'_>,
        compiler: &oasis7_runtime::RuntimeSourceCompiler,
        startup: &mut NormalHostStartupWorkV1<'_>,
        transaction: TransactionLimits,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), HostColdStartupErrorV1>
    where
        L: FnMut(&[u8; 32]) -> Result<FactLocationV1, String>,
    {
        self.install_authenticated_cold_inner(
            endpoint,
            provider,
            pinned,
            ProcessInput::DevLocal(control),
            limits,
            configuration,
            projection,
            cold,
            execution,
            hints,
            mailbox_hint,
            physical,
            runtime_work,
            compiler,
            startup,
            transaction,
            memory,
            archive,
            cancelled,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn install_authenticated_cold_inner<L>(
        &mut self,
        endpoint: FileFormalEndpoint,
        provider: &RemoteMonotonicEndpointProvider,
        pinned: &TrustedFencingPolicyV1,
        process: ProcessInput<'_>,
        limits: WorkerLimitsV1,
        configuration: WorldRuntimeMemoryLimits,
        projection: ProvisionalAuthorizationProjectionV1,
        cold: &mut IwsColdParentWorkV1,
        execution: &mut IwsCandidateSourceWorkV1,
        hints: IwsColdParentHintsV1,
        mailbox_hint: L,
        io: &mut HostColdIoWorkV1,
        runtime_work: &mut NormalHostWorkV1<'_>,
        compiler: &oasis7_runtime::RuntimeSourceCompiler,
        startup: &mut NormalHostStartupWorkV1<'_>,
        transaction: TransactionLimits,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), HostColdStartupErrorV1>
    where
        L: FnMut(&[u8; 32]) -> Result<FactLocationV1, String>,
    {
        archive.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
        if !self.current.is_empty() || self.continuation.is_some() {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict).into());
        }
        let source = endpoint
            .into_authenticated_cold_host_source_bounded(
                provider,
                pinned,
                memory,
                archive,
                cold,
                execution,
                hints,
                mailbox_hint,
                &mut || cancelled.load(Ordering::Acquire),
            )
            .map_err(HostColdStartupErrorV1::Source)?;
        let resident = source.with_parent_read_view_bounded(
            memory,
            archive,
            &mut || cancelled.load(Ordering::Acquire),
            |parent| -> Result<_, IwsHostRuntimeErrorV1> {
                let mut stop = || cancelled.load(Ordering::Acquire);
                let mut common = archive.bounded_context(memory, &mut stop);
                source.validate_context(&mut common)?;
                let process = match process {
                    ProcessInput::Governed(policy) => {
                        super::process::prepare_governed_cold_process_from_verified_restore_v1(
                            parent.authority(),
                            parent.head(),
                            policy,
                            &mut common,
                        )?
                    }
                    #[cfg(feature = "node-libp2p")]
                    ProcessInput::DevLocal(control) => {
                        super::process::prepare_devlocal_cold_process_from_verified_restore_v1(
                            control,
                            parent.authority(),
                            parent.head(),
                            &mut common,
                            &mut io.process_canonical,
                        )?
                    }
                };
                let configuration =
                    oasis7_runtime::prepare_normal_host_cold_runtime_configuration_v1(
                        configuration,
                        &mut common,
                    )?;
                let pages = parent.pages();
                // Every original physical account moves into one concrete actual
                // page session and returns its true remainder on every exit.
                let budget = take(&mut io.state);
                let ceiling = amounts(&budget);
                let state = Loan {
                    value: pages
                        .state_session_with_cancelled(budget, MemoryClass::Ordinary, cancelled)
                        .map_err(physical)?,
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
                let objects=super::normal::io::open_originals_metered(pages,&mut io.originals,cancelled,&mut common)?;
                let object_io = ObjectIo {
                    physical: &mut io.originals,
                    logical: &mut io.original_values,
                    class: MemoryClass::Ordinary,
                    cancelled,
                };
                let original_holder = super::normal::originals::HostNormalOriginalsV1::new(&objects,object_io,memory).map_err(physical)?;
                let mut originals=original_holder.reader();
                let proof=parent.read_qualified_parent_proof_bounded(memory,archive,&mut||cancelled.load(Ordering::Acquire))?;
                proof.validate_parent_bounded(&parent,&mut common)?;
                let side=oasis7_formal_protocol::incremental_projection_sidecar::read_projection_sidecar_bounded_v1(parent.head(),parent.core(),proof.material(),&mut ColdProjectionReader(&mut originals),&mut common).map_err(|error|IwsHostRuntimeErrorV1::NormalSource(oasis7_runtime::BodyMutationError::Original(Box::new(error))))?;
                let bound=oasis7_runtime::incremental_normal_sources::bind_normal_tick_parent_v1(
                    oasis7_runtime::incremental_normal_sources::NormalTickParentRefsV1{core:parent.core(),storage:parent.storage(),roots:parent.roots(),runtime:parent.runtime(),cognition:parent.cognition(),retention:parent.retention_ledger(),head:parent.head(),authority:parent.authority()},&mut common)?;
                let roots_source=&**parent.roots();
                let roots_layout=std::mem::size_of_val(roots_source).checked_add(roots_source.world_id.len()).and_then(|n|roots_source.state.len().checked_mul(std::mem::size_of::<oasis7_formal_protocol::incremental_state::StateRootV1>()).and_then(|v|n.checked_add(v))).and_then(|n|roots_source.state.iter().try_fold(n,|n,root|n.checked_add(root.node_root.len()))).and_then(|n|n.checked_mul(4)).and_then(|n|n.checked_add(8192)).ok_or_else(||physical(PageStoreError::ResourceLimited("cold accepted RootSet clone layout")))?;
                common.charge(roots_layout as u64,roots_layout as u64)?;
                let _roots=memory.reserve(roots_layout,MemoryClass::Ordinary).map_err(PageStoreError::Memory).map_err(physical)?;
                let fence=bound.bootstrap_fence_data_v1();
                let mut tx=oasis7_formal_protocol::incremental_execution::ExecutionTransaction::begin(oasis7_formal_protocol::incremental_execution::ExecutionParent{commit:fence.parent_commit,roots:(**parent.roots()).clone(),authority:fence.authority},parent.head().verified_record_id(),&state.value,memory,transaction).map_err(IwsHostRuntimeErrorV1::Move)?;
                let views=super::normal::values::HostNormalValuesV1::new(&mut members.value,memory).map_err(IwsHostRuntimeErrorV1::Value)?;
                let(mut member_sink,mut fee_sink,mut event_sink)=(views.handle(),views.handle(),views.handle());
                // Actual accepted lease admission precedes Runtime construction;
                // no resident/current slot is needed to manufacture a worker.
                source.with_native_worker(memory,archive,&mut||cancelled.load(Ordering::Acquire),|lease|{
                    with_admitted_native_lease(lease,source.execution_environment(),&limits,memory,archive,cancelled,|worker|{
                        oasis7_runtime::bootstrap_authenticated_cold_normal_host_resident_v1(
                            &bound,&mut tx,&state.value,&values.value,&mut member_sink,&mut fee_sink,&mut event_sink,&mut originals,
                            |event|original_holder.persist_event(event),worker,projection,&process,&configuration,side,parent.proof_ledger(),parent.retention_ledger().ok_or_else(||FormalError::Invalid("cold parent missing signed retention commitment".into()))?,compiler,startup,runtime_work,
                            ||state.value.take_read_error().map(|error|ValueTreeError::Io(std::io::Error::other(error))),
                            ||cancelled.load(Ordering::Acquire),&mut common,&mut||source.fence(),
                        ).map_err(IwsHostRuntimeErrorV1::ColdNormal)
                    })
                })
            },
        )?;
        // Same-Core/record/fence and actual native admission all precede slot
        // preparation. The final transition is only a move into an empty slot.
        {
            let mut stop = || cancelled.load(Ordering::Acquire);
            let mut common = archive.bounded_context(memory, &mut stop);
            resident
                .validate_live_binding_v1(source.record_id(), &source.fence(), &mut common)
                .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
        }
        let current = HostCurrentV1 {
            source,
            limits,
            resident: Some(resident),
        };
        let prepared = self.current.prepare_empty(current, memory, cancelled)?;
        prepared.finish();
        Ok(())
    }
}

/// Sidecar DATA reads share the same actual Cold Original account and retained
/// byte/hash admission. This wrapper has no source/Root/acceptance constructor.
struct ColdProjectionReader<'a, T>(&'a mut T);
impl<
    T: oasis7_runtime::PaidProjectionRetainedOriginalReaderV1<
            RetainedError = oasis7_runtime::BodyMutationError,
        >,
> oasis7_formal_protocol::incremental_projection_sidecar::PaidProjectionOriginalReaderV1
    for ColdProjectionReader<'_, T>
{
    type Error = oasis7_runtime::BodyMutationError;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.0.allocation_budget()
    }
    fn allocation_class(&self) -> MemoryClass {
        self.0.allocation_class()
    }
    fn read_projection_original_bounded_v1(
        &mut self,
        id: &[u8; 32],
        max: usize,
        common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        self.0.read_retained_original_bounded_v1(id, max, common)
    }
}
