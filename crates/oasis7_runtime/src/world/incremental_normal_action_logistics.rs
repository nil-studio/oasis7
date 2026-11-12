//! RegisterLogisticsRoute owns the actual Current tuple predicate and creation.
//! This family does not turn a staged route/event into publication authority.
#[path = "incremental_normal_action_logistics_read.rs"]
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
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    LogisticsRouteV1, MaterialLedgerId, MaterialTransitPriority,
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_logistics_records::*,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};
use read::{charge, matches_current_tuple_v1, overlay, route_id_bounded_v1};
use std::mem::size_of;
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("logistics registration prepared source")
}
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("logistics registration layout").into()
}
pub(in crate::world) struct LogisticsRegistrationWorkV1 {
    pub pending_entries_remaining: usize,
    pub logistics: LogisticsWorkBudgetV1,
    pub max_id_bytes: usize,
    pub max_scalar_bytes: u64,
}
enum RegistrationBodyV1 {
    Rejected(crate::RejectReason),
    Registered(LogisticsRouteV1),
}
pub(in crate::world) struct PreparedLogisticsRegistrationV1<'s, 'r> {
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    agents: &'s ReservedValue<AgentSubtreeStructureQualificationV1>,
    logistics: &'s ReservedValue<LogisticsFamilyQualificationV1>,
    roots: &'s RootSetV1,
    body: RegistrationBodyV1,
    output_layout: ReservedValue<()>,
    verify_scan: Option<CurrentLogisticsRouteScanReservationV1>,
    verify_sort: Option<ReservedValue<()>>,
    identity: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    time: u64,
    fee: MemoryReservation,
}
/// Minted only after real tuple recheck, typed record creation and actor CAS.
pub(in crate::world) struct PreparedLogisticsRegistrationEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    identity: PreparedGuard,
    accepted: Option<SelectedActionAuditV1>,
    _layout: ReservedValue<()>,
}
impl PreparedLogisticsRegistrationEventV1 {
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
        if !self.event.uses_budget(memory) || !tx.uses_budget(memory) || self.event.time != now {
            return Err(bad());
        }
        if let Some(accepted) = &self.accepted {
            accepted.validate(tx)?;
            if !accepted.uses_budget(memory) {
                return Err(bad());
            }
        }
        Ok(())
    }
}
pub(in crate::world) enum CompletedLogisticsRegistrationV1 {
    Rejected {
        accepted: Option<SelectedActionAuditV1>,
        event: SelectedActionAuditV1,
        _layout: ReservedValue<()>,
    },
    Registered(PreparedLogisticsRegistrationEventV1),
}
impl CompletedLogisticsRegistrationV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match self {
            Self::Rejected { event, .. } => event.event(),
            Self::Registered(value) => value.event(),
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match self {
            Self::Rejected {
                accepted, event, ..
            } => {
                if let Some(value) = accepted {
                    value.validate(tx)?;
                }
                event.validate(tx)
            }
            Self::Registered(value) => {
                tx.validate_transaction_identity(&value.identity)?;
                if let Some(accepted) = &value.accepted {
                    accepted.validate(tx)?;
                }
                Ok(())
            }
        }
    }
}
fn ledger_len(value: &MaterialLedgerId) -> usize {
    match value {
        MaterialLedgerId::World => 0,
        MaterialLedgerId::Agent(id)
        | MaterialLedgerId::Site(id)
        | MaterialLedgerId::Factory(id) => id.len(),
    }
}
fn deny(note: &str) -> RegistrationBodyV1 {
    RegistrationBodyV1::Rejected(crate::RejectReason::RuleDenied {
        notes: vec![note.to_owned()],
    })
}
/// Sort only paid borrowed IDs. Every lexical comparison and header shift is
/// charged before the operation; no fallible comparator or extra ID copy.
fn tuple_exists<M, R, E, C>(
    ids: &ReservedValue<Vec<ReservedValue<String>>>,
    route: &LogisticsRouteV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    max: u64,
    prepaid: Option<ReservedValue<()>>,
) -> Result<bool, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let layout = ids
        .len()
        .checked_mul(size_of::<&str>())
        .and_then(|n| n.checked_add(size_of::<Vec<&str>>()))
        .ok_or_else(limited)?;
    let _fee = match prepaid {
        Some(fee) => {
            if !fee.uses_budget(common.memory()) || fee.reserved_bytes() < layout {
                return Err(bad());
            }
            fee
        }
        None => common
            .memory()
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?
            .retain(()),
    };
    let mut ordered: Vec<&str> = Vec::with_capacity(ids.len());
    if ordered.capacity() > ids.len() {
        return Err(limited());
    }
    for id in ids.iter() {
        let mut lo = 0;
        let mut hi = ordered.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            charge(
                common,
                ordered[mid]
                    .len()
                    .checked_add(id.len())
                    .ok_or_else(limited)?,
            )?;
            if ordered[mid] < id.as_str() {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        charge(
            common,
            (ordered.len() - lo)
                .checked_mul(size_of::<&str>())
                .ok_or_else(limited)?,
        )?;
        ordered.insert(lo, id.as_str());
    }
    for id in &ordered {
        if matches_current_tuple_v1(
            id,
            &route.from_ledger,
            &route.to_ledger,
            &route.kind,
            route.distance_km,
            route.priority,
            context,
            common,
            max,
        )? {
            return Ok(true);
        }
    }
    for id in ordered {
        charge(
            common,
            id.len()
                .checked_add(route.route_id.len())
                .ok_or_else(limited)?,
        )?;
        if id == route.route_id {
            return Ok(true);
        }
    }
    Ok(false)
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_logistics_registration_v1<'s, 'r, M, P, R, E, C, PE, PC>(
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    agents: &'s ReservedValue<AgentSubtreeStructureQualificationV1>,
    logistics: &'s ReservedValue<LogisticsFamilyQualificationV1>,
    roots: &'s RootSetV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    work: &mut LogisticsRegistrationWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedLogisticsRegistrationV1<'s, 'r>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
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
        let action = outcome
            .override_action_for_selected_v1(selected, context.transaction)?
            .unwrap_or(&selected.envelope().action);
        let crate::Action::RegisterLogisticsRoute {
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            distance_km,
            priority,
            capacity_units,
            tariff_electricity_per_unit,
        } = action
        else {
            return Err(bad());
        };
        let payload = requester_agent_id
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(kind.len().checked_mul(3)?))
            .and_then(|n| {
                n.checked_add(
                    ledger_len(from_ledger)
                        .checked_add(ledger_len(to_ledger))?
                        .checked_mul(2)?,
                )
            })
            .and_then(|n| n.checked_add(64 * 3))
            .ok_or_else(limited)?;
        let reason = "tariff_electricity_per_unit must be >= 0"
            .len()
            .max("from_ledger and to_ledger cannot be the same".len())
            .max("logistics route tuple already registered".len())
            .max("distance_km must be within 0..=".len() + 20);
        let layout = payload
            .checked_add(reason.checked_mul(2).ok_or_else(limited)?)
            .and_then(|n| {
                n.checked_add(
                    size_of::<PreparedLogisticsRegistrationV1<'s, 'r>>()
                        + size_of::<CompletedLogisticsRegistrationV1>()
                        + size_of::<PreparedLogisticsRegistrationEventV1>()
                        + size_of::<crate::WorldEvent>()
                        + size_of::<LogisticsRouteV1>()
                        + size_of::<crate::RejectReason>()
                        + size_of::<Vec<String>>()
                        + size_of::<String>(),
                )
            })
            .ok_or_else(limited)?;
        let fee = common
            .memory()
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let output_layout = common
            .memory()
            .reserve(
                size_of::<CompletedLogisticsRegistrationV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let guard = context.transaction.guard();
        let presence = read_current_agent_presence_v1(
            requester_agent_id,
            agents,
            roots,
            context,
            parent_read,
            &mut work.pending_entries_remaining,
            &guard,
            live,
        )
        .map_err(overlay)?;
        let mut verify_scan = None;
        let mut verify_sort = None;
        let body = if presence != CurrentAgentPresenceV1::Existing {
            charge(common, requester_agent_id.len())?;
            RegistrationBodyV1::Rejected(crate::RejectReason::AgentNotFound {
                agent_id: requester_agent_id.clone(),
            })
        } else {
            charge(
                common,
                ledger_len(from_ledger)
                    .checked_add(ledger_len(to_ledger))
                    .ok_or_else(limited)?,
            )?;
            if from_ledger == to_ledger {
                deny("from_ledger and to_ledger cannot be the same")
            } else {
                charge(common, kind.len())?;
                let normalized = super::logistics::normalize_logistics_route_kind(kind);
                if let Some(kind) = normalized {
                    if *distance_km < 0
                        || *distance_km > super::logistics::MATERIAL_TRANSFER_MAX_DISTANCE_KM
                    {
                        RegistrationBodyV1::Rejected(crate::RejectReason::RuleDenied {
                            notes: vec![format!(
                                "distance_km must be within 0..={}",
                                super::logistics::MATERIAL_TRANSFER_MAX_DISTANCE_KM
                            )],
                        })
                    } else if *capacity_units <= 0 {
                        deny("capacity_units must be > 0")
                    } else if *tariff_electricity_per_unit < 0 {
                        deny("tariff_electricity_per_unit must be >= 0")
                    } else {
                        let id = route_id_bounded_v1(
                            from_ledger,
                            to_ledger,
                            &kind,
                            *distance_km,
                            *priority,
                            common,
                        )?;
                        charge(
                            common,
                            id.len()
                                .checked_add(requester_agent_id.len())
                                .and_then(|n| n.checked_add(ledger_len(from_ledger)))
                                .and_then(|n| n.checked_add(ledger_len(to_ledger)))
                                .ok_or_else(limited)?,
                        )?;
                        let route = LogisticsRouteV1 {
                            route_id: id.to_string(),
                            from_ledger: from_ledger.clone(),
                            to_ledger: to_ledger.clone(),
                            kind,
                            distance_km: *distance_km,
                            priority: *priority,
                            owner_agent_id: requester_agent_id.clone(),
                            available: true,
                            capacity_units: *capacity_units,
                            reserved_capacity_units: 0,
                            tariff_electricity_per_unit: *tariff_electricity_per_unit,
                        };
                        let ids = read_current_logistics_route_ids_v1(
                            roots,
                            logistics,
                            parent_read.state,
                            context,
                            &mut work.logistics.scan,
                            &mut work.pending_entries_remaining,
                            work.max_id_bytes,
                        )
                        .map_err(overlay)?;
                        if tuple_exists(&ids, &route, context, common, work.max_scalar_bytes, None)?
                        {
                            deny("logistics route tuple already registered")
                        } else {
                            let sort_layout = ids
                                .len()
                                .checked_mul(size_of::<&str>())
                                .and_then(|n| n.checked_add(size_of::<Vec<&str>>()))
                                .ok_or_else(limited)?;
                            verify_sort = Some(
                                common
                                    .memory()
                                    .reserve(sort_layout, MemoryClass::Ordinary)
                                    .map_err(ValueTreeError::Memory)?
                                    .retain(()),
                            );
                            verify_scan = Some(
                                reserve_current_logistics_route_scan_v1(
                                    logistics,
                                    common.memory(),
                                    ids.len(),
                                    work.max_id_bytes,
                                )
                                .map_err(overlay)?,
                            );
                            RegistrationBodyV1::Registered(route)
                        }
                    }
                } else {
                    deny("material kind cannot be empty")
                }
            }
        };
        charge(common, 0)?;
        Ok(PreparedLogisticsRegistrationV1 {
            selected,
            outcome,
            agents,
            logistics,
            roots,
            body,
            output_layout,
            verify_scan,
            verify_sort,
            identity: context.transaction.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            time: now,
            fee,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_logistics_registration_v1<
    M,
    P,
    R,
    E,
    C,
    PE,
    PC,
    W,
    V,
    D,
    ME,
    CW,
    F,
>(
    prepared: PreparedLogisticsRegistrationV1<'_, '_>,
    accepted: SelectedAcceptedDispositionV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut LogisticsRegistrationWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    max_event_bytes: usize,
    audit: &mut WorldEventDecodeWorkV1,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedLogisticsRegistrationV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    V: ValueNodeReader,
    D: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    CW: FnMut() -> bool,
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
        let PreparedLogisticsRegistrationV1 {
            selected,
            agents,
            logistics,
            roots,
            body,
            output_layout,
            verify_scan,
            verify_sort,
            fee,
            ..
        } = prepared;
        let route = match body {
            RegistrationBodyV1::Rejected(reason) => {
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
                    .map(|event| CompletedLogisticsRegistrationV1::Rejected {
                        accepted: accepted.into_audit(),
                        event,
                        _layout: output_layout,
                    });
            }
            RegistrationBodyV1::Registered(route) => route,
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
            if read_current_agent_presence_v1(
                &route.owner_agent_id,
                agents,
                roots,
                &mut context,
                parent_read,
                &mut work.pending_entries_remaining,
                &guard,
                live,
            )
            .map_err(overlay)?
                != CurrentAgentPresenceV1::Existing
            {
                return Err(bad());
            }
            let ids = read_current_logistics_route_ids_with_reservation_v1(
                roots,
                logistics,
                parent_read.state,
                &mut context,
                &mut work.logistics.scan,
                &mut work.pending_entries_remaining,
                verify_scan.ok_or_else(bad)?,
            )
            .map_err(overlay)?;
            if tuple_exists(
                &ids,
                &route,
                &mut context,
                common,
                work.max_scalar_bytes,
                verify_sort,
            )? {
                return Err(bad());
            }
            let guard = context.transaction.guard();
            create_logistics_record_overlay_v1(
                LogisticsRecordCreateV1 {
                    key: LogisticsRecordKeyV1 {
                        kind: LogisticsRecordKindV1::Route,
                        id: &route.route_id,
                    },
                    after: LogisticsRecordRefV1::Route(&route),
                    qualification: logistics,
                },
                &mut context,
                write,
                &mut work.logistics,
                &guard,
                live,
            )
            .map_err(overlay)?;
        }
        let key = AgentBodyKeyV1::new(&route.owner_agent_id).map_err(BodyMutationError::Invalid)?;
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
        let after = write_body_value(
            &AgentBodyValueV1 {
                body_view: old.body_view,
                last_active: step.time(),
            },
            step.mutation,
        )?;
        step.transaction
            .write(StateNamespaceV1::Business, key.as_bytes(), Some(after))?;
        charge(common, route.owner_agent_id.len())?;
        let requester = route.owner_agent_id.clone();
        let sequence = step.read_allocator(RuntimeAllocatorV1::Event)?;
        let (id, _, _) = super::World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(crate::DomainEvent::LogisticsRouteRegistered {
                requester_agent_id: requester,
                route_id: Some(route.route_id),
                from_ledger: route.from_ledger,
                to_ledger: route.to_ledger,
                kind: route.kind,
                distance_km: route.distance_km,
                priority: route.priority,
                owner_agent_id: route.owner_agent_id,
                available: true,
                capacity_units: route.capacity_units,
                tariff_electricity_per_unit: route.tariff_electricity_per_unit,
            }),
        };
        let cap = PreparedLogisticsRegistrationEventV1 {
            event: fee.retain(event),
            identity: step.transaction.guard(),
            accepted: accepted.into_audit(),
            _layout: output_layout,
        };
        step.append_prepared_logistics_registration_event_v1(
            &cap,
            materials,
            live,
            max_event_bytes,
            &mut *persist,
        )?;
        Ok(CompletedLogisticsRegistrationV1::Registered(cap))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
