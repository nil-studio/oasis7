//! One private physical IO loan over the actual admitted parent's page pool.
//! Provisional immutable writes do not create Begin or accepted-head authority.
use super::originals::HostNormalOriginalsV1;
use super::values::HostNormalValuesV1;
use super::*;
use oasis7_distfs::incremental_page_store::{
    PageIoBudget, PageStoreError, StatePageSession,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueIoBudget,
};

/// Original independent physical and logical allowances, continuously borrowed.
/// This type has no default or implicit allowance constructor.
pub struct HostNormalIoWorkV1 {
    pub state_pages: PageIoBudget,
    pub value_pages: PageIoBudget,
    pub original_pages: PageIoBudget,
    pub original_values: ValueIoBudget,
    pub replica_value_pages: PageIoBudget,
    pub replica_original_pages: PageIoBudget,
    pub replica_original_values: ValueIoBudget,
}

fn exhausted_pages() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 0,
        read_bytes_remaining: 0,
        writes_remaining: 0,
        write_bytes_remaining: 0,
    }
}
fn io_error(error: PageStoreError) -> IwsHostRuntimeErrorV1 {
    IwsHostRuntimeErrorV1::Physical(error)
}

/// Opening the real object namespace is IO too, before a stream adapter exists.
/// Borrow the same Common once and reconcile the four actual physical lanes.
pub(in crate::iws_host_runtime) fn open_originals_metered<'a>(
    pages: &'a oasis7_distfs::incremental_page_store::IncrementalPageStore,
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<IncrementalObjectStore<'a>, IwsHostRuntimeErrorV1> {
    // open() also validates the held lease/directory and fsyncs the marker
    // and namespace; these fixed metadata operations are outside PageIo lanes.
    // Cover their path copies/inspection before entering the exclusive child.
    let path_bytes = u64::try_from(pages.root().as_os_str().as_encoded_bytes().len())
        .map_err(|_| FormalError::Invalid("normal original namespace path length".into()))?;
    common.charge(
        path_bytes
            .checked_add(64)
            .ok_or_else(|| FormalError::Invalid("normal original open work".into()))?,
        path_bytes
            .checked_mul(8)
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(|| FormalError::Invalid("normal original open bytes".into()))?,
    )?;
    let original = io.clone();
    let (_, meter, _) = common.parts();
    let child = meter.begin_child()?;
    let steps = child.remaining_steps() / 2;
    let bytes = child.remaining_read_bytes() / 2;
    io.reads_remaining = io.reads_remaining.min(steps);
    io.writes_remaining = io.writes_remaining.min(steps);
    io.read_bytes_remaining = io.read_bytes_remaining.min(bytes);
    io.write_bytes_remaining = io.write_bytes_remaining.min(bytes);
    let before = io.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        IncrementalObjectStore::open(pages, MemoryClass::Ordinary, io, cancelled)
    }));
    let spent = (|| -> Result<(u64, u64), FormalError> {
        let bad = || FormalError::Invalid("normal original namespace IO accounting".into());
        let steps = before
            .reads_remaining
            .checked_sub(io.reads_remaining)
            .and_then(|n| n.checked_add(before.writes_remaining.checked_sub(io.writes_remaining)?))
            .ok_or_else(bad)?;
        let bytes = before
            .read_bytes_remaining
            .checked_sub(io.read_bytes_remaining)
            .and_then(|n| {
                n.checked_add(
                    before
                        .write_bytes_remaining
                        .checked_sub(io.write_bytes_remaining)?,
                )
            })
            .ok_or_else(bad)?;
        child.consume(steps, bytes)?;
        Ok((steps, bytes))
    })();
    if spent.is_ok() && result.is_ok() {
        child.finish();
        io.reads_remaining =
            original.reads_remaining - (before.reads_remaining - io.reads_remaining);
        io.writes_remaining =
            original.writes_remaining - (before.writes_remaining - io.writes_remaining);
        io.read_bytes_remaining =
            original.read_bytes_remaining - (before.read_bytes_remaining - io.read_bytes_remaining);
        io.write_bytes_remaining = original.write_bytes_remaining
            - (before.write_bytes_remaining - io.write_bytes_remaining);
    } else {
        *io = exhausted_pages();
        drop(child);
    }
    match result {
        Err(payload) => std::panic::resume_unwind(payload),
        Ok(Err(error)) => Err(io_error(error)),
        Ok(Ok(value)) => {
            spent?;
            common.consume_steps(0)?;
            Ok(value)
        }
    }
}

impl IwsHostRuntimeV1 {
    /// Private to the fixed Host consumer. Neither the sandbox nor arbitrary
    /// roots/IO handles are made available through the public Host API.
    pub(super) fn with_normal_parent_io_worker<T>(
        &self,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        input: &crate::iws_normal_input::PaidVerifiedNormalTickInputV1<'_>,
        input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
        io: &mut HostNormalIoWorkV1,
        cancelled: &AtomicBool,
        consume: impl FnOnce(
            &ReservedValue<BoundNormalTickParentV1<'_>>,
            &HostAcceptedParentReadViewV1<'_>,
            &crate::iws_normal_input::PaidVerifiedNormalTickInputV1<'_>,
            &crate::iws_normal_input_plan::PaidNormalInputPlanV1<'_, '_>,
            &StatePageSession<'_>,
            &HostNormalValuesV1<'_, '_>,
            &HostNormalOriginalsV1<'_, '_, '_>,
            &mut BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
            &mut FormalBoundedContextV1<'_>,
        ) -> Result<T, IwsHostRuntimeErrorV1>,
    ) -> Result<T, IwsHostRuntimeErrorV1> {
        self.with_normal_parent_worker(
            input,
            memory,
            archive,
            cancelled,
            |bound, parent, worker| {
                input
                    .frozen_sources()
                    .with_replica_parent_read_view_bounded(
                        memory,
                        archive,
                        &mut || cancelled.load(Ordering::Acquire),
                        |replica| {
                            if replica.head().verified_record_id()
                                != parent.head().verified_record_id()
                                || replica.fence() != parent.fence()
                                || replica.namespace_identities().0
                                    == parent.namespace_identities().0
                            {
                                return Err(FormalError::Invalid(
                                    "normal actual independent replica parent".into(),
                                )
                                .into());
                            }
                            let mut stop = || cancelled.load(Ordering::Acquire);
                            let mut common = archive.bounded_context(memory, &mut stop);
                            input.validate_context(&mut common)?;
                            bound.validate_context(&mut common)?;
                            let world_bytes = u64::try_from(input.data().context().world_id.len())
                                .map_err(|_| {
                                    FormalError::Invalid("normal input world length".into())
                                })?;
                            common.charge(
                                1,
                                world_bytes.checked_add(32).ok_or_else(|| {
                                    FormalError::Invalid("normal input binding work".into())
                                })?,
                            )?;
                            // Keep both real Frozen holders and original Node provenance live
                            // across every physical session and worker invocation. Reject an
                            // input from a different accepted parent before provisional IO.
                            if input.data().checked().parent_record_id()
                                != parent.core().candidate_record_id()
                                || input.data().context().world_id != parent.roots().world_id
                            {
                                return Err(FormalError::Invalid(
                                    "normal Host parent/input mismatch".into(),
                                )
                                .into());
                            }
                            // Complete the original reserved-first input classification before
                            // opening any provisional physical writer. All typed Runtime and
                            // side graphs keep this real Frozen/Node input borrowed throughout
                            // the fixed consumer; codec refusal is never an empty input block.
                            let plan =
                                match crate::iws_normal_input_plan::prepare_normal_input_plan_v1(
                                    input,
                                    input_limits,
                                    &mut common,
                                )
                                .map_err(IwsHostRuntimeErrorV1::InputDecode)?
                                {
                                    Ok(plan) => plan,
                                    Err(refusal) => {
                                        refusal
                                            .validate_context(&mut common)
                                            .map_err(IwsHostRuntimeErrorV1::InputDecode)?;
                                        let note = refusal.message();
                                        common.charge(1, note.len() as u64)?;
                                        let bytes = note
                                            .len()
                                            .checked_add(std::mem::size_of::<String>())
                                            .ok_or_else(|| {
                                                FormalError::Invalid(
                                                    "normal input refusal layout".into(),
                                                )
                                            })?;
                                        let fee =
                                            memory.reserve(bytes, MemoryClass::Ordinary).map_err(
                                                |e| IwsHostRuntimeErrorV1::InputDecode(e.into()),
                                            )?;
                                        let mut copy = String::new();
                                        copy.try_reserve_exact(note.len()).map_err(|_| {
                                            FormalError::Invalid(
                                                "normal input refusal allocation".into(),
                                            )
                                        })?;
                                        if copy.capacity() > note.len() {
                                            return Err(FormalError::Invalid(
                                                "normal input refusal capacity".into(),
                                            )
                                            .into());
                                        }
                                        copy.push_str(note);
                                        return Err(IwsHostRuntimeErrorV1::InputRefusal(
                                            fee.retain(copy),
                                        ));
                                    }
                                };
                            plan.validate_context(&mut common)
                                .map_err(IwsHostRuntimeErrorV1::InputDecode)?;
                            // This is an explicit writable capability opened on these exact
                            // accepted pages, never a read-only session upgraded by a cast.
                            let objects = open_originals_metered(
                                parent.pages(),
                                &mut io.original_pages,
                                cancelled,
                                &mut common,
                            )?;
                            if !objects.uses_budget(memory) {
                                return Err(FormalError::Invalid(
                                    "normal original same memory pool".into(),
                                )
                                .into());
                            }
                            let replica_objects = open_originals_metered(
                                replica.pages(),
                                &mut io.replica_original_pages,
                                cancelled,
                                &mut common,
                            )?;
                            if !replica_objects.uses_budget(memory) {
                                return Err(FormalError::Invalid(
                                    "normal replica original memory pool".into(),
                                )
                                .into());
                            }
                            // Constructor only checks cancellation and reserves its header;
                            // no physical read/write is possible before the session exists.
                            // Park zero before moving the actual allowance. The scalar snapshot
                            // is used solely to restore that untouched loan on constructor Err.
                            let state_loan =
                                std::mem::replace(&mut io.state_pages, exhausted_pages());
                            let state_before_constructor = state_loan.clone();
                            let state = match parent.pages().state_session_with_cancelled(
                                state_loan,
                                MemoryClass::Ordinary,
                                cancelled,
                            ) {
                                Ok(state) => state,
                                Err(error) => {
                                    io.state_pages = state_before_constructor;
                                    return Err(io_error(error));
                                }
                            };
                            let mut values = parent.pages().value_session_with_cancelled(
                                std::mem::replace(&mut io.value_pages, exhausted_pages()),
                                cancelled,
                            );
                            let mut replica_values = replica.pages().value_session_with_cancelled(
                                std::mem::replace(&mut io.replica_value_pages, exhausted_pages()),
                                cancelled,
                            );
                            let replica_originals_io = ObjectIo {
                                physical: &mut io.replica_original_pages,
                                logical: &mut io.replica_original_values,
                                class: MemoryClass::Ordinary,
                                cancelled,
                            };
                            let originals_io = ObjectIo {
                                physical: &mut io.original_pages,
                                logical: &mut io.original_values,
                                class: MemoryClass::Ordinary,
                                cancelled,
                            };
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    let originals = HostNormalOriginalsV1::new_pair(
                                        &objects,
                                        originals_io,
                                        &replica_objects,
                                        replica_originals_io,
                                        memory,
                                    )
                                    .map_err(io_error)?;
                                    let value_views = HostNormalValuesV1::new_pair(
                                        &mut values,
                                        &mut replica_values,
                                        memory,
                                    )
                                    .map_err(IwsHostRuntimeErrorV1::Value)?;
                                    consume(
                                        bound,
                                        parent,
                                        input,
                                        &plan,
                                        &state,
                                        &value_views,
                                        &originals,
                                        worker,
                                        &mut common,
                                    )
                                }));
                            // Reconcile even on Err/panic. A poisoned physical session cannot
                            // return a speculative allowance: its parked zero stays exhausted.
                            let state_error = match state.remaining_io() {
                                Ok(left) => {
                                    io.state_pages = left;
                                    None
                                }
                                Err(error) => Some(error),
                            };
                            let values_error = match values.remaining_io() {
                                Ok(left) => {
                                    io.value_pages = left;
                                    None
                                }
                                Err(error) => Some(error),
                            };
                            let replica_values_error = match replica_values.remaining_io() {
                                Ok(left) => {
                                    io.replica_value_pages = left;
                                    None
                                }
                                Err(error) => Some(error),
                            };
                            match result {
                                Err(payload) => std::panic::resume_unwind(payload),
                                Ok(Err(error)) => Err(error),
                                Ok(Ok(value)) => {
                                    if let Some(error) = state_error {
                                        return Err(io_error(error));
                                    }
                                    if let Some(error) = values_error.or(replica_values_error) {
                                        return Err(io_error(error));
                                    }
                                    archive.consume_steps(1, &mut || {
                                        cancelled.load(Ordering::Acquire)
                                    })?;
                                    Ok(value)
                                }
                            }
                        },
                    )
            },
        )
    }
}

impl IwsHostRuntimeV1 {
    /// Fixed package persistence borrows the same genuine dual namespace IO.
    /// No worker, raw sandbox, refill or caller supplied namespace escapes.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::iws_host_runtime) fn with_normal_package_originals_bounded<T>(
        &self,
        input: &crate::iws_normal_input::PaidVerifiedNormalTickInputV1<'_>,
        prepared: &oasis7_runtime::PreparedNormalHostRuntimeV1,
        input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
        physical: &mut HostNormalIoWorkV1,
        archive: &FormalArchiveReadWorkV1,
        memory: &MemoryBudget,
        cancelled: &AtomicBool,
        consume: impl FnOnce(
            &HostNormalOriginalsV1<'_, '_, '_>,
            &mut FormalBoundedContextV1<'_>,
        ) -> Result<T, IwsHostRuntimeErrorV1>,
    ) -> Result<T, IwsHostRuntimeErrorV1> {
        self.with_normal_parent_io_worker(
            memory,
            archive,
            input,
            input_limits,
            physical,
            cancelled,
            |_, parent, input, _, _, _, originals, _, common| {
                input.validate_context(common)?;
                prepared
                    .validate_input_source_bounded_v1(input.data(), common)
                    .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
                prepared
                    .candidate()
                    .validate_live_fence(&parent.fence())
                    .map_err(IwsHostRuntimeErrorV1::Move)?;
                consume(originals, common)
            },
        )
    }
}
