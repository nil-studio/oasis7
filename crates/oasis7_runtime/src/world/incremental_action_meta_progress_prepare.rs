//! Pre-Accepted business validation, original ordering and complete DTO admission.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare<'s, 'r, M, P, V, W, E, C, PE, PC, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &MetaProgressSourcesV1<'_>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut MetaProgressWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<PreparedMetaProgressActionV1<'s, 'r>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    D: FnMut() -> bool,
{
    step.transaction.validate_guard(guard, live)?;
    selected.validate(step.transaction)?;
    let memory = step.mutation.memory;
    if !step.transaction.uses_budget(memory)
        || read.values.class != MemoryClass::Ordinary
        || parent_read.values.class != MemoryClass::Ordinary
        || !read.values.memory.same_pool(memory)
        || !parent_read.values.memory.same_pool(memory)
        || !sources.agents.uses_budget(memory)
        || sources.agents.allocation_class() != MemoryClass::Ordinary
        || !sources.native.uses_budget(memory)
    {
        return Err(BodyMutationError::Invalid("meta source pool/class"));
    }
    let _phase = memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if roots.digest()? != step.transaction.parent_fence().parent_roots {
        return Err(BodyMutationError::Invalid("meta parent roots"));
    }
    sources.native.validate_binding(roots)?;
    let replacement = outcome.override_action_for_selected_v1(selected, step.transaction)?;
    if outcome.decision.verdict == crate::RuleVerdict::Deny {
        return Err(BodyMutationError::Invalid("meta denied Rule outcome"));
    }
    let action = replacement.unwrap_or(&selected.envelope().action);
    let crate::Action::GrantMetaProgress {
        operator_agent_id,
        target_agent_id,
        track,
        points,
        achievement_id,
    } = action
    else {
        return Err(BodyMutationError::Invalid("meta effective variant"));
    };
    let text = operator_agent_id
        .len()
        .checked_add(target_agent_id.len())
        .and_then(|n| n.checked_add(track.len()))
        .and_then(|n| n.checked_add(achievement_id.as_ref().map_or(0, String::len)))
        .ok_or(ValueTreeError::ResourceLimited("meta event strings"))?;
    work.spend(text)?;
    if (read.cancelled)() || (parent_read.cancelled)() || (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    // These are all newly copied exact-size Strings, never caller Vec/String
    // capacity. The fixed surplus admits a rejection's one note/String vector.
    let event_fee = memory
        .reserve(
            std::mem::size_of::<crate::WorldEvent>()
                .checked_add(text)
                .and_then(|n| n.checked_add(512))
                .ok_or(ValueTreeError::ResourceLimited("meta event owned layout"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let completion = CompletedMetaProgressV1::reserve_rejection(memory)?;
    let reason = if !read::agent(
        step,
        operator_agent_id,
        sources,
        roots,
        read,
        parent_read,
        work,
        live,
    )? {
        Some(crate::RejectReason::AgentNotFound {
            agent_id: operator_agent_id.to_owned(),
        })
    } else if !read::agent(
        step,
        target_agent_id,
        sources,
        roots,
        read,
        parent_read,
        work,
        live,
    )? {
        Some(crate::RejectReason::AgentNotFound {
            agent_id: target_agent_id.to_owned(),
        })
    } else if track.trim().is_empty() {
        Some(crate::RejectReason::RuleDenied {
            notes: vec!["meta progression track cannot be empty".to_owned()],
        })
    } else if *points == 0 {
        Some(crate::RejectReason::InvalidAmount { amount: *points })
    } else if achievement_id
        .as_deref()
        .map(str::trim)
        .is_some_and(str::is_empty)
    {
        Some(crate::RejectReason::RuleDenied {
            notes: vec!["achievement_id cannot be empty".to_owned()],
        })
    } else {
        None
    };
    let intent = if let Some(reason) = reason {
        MetaIntentV1::Rejected(reason)
    } else {
        let track = track.trim();
        let achievement = achievement_id.as_deref().map(str::trim);
        let now = step.time();
        let read_stop = RefCell::new(&mut read.cancelled);
        let mutation_stop = RefCell::new(&mut step.mutation.cancelled);
        let mut ctx = WorldFieldReadContextV1 {
            root: read.root,
            state: read.state,
            values: ValueReadContext {
                source: read.values.source,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || (read_stop.borrow_mut())() || (mutation_stop.borrow_mut())(),
        };
        let guard = step.transaction.guard();
        let current = recover_current_meta_progress_v1(
            target_agent_id,
            sources.native,
            roots,
            step.transaction,
            &mut ctx,
            &mut work.native_scan,
            &mut work.native_read,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
        let delta = prepare_current_meta_progress_delta_v1(
            &current,
            track,
            *points,
            achievement,
            now,
            sources.native,
            roots,
            step.transaction,
            &mut ctx,
            &mut work.native_scan,
            &mut work.native_read,
            &mut work.mutation,
            live,
        )
        .map_err(overlay_error)?;
        MetaIntentV1::Grant {
            delta,
            body: crate::DomainEvent::MetaProgressGranted {
                operator_agent_id: operator_agent_id.to_owned(),
                target_agent_id: target_agent_id.to_owned(),
                track: track.to_owned(),
                points: *points,
                achievement_id: achievement.map(str::to_owned),
            },
        }
    };
    if (read.cancelled)() || (parent_read.cancelled)() || (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(PreparedMetaProgressActionV1 {
        selected,
        outcome,
        intent,
        time: step.time(),
        identity: step.transaction.guard(),
        memory: memory.clone(),
        event_fee,
        completion,
    })
}
