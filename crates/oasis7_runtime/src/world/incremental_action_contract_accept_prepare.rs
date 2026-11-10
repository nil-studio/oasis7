//! Pre-Accepted business validation, original ordering and complete DTO admission.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare<'s, 'r, M, P, V, W, E, C, PE, PC, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &ContractAcceptSourcesV1<'_>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ContractAcceptWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<PreparedContractAcceptActionV1<'s, 'r>, BodyMutationError>
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
        return Err(BodyMutationError::Invalid(
            "contract_accept source pool/class",
        ));
    }
    let _phase = memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if roots.digest()? != step.transaction.parent_fence().parent_roots {
        return Err(BodyMutationError::Invalid("contract_accept parent roots"));
    }
    sources.diplomacy.validate_binding(&roots.digest()?)?;
    let replacement = outcome.override_action_for_selected_v1(selected, step.transaction)?;
    if outcome.decision.verdict == crate::RuleVerdict::Deny {
        return Err(BodyMutationError::Invalid(
            "contract_accept denied Rule outcome",
        ));
    }
    let action = replacement.unwrap_or(&selected.envelope().action);
    if !matches!(
        selected.envelope().action,
        crate::Action::AcceptEconomicContract { .. }
    ) {
        return Err(BodyMutationError::Invalid(
            "contract_accept original variant",
        ));
    }
    let crate::Action::AcceptEconomicContract {
        accepter_agent_id,
        contract_id,
    } = action
    else {
        return Err(BodyMutationError::Invalid(
            "contract accept effective variant",
        ));
    };
    let text = accepter_agent_id
        .len()
        .checked_add(contract_id.len())
        .ok_or(ValueTreeError::ResourceLimited(
            "contract accept event strings",
        ))?;
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
                .and_then(|n| n.checked_add(usize::try_from(work.max_text_bytes).ok()?))
                .ok_or(ValueTreeError::ResourceLimited(
                    "contract_accept event owned layout",
                ))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let completion = CompletedContractAcceptV1::reserve_rejection(memory)?;
    let mut reason = None;
    let mut record = None;
    if !read::agent(
        step,
        accepter_agent_id,
        sources,
        roots,
        read,
        parent_read,
        work,
        live,
    )? {
        reason = Some(crate::RejectReason::AgentNotFound {
            agent_id: accepter_agent_id.to_owned(),
        });
    } else if contract_id.trim().is_empty() {
        reason = Some(denied("contract_id cannot be empty".into()));
    } else {
        let guard = step.transaction.guard();
        let current = open_current_diplomacy_contract_record_v1(
            contract_id.trim(),
            sources.diplomacy,
            roots,
            step.transaction,
            read,
            &mut work.contract_scan,
            &mut work.cells_remaining,
            work.max_text_bytes,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
        if let Some(current) = current {
            let before = read::contract(contract_id.trim(), &current, read, work)?;
            if before.status != oasis7_world_state::gameplay_state::EconomicContractStatus::Open {
                reason = Some(denied(format!(
                    "economic contract is not open: {}",
                    contract_id.trim()
                )));
            } else if before.counterparty_agent_id != *accepter_agent_id {
                reason = Some(denied(format!(
                    "contract accepter mismatch expected {}",
                    before.counterparty_agent_id
                )));
            } else if step.time() > before.expires_at {
                reason = Some(denied(format!(
                    "economic contract has expired at {}",
                    before.expires_at
                )));
            } else {
                record = Some((current, before));
            }
        } else {
            reason = Some(denied(format!(
                "economic contract not found: {}",
                contract_id.trim()
            )));
        }
    }
    let intent = if let Some(reason) = reason {
        ContractAcceptIntentV1::Rejected(reason)
    } else {
        let (current, before) = record.ok_or(BodyMutationError::Invalid(
            "contract accept prepared source",
        ))?;
        let size = std::mem::size_of::<oasis7_world_state::gameplay_state::EconomicContractState>()
            .checked_add(before.contract_id.len())
            .and_then(|n| n.checked_add(before.creator_agent_id.len()))
            .and_then(|n| n.checked_add(before.counterparty_agent_id.len()))
            .and_then(|n| n.checked_add(before.description.len()))
            .and_then(|n| n.checked_add(before.settlement_notes.as_ref().map_or(0, String::len)))
            .ok_or(ValueTreeError::ResourceLimited(
                "contract accept after layout",
            ))?;
        let fee = memory
            .reserve(size, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut after = (*before).clone();
        after.status = oasis7_world_state::gameplay_state::EconomicContractStatus::Accepted;
        after.accepted_at = Some(step.time());
        ContractAcceptIntentV1::Resolve {
            current,
            before,
            after: fee.retain(after),
            body: crate::DomainEvent::EconomicContractAccepted {
                accepter_agent_id: accepter_agent_id.to_owned(),
                contract_id: contract_id.trim().to_owned(),
            },
        }
    };
    if (read.cancelled)() || (parent_read.cancelled)() || (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(PreparedContractAcceptActionV1 {
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
