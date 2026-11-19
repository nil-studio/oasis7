//! Actual GEN/cold Cognition admission from the installed Host's accepted head.
//! The independent paid owner releases the Host borrow before normal advance;
//! warm execution must retain/promote its cache instead of repeating this scan.
use super::cold::{Loan, amounts, take};
use super::*;
use oasis7_distfs::incremental_page_store::{PageStoreError, objects::ObjectIo};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits},
    incremental_state::StateRootV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_runtime::{
    NormalHostCognitionBootstrapErrorV1, NormalHostCognitionParentSourcesV1, NormalHostWorkV1,
    PaidNormalHostCognitionParentSourcesV1,
    incremental_normal_sources::{NormalTickParentRefsV1, bind_normal_tick_parent_v1},
};
use oasis7_world_state::{
    incremental_cognition_proof_ledger::ProofLedgerSourceErrorV1,
    incremental_world_result_router_accepted::AcceptedWorldResultRouterErrorV1,
};
use std::mem::size_of_val;

#[path = "iws_host_cognition_startup_read.rs"]
mod read;
pub use read::HostCognitionStartupWorkV1;

#[derive(Debug)]
pub enum HostCognitionStartupErrorV1 {
    Host(IwsHostRuntimeErrorV1),
    Router(AcceptedWorldResultRouterErrorV1<PageStoreError>),
    Ledger(ProofLedgerSourceErrorV1<PageStoreError>),
    Runtime(NormalHostCognitionBootstrapErrorV1<PageStoreError>),
}
impl From<IwsHostRuntimeErrorV1> for HostCognitionStartupErrorV1 {
    fn from(error: IwsHostRuntimeErrorV1) -> Self {
        Self::Host(error)
    }
}
impl From<FormalError> for HostCognitionStartupErrorV1 {
    fn from(error: FormalError) -> Self {
        Self::Host(error.into())
    }
}
impl From<PageStoreError> for HostCognitionStartupErrorV1 {
    fn from(error: PageStoreError) -> Self {
        Self::Host(IwsHostRuntimeErrorV1::Physical(error))
    }
}
impl std::fmt::Display for HostCognitionStartupErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Host Cognition startup: {self:?}")
    }
}
impl std::error::Error for HostCognitionStartupErrorV1 {}

fn invalid(message: &'static str) -> HostCognitionStartupErrorV1 {
    FormalError::Invalid(message.into()).into()
}

/// The transaction owns this exact RootSet clone. Admit its complete string
/// and vector layout before cloning and retain the fee across the readonly TX.
fn reserve_roots_clone(
    roots: &RootSetV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, HostCognitionStartupErrorV1> {
    let layout = size_of_val(roots)
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
        .ok_or_else(|| invalid("Host Cognition RootSet layout"))?;
    let bytes = u64::try_from(layout).map_err(|_| invalid("Host Cognition RootSet width"))?;
    common.charge(bytes, bytes)?;
    common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)
        .map_err(Into::into)
}

impl IwsHostRuntimeV1 {
    /// Build a separate actual ledger/evidence owner once at GEN/cold admission.
    /// All IO accounts are the caller's remaining accounts. The returned value
    /// owns no Host, endpoint, World, transaction, worker or accepted permission.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_cognition_startup_bounded(
        &self,
        io: &mut HostColdIoWorkV1,
        source_work: &mut HostCognitionStartupWorkV1,
        runtime_work: &mut NormalHostWorkV1,
        transaction: TransactionLimits,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<PaidNormalHostCognitionParentSourcesV1, HostCognitionStartupErrorV1> {
        archive.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
        if self.continuation.is_some() {
            return Err(invalid("Cognition bootstrap during normal continuation"));
        }
        let current = self
            .current
            .current()
            .ok_or_else(|| invalid("Cognition installed Host"))?;
        let resident = current
            .resident
            .as_ref()
            .ok_or_else(|| invalid("Cognition installed resident"))?;
        let source = &current.source;
        source.with_parent_read_view_bounded(
            memory,
            archive,
            &mut || cancelled.load(Ordering::Acquire),
            |parent| -> Result<_, HostCognitionStartupErrorV1> {
                let mut stop = || cancelled.load(Ordering::Acquire);
                let mut common = archive.bounded_context(memory, &mut stop);
                source.validate_context(&mut common)?;
                let proof =
                    parent.read_qualified_parent_proof_bounded(memory, archive, &mut || {
                        cancelled.load(Ordering::Acquire)
                    })?;
                proof.validate_parent_bounded(&parent, &mut common)?;
                let bound = bind_normal_tick_parent_v1(
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
                let ledger =
                    read::recover(&parent, io, source_work, memory, cancelled, &mut common)?;
                // The legacy source phase has finished its exclusive child.
                // The readonly Root below resumes the same original Common.
                let pages = parent.pages();
                let budget = take(&mut io.state);
                let ceiling = amounts(&budget);
                let state = Loan {
                    value: pages.state_session_with_cancelled(
                        budget,
                        MemoryClass::Ordinary,
                        cancelled,
                    )?,
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
                let objects = super::normal::io::open_originals_metered(
                    pages,
                    &mut io.originals,
                    cancelled,
                    &mut common,
                )?;
                let original_holder = super::normal::originals::HostNormalOriginalsV1::new(
                    &objects,
                    ObjectIo {
                        physical: &mut io.originals,
                        logical: &mut io.original_values,
                        class: MemoryClass::Ordinary,
                        cancelled,
                    },
                    memory,
                )?;
                let mut originals = original_holder.reader();
                let _roots = reserve_roots_clone(parent.roots(), &mut common)?;
                let fence = bound.bootstrap_fence_data_v1();
                let mut tx = ExecutionTransaction::begin(
                    ExecutionParent {
                        commit: fence.parent_commit,
                        roots: (**parent.roots()).clone(),
                        authority: fence.authority,
                    },
                    parent.head().verified_record_id(),
                    &state.value,
                    memory,
                    transaction,
                )
                .map_err(IwsHostRuntimeErrorV1::Move)?;
                let views =
                    super::normal::values::HostNormalValuesV1::new(&mut members.value, memory)
                        .map_err(IwsHostRuntimeErrorV1::Value)?;
                let (mut member_sink, mut fee_sink, mut event_sink) =
                    (views.handle(), views.handle(), views.handle());
                source.with_native_worker(
                    memory,
                    archive,
                    &mut || cancelled.load(Ordering::Acquire),
                    |lease| {
                        // Keep the Runtime's original typed failure as the inner
                        // result; native admission still uses its original errors.
                        with_admitted_native_lease(
                            lease,
                            source.execution_environment(),
                            &current.limits,
                            memory,
                            archive,
                            cancelled,
                            |worker| {
                                Ok(oasis7_runtime::bootstrap_normal_host_cognition_parent_v1(
                                    &bound,
                                    &mut tx,
                                    &state.value,
                                    &values.value,
                                    &mut member_sink,
                                    &mut fee_sink,
                                    &mut event_sink,
                                    &mut originals,
                                    |event| original_holder.persist_event(event),
                                    worker,
                                    resident,
                                    ledger,
                                    runtime_work,
                                    || {
                                        state.value.take_read_error().map(|error| {
                                            ValueTreeError::Io(std::io::Error::other(error))
                                        })
                                    },
                                    || cancelled.load(Ordering::Acquire),
                                    &mut common,
                                    &mut || source.fence(),
                                )
                                .map_err(HostCognitionStartupErrorV1::Runtime))
                            },
                        )
                        .map_err(HostCognitionStartupErrorV1::Host)?
                    },
                )
            },
        )
    }

    /// Recheck the retained evidence against the actual current parent. The
    /// returned borrows are tied only to `evidence`; the Host borrow ends here,
    /// allowing the fixed normal consumer to advance both actual Host slots.
    pub fn cognition_parent_sources_bounded<'a>(
        &self,
        evidence: &'a PaidNormalHostCognitionParentSourcesV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<NormalHostCognitionParentSourcesV1<'a>, HostCognitionStartupErrorV1> {
        let current = self
            .current
            .current()
            .ok_or_else(|| invalid("Cognition current Host"))?;
        current.source.with_parent_read_view_bounded(
            memory,
            archive,
            &mut || cancelled.load(Ordering::Acquire),
            |parent| -> Result<_, HostCognitionStartupErrorV1> {
                let mut stop = || cancelled.load(Ordering::Acquire);
                let mut common = archive.bounded_context(memory, &mut stop);
                let bound = bind_normal_tick_parent_v1(
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
                    .as_sources_bounded(&bound, &mut common)
                    .map_err(IwsHostRuntimeErrorV1::NormalSource)
                    .map_err(Into::into)
            },
        )
    }
}
