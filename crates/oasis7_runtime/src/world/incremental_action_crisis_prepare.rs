//! Pre-Accepted business validation, original ordering and complete DTO admission.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare<'s, 'r, M, P, V, W, E, C, PE, PC, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &CrisisSourcesV1<'_>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut CrisisWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<PreparedCrisisActionV1<'s, 'r>, BodyMutationError>
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
        || !sources.diplomacy.uses_budget(memory)
    {
        return Err(BodyMutationError::Invalid("crisis source pool/class"));
    }
    let _phase = memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if roots.digest()? != step.transaction.parent_fence().parent_roots {
        return Err(BodyMutationError::Invalid("crisis parent roots"));
    }
    sources.diplomacy.validate_binding(&roots.digest()?)?;
    let replacement = outcome.override_action_for_selected_v1(selected, step.transaction)?;
    if outcome.decision.verdict == crate::RuleVerdict::Deny {
        return Err(BodyMutationError::Invalid("crisis denied Rule outcome"));
    }
    let action = replacement.unwrap_or(&selected.envelope().action);
    if !matches!(
        selected.envelope().action,
        crate::Action::ResolveCrisis { .. }
    ) {
        return Err(BodyMutationError::Invalid("crisis original variant"));
    }
    let crate::Action::ResolveCrisis {
        resolver_agent_id,
        crisis_id,
        strategy,
        success,
    } = action
    else {
        return Err(BodyMutationError::Invalid("crisis effective variant"));
    };
    let text = resolver_agent_id
        .len()
        .checked_add(crisis_id.len())
        .and_then(|n| n.checked_add(strategy.len()))
        .ok_or(ValueTreeError::ResourceLimited("crisis event strings"))?;
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
                .ok_or(ValueTreeError::ResourceLimited("crisis event owned layout"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let completion = CompletedCrisisV1::reserve_rejection(memory)?;
    let mut reason = None;
    let mut record = None;
    if !read::agent(
        step,
        resolver_agent_id,
        sources,
        roots,
        read,
        parent_read,
        work,
        live,
    )? {
        reason = Some(crate::RejectReason::AgentNotFound {
            agent_id: resolver_agent_id.to_owned(),
        });
    } else if crisis_id.trim().is_empty() {
        reason = Some(denied("crisis_id cannot be empty".into()));
    } else {
        let guard = step.transaction.guard();
        let current = open_current_diplomacy_crisis_record_v1(
            crisis_id.trim(),
            sources.diplomacy,
            roots,
            step.transaction,
            read,
            &mut work.crisis_scan,
            &mut work.cells_remaining,
            work.max_text_bytes,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
        if let Some(current) = current {
            let before = read::crisis(crisis_id.trim(), &current, read, work)?;
            if before.status != oasis7_world_state::gameplay_state::CrisisStatus::Active {
                reason = Some(denied(format!(
                    "crisis is not active and cannot be resolved: {}",
                    crisis_id.trim()
                )));
            } else if step.time() > before.expires_at {
                reason = Some(denied(format!(
                    "crisis expired at {} and cannot be resolved: {}",
                    before.expires_at,
                    crisis_id.trim()
                )));
            } else if strategy.trim().is_empty() {
                reason = Some(denied("crisis strategy cannot be empty".into()));
            } else {
                record = Some((current, before));
            }
        } else {
            reason = Some(denied(format!("crisis not found: {}", crisis_id.trim())));
        }
    }
    let intent = if let Some(reason) = reason {
        CrisisIntentV1::Rejected(reason)
    } else {
        let (current, before) =
            record.ok_or(BodyMutationError::Invalid("crisis prepared source"))?;
        let size = std::mem::size_of::<oasis7_world_state::gameplay_state::CrisisState>()
            .checked_add(before.crisis_id.len())
            .and_then(|n| n.checked_add(before.kind.len()))
            .and_then(|n| n.checked_add(resolver_agent_id.len()))
            .and_then(|n| n.checked_add(strategy.trim().len()))
            .ok_or(ValueTreeError::ResourceLimited("crisis after layout"))?;
        let fee = memory
            .reserve(size, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let impact = i64::from(before.severity.max(1))
            .saturating_mul(crate::world::event_processing::crisis_base_impact_per_severity_v1());
        let impact = if *success { impact } else { -impact };
        let after = fee.retain(oasis7_world_state::gameplay_state::CrisisState {
            crisis_id: before.crisis_id.clone(),
            kind: before.kind.clone(),
            severity: before.severity,
            status: oasis7_world_state::gameplay_state::CrisisStatus::Resolved,
            opened_at: before.opened_at,
            expires_at: before.expires_at,
            resolver_agent_id: Some(resolver_agent_id.to_owned()),
            strategy: Some(strategy.trim().to_owned()),
            success: Some(*success),
            impact,
            resolved_at: Some(step.time()),
        });
        CrisisIntentV1::Resolve {
            current,
            before,
            after,
            body: crate::DomainEvent::CrisisResolved {
                resolver_agent_id: resolver_agent_id.to_owned(),
                crisis_id: crisis_id.trim().to_owned(),
                strategy: strategy.trim().to_owned(),
                success: *success,
                impact,
            },
        }
    };
    if (read.cancelled)() || (parent_read.cancelled)() || (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(PreparedCrisisActionV1 {
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

fn denied(note: String) -> crate::RejectReason {
    crate::RejectReason::RuleDenied { notes: vec![note] }
}
