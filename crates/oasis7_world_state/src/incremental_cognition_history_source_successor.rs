//! Ordinary source inheritance follows the actual accepted predecessor bridge.
use super::*;
use crate::{incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFactKindV1},
    incremental_material::{CoreError, VerifiedFactHistoryExtensionV1, inherit_fact_occurrence_v1},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};

pub struct CognitionHeadSourceSuccessorV1<'a> {
    stream: AuthenticatedHeadStreamV1,
    parent_index: &'a ReservedValue<CognitionHistoryIndexQualificationV1>,
    after: &'a ReservedValue<AcceptedCognitionHistoryViewV1>,
    replay: Option<ReservedValue<CognitionHistoryIndexQualificationV1>>,
    failed: bool,
}
impl<'a> CognitionHeadSourceSuccessorV1<'a> {
    pub fn new(
        parent: &ReservedValue<AuthenticatedCognitionHeadSourceV1>,
        before: &ReservedValue<AcceptedCognitionHistoryViewV1>,
        after: &'a ReservedValue<AcceptedCognitionHistoryViewV1>,
        parent_index: &'a ReservedValue<CognitionHistoryIndexQualificationV1>,
        extension: &ReservedValue<VerifiedFactHistoryExtensionV1>,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        let invalid = || ValueTreeError::Invalid("authenticated head successor");
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if !parent.uses_budget(memory)
            || parent.reserved_bytes() < 4096
            || !parent.state.uses_budget(memory)
            || parent.state.allocation_class() != MemoryClass::Ordinary
            || !before.uses_budget(memory)
            || before.class() != MemoryClass::Ordinary
            || !after.uses_budget(memory)
            || after.class() != MemoryClass::Ordinary
            || parent.head != before.occurrence().head_record_id()
            || parent.roots != before.manifest().roots
            || parent.records != parent_index.record_count()
            || !parent_index.uses_budget(memory)
            || parent_index.class() != MemoryClass::Ordinary
            || before.manifest().revision.checked_add(1) != Some(after.manifest().revision)
        {
            return Err(invalid());
        }
        parent_index.validate_binding(parent.field, before.manifest().index_ref(parent.field))?;
        let inherited =
            inherit_fact_occurrence_v1(before.occurrence(), extension, memory, &mut cancelled)
                .map_err(|error| match error {
                    CoreError::Cancelled => ValueTreeError::Cancelled,
                    CoreError::Execution(ExecutionError::Memory(error)) => {
                        ValueTreeError::Memory(error)
                    }
                    _ => invalid(),
                })?;
        let current = after.occurrence();
        if inherited.head_record_id() != current.head_record_id()
            || inherited.roots_id() != current.roots_id()
            || inherited.head_body_id() != current.head_body_id()
            || current.fact_body_id() != current.head_body_id()
            || current.kind() != ExecutionFactKindV1::Result
        {
            return Err(invalid());
        }
        let expected = after.manifest().index_ref(parent.field).records;
        if expected < parent.records {
            return Err(invalid());
        }
        let builder = CognitionHeadHashBuilderV1::append_to(
            &parent.state,
            memory,
            MemoryClass::Ordinary,
            &mut cancelled,
        )?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(Self {
            stream: AuthenticatedHeadStreamV1 {
                builder,
                failed: false,
                next: parent.records,
                field: parent.field,
                head: current.head_record_id(),
                roots: after.manifest().roots,
                expected,
                last: parent.last,
                last_record: parent.last_record.clone(),
            },
            parent_index,
            after,
            replay: None,
            failed: false,
        })
    }
    pub fn push<M, R, E, C, W, CW, I: Read>(
        &mut self,
        original: &ReservedValue<IndexedCognitionOriginalV1>,
        input: &mut I,
        read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        indexes: &mut CognitionHistoryIndexWorkV1,
        cbor: &mut CognitionHistoryCborWorkV1,
        hashes: &mut CognitionHeadHashWorkV1,
    ) -> Result<(), MemberRecordError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter,
        CW: FnMut() -> bool,
    {
        if self.failed {
            return Err(MemberRecordError::Invalid("poisoned head successor"));
        }
        self.failed = true;
        read.check()?;
        if original.row().location.height != self.after.occurrence().location().height {
            return Err(MemberRecordError::Invalid(
                "head successor current occurrence",
            ));
        }
        self.stream.push(
            original,
            self.after,
            input,
            read.values.memory,
            cbor,
            hashes,
            || (read.cancelled)() || (write.cancelled)(),
        )?;
        let next = prepare_cognition_history_index_append_v1(
            self.replay.as_ref().unwrap_or(self.parent_index),
            original.row(),
            read,
            write,
            indexes,
        )?;
        self.replay = Some(next);
        read.check()?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        self.failed = false;
        Ok(())
    }
    pub fn finish(
        self,
        memory: &MemoryBudget,
        hashes: &mut CognitionHeadHashWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<AuthenticatedCognitionHeadSourceV1>, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if self.failed {
            return Err(ValueTreeError::Invalid("poisoned head successor"));
        }
        self.replay
            .as_ref()
            .unwrap_or(self.parent_index)
            .validate_binding(
                self.stream.field,
                self.after.manifest().index_ref(self.stream.field),
            )?;
        self.stream.finish(self.after, memory, hashes, cancelled)
    }
}
