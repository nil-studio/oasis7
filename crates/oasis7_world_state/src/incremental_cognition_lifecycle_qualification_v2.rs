//! Per-key lifecycle admission only. Cache/head correspondence and complete
//! History readiness are separate qualifications.
use crate::{
    incremental_cognition_history::*, incremental_cognition_lifecycle_index_v2::*,
    incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFactKindV1},
    incremental_material::{CoreError, VerifiedFactHistoryExtensionV1, inherit_fact_occurrence_v1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[path = "incremental_cognition_lifecycle_candidate_v3.rs"]
mod candidate_v3;
pub use candidate_v3::*;
#[path = "incremental_cognition_lifecycle_attested_v3.rs"]
mod attested_v3;
pub use attested_v3::*;
pub struct CognitionLifecycleQualificationV2 {
    root: StateRootV1,
    keys: u64,
    journal: StateRootV1,
    records: u64,
    head: [u8; 32],
    roots: [u8; 32],
    class: MemoryClass,
}
impl CognitionLifecycleQualificationV2 {
    pub(crate) fn class(&self) -> MemoryClass {
        self.class
    }
    pub(crate) fn validate_accepted_binding(
        &self,
        view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
        current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
        journal: &ReservedValue<CognitionHistoryIndexQualificationV1>,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        binding(view, current, journal, memory)?;
        if self.class != MemoryClass::Ordinary
            || self.head != view.occurrence().head_record_id()
            || self.roots != view.manifest().roots
            || self.root != *declared(view)?
            || self.journal != *journal.root()
            || self.records != journal.record_count()
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn keys(&self) -> u64 {
        self.keys
    }
    pub fn journal_records(&self) -> u64 {
        self.records
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition lifecycle qualification")
}
fn declared(view: &AcceptedCognitionHistoryViewV1) -> Result<&StateRootV1, ValueTreeError> {
    view.derived()
        .and_then(|v| v.lifecycle.as_ref())
        .ok_or_else(invalid)?
        .validate_binding(
            &lifecycle_schema(),
            &lifecycle_owner(),
            StateNamespaceV1::Artifacts,
        )
        .map_err(|_| invalid())
}
fn binding(
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    index: &ReservedValue<CognitionHistoryIndexQualificationV1>,
    memory: &MemoryBudget,
) -> Result<(), ValueTreeError> {
    if !view.uses_budget(memory)
        || view.reserved_bytes() < 16 << 10
        || view.class() != MemoryClass::Ordinary
        || !current.uses_budget(memory)
        || current.reserved_bytes() < 1024
        || current.class() != MemoryClass::Ordinary
        || !current.enabled()
        || current.head_record_id() != view.occurrence().head_record_id()
        || current.roots_id() != view.manifest().roots
        || !index.uses_budget(memory)
        || index.reserved_bytes() < std::mem::size_of::<CognitionHistoryIndexQualificationV1>()
        || index.class() != MemoryClass::Ordinary
    {
        return Err(invalid());
    }
    index.validate_binding(
        CognitionHistoryFieldV1::JournalEvents,
        view.manifest()
            .index_ref(CognitionHistoryFieldV1::JournalEvents),
    )?;
    Ok(())
}
pub fn qualify_empty_cognition_lifecycle_v2(
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    journal: &ReservedValue<CognitionHistoryIndexQualificationV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CognitionLifecycleQualificationV2>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    binding(view, current, journal, memory)?;
    let empty = StateRootV1::empty(StateNamespaceV1::Artifacts).map_err(|_| invalid())?;
    if journal.record_count() != 0 || declared(view)? != &empty {
        return Err(invalid());
    }
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(CognitionLifecycleQualificationV2 {
        root: empty,
        keys: 0,
        journal: journal.root().clone(),
        records: 0,
        head: current.head_record_id(),
        roots: current.roots_id(),
        class: MemoryClass::Ordinary,
    }))
}
/// Independently replay both the actual Journal locator append and per-key
/// scalar transition under the exact accepted successor. All pages are CAS
/// preparation; only final root equality returns the new owner qualification.
pub struct CognitionLifecycleReceiverV2<'a> {
    pub extension: &'a ReservedValue<VerifiedFactHistoryExtensionV1>,
    pub parent: &'a ReservedValue<CognitionLifecycleQualificationV2>,
    pub before: &'a ReservedValue<AcceptedCognitionHistoryViewV1>,
    pub after: &'a ReservedValue<AcceptedCognitionHistoryViewV1>,
    pub current: &'a ReservedValue<CognitionHistoryCurrentBindingV1>,
    pub journal: &'a ReservedValue<CognitionHistoryIndexQualificationV1>,
}
/// Streaming changed-record receiver; no collection proportional to delta or H.
/// Any failed push poisons finish, even if the caller swallows the error.
pub struct CognitionLifecycleReceiverBuilderV2<'a> {
    request: CognitionLifecycleReceiverV2<'a>,
    charge: oasis7_proto::memory_budget::MemoryReservation,
    root: StateRootV1,
    keys: u64,
    expected: u64,
    seen: u64,
    index: Option<ReservedValue<CognitionHistoryIndexQualificationV1>>,
    failed: bool,
}
impl<'a> CognitionLifecycleReceiverBuilderV2<'a> {
    pub fn new(
        request: CognitionLifecycleReceiverV2<'a>,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let CognitionLifecycleReceiverV2 {
            extension,
            parent,
            before,
            after,
            current,
            journal,
        } = request;
        if !parent.uses_budget(memory)
            || parent.reserved_bytes() < 4096
            || parent.class != MemoryClass::Ordinary
            || parent.head != before.occurrence().head_record_id()
            || parent.roots != before.manifest().roots
            || parent.root != *declared(before)?
            || parent.journal != journal.root().clone()
            || parent.records != journal.record_count()
            || !journal.uses_budget(memory)
            || journal.class() != MemoryClass::Ordinary
            || !before.uses_budget(memory)
            || before.class() != MemoryClass::Ordinary
            || !after.uses_budget(memory)
            || after.class() != MemoryClass::Ordinary
            || !current.uses_budget(memory)
            || current.class() != MemoryClass::Ordinary
            || !current.enabled()
            || current.roots_id() != after.manifest().roots
            || current.head_record_id() != after.occurrence().head_record_id()
            || before.manifest().revision.checked_add(1) != Some(after.manifest().revision)
            || after.manifest().previous_view != Some(before.occurrence().location())
            || before.occurrence().location().height.checked_add(1)
                != Some(after.occurrence().location().height)
        {
            return Err(invalid());
        }
        // The public previous-view hint and adjacent heights do not prove
        // ancestry. Formal inherits this exact selected occurrence only across
        // its authenticated direct accepted extension, with fresh Ordinary
        // credit and the caller's cancellation channel.
        let inherited =
            inherit_fact_occurrence_v1(before.occurrence(), extension, memory, &mut cancelled)
                .map_err(|error| match error {
                    CoreError::Cancelled => ValueTreeError::Cancelled,
                    CoreError::Execution(ExecutionError::Memory(error)) => {
                        ValueTreeError::Memory(error)
                    }
                    _ => invalid(),
                })?;
        let selected = after.occurrence();
        if inherited.head_record_id() != selected.head_record_id()
            || inherited.roots_id() != selected.roots_id()
            || inherited.head_body_id() != selected.head_body_id()
            || selected.kind() != ExecutionFactKindV1::Result
            || selected.fact_body_id() != selected.head_body_id()
        {
            return Err(invalid());
        }
        journal.validate_binding(
            CognitionHistoryFieldV1::JournalEvents,
            before
                .manifest()
                .index_ref(CognitionHistoryFieldV1::JournalEvents),
        )?;
        let expected = after
            .manifest()
            .index_ref(CognitionHistoryFieldV1::JournalEvents)
            .records
            .checked_sub(parent.records)
            .ok_or_else(invalid)?;
        let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
        let root = parent.root.clone();
        let keys = parent.keys;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(Self {
            request,
            charge,
            root,
            keys,
            expected,
            seen: 0,
            index: None,
            failed: false,
        })
    }
    pub fn push<M, R, E, C, W, D>(
        &mut self,
        original: &ReservedValue<IndexedCognitionOriginalV1>,
        keys: [&str; 5],
        read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, D>,
        work: &mut CognitionLifecycleWorkV2,
        index_work: &mut CognitionHistoryIndexWorkV1,
    ) -> Result<(), MemberRecordError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter + ValueNodeReader,
        D: FnMut() -> bool,
    {
        if self.failed {
            return Err(invalid().into());
        }
        self.failed = true;
        read.check()?;
        let memory = read.values.memory;
        let after = self.request.after;
        if !self.request.parent.uses_budget(memory)
            || self.seen >= self.expected
            || !original.uses_budget(memory)
            || original.reserved_bytes() < 2048
            || original.schema().semantic().class() != MemoryClass::Ordinary
            || original.schema().semantic().field() != CognitionHistoryFieldV1::JournalEvents
            || original.ordinal()
                != self
                    .request
                    .parent
                    .records
                    .checked_add(self.seen)
                    .ok_or_else(invalid)?
            || original.head_record_id() != &after.occurrence().head_record_id()
            || original.head_body_id() != &after.occurrence().head_body_id()
            || original.roots_id() != &after.manifest().roots
            || original.row().location.height != after.occurrence().location().height
        {
            return Err(invalid().into());
        }
        work.cells_remaining = work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("lifecycle append records"))?;
        let index = prepare_cognition_history_index_append_v1(
            self.index.as_ref().unwrap_or(self.request.journal),
            original.row(),
            read,
            write,
            index_work,
        )?;
        let prepared = prepare_cognition_lifecycle_append_v2(
            &self.root,
            self.keys,
            CognitionLifecycleAppendV2 {
                schema: original.schema_carrier(),
                keys,
                ordinal: original.ordinal(),
                location: original.row().location,
            },
            read,
            write,
            work,
        )?;
        self.root = prepared.root;
        self.keys = prepared.keys;
        self.index = Some(index);
        self.seen += 1;
        if (read.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        self.failed = false;
        Ok(())
    }
    pub fn finish<M, R, E, C, W, D>(
        mut self,
        read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, D>,
    ) -> Result<ReservedValue<CognitionLifecycleQualificationV2>, MemberRecordError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter + ValueNodeReader,
        D: FnMut() -> bool,
    {
        if self.failed || self.seen != self.expected {
            return Err(invalid().into());
        }
        read.check()?;
        let index = self.index.as_ref().unwrap_or(self.request.journal);
        binding(
            self.request.after,
            self.request.current,
            index,
            read.values.memory,
        )?;
        if &self.root != declared(self.request.after)? {
            return Err(invalid().into());
        }
        if (read.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let charge = self.charge.split(4096).map_err(ValueTreeError::Invalid)?;
        Ok(charge.retain(CognitionLifecycleQualificationV2 {
            root: self.root,
            keys: self.keys,
            journal: index.root().clone(),
            records: index.record_count(),
            head: self.request.current.head_record_id(),
            roots: self.request.current.roots_id(),
            class: MemoryClass::Ordinary,
        }))
    }
}
pub fn accept_cognition_lifecycle_appends_v2<'a, M, R, E, C, W, D, I>(
    request: CognitionLifecycleReceiverV2<'_>,
    originals: I,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    work: &mut CognitionLifecycleWorkV2,
    index_work: &mut CognitionHistoryIndexWorkV1,
) -> Result<ReservedValue<CognitionLifecycleQualificationV2>, MemberRecordError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
    I: IntoIterator<Item = (&'a ReservedValue<IndexedCognitionOriginalV1>, [&'a str; 5])>,
{
    read.check()?;
    let mut receiver =
        CognitionLifecycleReceiverBuilderV2::new(request, read.values.memory, || {
            (read.cancelled)() || (write.cancelled)()
        })?;
    for (original, keys) in originals {
        receiver.push(original, keys, read, write, work, index_work)?;
    }
    receiver.finish(read, write)
}
