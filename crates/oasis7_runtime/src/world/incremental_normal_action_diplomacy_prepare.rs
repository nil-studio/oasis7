//! Original outer risk gates, fixed four-action evaluation and sparse preflight.
use super::*;
use crate::world::event_processing::alliance_fixed::{
    AllianceFactsV1, evaluate_alliance_action_v1,
};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueReadContext,
};
use oasis7_world_state::{
    incremental_diplomacy::{DiplomacyFieldV1, open_current_diplomacy_relation_record_v1},
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_composite_family::observe_current_continuity_risk_v1,
    incremental_world_rule_continuity::{
        ContinuityMembershipKindV1, prepare_current_continuity_membership_v1,
        read_current_organization_membership_v1,
    },
    incremental_world_rule_organization::root::observe_current_organization_risk_v1,
};
struct Risk {
    frozen: bool,
    active: bool,
    estate: bool,
    registered: bool,
}
#[allow(clippy::too_many_arguments)]
fn risk<M, P, R, E, C, PE, PC>(
    id: &str,
    registration: bool,
    sources: &AllianceSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    work: &mut AllianceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Risk, BodyMutationError>
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
        parent.state,
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
    let mut read = WorldFieldReadContextV1 {
        root: sources.roots.state_root(StateNamespaceV1::Authorization),
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
    let guard = context.transaction.guard();
    let organizations = observe_current_organization_risk_v1(
        id,
        sources.organizations,
        sources.roots,
        context.transaction,
        &mut read,
        &mut work.scope_scan,
        &mut work.scope_cells,
        &mut work.pending_entries_remaining,
        work.max_text_bytes as u64,
        &guard,
        live,
    )
    .map_err(overlay)?;
    organizations
        .validate(context.transaction, common.memory(), live)
        .map_err(overlay)?;
    let frozen = frozen || organizations.risk_frozen();
    let active = organizations.active_case();
    let estate = organizations.undisposed_estate();
    drop(organizations);
    drop(read);
    let registered = if registration {
        let guard = context.transaction.guard();
        let current = read_current_organization_membership_v1(
            sources.continuity,
            sources.roots,
            id,
            "",
            parent.state,
            context,
            &mut work.scope_scan,
            &mut work.continuity_read,
            &guard,
            live,
        )
        .map_err(overlay)?;
        current
            .validate(context.transaction, common.memory(), live)
            .map_err(overlay)?;
        current.registered()
    } else {
        false
    };
    charge(common, 4)?;
    Ok(Risk {
        frozen,
        active,
        estate,
        registered,
    })
}
fn retained_note(
    action_id: crate::ActionId,
    note: &str,
    fees: &mut Fees,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<crate::WorldEventBody, BodyMutationError> {
    let n = note
        .len()
        .checked_add(
            size_of::<crate::WorldEventBody>()
                + size_of::<crate::RejectReason>()
                + size_of::<Vec<String>>()
                + 4 * size_of::<String>(),
        )
        .ok_or_else(limited)?;
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
    Ok(crate::WorldEventBody::Domain(
        crate::DomainEvent::ActionRejected {
            action_id,
            reason: crate::RejectReason::RuleDenied {
                notes: vec![note.to_owned()],
            },
        },
    ))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<M, P, R, E, C, RE, RC>(
    action_id: crate::ActionId,
    action: &crate::Action,
    sources: &AllianceSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, R, RE, RC>,
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
    RE: FnMut() -> Option<ValueTreeError>,
    RC: FnMut() -> bool,
{
    if let crate::Action::JoinAlliance { alliance_id, .. }
    | crate::Action::LeaveAlliance { alliance_id, .. }
    | crate::Action::DissolveAlliance { alliance_id, .. } = action
    {
        let dissolve = matches!(action, crate::Action::DissolveAlliance { .. });
        let risk = risk(
            alliance_id,
            dissolve,
            sources,
            context,
            read,
            work,
            common,
            live,
        )?;
        if risk.frozen || risk.active || dissolve && (risk.registered || risk.estate) {
            let body = retained_note(
                action_id,
                "organization_estate_requires_case_disposition",
                &mut fees,
                common,
            )?;
            return Ok(EvaluatedAllianceBodyV1 { body, fees });
        }
    }
    let mut source = facts::CurrentAllianceFactsV1 {
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
    let body = evaluate_alliance_action_v1(&mut source, action_id, action).map_err(evaluated)?;
    Ok(EvaluatedAllianceBodyV1 {
        body,
        fees: source.fees,
    })
}
fn created(
    body: &crate::WorldEventBody,
    now: u64,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<ReservedValue<AllianceState>>, BodyMutationError> {
    let crate::WorldEventBody::Domain(crate::DomainEvent::AllianceFormed {
        proposer_agent_id,
        alliance_id,
        members,
        charter,
    }) = body
    else {
        return Ok(None);
    };
    let mut payload = proposer_agent_id
        .len()
        .checked_add(alliance_id.len())
        .and_then(|n| n.checked_add(charter.len()))
        .ok_or_else(limited)?;
    for member in members {
        charge(common, member.len())?;
        payload = payload.checked_add(member.len()).ok_or_else(limited)?;
    }
    let layout = payload
        .checked_add(
            members
                .len()
                .checked_mul(size_of::<String>())
                .ok_or_else(limited)?,
        )
        .and_then(|n| n.checked_add(size_of::<AllianceState>()))
        .ok_or_else(limited)?;
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    charge(common, payload)?;
    let alliance = AllianceState {
        alliance_id: alliance_id.clone(),
        members: members.clone(),
        charter: charter.clone(),
        formed_by_agent_id: proposer_agent_id.clone(),
        formed_at: now,
    };
    if alliance.alliance_id.capacity() > alliance_id.len()
        || alliance.charter.capacity() > charter.len()
        || alliance.formed_by_agent_id.capacity() > proposer_agent_id.len()
        || alliance.members.capacity() > members.len()
        || alliance
            .members
            .iter()
            .zip(members)
            .any(|(a, b)| a.capacity() > b.len())
    {
        return Err(limited());
    }
    Ok(Some(fee.retain(alliance)))
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_alliance_action_v1<'s, 'r, 'source, M, P, R, E, C, PE, PC>(
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &'s AllianceSourcesV1<'s, 'source>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    work: &mut AllianceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedAllianceActionV1<'s, 'r, 'source>, BodyMutationError>
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
        if !matches!(
            action,
            crate::Action::FormAlliance { .. }
                | crate::Action::JoinAlliance { .. }
                | crate::Action::LeaveAlliance { .. }
                | crate::Action::DissolveAlliance { .. }
        ) {
            return Err(bad());
        }
        let event_fee = common
            .memory()
            .reserve(
                size_of::<crate::WorldEvent>()
                    + size_of::<PreparedAllianceActionV1<'s, 'r, 'source>>()
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
        let body = &evaluated_body.body;
        let mut before = None;
        let mut proof = None;
        let mut after_members = None;
        let mut membership = None;
        let organization = match body {
            crate::WorldEventBody::Domain(
                crate::DomainEvent::AllianceJoined { alliance_id, .. }
                | crate::DomainEvent::AllianceLeft { alliance_id, .. }
                | crate::DomainEvent::AllianceDissolved { alliance_id, .. },
            ) => Some(alliance_id.as_str()),
            _ => None,
        };
        if let Some(id) = organization {
            // Publication uses the normalized event ID, unlike the outer Action gate.
            let dissolve = matches!(
                body,
                crate::WorldEventBody::Domain(crate::DomainEvent::AllianceDissolved { .. })
            );
            let risk = risk(
                id,
                dissolve,
                sources,
                context,
                &mut business,
                work,
                common,
                live,
            )?;
            if risk.frozen || dissolve && (risk.registered || risk.estate) {
                return Err(original(crate::WorldError::ResourceBalanceInvalid {
                    reason: "organization_case_required".into(),
                }));
            }
            let guard = context.transaction.guard();
            let opened = open_current_diplomacy_relation_record_v1(
                DiplomacyFieldV1::Alliances,
                id,
                sources.diplomacy,
                sources.roots,
                context.transaction,
                &mut business,
                &mut work.index,
                &guard,
                live,
            )
            .map_err(overlay)?
            .ok_or_else(bad)?;
            let record =
                read::read_alliance_v1(id, &opened, &mut business, work.max_text_bytes, common)?;
            match body {
                crate::WorldEventBody::Domain(crate::DomainEvent::AllianceJoined {
                    member_agent_id,
                    ..
                }) => {
                    after_members = Some(
                        members::project_members_v1(
                            id,
                            &record,
                            member_agent_id,
                            members::MemberOperationV1::Join,
                            common,
                        )
                        .map_err(evaluated)?,
                    )
                }
                crate::WorldEventBody::Domain(crate::DomainEvent::AllianceLeft {
                    member_agent_id,
                    ..
                }) => {
                    after_members = Some(
                        members::project_members_v1(
                            id,
                            &record,
                            member_agent_id,
                            members::MemberOperationV1::Leave,
                            common,
                        )
                        .map_err(evaluated)?,
                    )
                }
                _ => {}
            }
            if let crate::WorldEventBody::Domain(
                event @ (crate::DomainEvent::AllianceJoined {
                    member_agent_id, ..
                }
                | crate::DomainEvent::AllianceLeft {
                    member_agent_id, ..
                }),
            ) = body
            {
                let guard = context.transaction.guard();
                let current = read_current_organization_membership_v1(
                    sources.continuity,
                    sources.roots,
                    id,
                    member_agent_id,
                    business.state,
                    context,
                    &mut work.scope_scan,
                    &mut work.continuity_read,
                    &guard,
                    live,
                )
                .map_err(overlay)?;
                let digest = hash::membership_event_hash_v1(event, common)?;
                let joined = matches!(event, crate::DomainEvent::AllianceJoined { .. });
                let mut prepared = prepare_current_continuity_membership_v1(
                    current,
                    if joined {
                        ContinuityMembershipKindV1::Joined
                    } else {
                        ContinuityMembershipKindV1::Left
                    },
                    now,
                    &digest,
                    context.transaction,
                    common,
                    live,
                )
                .map_err(membership_error)?;
                if prepared.requires_controller() {
                    let controller_sources = controller::ControllerSourcesV1 {
                        roots: sources.roots,
                        orders: sources.orders,
                        agents: sources.agents,
                        native: sources.native,
                    };
                    let controller = controller::resolve_current_controller_v1(
                        member_agent_id,
                        &controller_sources,
                        context,
                        &mut business,
                        &mut work.controller,
                        common,
                        live,
                    )?
                    .ok_or_else(|| {
                        original(crate::WorldError::ResourceBalanceInvalid {
                            reason: "organization_controller_binding_unavailable".into(),
                        })
                    })?;
                    controller.validate(context.transaction, common)?;
                    prepared = prepared
                        .with_controller(controller.value(), common)
                        .map_err(membership_error)?;
                }
                membership = Some(prepared);
            }
            before = Some(record);
            proof = Some(opened);
        }
        let created = created(body, now, common)?;
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
        Ok(PreparedAllianceActionV1 {
            selected,
            outcome,
            sources,
            body: evaluated_body.body,
            before,
            proof,
            created,
            after_members,
            membership,
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
