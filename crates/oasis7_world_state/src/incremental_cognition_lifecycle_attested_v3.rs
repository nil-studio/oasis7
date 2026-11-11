//! Logical v3 lifecycle provenance, distinct from the v2 replay qualification.
use super::*;

pub struct AttestedCognitionLifecycleV3 {
    pub(super) root: StateRootV1,
    pub(super) keys: u64,
    pub(super) journal: StateRootV1,
    pub(super) records: u64,
    pub(super) head: [u8; 32],
    pub(super) roots: [u8; 32],
    pub(super) class: MemoryClass,
}
impl AttestedCognitionLifecycleV3 {
    pub(crate) fn validate_accepted(
        &self,
        view: &AcceptedCognitionHistoryViewV1,
    ) -> Result<(), ValueTreeError> {
        let summary = view.source_v3().ok_or_else(invalid)?;
        let index = view
            .manifest()
            .index_ref(CognitionHistoryFieldV1::JournalEvents);
        if self.class != view.class()
            || self.head != view.occurrence().head_record_id()
            || self.roots != view.manifest().roots
            || self.root != *declared(view)?
            || self.journal != index.root
            || self.records != index.records
            || self.records != summary.lifecycle_journal_records
            || self.keys != summary.lifecycle_keys
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
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
pub fn recover_attested_cognition_lifecycle_v3(
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AttestedCognitionLifecycleV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    crate::incremental_cognition_history::validate_accepted_v3_current(view, current, memory)?;
    if !current.enabled() {
        return Err(invalid());
    }
    let source = view.source_v3().ok_or_else(invalid)?;
    let index = view
        .manifest()
        .index_ref(CognitionHistoryFieldV1::JournalEvents);
    if !index.present || source.lifecycle_journal_records != index.records {
        return Err(invalid());
    }
    let charge = memory.reserve(4096, view.class())?;
    let value = AttestedCognitionLifecycleV3 {
        root: declared(view)?.clone(),
        keys: source.lifecycle_keys,
        journal: index.root.clone(),
        records: index.records,
        head: view.occurrence().head_record_id(),
        roots: view.manifest().roots,
        class: view.class(),
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(value))
}
pub fn readmit_attested_cognition_lifecycle_ordinary_v3(
    source: &ReservedValue<AttestedCognitionLifecycleV3>,
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AttestedCognitionLifecycleV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    crate::incremental_cognition_history::validate_accepted_v3_current(view, current, memory)?;
    let summary = view.source_v3().ok_or_else(invalid)?;
    let index = view
        .manifest()
        .index_ref(CognitionHistoryFieldV1::JournalEvents);
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 4096
        || source.class != MemoryClass::Recovery
        || view.class() != MemoryClass::Ordinary
        || !current.enabled()
        || !index.present
        || source.root != *declared(view)?
        || source.keys != summary.lifecycle_keys
        || source.records != summary.lifecycle_journal_records
        || source.records != index.records
        || source.journal != index.root
        || source.head != view.occurrence().head_record_id()
        || source.roots != view.manifest().roots
    {
        return Err(invalid());
    }
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let value = AttestedCognitionLifecycleV3 {
        root: source.root.clone(),
        keys: source.keys,
        journal: source.journal.clone(),
        records: source.records,
        head: source.head,
        roots: source.roots,
        class: MemoryClass::Ordinary,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(value))
}
