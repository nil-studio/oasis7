//! ExpandBodyInterface: concrete Current body rows, original fixed predicate,
//! real Accepted disposition, then full scalar/row CAS and private event.
#[path = "incremental_normal_action_body_interface_read.rs"]
mod read;
use super::{
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_allocators::RuntimeAllocatorV1,
    incremental_body::{BodyMutationError, write_body_value},
    incremental_body_event::{
        AgentEventOriginalV1, materials::CorePolicyMaterialContextV1, read_fixed,
    },
    incremental_module_invocation::{RootRulePhaseOutcomeV1, SelectedAcceptedDispositionV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    agent_state::{BodyModuleSlot, BodySlotType, CargoEntityKind},
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_fields::{AgentFieldV1, AgentScalarRefV1},
    incremental_agent_subtree::{
        AgentSubtreeCellChangeV1, AgentSubtreeQualifiedRequestV1, AgentSubtreeRowChangeV1,
        AgentSubtreeRowRefV1, AgentSubtreeScalarRequestV1, AgentSubtreeStructureQualificationV1,
        change_agent_subtree_row_v1, change_agent_subtree_scalar_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_value_mutation::ValueMutationContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};
use read::{CurrentBodyInterfaceV1, read_current_body_interface_v1};
use std::{cell::RefCell, mem::size_of};
/// Preserves the original typed World preflight error without pretending that
/// WorldError implements the storage-error trait used by the transaction API.
#[derive(Debug)]
pub(in crate::world) struct BodyInterfacePreflightWorldErrorV1(pub(crate) crate::WorldError);
impl std::fmt::Display for BodyInterfacePreflightWorldErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            crate::WorldError::ResourceBalanceInvalid { reason } => f.write_str(reason),
            error => write!(f, "{error:?}"),
        }
    }
}
impl std::error::Error for BodyInterfacePreflightWorldErrorV1 {}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("body interface prepared source")
}
fn limit() -> BodyMutationError {
    ValueTreeError::ResourceLimited("body interface prepared layout").into()
}
fn charge(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    ctx.charge(1, u64::try_from(n).map_err(|_| limit())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
pub(in crate::world) struct PreparedBodyInterfaceActionV1<'s, 'r> {
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    before: Option<CurrentBodyInterfaceV1>,
    body: crate::WorldEventBody,
    qualification: &'s ReservedValue<AgentSubtreeStructureQualificationV1>,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    time: u64,
    identity: PreparedGuard,
    verify_fee: Option<ReservedValue<()>>,
    event_fee: MemoryReservation,
}
/// Only an actual completed Current CAS/last-active publication mints this cap.
pub(in crate::world) struct PreparedBodyInterfaceEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    identity: PreparedGuard,
    accepted: Option<SelectedActionAuditV1>,
}
impl PreparedBodyInterfaceEventV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if let Some(accepted) = &self.accepted {
            accepted.validate(tx)?;
            if !accepted.uses_budget(memory) {
                return Err(bad());
            }
        }
        if !self.event.uses_budget(memory) || !tx.uses_budget(memory) || self.event.time != now {
            return Err(bad());
        }
        Ok(())
    }
}
pub(in crate::world) enum CompletedBodyInterfaceV1 {
    Rejected {
        accepted: Option<SelectedActionAuditV1>,
        event: SelectedActionAuditV1,
    },
    Domain(PreparedBodyInterfaceEventV1),
}
impl CompletedBodyInterfaceV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match self {
            Self::Rejected { event, .. } => event.event(),
            Self::Domain(d) => d.event(),
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match self {
            Self::Rejected { accepted, event } => {
                if let Some(accepted) = accepted {
                    accepted.validate(tx)?;
                }
                event.validate(tx)
            }
            Self::Domain(d) => {
                tx.validate_transaction_identity(&d.identity)?;
                if let Some(accepted) = &d.accepted {
                    accepted.validate(tx)?;
                }
                Ok(())
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_body_interface_v1<'s, 'r, M, R, E, C>(
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    qualification: &'s ReservedValue<AgentSubtreeStructureQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    max_text_bytes: usize,
    live: &ExecutionFence,
) -> Result<PreparedBodyInterfaceActionV1<'s, 'r>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        charge(common, 0)?;
        selected.validate(context.transaction)?;
        if !outcome.uses_budget(common.memory())
            || !context.values.memory.same_pool(common.memory())
            || outcome.decision.verdict == crate::RuleVerdict::Deny
        {
            return Err(bad());
        }
        let effective = outcome
            .override_action_for_selected_v1(selected, context.transaction)?
            .unwrap_or(&selected.envelope().action);
        let crate::Action::ExpandBodyInterface {
            agent_id,
            interface_module_item_id,
        } = effective
        else {
            return Err(bad());
        };
        let before = read_current_body_interface_v1(
            agent_id,
            qualification,
            context,
            common,
            max_text_bytes,
            None,
            live,
        )?;
        // At most one agent/item output string, one slot-id candidate (5+20
        // chars, String formatting may double), and the longest fixed rejection
        // text (or capacity prefix + u16's at most five digits, also doubled).
        let reason = "interface module item unavailable or depleted"
            .len()
            .max("body slot capacity reached max ".len() + 5);
        let bytes = size_of::<crate::WorldEvent>()
            .checked_add(size_of::<PreparedBodyInterfaceActionV1<'s, 'r>>())
            .and_then(|n| n.checked_add(size_of::<PreparedBodyInterfaceEventV1>()))
            .and_then(|n| n.checked_add(size_of::<CompletedBodyInterfaceV1>()))
            .and_then(|n| n.checked_add(agent_id.len()))
            .and_then(|n| n.checked_add(interface_module_item_id.len()))
            .and_then(|n| n.checked_add(4 * (5 + 20) + 2 * reason + size_of::<BodyModuleSlot>()))
            .ok_or_else(limit)?;
        let event_fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let verify_fee = before
            .as_ref()
            .map(|b| {
                common
                    .memory()
                    .reserve(b.layout_bytes, MemoryClass::Ordinary)
                    .map(|f| f.retain(()))
                    .map_err(ValueTreeError::Memory)
            })
            .transpose()?;
        let body = super::body::evaluate_expand_body_interface_state(
            selected.envelope().id,
            agent_id,
            interface_module_item_id,
            before.as_ref().map(|b| &b.body),
            &mut |n| charge(common, n),
        )?;
        // Original CorePolicy preflight consumes the FIRST matching kind, even
        // if a later duplicate made has_interface_module_item true. Preserve
        // its depleted-first error before Accepted, never convert to rejection.
        if let (
            Some(before),
            crate::WorldEventBody::Domain(crate::DomainEvent::BodyInterfaceExpanded {
                consumed_item_id,
                ..
            }),
        ) = (&before, &body)
        {
            for row in &before.body.cargo_entries {
                charge(common, row.entity_id.len())?;
                if row.entity_id == *consumed_item_id
                    && row.entity_kind == CargoEntityKind::InterfaceModuleItem
                {
                    if row.quantity <= 0 {
                        let n="consume interface module item failed for : interface module item depleted:  quantity=".len()
                            .checked_add(agent_id.len()).and_then(|n|n.checked_add(consumed_item_id.len())).and_then(|n|n.checked_add(20)).ok_or_else(limit)?;
                        let _error_fee = common
                            .memory()
                            .reserve(
                                n.checked_mul(2)
                                    .and_then(|n| n.checked_add(size_of::<crate::WorldError>()))
                                    .ok_or_else(limit)?,
                                MemoryClass::Ordinary,
                            )
                            .map_err(ValueTreeError::Memory)?;
                        charge(common, n)?;
                        return Err(BodyMutationError::Original(Box::new(
                            BodyInterfacePreflightWorldErrorV1(
                                crate::WorldError::ResourceBalanceInvalid {
                                    reason: format!(
                                        "consume interface module item failed for {agent_id}: interface module item depleted: {consumed_item_id} quantity={}",
                                        row.quantity
                                    ),
                                },
                            ),
                        )));
                    }
                    break;
                }
            }
        }
        charge(common, 0)?;
        Ok(PreparedBodyInterfaceActionV1 {
            selected,
            outcome,
            before,
            body,
            qualification,
            core: common.work().identity(),
            memory: common.memory().clone(),
            time: now,
            identity: context.transaction.guard(),
            verify_fee,
            event_fee,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
fn same_body(
    a: &CurrentBodyInterfaceV1,
    b: &CurrentBodyInterfaceV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<bool, BodyMutationError> {
    charge(ctx, size_of::<u16>() * 4 + size_of::<u64>() * 4)?;
    if a.slot_next != b.slot_next
        || a.cargo_next != b.cargo_next
        || a.body.slot_capacity != b.body.slot_capacity
        || a.body.expansion_level != b.body.expansion_level
        || a.body.slots.len() != b.body.slots.len()
        || a.body.cargo_entries.len() != b.body.cargo_entries.len()
    {
        return Ok(false);
    }
    charge(
        ctx,
        a.body
            .frame_kind
            .len()
            .checked_add(b.body.frame_kind.len())
            .ok_or_else(limit)?,
    )?;
    if a.body.frame_kind != b.body.frame_kind {
        return Ok(false);
    }
    for ((ao, a), (bo, b)) in a
        .slots
        .iter()
        .zip(&a.body.slots)
        .zip(b.slots.iter().zip(&b.body.slots))
    {
        charge(ctx, size_of::<BodyModuleSlot>() * 2 + 16)?;
        charge(
            ctx,
            a.slot_id
                .len()
                .checked_add(b.slot_id.len())
                .and_then(|n| n.checked_add(a.installed_module.as_ref().map_or(0, String::len)))
                .and_then(|n| n.checked_add(b.installed_module.as_ref().map_or(0, String::len)))
                .ok_or_else(limit)?,
        )?;
        if ao != bo || a != b {
            return Ok(false);
        }
    }
    for ((ao, a), (bo, b)) in a
        .cargo
        .iter()
        .zip(&a.body.cargo_entries)
        .zip(b.cargo.iter().zip(&b.body.cargo_entries))
    {
        charge(ctx, size_of::<u64>() * 2 + size_of::<i64>() * 4 + 2)?;
        charge(
            ctx,
            a.entity_id
                .len()
                .checked_add(b.entity_id.len())
                .ok_or_else(limit)?,
        )?;
        if ao != bo || a != b {
            return Ok(false);
        }
    }
    Ok(true)
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_body_interface_v1<M, P, R, E, C, W, V, D, ME, F>(
    prepared: PreparedBodyInterfaceActionV1<'_, '_>,
    accepted: SelectedAcceptedDispositionV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    max_text_bytes: usize,
    max_event_bytes: usize,
    audit_work: &mut WorldEventDecodeWorkV1,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedBodyInterfaceV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    V: ValueNodeReader,
    D: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        charge(common, 0)?;
        step.transaction
            .validate_transaction_identity(&prepared.identity)?;
        accepted.validate(prepared.selected, step.transaction, step.mutation.memory)?;
        if prepared.core != common.work().identity()
            || !prepared.memory.same_pool(common.memory())
            || !prepared.memory.same_pool(step.mutation.memory)
            || prepared.time != step.time()
        {
            return Err(bad());
        }
        prepared
            .outcome
            .override_action_for_selected_v1(prepared.selected, step.transaction)?;
        let PreparedBodyInterfaceActionV1 {
            selected,
            before,
            body,
            qualification,
            verify_fee,
            event_fee,
            ..
        } = prepared;
        let domain = match body {
            crate::WorldEventBody::Domain(domain) => domain,
            _ => return Err(bad()),
        };
        if let crate::DomainEvent::ActionRejected { reason, .. } = domain {
            return step
                .append_selected_rejection_owned(
                    materials,
                    live,
                    selected,
                    reason,
                    max_event_bytes,
                    audit_work,
                    &mut *persist,
                )
                .map(|event| CompletedBodyInterfaceV1::Rejected {
                    accepted: accepted.into_audit(),
                    event,
                });
        }
        let agent = match &domain {
            crate::DomainEvent::BodyInterfaceExpanded { agent_id, .. }
            | crate::DomainEvent::BodyInterfaceExpandRejected { agent_id, .. } => agent_id.as_str(),
            _ => return Err(bad()),
        };
        {
            let read_stop = RefCell::new(&mut read.cancelled);
            let write_stop = RefCell::new(&mut step.mutation.cancelled);
            let mut context = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
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
                cancelled: || (read_stop.borrow_mut())() || (write_stop.borrow_mut())(),
            };
            let mut write = ValueMutationContextV1 {
                sink: &mut *step.mutation.sink,
                memory: step.mutation.memory,
                io: &mut *step.mutation.io,
                cancelled: || (read_stop.borrow_mut())() || (write_stop.borrow_mut())(),
            };
            let current = read_current_body_interface_v1(
                agent,
                qualification,
                &mut context,
                common,
                max_text_bytes,
                verify_fee,
                live,
            )?
            .ok_or_else(bad)?;
            let before = before.ok_or_else(bad)?;
            if !same_body(&before, &current, common)? {
                return Err(bad());
            }
            if let crate::DomainEvent::BodyInterfaceExpanded {
                slot_capacity,
                expansion_level,
                consumed_item_id,
                new_slot_id,
                slot_type,
                ..
            } = &domain
            {
                let mut count = current.body.cargo_entries.len() as u64;
                let mut chosen = None;
                for (index, row) in current.body.cargo_entries.iter().enumerate() {
                    charge(common, row.entity_id.len())?;
                    if row.entity_id == *consumed_item_id
                        && row.entity_kind == CargoEntityKind::InterfaceModuleItem
                    {
                        chosen = Some(index);
                        break;
                    }
                }
                let chosen = chosen.ok_or_else(bad)?;
                for (index, row) in current.body.cargo_entries.iter().enumerate() {
                    charge(common, row.entity_id.len())?;
                    let after = if index == chosen {
                        row.quantity.checked_sub(1).ok_or_else(bad)?
                    } else {
                        row.quantity
                    };
                    if row.entity_id == *consumed_item_id && after <= 0 {
                        let guard = context.transaction.guard();
                        change_agent_subtree_row_v1(
                            AgentSubtreeRowChangeV1 {
                                agent,
                                qualification,
                                ordinal: current.cargo[index],
                                expected_count: count,
                                expected_next: current.cargo_next,
                                expected: Some(AgentSubtreeRowRefV1::Cargo(row)),
                                after: None,
                                max_scalar_bytes: (max_text_bytes as u64)
                                    .checked_add(64)
                                    .ok_or_else(limit)?,
                            },
                            &mut context,
                            &mut write,
                            &guard,
                            live,
                        )
                        .map_err(overlay)?;
                        count = count.checked_sub(1).ok_or_else(bad)?;
                    } else if index == chosen {
                        let guard = context.transaction.guard();
                        change_agent_subtree_scalar_v1(
                            AgentSubtreeCellChangeV1 {
                                query: AgentSubtreeQualifiedRequestV1 {
                                    cell: AgentSubtreeScalarRequestV1 {
                                        agent,
                                        field: AgentFieldV1::CargoQuantity,
                                        ordinal: current.cargo[index],
                                    },
                                    qualification,
                                },
                                expected: Some(AgentScalarRefV1::I64(row.quantity)),
                                after: Some(AgentScalarRefV1::I64(after)),
                                max_scalar_bytes: 64,
                            },
                            &mut context,
                            &mut write,
                            &guard,
                            live,
                        )
                        .map_err(overlay)?;
                    }
                }
                for (field, old, next) in [
                    (
                        AgentFieldV1::SlotCapacity,
                        current.body.slot_capacity,
                        *slot_capacity,
                    ),
                    (
                        AgentFieldV1::ExpansionLevel,
                        current.body.expansion_level,
                        *expansion_level,
                    ),
                ] {
                    let guard = context.transaction.guard();
                    change_agent_subtree_scalar_v1(
                        AgentSubtreeCellChangeV1 {
                            query: AgentSubtreeQualifiedRequestV1 {
                                cell: AgentSubtreeScalarRequestV1 {
                                    agent,
                                    field,
                                    ordinal: 0,
                                },
                                qualification,
                            },
                            expected: Some(AgentScalarRefV1::U16(old)),
                            after: Some(AgentScalarRefV1::U16(next)),
                            max_scalar_bytes: 64,
                        },
                        &mut context,
                        &mut write,
                        &guard,
                        live,
                    )
                    .map_err(overlay)?;
                }
                charge(common, new_slot_id.len())?;
                let slot = BodyModuleSlot {
                    slot_id: new_slot_id.clone(),
                    slot_type: *slot_type,
                    installed_module: None,
                    locked: false,
                };
                let guard = context.transaction.guard();
                change_agent_subtree_row_v1(
                    AgentSubtreeRowChangeV1 {
                        agent,
                        qualification,
                        ordinal: current.slot_next,
                        expected_count: current.body.slots.len() as u64,
                        expected_next: current.slot_next,
                        expected: None,
                        after: Some(AgentSubtreeRowRefV1::Slot(&slot)),
                        max_scalar_bytes: (max_text_bytes as u64)
                            .checked_add(64)
                            .ok_or_else(limit)?,
                    },
                    &mut context,
                    &mut write,
                    &guard,
                    live,
                )
                .map_err(overlay)?;
            }
        }
        let key = AgentBodyKeyV1::new(agent).map_err(BodyMutationError::Invalid)?;
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
        if old.last_active != step.time() {
            let value = write_body_value(
                &AgentBodyValueV1 {
                    body_view: old.body_view,
                    last_active: step.time(),
                },
                step.mutation,
            )?;
            step.transaction
                .write(StateNamespaceV1::Business, key.as_bytes(), Some(value))?;
        }
        let seq = step.read_allocator(RuntimeAllocatorV1::Event)?;
        let (id, _, _) = super::World::preview_next_event_id(seq.next, seq.era);
        let event = crate::WorldEvent {
            id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(domain),
        };
        let cap = PreparedBodyInterfaceEventV1 {
            event: event_fee.retain(event),
            identity: step.transaction.guard(),
            accepted: accepted.into_audit(),
        };
        step.append_prepared_body_interface_event_v1(
            &cap,
            materials,
            live,
            max_event_bytes,
            &mut *persist,
        )?;
        charge(common, 0)?;
        Ok(CompletedBodyInterfaceV1::Domain(cap))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
