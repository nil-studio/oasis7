use super::*;
use crate::{
    incremental_agent_subtree::AgentMailboxReferenceV1,
    incremental_agent_subtree::{AgentSubtreeKeyV1, parse_agent_subtree_key_v1},
    incremental_event_fields::{MAILBOX_EVENT_REFERENCE_BYTES, mailbox_event_id_v1},
};
pub(super) struct MailboxOriginalQualificationV1 {
    pub(super) roots: [u8; 32],
    pub(super) anchor: MailboxProvenanceAnchorV1,
    pub(super) class: MemoryClass,
    pub(super) references: u64,
}
fn occurrence(remaining: &mut u64) -> Result<(), ValueTreeError> {
    *remaining = remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "current mailbox original occurrences",
        ))?;
    Ok(())
}
pub(super) fn qualify<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
    B: MailboxWorldEventOriginalReaderV1<O>,
>(
    roots: &RootSetV1,
    scope: &MailboxProvenanceScopeV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawCompleteWorkV1,
    objects: &mut O,
    originals: &mut B,
) -> Result<
    (
        ReservedValue<agents::AgentSubtreeStructureQualificationV1>,
        ReservedValue<MailboxOriginalQualificationV1>,
    ),
    WorldRawCompleteErrorV1<O::Error>,
> {
    check(ctx)?;
    let memory = ctx.values.memory;
    let anchor = scope.anchor(memory, ctx.values.class)?;
    if anchor.roots_id() != roots.digest().map_err(BodyMutationError::from)? {
        return Err(invalid().into());
    }
    let class = ctx.values.class;
    let charge = memory.reserve(512, class).map_err(ValueTreeError::Memory)?;
    let mut failure = None;
    let mut references = 0u64;
    let stop = RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: ctx.root,
        state: ctx.state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory,
            class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: || stop.borrow_mut()(),
    };
    let original_work = &mut work.originals;
    let remaining = &mut work.mailbox_occurrences_remaining;
    let result = agents::qualify_agent_subtree_structure_v1(
        roots,
        &mut local,
        &mut work.state_scan,
        &mut work.agents,
        &mut |reference| {
            occurrence(remaining)?;
            let proof = match originals.checked_world_event(
                AgentMailboxReferenceV1 {
                    agent: reference.agent,
                    ordinal: reference.ordinal,
                    original: reference.original,
                },
                scope,
                objects,
                memory,
                class,
                original_work,
                &mut || stop.borrow_mut()(),
            ) {
                Ok(value) => value,
                Err(e) => {
                    failure = Some(e);
                    return Err(ValueTreeError::Invalid("mailbox original source failed"));
                }
            };
            CheckedMailboxWorldEventOriginalV1::validate(
                &proof,
                &reference.original,
                anchor,
                memory,
                class,
            )?;
            references = references.checked_add(1).ok_or_else(invalid)?;
            Ok(())
        },
    );
    check(&mut local)?;
    if let Some(error) = failure {
        return Err(WorldRawCompleteErrorV1::Original(error));
    }
    let agent = result?;
    if agent.mailbox_reference_count() != references {
        return Err(invalid().into());
    }
    let mailbox = charge.retain(MailboxOriginalQualificationV1 {
        roots: roots.digest().map_err(BodyMutationError::from)?,
        anchor,
        class,
        references,
    });
    Ok((agent, mailbox))
}
pub(super) fn accept<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
    B: MailboxWorldEventOriginalReaderV1<O>,
>(
    witness: &VerifiedWorldTransitionV1,
    agent: &ReservedValue<agents::AgentSubtreeStructureQualificationV1>,
    scope: &MailboxProvenanceScopeV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawCompleteWorkV1,
    objects: &mut O,
    originals: &mut B,
) -> Result<ReservedValue<MailboxOriginalQualificationV1>, WorldRawCompleteErrorV1<O::Error>> {
    let anchor = scope.anchor(ctx.values.memory, ctx.values.class)?;
    if anchor.roots_id() != *witness.after_digest() {
        return Err(invalid().into());
    }
    let charge = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    for write in &witness.record().writes {
        check(ctx)?;
        if write.mutation.after == write.mutation.before {
            continue;
        }
        let Some(AgentSubtreeKeyV1::Mailbox {
            agent,
            ordinal: Some(ordinal),
        }) = parse_agent_subtree_key_v1(write.namespace, &write.mutation.key)?
        else {
            continue;
        };
        let Some(id) = write.mutation.after else {
            continue;
        };
        occurrence(&mut work.mailbox_occurrences_remaining)?;
        let bytes = schedule::fixed::<MAILBOX_EVENT_REFERENCE_BYTES, _, _, _, _>(&id, ctx)?;
        let original = mailbox_event_id_v1(&bytes).map_err(ValueTreeError::Invalid)?;
        let proof = originals
            .checked_world_event(
                AgentMailboxReferenceV1 {
                    agent,
                    ordinal,
                    original,
                },
                scope,
                objects,
                ctx.values.memory,
                ctx.values.class,
                &mut work.originals,
                &mut ctx.cancelled,
            )
            .map_err(WorldRawCompleteErrorV1::Original)?;
        CheckedMailboxWorldEventOriginalV1::validate(
            &proof,
            &original,
            anchor,
            ctx.values.memory,
            ctx.values.class,
        )?;
        check(ctx)?;
    }
    Ok(charge.retain(MailboxOriginalQualificationV1 {
        roots: *witness.after_digest(),
        anchor,
        class: ctx.values.class,
        references: agent.mailbox_reference_count(),
    }))
}
pub(super) fn readmit(
    source: &ReservedValue<MailboxOriginalQualificationV1>,
    roots: &RootSetV1,
    scope: &MailboxProvenanceScopeV1<'_>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<MailboxOriginalQualificationV1>, WorldRawCompleteStructuralErrorV1> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let MailboxProvenanceScopeV1::Accepted { .. } = scope else {
        return Err(invalid().into());
    };
    let anchor = scope.anchor(memory, MemoryClass::Ordinary)?;
    if source.anchor != anchor {
        return Err(invalid().into());
    }
    if source.class != MemoryClass::Recovery
        || !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.roots != roots.digest().map_err(BodyMutationError::from)?
    {
        return Err(invalid().into());
    }
    let charge = memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let output = charge.retain(MailboxOriginalQualificationV1 {
        roots: source.roots,
        anchor,
        class: MemoryClass::Ordinary,
        references: source.references,
    });
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(output)
}
