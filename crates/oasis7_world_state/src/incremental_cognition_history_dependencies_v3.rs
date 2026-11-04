//! Authenticated direct edges only, never a logical/physical GC receipt.
use super::*;
use oasis7_formal_protocol::{
    incremental_material::FactLocationV1, incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::cell::RefCell;
pub enum CognitionHistoryDirectDependencyV3<'a> {
    Existing(CognitionHistoryDirectDependencyV2<'a>),
    LastOriginal {
        field: CognitionHistoryFieldV1,
        location: FactLocationV1,
        bytes: u64,
    },
    PreviousSelectedResult(FactLocationV1),
}
pub fn visit_cognition_history_direct_dependencies_v3<E>(
    selected: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    memory: &MemoryBudget,
    edges_remaining: &mut u64,
    mut visitor: impl FnMut(CognitionHistoryDirectDependencyV3<'_>) -> Result<(), E>,
    cancelled: impl FnMut() -> bool,
) -> Result<(), CognitionHistoryDependencyVisitErrorV2<E>> {
    use CognitionHistoryDependencyVisitErrorV2::*;
    let cancelled = RefCell::new(cancelled);
    if (cancelled.borrow_mut())() {
        return Err(Value(ValueTreeError::Cancelled));
    }
    if !selected.uses_budget(memory) || selected.reserved_bytes() < 16 << 10 {
        return Err(Value(ValueTreeError::Invalid("v3 selected dependencies")));
    }
    let summary = selected
        .source_v3()
        .ok_or(Value(ValueTreeError::Invalid("v3 source profile missing")))?;
    if !summary.uses_budget(memory)
        || summary.reserved_bytes() < COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES
    {
        return Err(Value(ValueTreeError::Invalid("v3 summary carrier")));
    }
    summary
        .validate_shape(
            selected.manifest(),
            selected
                .derived()
                .ok_or(Value(ValueTreeError::Invalid("v3 dependencies refs")))?,
        )
        .map_err(Value)?;
    visit_cognition_history_direct_dependencies_v2(
        selected,
        memory,
        edges_remaining,
        |edge| visitor(CognitionHistoryDirectDependencyV3::Existing(edge)),
        || (cancelled.borrow_mut())(),
    )?;
    let mut visit = |edge| {
        if (cancelled.borrow_mut())() {
            return Err(Value(ValueTreeError::Cancelled));
        }
        *edges_remaining =
            edges_remaining
                .checked_sub(1)
                .ok_or(Value(ValueTreeError::ResourceLimited(
                    "v3 dependency edges",
                )))?;
        visitor(edge).map_err(Visitor)
    };
    for (slot, field) in [
        CognitionHistoryFieldV1::JournalEvents,
        CognitionHistoryFieldV1::EconomyJournal,
        CognitionHistoryFieldV1::EconomyProvisionJournal,
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(last) = &summary.last[slot] {
            // Keep duplicate references: the consumer must verify field kind,
            // exact flat ID/length and the typed last-index relation as well.
            visit(CognitionHistoryDirectDependencyV3::LastOriginal {
                field,
                location: last.location,
                bytes: last.original_bytes,
            })?;
        }
    }
    if let Some(previous) = selected.manifest().previous_view {
        visit(CognitionHistoryDirectDependencyV3::PreviousSelectedResult(
            previous,
        ))?;
    }
    if (cancelled.borrow_mut())() {
        return Err(Value(ValueTreeError::Cancelled));
    }
    Ok(())
}
