//! Accepted-bound publication: reopen actual Current, compare before, then CAS.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn commit<M, P, V, W, E, C, PE, PC, ME, D, F>(
    prepared: PreparedContractAcceptActionV1<'_, '_>,
    accepted: crate::world::incremental_module_invocation::SelectedAcceptedDispositionV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &ContractAcceptSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ContractAcceptWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedContractAcceptV1, BodyMutationError>
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
        return Err(BodyMutationError::Invalid(
            "contract_accept prepared clock/pool",
        ));
    }
    prepared
        .outcome
        .override_action_for_selected_v1(prepared.selected, step.transaction)?;
    if prepared.outcome.decision.verdict == crate::RuleVerdict::Deny {
        return Err(BodyMutationError::Invalid(
            "contract_accept prepared Rule denied",
        ));
    }
    let PreparedContractAcceptActionV1 {
        selected,
        intent,
        event_fee,
        completion,
        ..
    } = prepared;
    let (old_record, before_state, after_state, body) = match intent {
        ContractAcceptIntentV1::Rejected(reason) => {
            let rejected = step.append_selected_rejection_owned(
                materials,
                live,
                selected,
                reason,
                max_event_bytes,
                audit_work,
                &mut *persist,
            )?;
            return CompletedContractAcceptV1::from_rejection(
                accepted,
                rejected,
                selected,
                step.transaction,
                step.mutation.memory,
                completion,
            );
        }
        ContractAcceptIntentV1::Resolve {
            current,
            before,
            after,
            body,
        } => (current, before, after, body),
    };
    let crate::DomainEvent::EconomicContractAccepted {
        accepter_agent_id,
        contract_id,
        ..
    } = &body
    else {
        return Err(BodyMutationError::Invalid("contract_accept prepared body"));
    };
    if !read::agent(
        step,
        accepter_agent_id,
        sources,
        materials.roots,
        read,
        parent_read,
        work,
        live,
    )? {
        return Err(BodyMutationError::Invalid(
            "contract_accept resolver disappeared",
        ));
    }
    let guard = step.transaction.guard();
    let current = open_current_diplomacy_contract_record_v1(
        contract_id,
        sources.diplomacy,
        materials.roots,
        step.transaction,
        read,
        &mut work.contract_scan,
        &mut work.cells_remaining,
        work.max_text_bytes,
        &guard,
        live,
    )
    .map_err(overlay_error)?
    .ok_or(BodyMutationError::Invalid(
        "contract_accept disappeared after Accepted",
    ))?;
    if current.root() != old_record.root() {
        return Err(BodyMutationError::Invalid(
            "contract_accept changed after prepare",
        ));
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
                id: contract_id,
                qualification: &current,
                expected: DiplomacySourceV1::Contract(&before_state),
                after: DiplomacySourceV1::Contract(&after_state),
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
    event::touch_agents(step, accepter_agent_id, accepter_agent_id, work, live)?;
    let guard = step.transaction.guard();
    materials.contribute(step.transaction, step.values, step.mutation, &guard, live)?;
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
    let prepared = PreparedContractAcceptEventV1 {
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
    Ok(CompletedContractAcceptV1 {
        accepted: accepted.into_audit(),
        identity: step.transaction.guard(),
        result: CompletedContractAcceptResultV1::Published {
            event: prepared.event,
            identity: step.transaction.guard(),
            _owner: after_state,
        },
        _fee: completion.fee,
    })
}
