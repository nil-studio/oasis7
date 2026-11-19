//! Exact accepted component conjunction only; original semantic replay and
//! Runtime History families remain separate and this function issues no token.
use crate::incremental_cognition_history::*;
use crate::incremental_cognition_lifecycle_qualification_v2::CognitionLifecycleQualificationV2;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub fn validate_accepted_cognition_history_components_v2(
    selected: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    indexes: [Option<&ReservedValue<CognitionHistoryIndexQualificationV1>>; 4],
    caches: &ReservedValue<BoundPersistedCognitionHeadCachesV2>,
    lifecycle: Option<&ReservedValue<CognitionLifecycleQualificationV2>>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let invalid = || ValueTreeError::Invalid("accepted History component binding");
    if !selected.uses_budget(memory)
        || selected.class() != MemoryClass::Ordinary
        || selected.reserved_bytes() < 16 << 10
        || current.reserved_bytes() < 1024
        || caches.reserved_bytes() < 1024
        || !current.uses_budget(memory)
        || current.class() != MemoryClass::Ordinary
        || (**current).class() != MemoryClass::Ordinary
        || !caches.uses_budget(memory)
        || caches.class() != MemoryClass::Ordinary
        || selected.derived().is_none()
        || selected.occurrence().allocation_class() != MemoryClass::Ordinary
        || selected.occurrence().roots_id() != selected.manifest().roots
        || selected.occurrence().head_body_id() != selected.occurrence().fact_body_id()
        || current.roots_id() != selected.manifest().roots
        || current.head_record_id() != selected.occurrence().head_record_id()
        || caches.roots_id() != current.roots_id()
        || caches.head_record_id() != current.head_record_id()
        || current.enabled() != selected.manifest().enabled
    {
        return Err(invalid());
    }
    for (field, index) in CognitionHistoryFieldV1::ALL.into_iter().zip(indexes) {
        let declared = selected.manifest().index_ref(field);
        match (declared.present, index) {
            (true, Some(index)) => {
                if !index.uses_budget(memory)
                    || index.class() != MemoryClass::Ordinary
                    || (**index).class() != MemoryClass::Ordinary
                    || index.reserved_bytes()
                        < std::mem::size_of::<CognitionHistoryIndexQualificationV1>()
                {
                    return Err(invalid());
                }
                index.validate_binding(field, declared)?;
            }
            (false, None) => {}
            _ => return Err(invalid()),
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
    }
    match (current.enabled(), lifecycle, indexes[0]) {
        (true, Some(lifecycle), Some(journal)) => {
            if !lifecycle.uses_budget(memory)
                || lifecycle.class() != MemoryClass::Ordinary
                || lifecycle.reserved_bytes() < 4096
            {
                return Err(invalid());
            }
            lifecycle.validate_accepted_binding(selected, current, journal, memory)?;
        }
        (false, None, None) => {}
        _ => return Err(invalid()),
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
