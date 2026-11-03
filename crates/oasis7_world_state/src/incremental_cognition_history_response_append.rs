//! Provisional ordinary response-original and locator contribution. Acceptance
//! still requires the actual candidate/receiver and response semantic gates.
use super::*;
use crate::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_material::FactLocationV1,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

pub struct CognitionResponseAppendRequestV1<'a> {
    pub record: CognitionHistoryRecordRefV1<'a>,
    pub parent_view: &'a ReservedValue<AcceptedCognitionHistoryViewV1>,
    pub parent_current: &'a ReservedValue<CognitionHistoryCurrentBindingV1>,
    pub parent_index: &'a ReservedValue<CognitionHistoryIndexQualificationV1>,
    pub guard: &'a PreparedGuard,
    pub live: &'a ExecutionFence,
}
pub fn contribute_cognition_response_append_v1<K, M, R, E, C, W, D, O, S>(
    request: CognitionResponseAppendRequestV1<'_>,
    transaction: &mut ExecutionTransaction<'_, K>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    originals: &mut CognitionHistoryWriteContextV1<'_, O>,
    sink: &mut S,
    work: &mut CognitionHistoryIndexWorkV1,
) -> Result<ReservedValue<CognitionHistoryIndexQualificationV1>, WorldFieldOverlayErrorV1>
where
    K: MerkleNodeReader,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
    O: FnMut() -> bool,
    S: CognitionHistoryOriginalSinkV1,
{
    transaction.contribute(|tx| {
        tx.validate_guard(request.guard, request.live)?;
        read.check()?;
        let memory = read.values.memory;
        let fence = tx.parent_fence();
        if read.values.class != MemoryClass::Ordinary
            || request.parent_view.class() != MemoryClass::Ordinary
            || request.parent_current.class() != MemoryClass::Ordinary
            || request.parent_index.class() != MemoryClass::Ordinary
            || !tx.uses_budget(memory)
            || !write.memory.same_pool(memory)
            || !originals.memory.same_pool(memory)
            || !request.parent_view.uses_budget(memory)
            || !request.parent_current.uses_budget(memory)
            || !request.parent_index.uses_budget(memory)
            || request.record.field() != CognitionHistoryFieldV1::Responses
            || request.record.ordinal() != request.parent_index.record_count()
            || !request.parent_current.enabled()
            || request.parent_current.roots_id() != fence.parent_roots
            || request.parent_current.head_record_id()
                != request.parent_view.occurrence().head_record_id()
            || request.parent_view.occurrence().head_body_id() != fence.parent_commit
            || request.parent_view.manifest().roots != fence.parent_roots
        {
            return Err(ValueTreeError::Invalid("Cognition response append parent").into());
        }
        request.parent_index.validate_binding(
            CognitionHistoryFieldV1::Responses,
            request
                .parent_view
                .manifest()
                .index_ref(CognitionHistoryFieldV1::Responses),
        )?;
        let height = request
            .parent_view
            .occurrence()
            .location()
            .height
            .checked_add(1)
            .ok_or(ValueTreeError::Invalid("Cognition response append height"))?;
        let ordinal = u64::try_from(tx.ordered_fact_count())
            .map_err(|_| ValueTreeError::Invalid("Cognition response append fact ordinal"))?;
        let original = contribute_cognition_history_original_v1(
            request.record,
            tx,
            originals,
            sink,
            request.guard,
            request.live,
        )?;
        let index = prepare_cognition_history_index_append_v1(
            request.parent_index,
            CognitionHistoryIndexRowV1 {
                location: FactLocationV1 {
                    height,
                    ordinal,
                    original: *original.id(),
                },
                encoded_bytes: original.encoded_bytes(),
            },
            read,
            write,
            work,
        )?;
        if (originals.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        read.check()?;
        Ok(index)
    })
}
