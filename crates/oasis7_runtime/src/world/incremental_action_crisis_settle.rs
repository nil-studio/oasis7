//! Accepted-bound publication: reopen actual Current, compare before, then CAS.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn commit<M, P, V, W, E, C, PE, PC, ME, D, F>(
    prepared: PreparedCrisisActionV1<'_, '_>,
    accepted: SelectedActionAuditV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &CrisisSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut CrisisWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedCrisisV1, BodyMutationError>
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
        return Err(BodyMutationError::Invalid("crisis prepared clock/pool"));
    }
    prepared
        .outcome
        .override_action_for_selected_v1(prepared.selected, step.transaction)?;
    if prepared.outcome.decision.verdict == crate::RuleVerdict::Deny {
        return Err(BodyMutationError::Invalid("crisis prepared Rule denied"));
    }
    let PreparedCrisisActionV1 {
        selected,
        intent,
        event_fee,
        completion,
        ..
    } = prepared;
    let (old_record, before_state, after_state, body) = match intent {
        CrisisIntentV1::Rejected(reason) => {
            let rejected = step.append_selected_rejection_owned(
                materials,
                live,
                selected,
                reason,
                max_event_bytes,
                audit_work,
                &mut *persist,
            )?;
            return CompletedCrisisV1::from_rejection(
                accepted,
                rejected,
                selected,
                step.transaction,
                step.mutation.memory,
                completion,
            );
        }
        CrisisIntentV1::Resolve {
            current,
            before,
            after,
            body,
        } => (current, before, after, body),
    };
    let crate::DomainEvent::CrisisResolved {
        resolver_agent_id,
        crisis_id,
        ..
    } = &body
    else {
        return Err(BodyMutationError::Invalid("crisis prepared body"));
    };
    if !read::agent(
        step,
        resolver_agent_id,
        sources,
        materials.roots,
        read,
        parent_read,
        work,
        live,
    )? {
        return Err(BodyMutationError::Invalid("crisis resolver disappeared"));
    }
    let guard = step.transaction.guard();
    let current = open_current_diplomacy_crisis_record_v1(
        crisis_id,
        sources.diplomacy,
        materials.roots,
        step.transaction,
        read,
        &mut work.crisis_scan,
        &mut work.cells_remaining,
        work.max_text_bytes,
        &guard,
        live,
    )
    .map_err(overlay_error)?
    .ok_or(BodyMutationError::Invalid(
        "crisis disappeared after Accepted",
    ))?;
    if current.root() != old_record.root() {
        return Err(BodyMutationError::Invalid("crisis changed after prepare"));
    }
    {
        let stop = RefCell::new(&mut read.cancelled);
        let mutation = RefCell::new(&mut step.mutation.cancelled);
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: step.transaction,
            values: ValueReadContext {
                source: read.values.source,
                memory: step.mutation.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut *step.mutation.sink,
            memory: step.mutation.memory,
            io: &mut *step.mutation.io,
            key_work: &mut *materials.write_keys,
            limits: materials.write_limits,
            cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
        };
        let guard = overlay.transaction.guard();
        change_qualified_diplomacy_scalars_v1(
            DiplomacyScalarsChangeV1 {
                id: crisis_id,
                qualification: &current,
                expected: DiplomacySourceV1::Crisis(&before_state),
                after: DiplomacySourceV1::Crisis(&after_state),
                max_current_payload_bytes: work.max_text_bytes,
                max_pending_node_bytes: work.max_pending_node_bytes,
            },
            &mut overlay,
            &mut write,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
    }
    event::touch_agents(step, resolver_agent_id, resolver_agent_id, work, live)?;
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
    let prepared = PreparedCrisisEventV1 {
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
    Ok(CompletedCrisisV1 {
        accepted,
        result: CompletedCrisisResultV1::Published {
            event: prepared.event,
            identity: step.transaction.guard(),
            _owner: after_state,
        },
        _fee: completion.fee,
    })
}
