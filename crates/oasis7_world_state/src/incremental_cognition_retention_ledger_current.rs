//! The unique mutable side-ledger for one actual transaction. Public path
//! arguments are storage DATA; Runtime's private original-plan bridge alone
//! performs terminal/GC/pin business admission.
use super::*;
pub struct CurrentCognitionRetentionLedgerV1 {
    payload: Option<ReservedValue<JsonSubtreeQualificationV1>>,
    parent_record: [u8; 32],
    parent_body: [u8; 32],
    parent_roots: [u8; 32],
    guard: PreparedGuard,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
pub struct FrozenCognitionRetentionLedgerV1 {
    source: CurrentCognitionRetentionLedgerV1,
    guard: PreparedGuard,
}
impl CurrentCognitionRetentionLedgerV1 {
    pub fn validate_context<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        common.consume_steps(1).map_err(formal)?;
        tx.validate_exact_transaction_identity(&self.guard)
            .map_err(|_| invalid())?;
        tx.validate_guard(&tx.guard(), live)
            .map_err(|_| invalid())?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
            || tx.parent_fence().parent_commit != self.parent_body
            || tx.parent_fence().parent_roots != self.parent_roots
            || self
                .payload
                .as_ref()
                .is_some_and(|q| !q.uses_budget(common.memory()))
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn payload_root_data_v1(&self) -> Option<&StateRootV1> {
        self.payload.as_ref().map(|q| q.root())
    }
    pub fn freeze<M: MerkleNodeReader>(
        self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<FrozenCognitionRetentionLedgerV1, ValueTreeError> {
        self.validate_context(tx, common, live)?;
        Ok(FrozenCognitionRetentionLedgerV1 {
            source: self,
            guard: tx.guard(),
        })
    }
}
pub fn begin_cognition_retention_current_v1<M: MerkleNodeReader>(
    parent: &ReservedValue<AcceptedCognitionRetentionLedgerV1>,
    head: &VerifiedFormalCommitV1,
    tx: &mut ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentCognitionRetentionLedgerV1, ValueTreeError> {
    let result = (|| {
        common
            .charge(
                3,
                (size_of::<CurrentCognitionRetentionLedgerV1>() + 512) as u64,
            )
            .map_err(formal)?;
        parent.check(common.memory(), MemoryClass::Ordinary)?;
        if !parent.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
            || parent.record != head.verified_record_id()
            || parent.body != tx.parent_fence().parent_commit
            || parent.manifest.roots != tx.parent_fence().parent_roots
        {
            return Err(invalid());
        }
        tx.validate_guard(&tx.guard(), live)
            .map_err(|_| invalid())?;
        let memory = common.memory().clone();
        let fee = memory.reserve(
            size_of::<CurrentCognitionRetentionLedgerV1>(),
            MemoryClass::Ordinary,
        )?;
        let (_, _, stop) = common.parts();
        let payload = parent
            .payload
            .as_ref()
            .map(|q| copy_json_subtree_metadata_ordinary_v1(q, profile(), &memory, &mut *stop))
            .transpose()?;
        if stop() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(CurrentCognitionRetentionLedgerV1 {
            payload,
            parent_record: parent.record,
            parent_body: parent.body,
            parent_roots: parent.manifest.roots,
            guard: tx.guard(),
            core: common.work().identity(),
            fee,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
/// Existing retention mutates only the actual selected path. Original plans
/// choose those paths; no complete JSON rebuild is performed here.
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn edit_current_cognition_retention_ledger_v1<M, P, R, E, C, W, D>(
    current: &mut CurrentCognitionRetentionLedgerV1,
    path: &[JsonPathSegmentV1<'_>],
    edit: JsonSubtreeEditV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(), WorldFieldOverlayErrorV1> {
            current.validate_context(tx, common, live)?;
            if !read.values.memory.same_pool(common.memory())
                || !write.memory.same_pool(common.memory())
                || read.values.class != MemoryClass::Ordinary
            {
                return Err(invalid().into());
            }
            let old = current.payload.as_ref().ok_or_else(invalid)?;
            let next = io::run(
                read,
                Some(write),
                work,
                common,
                |read, write, work, stop| {
                    let write = write.ok_or_else(invalid)?;
                    let stop = std::cell::RefCell::new(stop);
                    let read_stop = &mut read.cancelled;
                    let write_stop = &mut write.cancelled;
                    let mut source = WorldFieldReadContextV1 {
                        root: old.root(),
                        state: read.state,
                        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                            source: read.values.source,
                            memory: read.values.memory,
                            class: read.values.class,
                            io: &mut *read.values.io,
                        },
                        key_work: &mut *read.key_work,
                        semantic_bytes: read.semantic_bytes,
                        record_bytes: read.record_bytes,
                        take_state_error: &mut read.take_state_error,
                        cancelled: || read_stop() || (stop.borrow_mut())(),
                    };
                    let mut output = MemberRecordWriteContext {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        key_work: &mut *write.key_work,
                        limits: write.limits,
                        cancelled: || write_stop() || (stop.borrow_mut())(),
                    };
                    let change = prepare_json_subtree_change_v1(
                        profile(),
                        old,
                        path,
                        edit,
                        &mut source,
                        &mut output,
                        work.producer,
                    )
                    .map_err(member)?;
                    let next = advance_json_subtree_v1(
                        profile(),
                        old,
                        change.after(),
                        &mut source,
                        work.changed,
                    )?;
                    source.check()?;
                    Ok(next)
                },
            )?;
            current.payload = Some(next);
            Ok(())
        },
    ));
    match result {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
/// Explicit optional-field CAS, limited to first creation or removal. Original
/// pin/GC/marker plans use the path writer for all changes to existing payloads.
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn change_current_cognition_retention_presence_v1<M, P, R, E, C, W, D>(
    current: &mut CurrentCognitionRetentionLedgerV1,
    expected: Option<&StateRootV1>,
    after: Option<&serde_json::Value>,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(), WorldFieldOverlayErrorV1> {
            current.validate_context(tx, common, live)?;
            if current.payload_root_data_v1() != expected
                || expected.is_some() && after.is_some()
                || !read.values.memory.same_pool(common.memory())
                || !write.memory.same_pool(common.memory())
                || read.values.class != MemoryClass::Ordinary
            {
                return Err(invalid().into());
            }
            let Some(value) = after else {
                current.payload = None;
                return Ok(());
            };
            let q = io::run(
                read,
                Some(write),
                work,
                common,
                |read, write, work, stop| {
                    let write = write.ok_or_else(invalid)?;
                    let stop = std::cell::RefCell::new(stop);
                    let read_stop = &mut read.cancelled;
                    let write_stop = &mut write.cancelled;
                    let mut output = MemberRecordWriteContext {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        key_work: &mut *write.key_work,
                        limits: write.limits,
                        cancelled: || write_stop() || (stop.borrow_mut())(),
                    };
                    let root =
                        prepare_json_subtree_v1(profile(), value, &mut output, work.producer)
                            .map_err(member)?;
                    let mut source = WorldFieldReadContextV1 {
                        root: &root,
                        state: read.state,
                        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                            source: read.values.source,
                            memory: read.values.memory,
                            class: read.values.class,
                            io: &mut *read.values.io,
                        },
                        key_work: &mut *read.key_work,
                        semantic_bytes: read.semantic_bytes,
                        record_bytes: read.record_bytes,
                        take_state_error: &mut read.take_state_error,
                        cancelled: || read_stop() || (stop.borrow_mut())(),
                    };
                    qualify_json_subtree_v1(profile(), &root, &mut source, work.json)
                },
            )?;
            current.payload = Some(q);
            Ok(())
        },
    ));
    match result {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
impl FrozenCognitionRetentionLedgerV1 {
    pub fn payload_root_data_v1(&self) -> Option<&StateRootV1> {
        self.source.payload.as_ref().map(|q| q.root())
    }
    pub fn parent_record_data_v1(&self) -> [u8; 32] {
        self.source.parent_record
    }
    pub fn validate_read_context_v1<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        tx.validate_read_source_guard(&self.guard, live)
            .map_err(|_| invalid())?;
        if !self.source.fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.source.parent_body != tx.parent_fence().parent_commit
            || self.source.parent_roots != tx.parent_fence().parent_roots
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn validate_context<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        self.source.validate_context(tx, common, live)?;
        tx.validate_read_source_guard(&self.guard, live)
            .map_err(|_| invalid())
    }
    /// After all permitted extra reads, take the actual strict terminal cut. The
    /// old source cut must prove no writes/facts/branch resolves occurred since.
    pub fn bind_terminal_read_cut_v1<M: MerkleNodeReader>(
        mut self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, ValueTreeError> {
        self.validate_context(tx, common, live)?;
        self.guard = tx.guard();
        Ok(self)
    }
    pub fn validate_sealed(
        &self,
        sealed: &SealedExecutionBindingV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), ValueTreeError> {
        common.consume_steps(1).map_err(formal)?;
        if self.source.core != common.work().identity()
            || !self.source.fee.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        sealed
            .validate_source_guard(&self.guard, common.memory())
            .map_err(|_| invalid())
    }
    pub fn manifest(
        &self,
        after: &RootSetV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<RetentionLedgerManifestV1>, ValueTreeError> {
        common.charge(8192, 8192).map_err(formal)?;
        if self.source.core != common.work().identity()
            || !self.source.fee.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        let fee = common.memory().reserve(
            size_of::<RetentionLedgerManifestV1>() + 8192,
            MemoryClass::Ordinary,
        )?;
        after.validate().map_err(|_| invalid())?;
        Ok(fee.retain(RetentionLedgerManifestV1 {
            roots: after.digest().map_err(|_| invalid())?,
            previous_record: Some(self.source.parent_record),
            payload: self.source.payload.as_ref().map(|q| q.root().clone()),
        }))
    }
}

/// Actual selected value/root DATA for the fixed original-plan dispatcher.
/// Missing means a proved missing path in this Current graph, never missing IO.
pub struct PaidCurrentCognitionRetentionSelectionV1 {
    value: Option<ReservedValue<serde_json::Value>>,
    root: Option<StateRootV1>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
impl PaidCurrentCognitionRetentionSelectionV1 {
    pub fn value(&self) -> Option<&serde_json::Value> {
        self.value.as_deref()
    }
    pub fn expected_root(&self) -> Option<&StateRootV1> {
        self.root.as_ref()
    }
    pub fn validate_context<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        common.consume_steps(1).map_err(formal)?;
        tx.validate_read_source_guard(&self.guard, live)
            .map_err(|_| invalid())?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || self
                .value
                .as_ref()
                .is_some_and(|v| !v.uses_budget(common.memory()))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub fn read_current_cognition_retention_selection_v1<M, P, R, E, C>(
    current: &CurrentCognitionRetentionLedgerV1,
    path: &[JsonPathSegmentV1<'_>],
    tx: &ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCurrentCognitionRetentionSelectionV1, ValueTreeError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    current.validate_context(tx, common, live)?;
    if read.values.class != MemoryClass::Ordinary || !read.values.memory.same_pool(common.memory())
    {
        return Err(invalid());
    }
    common
        .charge(
            1,
            (size_of::<PaidCurrentCognitionRetentionSelectionV1>() + 512) as u64,
        )
        .map_err(formal)?;
    let fee = common.memory().reserve(
        size_of::<PaidCurrentCognitionRetentionSelectionV1>() + 512,
        MemoryClass::Ordinary,
    )?;
    let selected = match current.payload.as_ref() {
        None => None,
        Some(parent) => io::run(
            read,
            None::<&mut MemberRecordWriteContext<'_, R, C>>,
            work,
            common,
            |read, _, work, stop| {
                let caller_stop = &mut read.cancelled;
                let mut source = WorldFieldReadContextV1 {
                    root: parent.root(),
                    state: read.state,
                    values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                        source: read.values.source,
                        memory: read.values.memory,
                        class: read.values.class,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: || caller_stop() || stop(),
                };
                let q = crate::incremental_json_records::qualify_optional_json_path_records_v1(
                    profile(),
                    parent,
                    path,
                    &mut source.records(),
                    work.json,
                )
                .map_err(member)?;
                let Some(q) = q else {
                    source.check()?;
                    return Ok(None);
                };
                let limit = usize::try_from(work.json.max_payload_bytes).map_err(|_| {
                    ValueTreeError::ResourceLimited("retention selected heap bound")
                })?;
                let (value, fee) = crate::incremental_json_records::materialize_records(
                    profile(),
                    &q,
                    &mut source.records(),
                    work.json,
                    limit,
                )?;
                source.check()?;
                Ok(Some((q.root().clone(), fee.retain(value))))
            },
        )?,
    };
    let (root, value) = match selected {
        None => (None, None),
        Some((root, value)) => (Some(root), Some(value)),
    };
    Ok(PaidCurrentCognitionRetentionSelectionV1 {
        value,
        root,
        guard: tx.guard(),
        core: common.work().identity(),
        fee,
    })
}
