//! Accepted-bound publication: reopen actual Current, compare before, then CAS.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn commit<M, P, V, W, E, C, PE, PC, ME, D, F>(
    prepared: PreparedMetaProgressActionV1<'_, '_>,
    accepted: SelectedActionAuditV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &MetaProgressSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut MetaProgressWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedMetaProgressV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    step.transaction
        .validate_transaction_identity(&prepared.identity)?;
    validate_accepted(
        &accepted,
        prepared.selected,
        step.transaction,
        step.mutation.memory,
        step.time(),
    )?;
    if !prepared.memory.same_pool(step.mutation.memory) || prepared.time != step.time() {
        return Err(BodyMutationError::Invalid("meta prepared clock/pool"));
    }
    prepared
        .outcome
        .override_action_for_selected_v1(prepared.selected, step.transaction)?;
    if prepared.outcome.decision.verdict == crate::RuleVerdict::Deny {
        return Err(BodyMutationError::Invalid("meta prepared Rule denied"));
    }
    let PreparedMetaProgressActionV1 {
        selected,
        intent,
        event_fee,
        completion,
        ..
    } = prepared;
    let (delta, body) = match intent {
        MetaIntentV1::Rejected(reason) => {
            let rejected = step.append_selected_rejection_owned(
                materials,
                live,
                selected,
                reason,
                max_event_bytes,
                audit_work,
                &mut *persist,
            )?;
            return CompletedMetaProgressV1::from_rejection(
                accepted,
                rejected,
                selected,
                step.transaction,
                step.mutation.memory,
                completion,
            );
        }
        MetaIntentV1::Grant { delta, body } => (delta, body),
    };
    let crate::DomainEvent::MetaProgressGranted {
        operator_agent_id,
        target_agent_id,
        ..
    } = &body
    else {
        return Err(BodyMutationError::Invalid("meta prepared body"));
    };
    work.spend(
        operator_agent_id
            .len()
            .checked_add(target_agent_id.len())
            .ok_or(ValueTreeError::ResourceLimited("meta commit identifiers"))?,
    )?;
    // Accepted is a real revision change. Recheck the actual same Current agent
    // sources, then obtain a newly decoded Meta owner; no old guard is resigned.
    for actor in [operator_agent_id.as_str(), target_agent_id.as_str()] {
        if !read::agent(
            step,
            actor,
            sources,
            materials.roots,
            read,
            parent_read,
            work,
            live,
        )? {
            return Err(BodyMutationError::Invalid(
                "meta agent disappeared after preparation",
            ));
        }
    }
    {
        let stop = RefCell::new(&mut read.cancelled);
        let mutation = RefCell::new(&mut step.mutation.cancelled);
        let mut ctx = WorldFieldReadContextV1 {
            root: read.root,
            state: read.state,
            values: ValueReadContext {
                source: read.values.source,
                memory: step.mutation.memory,
                class: MemoryClass::Ordinary,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
        };
        let guard = step.transaction.guard();
        let current = recover_current_meta_progress_v1(
            target_agent_id,
            sources.native,
            materials.roots,
            step.transaction,
            &mut ctx,
            &mut work.native_scan,
            &mut work.native_read,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
        let mut write = MemberRecordWriteContext {
            sink: &mut *step.mutation.sink,
            memory: step.mutation.memory,
            io: &mut *step.mutation.io,
            key_work: &mut *materials.write_keys,
            limits: materials.write_limits,
            cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
        };
        write_prepared_current_meta_progress_v1(
            &delta,
            &current,
            step.transaction,
            &mut ctx,
            &mut write,
            &mut work.records_remaining,
            &mut work.native_write,
            &mut work.mutation,
            live,
        )
        .map_err(overlay_error)?;
    }
    event::touch_agents(step, operator_agent_id, target_agent_id, work, live)?;
    // Read the real allocator only now, after Accepted and actual business CAS.
    let sequence =
        step.read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
    let (id, _, _) = World::preview_next_event_id(sequence.next, sequence.era);
    let event = event_fee.retain(crate::WorldEvent {
        id,
        time: step.time(),
        caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
        body: crate::WorldEventBody::Domain(body),
    });
    let prepared = PreparedMetaProgressEventV1 {
        event,
        action_id: selected.envelope().id,
        time: step.time(),
        identity: step.transaction.guard(),
    };
    prepared.validate(step.transaction, step.mutation.memory, step.time())?;
    let before = step.begin_world_event()?;
    event::sink(step, &prepared, work, max_event_bytes, live, persist)?;
    step.finish_world_event(before)?;
    if (read.cancelled)() || (parent_read.cancelled)() || (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(CompletedMetaProgressV1 {
        accepted,
        result: CompletedMetaResultV1::Published {
            event: prepared.event,
            identity: step.transaction.guard(),
            _delta: delta,
        },
        _fee: completion.fee,
    })
}
