//! NEW unhooked DeclareWar owner, planned as a private Diplomacy child.
//! It consumes real Selected/Rule/Accepted owners and never rebuilds a World.
use super::*;
#[path = "incremental_normal_action_war_facts.rs"]
mod current;
#[path = "incremental_normal_action_war_resources.rs"]
mod resources;
use crate::world::{
    event_processing::war_fixed::{WarFactsV1, evaluate_war_action_v1},
    incremental_allocators::RuntimeAllocatorV1,
    incremental_body::write_body_value,
    incremental_body_event::{
        AgentEventOriginalV1, materials::CorePolicyMaterialContextV1, read_fixed,
    },
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_state::{StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
use oasis7_world_state::{
    WarState,
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_diplomacy::*,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
    incremental_world_rule_composite_family::observe_current_continuity_risk_v1,
    incremental_world_rule_organization::root::observe_current_organization_risk_v1,
};
pub(in crate::world) struct PreparedWarActionV1<'s, 'r, 'source> {
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &'s AllianceSourcesV1<'s, 'source>,
    body: crate::WorldEventBody,
    record: Option<ReservedValue<WarState>>,
    fees: Fees,
    verification: Fees,
    identity: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    time: u64,
    event_fee: MemoryReservation,
    layout: ReservedValue<()>,
}
fn root_risk<M, P, R, E, C, PE, PC>(
    id: &str,
    sources: &AllianceSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    work: &mut AllianceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<bool, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    charge(common, id.len())?;
    let guard = context.transaction.guard();
    let continuity = observe_current_continuity_risk_v1(
        id,
        sources.continuity,
        sources.roots,
        read.state,
        context,
        &mut work.scope_scan,
        &mut work.continuity_read,
        &guard,
        live,
    )
    .map_err(overlay)?;
    continuity
        .validate(context.transaction, common.memory(), live)
        .map_err(overlay)?;
    let frozen = continuity.risk_frozen();
    drop(continuity);
    if frozen {
        return Ok(true);
    }
    let mut auth = WorldFieldReadContextV1 {
        root: sources.roots.state_root(StateNamespaceV1::Authorization),
        state: read.state,
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
    let organization = observe_current_organization_risk_v1(
        id,
        sources.organizations,
        sources.roots,
        context.transaction,
        &mut auth,
        &mut work.scope_scan,
        &mut work.scope_cells,
        &mut work.pending_entries_remaining,
        work.max_text_bytes as u64,
        &guard,
        live,
    )
    .map_err(overlay)?;
    organization
        .validate(context.transaction, common.memory(), live)
        .map_err(overlay)?;
    Ok(organization.risk_frozen())
}
fn evaluate<M, P, R, E, C, PE, PC>(
    action_id: crate::ActionId,
    action: &crate::Action,
    sources: &AllianceSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    work: &mut AllianceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    mut fees: Fees,
) -> Result<EvaluatedAllianceBodyV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let crate::Action::DeclareWar {
        aggressor_alliance_id,
        defender_alliance_id,
        ..
    } = action
    else {
        return Err(bad());
    };
    if root_risk(
        aggressor_alliance_id,
        sources,
        context,
        read,
        work,
        common,
        live,
    )? || root_risk(
        defender_alliance_id,
        sources,
        context,
        read,
        work,
        common,
        live,
    )? {
        let note = "organization_dissolution_risk_frozen";
        let n = note.len()
            + size_of::<crate::WorldEventBody>()
            + size_of::<crate::RejectReason>()
            + size_of::<Vec<String>>()
            + 4 * size_of::<String>();
        if let Some(fee) = &fees[0] {
            if !fee.uses_budget(common.memory()) || fee.reserved_bytes() < n {
                return Err(limited());
            }
        } else {
            fees[0] = Some(
                common
                    .memory()
                    .reserve(n, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?
                    .retain(()),
            );
        }
        charge(common, note.len())?;
        return Ok(EvaluatedAllianceBodyV1 {
            body: crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                action_id,
                reason: crate::RejectReason::RuleDenied {
                    notes: vec![note.to_owned()],
                },
            }),
            fees,
        });
    }
    let inner = facts::CurrentAllianceFactsV1 {
        agents: sources.agents,
        diplomacy: sources.diplomacy,
        roots: sources.roots,
        context,
        read,
        common,
        index: &mut work.index,
        pending: &mut work.pending_entries_remaining,
        live,
        max_text: work.max_text_bytes,
        fees,
    };
    let mut source = current::CurrentWarFactsV1 { source: inner };
    let body = evaluate_war_action_v1(&mut source, action_id, action).map_err(evaluated)?;
    Ok(EvaluatedAllianceBodyV1 {
        body,
        fees: source.source.fees,
    })
}
fn make_record(
    body: &crate::WorldEventBody,
    now: u64,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<ReservedValue<WarState>>, BodyMutationError> {
    let crate::WorldEventBody::Domain(crate::DomainEvent::WarDeclared {
        initiator_agent_id,
        war_id,
        aggressor_alliance_id,
        defender_alliance_id,
        objective,
        intensity,
        mobilization_electricity_cost,
        mobilization_data_cost,
    }) = body
    else {
        return Ok(None);
    };
    let mut bytes = size_of::<WarState>();
    for s in [
        initiator_agent_id,
        war_id,
        aggressor_alliance_id,
        defender_alliance_id,
        objective,
    ] {
        charge(common, s.len())?;
        bytes = bytes.checked_add(s.len()).ok_or_else(limited)?;
    }
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    charge(common, bytes)?;
    let record = WarState {
        war_id: war_id.clone(),
        initiator_agent_id: initiator_agent_id.clone(),
        aggressor_alliance_id: aggressor_alliance_id.clone(),
        defender_alliance_id: defender_alliance_id.clone(),
        objective: objective.clone(),
        intensity: *intensity,
        active: true,
        declared_mobilization_electricity_cost: *mobilization_electricity_cost,
        declared_mobilization_data_cost: *mobilization_data_cost,
        max_duration_ticks: 6_u64.saturating_add(u64::from(*intensity) * 2),
        aggressor_score: 0,
        defender_score: 0,
        concluded_at: None,
        winner_alliance_id: None,
        loser_alliance_id: None,
        settlement_summary: None,
        participant_outcomes: Vec::new(),
        declared_at: now,
    };
    for (s, before) in [
        (&record.war_id, war_id),
        (&record.initiator_agent_id, initiator_agent_id),
        (&record.aggressor_alliance_id, aggressor_alliance_id),
        (&record.defender_alliance_id, defender_alliance_id),
        (&record.objective, objective),
    ] {
        if s.capacity() > before.len() {
            return Err(limited());
        }
    }
    Ok(Some(fee.retain(record)))
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_war_action_v1<'s, 'r, 'source, M, P, R, E, C, PE, PC>(
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &'s AllianceSourcesV1<'s, 'source>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    work: &mut AllianceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedWarActionV1<'s, 'r, 'source>, BodyMutationError>
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
        let event_fee = common
            .memory()
            .reserve(
                size_of::<crate::WorldEvent>()
                    + size_of::<PreparedWarActionV1<'s, 'r, 'source>>()
                    + size_of::<PreparedAllianceEventV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let layout = common
            .memory()
            .reserve(
                size_of::<CompletedAllianceActionV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let mut business = WorldFieldReadContextV1 {
            root: sources.roots.state_root(StateNamespaceV1::Business),
            state: parent.state,
            values: ValueReadContext {
                source: parent.values.source,
                memory: parent.values.memory,
                class: parent.values.class,
                io: &mut *parent.values.io,
            },
            key_work: &mut *parent.key_work,
            semantic_bytes: parent.semantic_bytes,
            record_bytes: parent.record_bytes,
            take_state_error: &mut parent.take_state_error,
            cancelled: &mut parent.cancelled,
        };
        let evaluated_body = evaluate(
            selected.envelope().id,
            action,
            sources,
            context,
            &mut business,
            work,
            common,
            live,
            std::array::from_fn(|_| None),
        )?;
        if let crate::WorldEventBody::Domain(crate::DomainEvent::WarDeclared {
            aggressor_alliance_id,
            defender_alliance_id,
            ..
        }) = &evaluated_body.body
        {
            if root_risk(
                aggressor_alliance_id,
                sources,
                context,
                &mut business,
                work,
                common,
                live,
            )? || root_risk(
                defender_alliance_id,
                sources,
                context,
                &mut business,
                work,
                common,
                live,
            )? {
                return Err(original(crate::WorldError::ResourceBalanceInvalid {
                    reason: "organization_risk_frozen".into(),
                }));
            }
        }
        let record = make_record(&evaluated_body.body, now, common)?;
        let mut verification: Fees = std::array::from_fn(|_| None);
        for (i, fee) in evaluated_body.fees.iter().enumerate() {
            if let Some(fee) = fee {
                verification[i] = Some(
                    common
                        .memory()
                        .reserve(fee.reserved_bytes(), MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?
                        .retain(()),
                );
            }
        }
        Ok(PreparedWarActionV1 {
            selected,
            outcome,
            sources,
            body: evaluated_body.body,
            record,
            fees: evaluated_body.fees,
            verification,
            identity: context.transaction.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            time: now,
            event_fee,
            layout,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
fn same(
    a: &crate::WorldEventBody,
    b: &crate::WorldEventBody,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<bool, BodyMutationError> {
    let (
        crate::WorldEventBody::Domain(crate::DomainEvent::WarDeclared {
            initiator_agent_id: aa,
            war_id: ai,
            aggressor_alliance_id: ag,
            defender_alliance_id: ad,
            objective: ao,
            intensity: an,
            mobilization_electricity_cost: ae,
            mobilization_data_cost: ax,
        }),
        crate::WorldEventBody::Domain(crate::DomainEvent::WarDeclared {
            initiator_agent_id: ba,
            war_id: bi,
            aggressor_alliance_id: bg,
            defender_alliance_id: bd,
            objective: bo,
            intensity: bn,
            mobilization_electricity_cost: be,
            mobilization_data_cost: bx,
        }),
    ) = (a, b)
    else {
        return Ok(false);
    };
    for (a, b) in [(aa, ba), (ai, bi), (ag, bg), (ad, bd), (ao, bo)] {
        charge(common, a.len().checked_add(b.len()).ok_or_else(limited)?)?;
        if a != b {
            return Ok(false);
        }
    }
    charge(common, 2 * (size_of::<u32>() + 2 * size_of::<i64>()))?;
    Ok((an, ae, ax) == (bn, be, bx))
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_war_action_v1<M, P, V, R, W, D, E, C, PE, PC, CW, ME, F>(
    prepared: PreparedWarActionV1<'_, '_, '_>,
    accepted: SelectedAcceptedDispositionV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut AllianceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    max_event_bytes: usize,
    audit: &mut WorldEventDecodeWorkV1,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedAllianceActionV1, BodyMutationError>
where
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
        let action = prepared
            .outcome
            .override_action_for_selected_v1(prepared.selected, step.transaction)?
            .unwrap_or(&prepared.selected.envelope().action);
        let PreparedWarActionV1 {
            selected,
            sources,
            body,
            record,
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
        let (
            crate::WorldEventBody::Domain(crate::DomainEvent::WarDeclared {
                initiator_agent_id,
                war_id,
                mobilization_electricity_cost,
                mobilization_data_cost,
                ..
            }),
            Some(record),
        ) = (&body, record.as_ref())
        else {
            return Err(bad());
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
            let mut business = WorldFieldReadContextV1 {
                root: sources.roots.state_root(StateNamespaceV1::Business),
                state: parent.state,
                values: ValueReadContext {
                    source: parent.values.source,
                    memory: parent.values.memory,
                    class: parent.values.class,
                    io: &mut *parent.values.io,
                },
                key_work: &mut *parent.key_work,
                semantic_bytes: parent.semantic_bytes,
                record_bytes: parent.record_bytes,
                take_state_error: &mut parent.take_state_error,
                cancelled: &mut parent.cancelled,
            };
            let checked = evaluate(
                selected.envelope().id,
                action,
                sources,
                &mut context,
                &mut business,
                work,
                common,
                live,
                verification,
            )?;
            if !same(&body, &checked.body, common)? {
                return Err(bad());
            }
            drop(checked);
        }
        resources::debit_resource_v1(
            initiator_agent_id,
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            *mobilization_electricity_cost,
            sources.agents,
            step,
            read,
            work.max_text_bytes as u64,
            common,
            live,
        )?;
        resources::debit_resource_v1(
            initiator_agent_id,
            oasis7_agent_api::world_action::ResourceKind::Data,
            *mobilization_data_cost,
            sources.agents,
            step,
            read,
            work.max_text_bytes as u64,
            common,
            live,
        )?;
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
            create_qualified_diplomacy_empty_v1(
                DiplomacyEmptyCreateV1 {
                    id: war_id,
                    source: DiplomacySourceV1::War(record),
                    family: sources.diplomacy,
                    max_pending_node_bytes: work.max_pending_node_bytes,
                },
                &mut context,
                write,
                &guard,
                live,
                &mut work.write_cells,
            )
            .map_err(overlay)?;
        }
        let key = AgentBodyKeyV1::new(initiator_agent_id).map_err(BodyMutationError::Invalid)?;
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
        let sequence = step.read_allocator(RuntimeAllocatorV1::Event)?;
        let (id, _, _) = crate::world::World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id,
            time: step.time(),
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
        // The eventual same-private-cap sink must explicitly allow WarDeclared only
        // after this typed debit/create sequence; raw public Domain dispatch stays shut.
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
