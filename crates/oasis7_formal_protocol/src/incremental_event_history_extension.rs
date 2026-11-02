//! Logical occurrence inheritance across one authenticated accepted extension.
//! No original IO, availability lease, Domain semantics or owner grant.
use super::*;

type Error = CoreError<std::convert::Infallible>;

pub struct EventHistoryExtensionRequestV1<'a> {
    pub parent: &'a VerifiedFormalCommitV1,
    pub head: &'a VerifiedFormalCommitV1,
    pub core: &'a ReservedValue<CheckedCommitCore>,
}

/// Private construction binds the exact direct accepted predecessor and its
/// immutable prefix to the checked core of the exact accepted successor.
#[derive(Debug)]
pub struct VerifiedEventHistoryExtensionV1 {
    parent_record: [u8; 32],
    parent_roots: [u8; 32],
    parent_body: [u8; 32],
    parent_height: u64,
    head_record: [u8; 32],
    head_roots: [u8; 32],
    head_body: [u8; 32],
}

pub fn verify_event_history_extension_v1(
    request: &EventHistoryExtensionRequestV1<'_>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<VerifiedEventHistoryExtensionV1>, Error> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if !request.core.uses_budget(memory)
        || request.core.reserved_bytes() < std::mem::size_of::<CheckedCommitCore>()
        || !request.core.facts.uses_budget(memory)
    {
        return Err(CoreError::Invalid("event extension core pool/charge"));
    }
    // Covers all bounded original header/record digest encoders, plus output.
    let mut workspace = memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    let parent = request.parent.record();
    parent.validate().map_err(BindingError::from)?;
    if !incremental::is_iws_record(parent.schema_version) {
        return Err(CoreError::Invalid("event extension requires IWS parent"));
    }
    let core = request.core;
    let head = request.head.record();
    let (head_roots, head_body) = event_head_binding(request.head, &core.roots)?;
    let parent_record = parent.record_id().map_err(BindingError::from)?;
    let parent_roots = role(parent, ArtifactRole::IncrementalRootSet)?;
    let parent_body = role(parent, ArtifactRole::IncrementalCommitBody)?;
    let parent_history = role(parent, ArtifactRole::IncrementalHistoryPrefix)?;
    if head.world != parent.world
        || parent.position.checked_add(1) != Some(head.position)
        || parent.position == 0
        || head.parent_record_id != parent_record
        || head.before_state_root != parent.after_state_root
        || parent.after_state_root != hex::encode(parent_roots)
        || core.body.digest()? != head_body
        || core.body.roots != head_roots
        || !core.body.scope.matches(&core.roots)
        || core.body.height != core.roots.height
        || hex::encode(core.body.operation) != head.request_digest
        || core.body.parent_commit != Some(parent_body)
        || core.body.history != role(head, ArtifactRole::IncrementalHistoryPrefix)?
        || core.history.digest()? != core.body.history
        || core.history.scope != core.body.scope
        || core.history.count != head.position
        || core.history.previous != Some(parent_history)
    {
        return Err(CoreError::Invalid(
            "event extension accepted parent/core binding",
        ));
    }
    let parent_record = crate::incremental_history::wire::parse_hash(&parent_record)?;
    let head_record = crate::incremental_history::wire::parse_hash(
        &head.record_id().map_err(BindingError::from)?,
    )?;
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    let charge = workspace
        .split(std::mem::size_of::<VerifiedEventHistoryExtensionV1>())
        .map_err(|_| CoreError::Invalid("event extension summary accounting"))?;
    Ok(charge.retain(VerifiedEventHistoryExtensionV1 {
        parent_record,
        parent_roots,
        parent_body,
        parent_height: parent.position - 1,
        head_record,
        head_roots,
        head_body,
    }))
}

/// Retain the old exact occurrence under the authenticated successor prefix.
/// The changed head does not change its original Event body, height or ordinal.
/// No occurrence lookup or original read is performed by this operation.
pub fn inherit_event_fact_occurrence_v1(
    occurrence: &ReservedValue<VerifiedEventFactOccurrenceV1>,
    extension: &ReservedValue<VerifiedEventHistoryExtensionV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<VerifiedEventFactOccurrenceV1>, Error> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if !occurrence.uses_budget(memory)
        || !extension.uses_budget(memory)
        || occurrence.reserved_bytes() < std::mem::size_of::<VerifiedEventFactOccurrenceV1>()
        || extension.reserved_bytes() < std::mem::size_of::<VerifiedEventHistoryExtensionV1>()
    {
        return Err(CoreError::Invalid("event inheritance proof pool/charge"));
    }
    let charge = memory
        .reserve(
            std::mem::size_of::<VerifiedEventFactOccurrenceV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ExecutionError::Memory)?;
    if occurrence.head_record != extension.parent_record
        || occurrence.roots != extension.parent_roots
        || occurrence.head_body != extension.parent_body
        || occurrence.location.height > extension.parent_height
    {
        return Err(CoreError::Invalid("event inheritance predecessor binding"));
    }
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    Ok(charge.retain(VerifiedEventFactOccurrenceV1 {
        head_record: extension.head_record,
        roots: extension.head_roots,
        head_body: extension.head_body,
        event_body: occurrence.event_body,
        location: occurrence.location,
        original_bytes: occurrence.original_bytes,
    }))
}
