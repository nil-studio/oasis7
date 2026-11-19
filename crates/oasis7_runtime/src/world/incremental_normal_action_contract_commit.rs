//! Actual financial/continuity/contract CAS precedes the private event capability.
use super::*;
use crate::world::{
    incremental_allocators::RuntimeAllocatorV1,
    incremental_body::write_body_value,
    incremental_body_event::{
        AgentEventOriginalV1, materials::CorePolicyMaterialContextV1, read_fixed,
    },
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter, incremental_value::ValueNodeWriter,
};
use oasis7_world_state::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_field_member::MemberRecordWriteContext,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};
fn same_body(
    a: &crate::WorldEventBody,
    b: &crate::WorldEventBody,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<bool, BodyMutationError> {
    // Both use the original DomainEvent codec. Counting/streaming visits every
    // dynamic field before the native PartialEq, without allocating byte vectors.
    let mut counter = CompareCounter {
        common,
        first: None,
    };
    let first = serde_json::to_writer(&mut counter, a);
    if let Some(e) = counter.first.take() {
        return Err(e);
    }
    first.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let second = serde_json::to_writer(&mut counter, b);
    if let Some(e) = counter.first.take() {
        return Err(e);
    }
    second.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    Ok(a == b)
}
struct CompareCounter<'a, 'b> {
    common: &'a mut FormalBoundedContextV1<'b>,
    first: Option<BodyMutationError>,
}
impl std::io::Write for CompareCounter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if let Err(e) = charge(self.common, bytes.len()) {
            self.first = Some(e);
            return Err(std::io::Error::other("contract comparison work"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_contract_action_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
>(
    prepared: PreparedContractActionV1<'_, '_, '_>,
    accepted: SelectedAcceptedDispositionV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut ContractWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    max_event_bytes: usize,
    audit: &mut WorldEventDecodeWorkV1,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedAllianceActionV1, BodyMutationError> {
    let result = (|| {
        charge(common, 0)?;
        let now = step.time();
        step.transaction
            .validate_transaction_identity(&prepared.identity)?;
        accepted.validate(prepared.selected, step.transaction, step.mutation.memory)?;
        if prepared.core != common.work().identity()
            || !prepared.memory.same_pool(common.memory())
            || !prepared.memory.same_pool(step.mutation.memory)
            || prepared.time != now
        {
            return Err(bad());
        }
        let action = prepared
            .outcome
            .override_action_for_selected_v1(prepared.selected, step.transaction)?
            .unwrap_or(&prepared.selected.envelope().action);
        let PreparedContractActionV1 {
            selected,
            sources,
            body,
            before,
            after,
            financial,
            continuity,
            fees,
            verification,
            event_fee,
            layout,
            ..
        } = prepared;
        if let crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
            reason, ..
        }) = body
        {
            return step
                .append_selected_rejection_owned(
                    materials,
                    live,
                    selected,
                    reason,
                    max_event_bytes,
                    audit,
                    &mut *persist,
                )
                .map(|event| CompletedAllianceActionV1::Rejected {
                    accepted: accepted.into_audit(),
                    event,
                    _fees: fees,
                    _layout: layout,
                });
        }
        let after = after.ok_or_else(bad)?;
        {
            let mut context = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            let checked = evaluate(
                selected.envelope().id,
                action,
                sources,
                &mut context,
                parent,
                work,
                common,
                now,
                live,
                verification,
            )?;
            if !same_body(&body, &checked.body, common)? {
                return Err(bad());
            }
            if let (Some(a), Some(b)) = (before.as_deref(), checked.before.as_deref()) {
                let mut n = size_of::<EconomicContractState>();
                for text in [
                    &a.contract_id,
                    &a.creator_agent_id,
                    &a.counterparty_agent_id,
                    &a.description,
                    &b.contract_id,
                    &b.creator_agent_id,
                    &b.counterparty_agent_id,
                    &b.description,
                ] {
                    n = n.checked_add(text.len()).ok_or_else(limited)?;
                }
                for text in [a.settlement_notes.as_ref(), b.settlement_notes.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    n = n.checked_add(text.len()).ok_or_else(limited)?;
                }
                charge(common, n)?;
                if a != b {
                    return Err(bad());
                }
            } else if before.is_some() != checked.before.is_some() {
                return Err(bad());
            }
        }
        if let Some(plan) = &continuity {
            let mut context = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            settlement_continuity::apply_continuity_v1(
                plan,
                &after,
                &body,
                sources,
                &mut context,
                parent,
                write,
                work,
                common,
                now,
                live,
            )?;
        }
        if let Some(plan) = &financial {
            finance::apply_financial_v1(
                plan,
                before.as_deref().ok_or_else(bad)?,
                sources,
                step,
                read,
                write,
                common,
                live,
            )?;
        }
        let (actor, id) = match &body {
            crate::WorldEventBody::Domain(crate::DomainEvent::EconomicContractOpened {
                creator_agent_id,
                contract_id,
                ..
            }) => (creator_agent_id, contract_id),
            crate::WorldEventBody::Domain(crate::DomainEvent::EconomicContractSettled {
                operator_agent_id,
                contract_id,
                ..
            }) => (operator_agent_id, contract_id),
            _ => return Err(bad()),
        };
        {
            let mut context = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            let guard = context.transaction.guard();
            if let Some(before) = before.as_deref() {
                let current = open_current_diplomacy_contract_record_v1(
                    id,
                    sources.shared.diplomacy,
                    sources.shared.roots,
                    context.transaction,
                    parent,
                    &mut work.policy_scan,
                    &mut work.shared.scope_cells,
                    work.scalar.max_text_bytes,
                    &guard,
                    live,
                )
                .map_err(overlay)?
                .ok_or_else(bad)?;
                change_qualified_diplomacy_scalars_v1(
                    DiplomacyScalarsChangeV1 {
                        id,
                        qualification: &current,
                        expected: DiplomacySourceV1::Contract(before),
                        after: DiplomacySourceV1::Contract(&after),
                        max_current_payload_bytes: work.scalar.max_text_bytes,
                        max_pending_node_bytes: work.shared.max_pending_node_bytes,
                    },
                    &mut context,
                    write,
                    &guard,
                    live,
                )
                .map_err(overlay)?;
            } else {
                create_qualified_diplomacy_empty_v1(
                    DiplomacyEmptyCreateV1 {
                        id,
                        source: DiplomacySourceV1::Contract(&after),
                        family: sources.shared.diplomacy,
                        max_pending_node_bytes: work.shared.max_pending_node_bytes,
                    },
                    &mut context,
                    write,
                    &guard,
                    live,
                    &mut work.shared.write_cells,
                )
                .map_err(overlay)?;
            }
        }
        charge(common, actor.len())?;
        let key = AgentBodyKeyV1::new(actor).map_err(BodyMutationError::Invalid)?;
        let old = AgentBodyValueV1::from_canonical_bytes(&read_fixed::<
            AGENT_BODY_VALUE_BYTES,
            _,
            _,
            _,
            _,
        >(
            step.transaction,
            StateNamespaceV1::Business,
            key.as_bytes(),
            step.values,
            step.mutation,
        )?)
        .map_err(BodyMutationError::Invalid)?;
        let next = write_body_value(
            &AgentBodyValueV1 {
                body_view: old.body_view,
                last_active: now,
            },
            step.mutation,
        )?;
        step.transaction
            .write(StateNamespaceV1::Business, key.as_bytes(), Some(next))?;
        let sequence = step.read_allocator(RuntimeAllocatorV1::Event)?;
        let (id, _, _) = crate::world::World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id,
            time: now,
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body,
        };
        let cap = PreparedAllianceEventV1 {
            event: event_fee.retain(event),
            accepted: accepted.into_audit(),
            identity: step.transaction.guard(),
            _fees: fees,
            _layout: layout,
        };
        step.append_prepared_alliance_event_v1(
            &cap,
            materials,
            live,
            max_event_bytes,
            &mut *persist,
        )?;
        Ok(CompletedAllianceActionV1::Applied(cap))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
